# Boilerplate Detection

## Problem

When analyzing student submissions for similarity, it's common for all submissions to share significant amounts of identical boilerplate code. This can lead to false positives where unrelated submissions appear highly similar simply because they contain the same provided starter code.

For example, if 90% of the code in each submission is identical boilerplate:
- Two completely different solutions might show 90%+ similarity
- Actual cases of copying become harder to identify among the false positives

## Solution

The tool now supports two methods of boilerplate detection and removal:

### 1. Manual Boilerplate File

Provide a notebook file containing the boilerplate code that should be filtered out from all submissions.

**Usage:**
```bash
submission-similarity \
  --root-dir submissions \
  --assignment A1 \
  --question fibonacci \
  --cell-id cell42 \
  --language python \
  --boilerplate-file boilerplate.ipynb \
  --boilerplate-cell-id boilerplate
```

**When to use:**
- You have a template file that was distributed to all students
- You know exactly what code should be considered boilerplate
- You want precise control over what gets filtered

### 2. Automatic Boilerplate Detection

Automatically detect and remove code patterns (3-grams) that appear in a high percentage of submissions.

**Usage:**
```bash
submission-similarity \
  --root-dir submissions \
  --assignment A1 \
  --question fibonacci \
  --cell-id cell42 \
  --language python \
  --boilerplate-auto-threshold 0.9
```

**Parameters:**
- `--boilerplate-auto-threshold 0.9` - Remove 3-grams present in 90% or more of submissions
- `--boilerplate-auto-threshold 0.8` - Remove 3-grams present in 80% or more of submissions
- `--boilerplate-auto-threshold 0.0` - Disabled (default)

**When to use:**
- You don't have the original boilerplate file
- Students received similar but not identical starter code
- You want the tool to automatically identify common patterns

### 3. Combined Approach

You can use both methods together for maximum effect:

```bash
submission-similarity \
  --root-dir submissions \
  --assignment A1 \
  --question fibonacci \
  --cell-id cell42 \
  --language python \
  --boilerplate-file boilerplate.ipynb \
  --boilerplate-cell-id boilerplate \
  --boilerplate-auto-threshold 0.85
```

This will:
1. Remove all 3-grams from the specified boilerplate file
2. Additionally remove any 3-grams that appear in 85%+ of submissions
3. Report the total number of boilerplate 3-grams removed

## How It Works

The boilerplate detection works at the 3-gram level:
- Code is normalized (comments removed, whitespace removed)
- The normalized code is split into overlapping 3-character sequences (3-grams)
- Boilerplate 3-grams are identified via:
  - Manual: Extracted from the provided boilerplate file
  - Auto: Counted across all submissions; those appearing in threshold% or more are flagged
- All identified boilerplate 3-grams are removed from every submission
- Similarity is then computed on the remaining code

## Example Results

Given these submissions where all students received `def build_sequence(n):` as starter code:

**Without boilerplate detection:**
```
┌───────────┬───────────┬────────┬──────┐
│ Student A │ Student B │ Score  │ Flag │
├───────────┼───────────┼────────┼──────┤
│ alice     │ bob       │ 0.9793 │ HIGH │  ← Actually copied
│ carla     │ diego     │ 0.6887 │      │  ← Different solutions!
│ erin      │ frank     │ 0.6590 │      │  ← Different solutions!
└───────────┴───────────┴────────┴──────┘
```

**With auto boilerplate detection (90% threshold):**
```
┌───────────┬───────────┬────────┬──────┐
│ Student A │ Student B │ Score  │ Flag │
├───────────┼───────────┼────────┼──────┤
│ alice     │ bob       │ 0.9762 │ HIGH │  ← Still high (good!)
│ erin      │ frank     │ 0.5996 │      │  ← Reduced (good!)
│ carla     │ diego     │ 0.5904 │      │  ← Reduced (good!)
└───────────┴───────────┴────────┴──────┘

Warnings:
- auto-detected 11 boilerplate 3-gram(s) present in 90%+ of submissions
- removed 11 boilerplate 3-gram(s) from all submissions
```

## Report Output

The boilerplate configuration is saved in the JSON report:

```json
{
  "config": {
    "boilerplate_file": "/path/to/boilerplate.ipynb",
    "boilerplate_cell_id": "cell_starter",
    "boilerplate_auto_threshold": 0.9,
    "boilerplate_grams_removed": 23
  }
}
```

This allows you to:
- See exactly what boilerplate detection settings were used
- Reproduce the analysis with the same settings
- Know how many 3-grams were filtered out

## Best Practices

1. **Start with auto-detection**: Try `--boilerplate-auto-threshold 0.9` first to see what's detected
2. **Adjust threshold**: Lower to 0.85 or 0.8 if you have many shared patterns
3. **Use manual file for precision**: If you have the exact boilerplate, use `--boilerplate-file`
4. **Combine both methods**: For best results, use both manual and auto detection
5. **Check warnings**: Review the warnings to see how many grams were removed
6. **Compare results**: Run with and without boilerplate detection to validate

## Technical Details

- **3-gram based**: Works on character-level 3-grams, not tokens or lines
- **Language-aware**: Normalization respects language syntax (Python vs C-style comments)
- **Incremental removal**: Scans character-by-character, skipping characters that form boilerplate grams
- **Set-based deduplication**: Combined boilerplate from both methods is deduplicated
- **Preserved in report**: All settings and counts are saved in the JSON output
