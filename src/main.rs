use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};
use serde::Serialize;
use serde_json::Value;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "submission-similarity",
    version,
    about = "Finds high-similarity notebook submissions by nbgrader cell ID"
)]
struct Cli {
    #[arg(long)]
    root_dir: PathBuf,

    #[arg(long)]
    assignment: String,

    #[arg(long)]
    question: String,

    #[arg(long)]
    cell_id: String,

    #[arg(long, value_enum)]
    language: Language,

    #[arg(long, default_value_t = 0.85)]
    threshold: f64,

    #[arg(long, default_value_t = 100)]
    max_results: usize,

    #[arg(long, default_value = "similarity-report.json")]
    json_output: PathBuf,

    #[arg(long)]
    target_student: Option<String>,

    #[arg(long, default_value_t = false)]
    table_only_high: bool,

    #[arg(long, default_value_t = false)]
    lowercase: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum, Serialize)]
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
    normalized: String,
}

#[derive(Debug, Clone, Serialize)]
struct PairScore {
    student_a: String,
    student_b: String,
    score: f64,
    notebook_a: String,
    notebook_b: String,
}

#[derive(Debug, Serialize)]
struct ReportConfig {
    root_dir: String,
    assignment: String,
    question: String,
    cell_id: String,
    language: Language,
    threshold: f64,
    max_results: usize,
    target_student: Option<String>,
    table_only_high: bool,
    lowercase: bool,
}

#[derive(Debug, Serialize)]
struct Report {
    config: ReportConfig,
    submission_count: usize,
    pair_count: usize,
    high_similarity_count: usize,
    warnings: Vec<String>,
    high_similarity_pairs: Vec<PairScore>,
    all_pairs_sorted: Vec<PairScore>,
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

    let mut warnings = Vec::new();
    let submissions = collect_submissions(&cli, &mut warnings)?;
    if submissions.len() < 2 {
        return Err(format!(
            "found {} matching submission(s); need at least 2",
            submissions.len()
        ));
    }

    let all_pairs = compute_pair_scores(&submissions, cli.target_student.as_deref());
    let mut high_pairs: Vec<PairScore> = all_pairs
        .iter()
        .filter(|pair| pair.score >= cli.threshold)
        .cloned()
        .collect();
    high_pairs.sort_by(sort_scores_desc);

    print_table(
        &all_pairs,
        cli.threshold,
        cli.max_results,
        cli.table_only_high,
    );
    print_warnings(&warnings);

    let report = Report {
        config: ReportConfig {
            root_dir: cli.root_dir.display().to_string(),
            assignment: cli.assignment,
            question: cli.question,
            cell_id: cli.cell_id,
            language: cli.language,
            threshold: cli.threshold,
            max_results: cli.max_results,
            target_student: cli.target_student,
            table_only_high: cli.table_only_high,
            lowercase: cli.lowercase,
        },
        submission_count: submissions.len(),
        pair_count: all_pairs.len(),
        high_similarity_count: high_pairs.len(),
        warnings,
        high_similarity_pairs: high_pairs,
        all_pairs_sorted: all_pairs,
    };

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
    if !cli.root_dir.is_dir() {
        return Err(format!(
            "root directory does not exist or is not a directory: {}",
            cli.root_dir.display()
        ));
    }
    Ok(())
}

fn collect_submissions(cli: &Cli, warnings: &mut Vec<String>) -> Result<Vec<Submission>, String> {
    let mut submissions = Vec::new();

    let student_dirs = list_immediate_subdirs(&cli.root_dir)
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

        let assignment_dir = student_dir.join(&cli.assignment);
        if !assignment_dir.is_dir() {
            warnings.push(format!(
                "student '{student_name}': assignment directory missing: {}",
                assignment_dir.display()
            ));
            continue;
        }

        let matching_notebooks =
            find_matching_notebooks(&assignment_dir, &cli.question).map_err(|e| {
                format!(
                    "failed to search notebooks in {}: {e}",
                    assignment_dir.display()
                )
            })?;

        if matching_notebooks.is_empty() {
            warnings.push(format!(
                "student '{student_name}': no notebook filename contains '{}'",
                cli.question
            ));
            continue;
        }

        let notebook_path = &matching_notebooks[0];
        if matching_notebooks.len() > 1 {
            warnings.push(format!(
                "student '{student_name}': multiple notebooks matched question '{}'; using {}",
                cli.question,
                notebook_path.display()
            ));
        }

        let source = extract_cell_source_by_grade_id(notebook_path, &cli.cell_id).map_err(|e| {
            format!(
                "student '{student_name}': failed to extract grade_id '{}' from {}: {e}",
                cli.cell_id,
                notebook_path.display()
            )
        })?;

        let normalized = normalize_code(&source, cli.language, cli.lowercase);
        submissions.push(Submission {
            student: student_name,
            notebook_path: notebook_path.clone(),
            normalized,
        });
    }

    Ok(submissions)
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

fn print_table(all_pairs: &[PairScore], threshold: f64, max_results: usize, table_only_high: bool) {
    let rows: Vec<&PairScore> = if table_only_high {
        all_pairs
            .iter()
            .filter(|pair| pair.score >= threshold)
            .take(max_results)
            .collect()
    } else {
        all_pairs.iter().take(max_results).collect()
    };

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
}
