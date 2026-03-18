use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::stdout;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Parser, ValueEnum};
use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};
use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "submission-similarity",
    version,
    about = "Finds high-similarity notebook submissions by nbgrader cell ID"
)]
struct Cli {
    #[arg(long, required_unless_present = "load_report")]
    root_dir: Option<PathBuf>,

    #[arg(long, required_unless_present = "load_report")]
    assignment: Option<String>,

    #[arg(long, required_unless_present = "load_report")]
    question: Option<String>,

    #[arg(long, required_unless_present = "load_report")]
    cell_id: Option<String>,

    #[arg(long, value_enum, required_unless_present = "load_report")]
    language: Option<Language>,

    #[arg(long)]
    load_report: Option<PathBuf>,

    #[arg(long, default_value_t = 0.85)]
    threshold: f64,

    #[arg(long, default_value_t = 100)]
    max_results: usize,

    #[arg(long, default_value = "similarity-report.json")]
    json_output: PathBuf,

    #[arg(long)]
    solution_dir: Option<PathBuf>,

    #[arg(long)]
    solution_threshold: Option<f64>,

    #[arg(long)]
    target_student: Option<String>,

    #[arg(long, default_value_t = false)]
    table_only_high: bool,

    #[arg(long, default_value_t = false)]
    lowercase: bool,

    #[arg(long, default_value_t = false)]
    tui: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Language {
    C,
    Python,
    Java,
    Go,
}

#[derive(Debug, Clone)]
struct Submission {
    student: String,
    notebook_path: PathBuf,
    source: String,
    normalized: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PairScore {
    student_a: String,
    student_b: String,
    score: f64,
    notebook_a: String,
    notebook_b: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReportConfig {
    root_dir: String,
    assignment: String,
    question: String,
    cell_id: String,
    language: Language,
    #[serde(default = "default_threshold")]
    threshold: f64,
    #[serde(default = "default_max_results")]
    max_results: usize,
    solution_dir: Option<String>,
    solution_notebook: Option<String>,
    solution_threshold: Option<f64>,
    target_student: Option<String>,
    #[serde(default)]
    table_only_high: bool,
    #[serde(default)]
    lowercase: bool,
    #[serde(default)]
    tui: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Report {
    config: ReportConfig,
    submission_count: usize,
    pair_count: usize,
    #[serde(default)]
    excluded_high_pairs_due_to_solution: usize,
    high_similarity_count: usize,
    warnings: Vec<String>,
    high_similarity_pairs: Vec<PairScore>,
    all_pairs_sorted: Vec<PairScore>,
    #[serde(default)]
    pair_sources: HashMap<String, String>,
    #[serde(default)]
    deleted_pair_keys: Vec<String>,
    #[serde(default)]
    flagged_pair_keys: Vec<String>,
}

#[derive(Debug, Clone)]
struct SolutionSubmission {
    notebook_path: PathBuf,
    normalized: String,
}

#[derive(Debug, Clone)]
struct QuerySpec {
    root_dir: PathBuf,
    assignment: String,
    question: String,
    cell_id: String,
    language: Language,
}

#[derive(Debug)]
struct TuiEdits {
    deleted_pair_keys: HashSet<String>,
    flagged_pair_keys: HashSet<String>,
}

fn default_threshold() -> f64 {
    0.85
}

fn default_max_results() -> usize {
    100
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    validate_args(&cli)?;

    let mut report = if let Some(load_path) = &cli.load_report {
        load_report(load_path)?
    } else {
        let query = query_spec_from_cli(&cli)?;
        let mut warnings = Vec::new();
        let submissions = collect_submissions(&query, cli.lowercase, &mut warnings)?;
        if submissions.len() < 2 {
            return Err(format!(
                "found {} matching submission(s); need at least 2",
                submissions.len()
            ));
        }
        let source_by_notebook: HashMap<String, String> = submissions
            .iter()
            .map(|submission| {
                (
                    submission.notebook_path.display().to_string(),
                    submission.source.clone(),
                )
            })
            .collect();
        let solution_submission = collect_solution_submission(&cli, &query, &mut warnings)?;
        if let Some(solution) = &solution_submission {
            warnings.push(format!(
                "solution notebook used: {}",
                solution.notebook_path.display()
            ));
        }
        let solution_threshold = cli.solution_threshold.unwrap_or(cli.threshold);

        let all_pairs_initial = compute_pair_scores(&submissions, cli.target_student.as_deref());
        let (all_pairs, excluded_high_pairs_due_to_solution) =
            if let Some(solution) = &solution_submission {
                let similarity_to_solution =
                    submission_similarity_to_solution(&submissions, &solution.normalized);
                filter_pairs_by_solution_similarity(
                    all_pairs_initial,
                    &similarity_to_solution,
                    cli.threshold,
                    solution_threshold,
                )
            } else {
                (all_pairs_initial, 0)
            };
        if excluded_high_pairs_due_to_solution > 0 {
            warnings.push(format!(
                "excluded {excluded_high_pairs_due_to_solution} high-similarity pair(s) because one or both submissions were also highly similar to the solution"
            ));
        }
        let mut high_pairs: Vec<PairScore> = all_pairs
            .iter()
            .filter(|pair| pair.score >= cli.threshold)
            .cloned()
            .collect();
        high_pairs.sort_by(sort_scores_desc);
        Report {
            config: ReportConfig {
                root_dir: query.root_dir.display().to_string(),
                assignment: query.assignment,
                question: query.question,
                cell_id: query.cell_id,
                language: query.language,
                threshold: cli.threshold,
                max_results: cli.max_results,
                solution_dir: cli.solution_dir.clone().map(|p| p.display().to_string()),
                solution_notebook: solution_submission
                    .as_ref()
                    .map(|s| s.notebook_path.display().to_string()),
                solution_threshold: solution_submission.as_ref().map(|_| solution_threshold),
                target_student: cli.target_student.clone(),
                table_only_high: cli.table_only_high,
                lowercase: cli.lowercase,
                tui: cli.tui,
            },
            submission_count: submissions.len(),
            pair_count: all_pairs.len(),
            excluded_high_pairs_due_to_solution,
            high_similarity_count: high_pairs.len(),
            warnings,
            high_similarity_pairs: high_pairs,
            all_pairs_sorted: all_pairs,
            pair_sources: source_by_notebook,
            deleted_pair_keys: Vec::new(),
            flagged_pair_keys: Vec::new(),
        }
    };

    let effective_threshold = if cli.load_report.is_some()
        && (cli.threshold - default_threshold()).abs() < f64::EPSILON
    {
        report.config.threshold
    } else {
        cli.threshold
    };
    let effective_max_results =
        if cli.load_report.is_some() && cli.max_results == default_max_results() {
            report.config.max_results
        } else {
            cli.max_results
        };

    let mut deleted_pair_keys: HashSet<String> = report.deleted_pair_keys.iter().cloned().collect();
    let mut flagged_pair_keys: HashSet<String> = report.flagged_pair_keys.iter().cloned().collect();
    if cli.tui {
        let edits = run_tui(
            &report.all_pairs_sorted,
            &report.pair_sources,
            effective_threshold,
            effective_max_results,
            cli.table_only_high,
            &deleted_pair_keys,
            &flagged_pair_keys,
        )?;
        deleted_pair_keys = edits.deleted_pair_keys;
        flagged_pair_keys = edits.flagged_pair_keys;
    }

    let active_pairs = apply_deleted_pairs(&report.all_pairs_sorted, &deleted_pair_keys);
    if !cli.tui {
        print_table(
            &active_pairs,
            effective_threshold,
            effective_max_results,
            cli.table_only_high,
        );
    }
    let hidden_count = deleted_pair_keys.len();
    if cli.tui && hidden_count > 0 {
        report.warnings.push(format!(
            "hidden {hidden_count} pair(s) from TUI session before saving report"
        ));
    }
    report.deleted_pair_keys = deleted_pair_keys.into_iter().collect();
    report.deleted_pair_keys.sort();
    report.flagged_pair_keys = flagged_pair_keys.into_iter().collect();
    report.flagged_pair_keys.sort();
    let deleted_pair_set: HashSet<String> = report.deleted_pair_keys.iter().cloned().collect();
    report.pair_count = active_pairs.len();
    report.high_similarity_pairs = report
        .all_pairs_sorted
        .iter()
        .filter(|pair| !deleted_pair_set.contains(&pair_key(pair)))
        .filter(|pair| pair.score >= effective_threshold)
        .cloned()
        .collect();
    report.high_similarity_count = report.high_similarity_pairs.len();
    report.config.threshold = effective_threshold;
    report.config.max_results = effective_max_results;
    report.config.table_only_high = cli.table_only_high;
    report.config.tui = cli.tui;
    print_warnings(&report.warnings);

    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("failed to serialize report JSON: {e}"))?;
    fs::write(&cli.json_output, json).map_err(|e| {
        format!(
            "failed to write JSON report to {}: {e}",
            cli.json_output.display()
        )
    })?;
    println!("JSON report written to {}", cli.json_output.display());

    Ok(())
}

fn validate_args(cli: &Cli) -> Result<(), String> {
    if !(0.0..=1.0).contains(&cli.threshold) {
        return Err(format!(
            "threshold must be between 0.0 and 1.0, got {}",
            cli.threshold
        ));
    }
    if cli.max_results == 0 {
        return Err("max-results must be at least 1".to_owned());
    }
    if let Some(solution_dir) = &cli.solution_dir
        && !solution_dir.is_dir()
    {
        return Err(format!(
            "solution directory does not exist or is not a directory: {}",
            solution_dir.display()
        ));
    }
    if let Some(solution_threshold) = cli.solution_threshold
        && !(0.0..=1.0).contains(&solution_threshold)
    {
        return Err(format!(
            "solution-threshold must be between 0.0 and 1.0, got {}",
            solution_threshold
        ));
    }
    if cli.solution_threshold.is_some() && cli.solution_dir.is_none() {
        return Err("solution-threshold requires --solution-dir".to_owned());
    }
    if let Some(root_dir) = &cli.root_dir
        && !root_dir.is_dir()
    {
        return Err(format!(
            "root directory does not exist or is not a directory: {}",
            root_dir.display()
        ));
    }
    if let Some(load_report) = &cli.load_report
        && !load_report.is_file()
    {
        return Err(format!(
            "load-report file does not exist: {}",
            load_report.display()
        ));
    }
    Ok(())
}

fn query_spec_from_cli(cli: &Cli) -> Result<QuerySpec, String> {
    let root_dir = cli
        .root_dir
        .clone()
        .ok_or_else(|| "--root-dir is required unless --load-report is used".to_owned())?;
    let assignment = cli
        .assignment
        .clone()
        .ok_or_else(|| "--assignment is required unless --load-report is used".to_owned())?;
    let question = cli
        .question
        .clone()
        .ok_or_else(|| "--question is required unless --load-report is used".to_owned())?;
    let cell_id = cli
        .cell_id
        .clone()
        .ok_or_else(|| "--cell-id is required unless --load-report is used".to_owned())?;
    let language = cli
        .language
        .ok_or_else(|| "--language is required unless --load-report is used".to_owned())?;
    Ok(QuerySpec {
        root_dir,
        assignment,
        question,
        cell_id,
        language,
    })
}

fn load_report(path: &Path) -> Result<Report, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("failed reading report {}: {e}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|e| format!("failed parsing report JSON {}: {e}", path.display()))
}

fn collect_submissions(
    query: &QuerySpec,
    lowercase: bool,
    warnings: &mut Vec<String>,
) -> Result<Vec<Submission>, String> {
    let mut submissions = Vec::new();

    let student_dirs = list_immediate_subdirs(&query.root_dir)
        .map_err(|e| format!("failed to list students: {e}"))?;

    for student_dir in student_dirs {
        let student_name = student_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                format!(
                    "invalid UTF-8 student directory name: {}",
                    student_dir.display()
                )
            })?
            .to_owned();

        let assignment_dir = student_dir.join(&query.assignment);
        if !assignment_dir.is_dir() {
            warnings.push(format!(
                "student '{student_name}': assignment directory missing: {}",
                assignment_dir.display()
            ));
            continue;
        }

        let matching_notebooks = find_matching_notebooks(&assignment_dir, &query.question)
            .map_err(|e| {
                format!(
                    "failed to search notebooks in {}: {e}",
                    assignment_dir.display()
                )
            })?;

        if matching_notebooks.is_empty() {
            warnings.push(format!(
                "student '{student_name}': no notebook filename contains '{}'",
                query.question
            ));
            continue;
        }

        let notebook_path = &matching_notebooks[0];
        if matching_notebooks.len() > 1 {
            warnings.push(format!(
                "student '{student_name}': multiple notebooks matched question '{}'; using {}",
                query.question,
                notebook_path.display()
            ));
        }

        let source =
            extract_cell_source_by_grade_id(notebook_path, &query.cell_id).map_err(|e| {
                format!(
                    "student '{student_name}': failed to extract grade_id '{}' from {}: {e}",
                    query.cell_id,
                    notebook_path.display()
                )
            })?;

        let normalized = normalize_code(&source, query.language, lowercase);
        submissions.push(Submission {
            student: student_name,
            notebook_path: notebook_path.clone(),
            source,
            normalized,
        });
    }

    Ok(submissions)
}

fn collect_solution_submission(
    cli: &Cli,
    query: &QuerySpec,
    warnings: &mut Vec<String>,
) -> Result<Option<SolutionSubmission>, String> {
    let Some(solution_dir) = &cli.solution_dir else {
        return Ok(None);
    };
    let assignment_dir = solution_dir.join(&query.assignment);
    if !assignment_dir.is_dir() {
        return Err(format!(
            "solution assignment directory missing: {}",
            assignment_dir.display()
        ));
    }
    let matching_notebooks =
        find_matching_notebooks(&assignment_dir, &query.question).map_err(|e| {
            format!(
                "failed to search solution notebooks in {}: {e}",
                assignment_dir.display()
            )
        })?;
    if matching_notebooks.is_empty() {
        return Err(format!(
            "solution directory has no notebook filename containing '{}'",
            query.question
        ));
    }
    let notebook_path = &matching_notebooks[0];
    if matching_notebooks.len() > 1 {
        warnings.push(format!(
            "solution directory: multiple notebooks matched question '{}'; using {}",
            query.question,
            notebook_path.display()
        ));
    }
    let source = extract_cell_source_by_grade_id(notebook_path, &query.cell_id).map_err(|e| {
        format!(
            "solution directory: failed to extract grade_id '{}' from {}: {e}",
            query.cell_id,
            notebook_path.display()
        )
    })?;
    let normalized = normalize_code(&source, query.language, cli.lowercase);
    Ok(Some(SolutionSubmission {
        notebook_path: notebook_path.clone(),
        normalized,
    }))
}

fn list_immediate_subdirs(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn find_matching_notebooks(
    assignment_dir: &Path,
    question: &str,
) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut notebooks = Vec::new();
    for entry in fs::read_dir(assignment_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if filename.ends_with(".ipynb") && filename.contains(question) {
            notebooks.push(path);
        }
    }
    notebooks.sort();
    Ok(notebooks)
}

fn extract_cell_source_by_grade_id(notebook_path: &Path, cell_id: &str) -> Result<String, String> {
    let text = fs::read_to_string(notebook_path)
        .map_err(|e| format!("failed reading notebook {}: {e}", notebook_path.display()))?;
    let json: Value =
        serde_json::from_str(&text).map_err(|e| format!("invalid notebook JSON: {e}"))?;
    let cells = json
        .get("cells")
        .and_then(Value::as_array)
        .ok_or_else(|| "notebook missing 'cells' array".to_owned())?;

    for cell in cells {
        let grade_id_matches = cell
            .get("metadata")
            .and_then(|m| m.get("nbgrader"))
            .and_then(|n| n.get("grade_id"))
            .and_then(Value::as_str)
            .is_some_and(|grade_id| grade_id == cell_id);

        if !grade_id_matches {
            continue;
        }

        return cell_source_to_string(cell.get("source"));
    }

    Err(format!("no cell found with nbgrader.grade_id='{cell_id}'"))
}

fn cell_source_to_string(source: Option<&Value>) -> Result<String, String> {
    let value = source.ok_or_else(|| "cell is missing source field".to_owned())?;
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Array(lines) => {
            let mut out = String::new();
            for line in lines {
                let text = line
                    .as_str()
                    .ok_or_else(|| "source array contained a non-string entry".to_owned())?;
                out.push_str(text);
            }
            Ok(out)
        }
        _ => Err("cell source has unsupported JSON type".to_owned()),
    }
}

fn compute_pair_scores(submissions: &[Submission], target_student: Option<&str>) -> Vec<PairScore> {
    let mut pairs = Vec::new();

    for i in 0..submissions.len() {
        for j in (i + 1)..submissions.len() {
            let a = &submissions[i];
            let b = &submissions[j];
            if let Some(target) = target_student
                && a.student != target
                && b.student != target
            {
                continue;
            }

            let score = cosine_similarity_3gram(&a.normalized, &b.normalized);
            pairs.push(PairScore {
                student_a: a.student.clone(),
                student_b: b.student.clone(),
                score,
                notebook_a: a.notebook_path.display().to_string(),
                notebook_b: b.notebook_path.display().to_string(),
            });
        }
    }

    pairs.sort_by(sort_scores_desc);
    pairs
}

fn submission_similarity_to_solution(
    submissions: &[Submission],
    normalized_solution: &str,
) -> HashMap<String, f64> {
    submissions
        .iter()
        .map(|submission| {
            (
                submission.notebook_path.display().to_string(),
                cosine_similarity_3gram(&submission.normalized, normalized_solution),
            )
        })
        .collect()
}

fn filter_pairs_by_solution_similarity(
    all_pairs: Vec<PairScore>,
    similarity_to_solution: &HashMap<String, f64>,
    high_pair_threshold: f64,
    solution_threshold: f64,
) -> (Vec<PairScore>, usize) {
    let mut excluded = 0usize;
    let kept: Vec<PairScore> = all_pairs
        .into_iter()
        .filter(|pair| {
            let pair_is_high = pair.score >= high_pair_threshold;
            if !pair_is_high {
                return true;
            }
            let a_high = similarity_to_solution
                .get(&pair.notebook_a)
                .is_some_and(|score| *score >= solution_threshold);
            let b_high = similarity_to_solution
                .get(&pair.notebook_b)
                .is_some_and(|score| *score >= solution_threshold);
            let should_exclude = a_high || b_high;
            if should_exclude {
                excluded += 1;
            }
            !should_exclude
        })
        .collect();
    (kept, excluded)
}

fn sort_scores_desc(a: &PairScore, b: &PairScore) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.student_a.cmp(&b.student_a))
        .then_with(|| a.student_b.cmp(&b.student_b))
}

fn cosine_similarity_3gram(left: &str, right: &str) -> f64 {
    if left.is_empty() || right.is_empty() {
        return if left == right { 1.0 } else { 0.0 };
    }

    let left_grams = grams3(left);
    let right_grams = grams3(right);
    if left_grams.is_empty() || right_grams.is_empty() {
        return if left == right { 1.0 } else { 0.0 };
    }

    let mut dot = 0.0;
    for (gram, l_count) in &left_grams {
        if let Some(r_count) = right_grams.get(gram) {
            dot += (*l_count as f64) * (*r_count as f64);
        }
    }
    if dot == 0.0 {
        return 0.0;
    }

    let left_mag = (left_grams
        .values()
        .map(|c| (*c as f64).powi(2))
        .sum::<f64>())
    .sqrt();
    let right_mag = (right_grams
        .values()
        .map(|c| (*c as f64).powi(2))
        .sum::<f64>())
    .sqrt();
    if left_mag == 0.0 || right_mag == 0.0 {
        return 0.0;
    }

    dot / (left_mag * right_mag)
}

fn grams3(input: &str) -> HashMap<[u8; 3], usize> {
    let bytes = input.as_bytes();
    if bytes.len() < 3 {
        return HashMap::new();
    }
    let mut map = HashMap::new();
    for win in bytes.windows(3) {
        let gram = [win[0], win[1], win[2]];
        *map.entry(gram).or_insert(0) += 1;
    }
    map
}

fn normalize_code(source: &str, language: Language, lowercase: bool) -> String {
    let no_comments = match language {
        Language::C | Language::Java | Language::Go => strip_c_style_comments(source),
        Language::Python => strip_python_comments_and_docstrings(source),
    };

    let mut out: String = no_comments
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    if lowercase {
        out.make_ascii_lowercase();
    }
    out
}

fn strip_c_style_comments(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut in_string = false;
    let mut in_char = false;
    let mut in_raw_go_string = false;

    while i < bytes.len() {
        let c = bytes[i] as char;

        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                out.push(c);
            }
            i += 1;
            continue;
        }

        if in_block_comment {
            if c == '*' && i + 1 < bytes.len() && bytes[i + 1] as char == '/' {
                in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if in_char {
            out.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == '\'' {
                in_char = false;
            }
            i += 1;
            continue;
        }

        if in_raw_go_string {
            out.push(c);
            if c == '`' {
                in_raw_go_string = false;
            }
            i += 1;
            continue;
        }

        if c == '/' && i + 1 < bytes.len() {
            let next = bytes[i + 1] as char;
            if next == '/' {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if next == '*' {
                in_block_comment = true;
                i += 2;
                continue;
            }
        }

        if c == '"' {
            in_string = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '\'' {
            in_char = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '`' {
            in_raw_go_string = true;
            out.push(c);
            i += 1;
            continue;
        }

        out.push(c);
        i += 1;
    }

    out
}

fn strip_python_comments_and_docstrings(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    let mut at_line_start = true;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_triple_single = false;
    let mut in_triple_double = false;
    let mut can_start_docstring = true;

    while i < bytes.len() {
        let c = bytes[i] as char;

        if in_triple_single {
            if i + 2 < bytes.len()
                && bytes[i] == b'\''
                && bytes[i + 1] == b'\''
                && bytes[i + 2] == b'\''
            {
                in_triple_single = false;
                i += 3;
            } else {
                i += 1;
            }
            continue;
        }
        if in_triple_double {
            if i + 2 < bytes.len()
                && bytes[i] == b'"'
                && bytes[i + 1] == b'"'
                && bytes[i + 2] == b'"'
            {
                in_triple_double = false;
                i += 3;
            } else {
                i += 1;
            }
            continue;
        }

        if in_single_quote {
            out.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == '\'' {
                in_single_quote = false;
            }
            if c == '\n' {
                at_line_start = true;
                can_start_docstring = true;
            } else if !c.is_whitespace() {
                at_line_start = false;
                can_start_docstring = false;
            }
            i += 1;
            continue;
        }

        if in_double_quote {
            out.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == '"' {
                in_double_quote = false;
            }
            if c == '\n' {
                at_line_start = true;
                can_start_docstring = true;
            } else if !c.is_whitespace() {
                at_line_start = false;
                can_start_docstring = false;
            }
            i += 1;
            continue;
        }

        if c == '#' {
            while i < bytes.len() && bytes[i] as char != '\n' {
                i += 1;
            }
            continue;
        }

        if i + 2 < bytes.len()
            && bytes[i] == b'\''
            && bytes[i + 1] == b'\''
            && bytes[i + 2] == b'\''
        {
            if can_start_docstring {
                in_triple_single = true;
                i += 3;
                continue;
            }
            out.push_str("'''");
            i += 3;
            in_single_quote = true;
            can_start_docstring = false;
            at_line_start = false;
            continue;
        }
        if i + 2 < bytes.len() && bytes[i] == b'"' && bytes[i + 1] == b'"' && bytes[i + 2] == b'"' {
            if can_start_docstring {
                in_triple_double = true;
                i += 3;
                continue;
            }
            out.push_str("\"\"\"");
            i += 3;
            in_double_quote = true;
            can_start_docstring = false;
            at_line_start = false;
            continue;
        }

        if c == '\'' {
            in_single_quote = true;
            out.push(c);
            i += 1;
            can_start_docstring = false;
            at_line_start = false;
            continue;
        }
        if c == '"' {
            in_double_quote = true;
            out.push(c);
            i += 1;
            can_start_docstring = false;
            at_line_start = false;
            continue;
        }

        out.push(c);
        if c == '\n' {
            at_line_start = true;
            can_start_docstring = true;
        } else if c.is_whitespace() {
            if at_line_start {
                can_start_docstring = true;
            }
        } else {
            at_line_start = false;
            can_start_docstring = false;
        }
        i += 1;
    }

    out
}

#[derive(Clone, Copy)]
enum TuiScreen {
    List,
    Compare { scroll: u16 },
    Help,
}

fn run_tui(
    all_pairs: &[PairScore],
    source_by_notebook: &HashMap<String, String>,
    threshold: f64,
    max_results: usize,
    start_high_only: bool,
    initial_hidden_pair_keys: &HashSet<String>,
    initial_flagged_pair_keys: &HashSet<String>,
) -> Result<TuiEdits, String> {
    enable_raw_mode().map_err(|e| format!("failed to enable raw mode: {e}"))?;
    execute!(stdout(), EnterAlternateScreen, Hide)
        .map_err(|e| format!("failed to switch to alternate screen: {e}"))?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)
        .map_err(|e| format!("failed to initialize terminal backend: {e}"))?;

    let result = run_tui_loop(
        &mut terminal,
        all_pairs,
        source_by_notebook,
        threshold,
        max_results,
        start_high_only,
        initial_hidden_pair_keys,
        initial_flagged_pair_keys,
    );

    disable_raw_mode().map_err(|e| format!("failed to disable raw mode: {e}"))?;
    execute!(terminal.backend_mut(), Show, LeaveAlternateScreen)
        .map_err(|e| format!("failed to restore terminal screen: {e}"))?;
    terminal
        .show_cursor()
        .map_err(|e| format!("failed to restore cursor: {e}"))?;

    result
}

fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    all_pairs: &[PairScore],
    source_by_notebook: &HashMap<String, String>,
    threshold: f64,
    max_results: usize,
    start_high_only: bool,
    initial_hidden_pair_keys: &HashSet<String>,
    initial_flagged_pair_keys: &HashSet<String>,
) -> Result<TuiEdits, String> {
    let mut selected = 0usize;
    let mut high_only = start_high_only;
    let mut show_deleted = false;
    let mut show_flagged_only = false;
    let mut screen = TuiScreen::List;
    let mut help_return_screen = TuiScreen::List;
    let mut list_state = ListState::default();
    let mut hidden_pair_keys: HashSet<String> = initial_hidden_pair_keys.clone();
    let mut flagged_pair_keys: HashSet<String> = initial_flagged_pair_keys.clone();
    let mut hidden_history: Vec<Vec<String>> = Vec::new();

    loop {
        let active_rows = tui_rows(
            all_pairs,
            threshold,
            max_results,
            high_only,
            show_flagged_only,
            &hidden_pair_keys,
            &flagged_pair_keys,
        );
        let deleted_rows = tui_deleted_rows(all_pairs, max_results, &hidden_pair_keys);
        let rows = if show_deleted {
            &deleted_rows
        } else {
            &active_rows
        };
        let flagged_submission_list = flagged_submissions(all_pairs, &flagged_pair_keys);
        if rows.is_empty() {
            selected = 0;
        } else if selected >= rows.len() {
            selected = rows.len() - 1;
        }

        list_state.select((!rows.is_empty()).then_some(selected));
        terminal
            .draw(|frame| {
                render_tui(
                    frame,
                    &rows,
                    source_by_notebook,
                    &list_state,
                    selected,
                    threshold,
                    max_results,
                    high_only,
                    show_deleted,
                    show_flagged_only,
                    deleted_rows.len(),
                    flagged_pair_keys.len(),
                    &flagged_submission_list,
                    &flagged_pair_keys,
                    screen,
                )
            })
            .map_err(|e| format!("failed to draw TUI frame: {e}"))?;

        if !event::poll(Duration::from_millis(200))
            .map_err(|e| format!("event poll failed: {e}"))?
        {
            continue;
        }
        let evt = event::read().map_err(|e| format!("event read failed: {e}"))?;
        let Event::Key(key) = evt else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match screen {
            TuiScreen::List => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('?') | KeyCode::F(1) => {
                    help_return_screen = screen;
                    screen = TuiScreen::Help;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    selected = selected.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let max_idx = rows.len().saturating_sub(1);
                    selected = (selected + 1).min(max_idx);
                }
                KeyCode::PageUp => {
                    selected = selected.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    let max_idx = rows.len().saturating_sub(1);
                    selected = (selected + 10).min(max_idx);
                }
                KeyCode::Char('g') | KeyCode::Home => {
                    selected = 0;
                }
                KeyCode::Char('G') | KeyCode::End => {
                    selected = rows.len().saturating_sub(1);
                }
                KeyCode::Char('h') | KeyCode::Char('f') => {
                    high_only = !high_only;
                    selected = 0;
                }
                KeyCode::Char('p') => {
                    show_flagged_only = !show_flagged_only;
                    show_deleted = false;
                    selected = 0;
                }
                KeyCode::Char('v') => {
                    show_deleted = !show_deleted;
                    selected = 0;
                }
                KeyCode::Char('u') => {
                    undo_last_hidden(&mut hidden_pair_keys, &mut hidden_history);
                }
                KeyCode::Enter => {
                    if !rows.is_empty() {
                        screen = TuiScreen::Compare { scroll: 0 };
                    }
                }
                KeyCode::Char('s') => {
                    if !rows.is_empty() && !show_deleted {
                        let key = pair_key(rows[selected]);
                        if !flagged_pair_keys.insert(key.clone()) {
                            flagged_pair_keys.remove(&key);
                        }
                    }
                }
                KeyCode::Char('d') | KeyCode::Delete | KeyCode::Backspace => {
                    if !rows.is_empty() && !show_deleted {
                        let key = pair_key(rows[selected]);
                        if hidden_pair_keys.insert(key.clone()) {
                            hidden_history.push(vec![key]);
                        }
                        let max_idx = rows.len().saturating_sub(2);
                        selected = selected.min(max_idx);
                    }
                }
                KeyCode::Char('A') => {
                    if !rows.is_empty() && !show_deleted {
                        let added = hide_pairs_for_student(
                            all_pairs,
                            &rows[selected].student_a,
                            &mut hidden_pair_keys,
                        );
                        if !added.is_empty() {
                            hidden_history.push(added);
                        }
                        selected = 0;
                    }
                }
                KeyCode::Char('B') => {
                    if !rows.is_empty() && !show_deleted {
                        let added = hide_pairs_for_student(
                            all_pairs,
                            &rows[selected].student_b,
                            &mut hidden_pair_keys,
                        );
                        if !added.is_empty() {
                            hidden_history.push(added);
                        }
                        selected = 0;
                    }
                }
                KeyCode::Char('r') => {
                    if !rows.is_empty() && show_deleted {
                        let key = pair_key(rows[selected]);
                        hidden_pair_keys.remove(&key);
                        for batch in &mut hidden_history {
                            batch.retain(|k| k != &key);
                        }
                        hidden_history.retain(|batch| !batch.is_empty());
                    }
                }
                _ => {}
            },
            TuiScreen::Compare { mut scroll } => match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('?') | KeyCode::F(1) => {
                    help_return_screen = screen;
                    screen = TuiScreen::Help;
                }
                KeyCode::Char('u') => {
                    undo_last_hidden(&mut hidden_pair_keys, &mut hidden_history);
                }
                KeyCode::Esc | KeyCode::Backspace => {
                    screen = TuiScreen::List;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    scroll = scroll.saturating_sub(1);
                    screen = TuiScreen::Compare { scroll };
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    scroll = scroll.saturating_add(1);
                    screen = TuiScreen::Compare { scroll };
                }
                KeyCode::PageUp => {
                    scroll = scroll.saturating_sub(10);
                    screen = TuiScreen::Compare { scroll };
                }
                KeyCode::PageDown => {
                    scroll = scroll.saturating_add(10);
                    screen = TuiScreen::Compare { scroll };
                }
                KeyCode::Home | KeyCode::Char('g') => {
                    screen = TuiScreen::Compare { scroll: 0 };
                }
                KeyCode::Char('s') => {
                    if !rows.is_empty() {
                        let key = pair_key(rows[selected]);
                        if !flagged_pair_keys.insert(key.clone()) {
                            flagged_pair_keys.remove(&key);
                        }
                    }
                }
                KeyCode::Char('d') | KeyCode::Delete => {
                    if !rows.is_empty() {
                        let key = pair_key(rows[selected]);
                        if hidden_pair_keys.insert(key.clone()) {
                            hidden_history.push(vec![key]);
                        }
                        screen = TuiScreen::List;
                    }
                }
                _ => {}
            },
            TuiScreen::Help => match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1) => {
                    screen = help_return_screen;
                }
                _ => {}
            },
        }
    }
    Ok(TuiEdits {
        deleted_pair_keys: hidden_pair_keys,
        flagged_pair_keys,
    })
}

fn tui_rows<'a>(
    all_pairs: &'a [PairScore],
    threshold: f64,
    max_results: usize,
    high_only: bool,
    flagged_only: bool,
    hidden_pair_keys: &HashSet<String>,
    flagged_pair_keys: &HashSet<String>,
) -> Vec<&'a PairScore> {
    let filtered = all_pairs
        .iter()
        .filter(|pair| !hidden_pair_keys.contains(&pair_key(pair)))
        .filter(|pair| !flagged_only || flagged_pair_keys.contains(&pair_key(pair)));
    if high_only {
        filtered
            .filter(|pair| pair.score >= threshold)
            .take(max_results)
            .collect()
    } else {
        filtered.take(max_results).collect()
    }
}

fn tui_deleted_rows<'a>(
    all_pairs: &'a [PairScore],
    max_results: usize,
    hidden_pair_keys: &HashSet<String>,
) -> Vec<&'a PairScore> {
    all_pairs
        .iter()
        .filter(|pair| hidden_pair_keys.contains(&pair_key(pair)))
        .take(max_results)
        .collect()
}

fn pair_key(pair: &PairScore) -> String {
    format!("{}\u{1f}|{}", pair.notebook_a, pair.notebook_b)
}

fn apply_deleted_pairs(
    all_pairs: &[PairScore],
    deleted_pair_keys: &HashSet<String>,
) -> Vec<PairScore> {
    all_pairs
        .iter()
        .filter(|pair| !deleted_pair_keys.contains(&pair_key(pair)))
        .cloned()
        .collect()
}

fn flagged_submissions(
    all_pairs: &[PairScore],
    flagged_pair_keys: &HashSet<String>,
) -> Vec<String> {
    let mut set = HashSet::new();
    for pair in all_pairs {
        if flagged_pair_keys.contains(&pair_key(pair)) {
            set.insert(pair.student_a.clone());
            set.insert(pair.student_b.clone());
        }
    }
    let mut out: Vec<String> = set.into_iter().collect();
    out.sort();
    out
}

fn hide_pairs_for_student(
    all_pairs: &[PairScore],
    student: &str,
    hidden_pair_keys: &mut HashSet<String>,
) -> Vec<String> {
    let mut added = Vec::new();
    for pair in all_pairs {
        if pair.student_a == student || pair.student_b == student {
            let key = pair_key(pair);
            if hidden_pair_keys.insert(key.clone()) {
                added.push(key);
            }
        }
    }
    added
}

fn undo_last_hidden(
    hidden_pair_keys: &mut HashSet<String>,
    hidden_history: &mut Vec<Vec<String>>,
) -> bool {
    if let Some(last_batch) = hidden_history.pop() {
        for key in last_batch {
            hidden_pair_keys.remove(&key);
        }
        true
    } else {
        false
    }
}

fn render_tui(
    frame: &mut Frame<'_>,
    rows: &[&PairScore],
    source_by_notebook: &HashMap<String, String>,
    list_state: &ListState,
    selected: usize,
    threshold: f64,
    max_results: usize,
    high_only: bool,
    show_deleted: bool,
    show_flagged_only: bool,
    deleted_count: usize,
    flagged_count: usize,
    flagged_submission_list: &[String],
    flagged_pair_keys: &HashSet<String>,
    screen: TuiScreen,
) {
    let root = frame.area();
    match screen {
        TuiScreen::List => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(12),
                    Constraint::Length(4),
                ])
                .split(root);

            let mode = if high_only {
                "High-only filter ON"
            } else {
                "Showing top scores"
            };
            let view_mode = if show_deleted {
                "Viewing deleted pairs"
            } else if show_flagged_only {
                "Viewing flagged pairs only"
            } else {
                "Viewing active pairs"
            };
            let header = Paragraph::new(Line::from(vec![
                Span::styled(
                    "Submission Similarity Viewer",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    "  |  threshold={threshold:.2}  max={max_results}  |  {mode}  |  {view_mode}  |  deleted={deleted_count} flagged={flagged_count}"
                )),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Overview"));
            frame.render_widget(header, chunks[0]);

            let body_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
                .split(chunks[1]);

            let list_items: Vec<ListItem<'_>> = rows
                .iter()
                .map(|pair| {
                    let flag = if pair.score >= threshold { "HIGH" } else { "" };
                    let suspicious = if flagged_pair_keys.contains(&pair_key(pair)) {
                        "SUS"
                    } else {
                        ""
                    };
                    ListItem::new(Line::from(vec![
                        Span::raw(format!(
                            "{:<14} {:<14} {:>7.4} ",
                            shorten(&pair.student_a, 16),
                            shorten(&pair.student_b, 16),
                            pair.score
                        )),
                        Span::styled(
                            flag,
                            if flag.is_empty() {
                                Style::default()
                            } else {
                                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                            },
                        ),
                        Span::raw(" "),
                        Span::styled(
                            suspicious,
                            if suspicious.is_empty() {
                                Style::default()
                            } else {
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD)
                            },
                        ),
                    ]))
                })
                .collect();

            let list = List::new(list_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(if show_deleted {
                            "Deleted Pairs (left)"
                        } else {
                            "Pairs (left)"
                        }),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ");
            let mut state = list_state.clone();
            frame.render_stateful_widget(list, body_chunks[0], &mut state);

            let preview_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(5), Constraint::Min(8)])
                .split(body_chunks[1]);
            if rows.is_empty() {
                let empty = Paragraph::new("No pairs available for the current filter.").block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Live Preview (right)"),
                );
                frame.render_widget(empty, body_chunks[1]);
            } else {
                let pair = rows[selected];
                let source_a = source_by_notebook
                    .get(&pair.notebook_a)
                    .map(String::as_str)
                    .unwrap_or("<source unavailable>");
                let source_b = source_by_notebook
                    .get(&pair.notebook_b)
                    .map(String::as_str)
                    .unwrap_or("<source unavailable>");

                let meta = Paragraph::new(vec![
                    Line::from(format!(
                        "Selected: {} ↔ {} (score {:.4})",
                        pair.student_a, pair.student_b, pair.score
                    )),
                    Line::from(format!("A: {}", pair.notebook_a)),
                    Line::from(format!("B: {}", pair.notebook_b)),
                    Line::from(format!(
                        "Suspicious: {}",
                        if flagged_pair_keys.contains(&pair_key(pair)) {
                            "yes"
                        } else {
                            "no"
                        }
                    )),
                    Line::from(format!(
                        "Flagged submissions: {}",
                        if flagged_submission_list.is_empty() {
                            "<none>".to_owned()
                        } else {
                            shorten(&flagged_submission_list.join(", "), 120)
                        }
                    )),
                ])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Live Preview (right)"),
                )
                .wrap(Wrap { trim: false });
                frame.render_widget(meta, preview_chunks[0]);

                let source_cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(preview_chunks[1]);
                let max_lines = source_cols[0].height.saturating_sub(2) as usize;
                let max_cols = source_cols[0].width.saturating_sub(2) as usize;

                let preview_a = Paragraph::new(preview_source(source_a, max_lines, max_cols))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("{} source", pair.student_a)),
                    )
                    .wrap(Wrap { trim: false });
                let preview_b = Paragraph::new(preview_source(source_b, max_lines, max_cols))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("{} source", pair.student_b)),
                    )
                    .wrap(Wrap { trim: false });
                frame.render_widget(preview_a, source_cols[0]);
                frame.render_widget(preview_b, source_cols[1]);
            }

            let footer = Paragraph::new(vec![
                Line::from(
                    "Move: ↑/↓ j/k | Jump: PgUp/PgDn Home/End | View: h/f high, p flagged, v deleted",
                ),
                Line::from(
                    "Actions: s suspicious, d/Delete hide, A/B hide by student, u undo, r restore (deleted), Enter compare, ? help, q quit",
                ),
            ])
            .block(Block::default().borders(Borders::ALL).title("Controls"))
            .wrap(Wrap { trim: false });
            frame.render_widget(footer, chunks[2]);
        }
        TuiScreen::Compare { scroll } => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Length(4),
                ])
                .split(root);

            if rows.is_empty() {
                let empty = Paragraph::new("No pair selected. Press Esc to go back.")
                    .block(Block::default().borders(Borders::ALL).title("Compare"));
                frame.render_widget(empty, chunks[1]);
            } else {
                let pair = rows[selected];
                let header = Paragraph::new(Line::from(vec![
                    Span::styled(
                        "Side-by-Side Source View",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!(
                        "  |  {} ↔ {}  |  score {:.4}  |  scroll {}",
                        pair.student_a, pair.student_b, pair.score, scroll
                    )),
                ]))
                .block(Block::default().borders(Borders::ALL).title("Compare"));
                frame.render_widget(header, chunks[0]);

                let columns = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(chunks[1]);

                let source_a = source_by_notebook
                    .get(&pair.notebook_a)
                    .map(String::as_str)
                    .unwrap_or("<source unavailable>");
                let source_b = source_by_notebook
                    .get(&pair.notebook_b)
                    .map(String::as_str)
                    .unwrap_or("<source unavailable>");

                let panel_a = Paragraph::new(source_a)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("{} | {}", pair.student_a, pair.notebook_a)),
                    )
                    .scroll((scroll, 0))
                    .wrap(Wrap { trim: false });
                let panel_b = Paragraph::new(source_b)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("{} | {}", pair.student_b, pair.notebook_b)),
                    )
                    .scroll((scroll, 0))
                    .wrap(Wrap { trim: false });

                frame.render_widget(panel_a, columns[0]);
                frame.render_widget(panel_b, columns[1]);
            }

            let footer = Paragraph::new(vec![
                Line::from(
                    "Scroll: ↑/↓ j/k | Fast: PgUp/PgDn | Top: Home/g | Actions: s suspicious, d/Delete hide+back, u undo",
                ),
                Line::from("Navigation: Esc/Backspace back to list | ? help | q quit"),
            ])
            .block(Block::default().borders(Borders::ALL).title("Controls"))
            .wrap(Wrap { trim: false });
            frame.render_widget(footer, chunks[2]);
        }
        TuiScreen::Help => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(12),
                    Constraint::Length(4),
                ])
                .split(root);

            let header = Paragraph::new(Line::from(vec![
                Span::styled(
                    "TUI Help / Controls",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  |  press Esc, ? or F1 to return"),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Help"));
            frame.render_widget(header, chunks[0]);

            let help_text = Paragraph::new(vec![
                Line::from("Global"),
                Line::from("  q: quit app"),
                Line::from("  ?: open/close this help"),
                Line::from(""),
                Line::from("List View"),
                Line::from("  ↑/↓ or j/k: move selection"),
                Line::from("  PgUp/PgDn: jump list"),
                Line::from("  Home/End: jump top/bottom"),
                Line::from("  Enter: open full side-by-side compare"),
                Line::from("  s: toggle suspicious flag on selected pair"),
                Line::from("  p: toggle flagged-only view"),
                Line::from("  v: toggle deleted-pairs view"),
                Line::from("  d/Delete/Backspace: hide selected pair"),
                Line::from("  A: hide all pairs involving selected student A"),
                Line::from("  B: hide all pairs involving selected student B"),
                Line::from("  u: undo last hide action"),
                Line::from("  r: restore selected pair (deleted view only)"),
                Line::from(""),
                Line::from("Compare View"),
                Line::from("  ↑/↓ or j/k: scroll sources"),
                Line::from("  PgUp/PgDn: fast scroll"),
                Line::from("  Home/g: jump top"),
                Line::from("  s: toggle suspicious flag"),
                Line::from("  d/Delete: hide pair and return"),
                Line::from("  Esc/Backspace: return to list"),
            ])
            .block(Block::default().borders(Borders::ALL).title("Reference"))
            .wrap(Wrap { trim: false });
            frame.render_widget(help_text, chunks[1]);

            let footer = Paragraph::new(Line::from(vec![
                Span::raw("Tip: save your decisions by setting "),
                Span::styled(
                    "--json-output",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" to your report path"),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Notes"))
            .wrap(Wrap { trim: false });
            frame.render_widget(footer, chunks[2]);
        }
    }
}

fn shorten(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if input.chars().count() <= max_chars {
        return input.to_owned();
    }
    let mut out = String::new();
    for ch in input.chars().take(max_chars.saturating_sub(3)) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn preview_source(source: &str, max_lines: usize, max_cols: usize) -> String {
    if max_lines == 0 || max_cols == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut count = 0usize;
    for (idx, line) in source.lines().enumerate() {
        if count >= max_lines {
            break;
        }
        let line_label = format!("{:>3}: ", idx + 1);
        let available = max_cols.saturating_sub(line_label.chars().count());
        out.push_str(&line_label);
        out.push_str(&shorten(line, available.max(1)));
        out.push('\n');
        count += 1;
    }
    let total_lines = source.lines().count();
    if total_lines > count && count > 0 {
        let marker = "...";
        let last_line_width = max_cols.min(marker.len());
        out.push_str(&marker[..last_line_width]);
    }
    if out.is_empty() {
        "<empty source>".to_owned()
    } else {
        out
    }
}

fn print_table(all_pairs: &[PairScore], threshold: f64, max_results: usize, table_only_high: bool) {
    let rows = tui_rows(
        all_pairs,
        threshold,
        max_results,
        table_only_high,
        false,
        &HashSet::new(),
        &HashSet::new(),
    );

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Student A", "Student B", "Score", "Flag"]);

    for pair in rows {
        let flag = if pair.score >= threshold { "HIGH" } else { "" };
        table.add_row(vec![
            pair.student_a.clone(),
            pair.student_b.clone(),
            format!("{:.4}", pair.score),
            flag.to_owned(),
        ]);
    }

    println!("{table}");
    println!(
        "Shown: {} pair(s). Threshold: {:.2}. High-similarity pairs in full set: {}",
        if table_only_high {
            all_pairs
                .iter()
                .filter(|pair| pair.score >= threshold)
                .take(max_results)
                .count()
        } else {
            all_pairs.iter().take(max_results).count()
        },
        threshold,
        all_pairs
            .iter()
            .filter(|pair| pair.score >= threshold)
            .count()
    );
}

fn print_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    eprintln!("Warnings ({}):", warnings.len());
    for warning in warnings {
        eprintln!("- {warning}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn strips_c_comments_and_keeps_strings() {
        let src = r#"
            int main() {
              // comment
              char* s = "/* not comment */";
              /* block */
              return 0;
            }
        "#;
        let out = strip_c_style_comments(src);
        assert!(!out.contains("// comment"));
        assert!(!out.contains("/* block */"));
        assert!(out.contains("/* not comment */"));
    }

    #[test]
    fn strips_python_comments_and_docstrings() {
        let src = r#"
def f():
    """this is a docstring"""
    x = 1  # inline
    return x
"#;
        let out = strip_python_comments_and_docstrings(src);
        assert!(!out.contains("docstring"));
        assert!(!out.contains("# inline"));
        assert!(out.contains("return x"));
    }

    #[test]
    fn normalization_removes_whitespace() {
        let src = "a = 1\n\n# comment\nb=2";
        let out = normalize_code(src, Language::Python, false);
        assert_eq!(out, "a=1b=2");
    }

    #[test]
    fn cosine_similarity_identical_is_one() {
        let a = "intmain(){return0;}";
        let b = "intmain(){return0;}";
        let score = cosine_similarity_3gram(a, b);
        assert!((score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn extracts_nbgrader_cell_source() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be valid")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("nb-{unique}.ipynb"));

        let notebook = r#"{
  "cells": [
    {"cell_type":"code","metadata":{"nbgrader":{"grade_id":"q1"}},"source":["x = 1\n"]},
    {"cell_type":"code","metadata":{"nbgrader":{"grade_id":"target-cell"}},"source":["print(42)\n","y=3\n"]}
  ]
}"#;
        fs::write(&path, notebook).expect("should write temporary notebook");

        let source =
            extract_cell_source_by_grade_id(&path, "target-cell").expect("should extract source");
        assert_eq!(source, "print(42)\ny=3\n");

        fs::remove_file(&path).expect("should clean temporary notebook");
    }

    #[test]
    fn tui_rows_respects_high_only_and_limit() {
        let pairs = vec![
            PairScore {
                student_a: "a".to_owned(),
                student_b: "b".to_owned(),
                score: 0.95,
                notebook_a: "a.ipynb".to_owned(),
                notebook_b: "b.ipynb".to_owned(),
            },
            PairScore {
                student_a: "a".to_owned(),
                student_b: "c".to_owned(),
                score: 0.5,
                notebook_a: "a.ipynb".to_owned(),
                notebook_b: "c.ipynb".to_owned(),
            },
        ];
        assert_eq!(
            tui_rows(
                &pairs,
                0.85,
                10,
                true,
                false,
                &HashSet::new(),
                &HashSet::new()
            )
            .len(),
            1
        );
        assert_eq!(
            tui_rows(
                &pairs,
                0.85,
                1,
                false,
                false,
                &HashSet::new(),
                &HashSet::new()
            )
            .len(),
            1
        );
    }

    #[test]
    fn clip_shortens_long_text() {
        let clipped = shorten("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(clipped, "abcdefg...");
    }

    #[test]
    fn preview_source_limits_lines_and_columns() {
        let src = "first very very long line\nsecond line\nthird line";
        let preview = preview_source(src, 2, 12);
        assert!(preview.contains("1:"));
        assert!(preview.contains("2:"));
        assert!(preview.contains("..."));
        assert!(!preview.contains("third line"));
    }

    #[test]
    fn filters_high_pairs_that_match_solution() {
        let pairs = vec![
            PairScore {
                student_a: "alice".to_owned(),
                student_b: "bob".to_owned(),
                score: 0.95,
                notebook_a: "a.ipynb".to_owned(),
                notebook_b: "b.ipynb".to_owned(),
            },
            PairScore {
                student_a: "carla".to_owned(),
                student_b: "diego".to_owned(),
                score: 0.92,
                notebook_a: "c.ipynb".to_owned(),
                notebook_b: "d.ipynb".to_owned(),
            },
            PairScore {
                student_a: "erin".to_owned(),
                student_b: "frank".to_owned(),
                score: 0.40,
                notebook_a: "e.ipynb".to_owned(),
                notebook_b: "f.ipynb".to_owned(),
            },
        ];
        let similarity_to_solution = HashMap::from([
            ("a.ipynb".to_owned(), 0.90),
            ("b.ipynb".to_owned(), 0.20),
            ("c.ipynb".to_owned(), 0.30),
            ("d.ipynb".to_owned(), 0.35),
            ("e.ipynb".to_owned(), 0.99),
            ("f.ipynb".to_owned(), 0.01),
        ]);

        let (kept, excluded) =
            filter_pairs_by_solution_similarity(pairs, &similarity_to_solution, 0.85, 0.85);
        assert_eq!(excluded, 1);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].student_a, "carla");
        assert_eq!(kept[1].student_a, "erin");
    }

    #[test]
    fn undo_last_hidden_restores_last_entry() {
        let mut hidden = HashSet::from(["a".to_owned(), "b".to_owned()]);
        let mut history = vec![vec!["a".to_owned()], vec!["b".to_owned()]];
        let undone = undo_last_hidden(&mut hidden, &mut history);
        assert!(undone);
        assert!(!hidden.contains("b"));
        assert!(hidden.contains("a"));
        assert_eq!(history, vec![vec!["a".to_owned()]]);
    }

    #[test]
    fn hide_pairs_for_student_hides_all_matching_pairs() {
        let pairs = vec![
            PairScore {
                student_a: "alice".to_owned(),
                student_b: "bob".to_owned(),
                score: 0.9,
                notebook_a: "a.ipynb".to_owned(),
                notebook_b: "b.ipynb".to_owned(),
            },
            PairScore {
                student_a: "alice".to_owned(),
                student_b: "carla".to_owned(),
                score: 0.8,
                notebook_a: "a.ipynb".to_owned(),
                notebook_b: "c.ipynb".to_owned(),
            },
            PairScore {
                student_a: "diego".to_owned(),
                student_b: "erin".to_owned(),
                score: 0.7,
                notebook_a: "d.ipynb".to_owned(),
                notebook_b: "e.ipynb".to_owned(),
            },
        ];
        let mut hidden = HashSet::new();
        let added = hide_pairs_for_student(&pairs, "alice", &mut hidden);
        assert_eq!(added.len(), 2);
        assert_eq!(hidden.len(), 2);
    }

    #[test]
    fn flagged_submissions_collects_unique_students() {
        let pairs = vec![
            PairScore {
                student_a: "alice".to_owned(),
                student_b: "bob".to_owned(),
                score: 0.9,
                notebook_a: "a.ipynb".to_owned(),
                notebook_b: "b.ipynb".to_owned(),
            },
            PairScore {
                student_a: "bob".to_owned(),
                student_b: "carla".to_owned(),
                score: 0.9,
                notebook_a: "b.ipynb".to_owned(),
                notebook_b: "c.ipynb".to_owned(),
            },
        ];
        let mut flagged = HashSet::new();
        flagged.insert(pair_key(&pairs[0]));
        flagged.insert(pair_key(&pairs[1]));
        let subs = flagged_submissions(&pairs, &flagged);
        assert_eq!(
            subs,
            vec!["alice".to_owned(), "bob".to_owned(), "carla".to_owned()]
        );
    }
}
