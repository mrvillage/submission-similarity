# Using Boilerplate Detection with Assignment A2

## Quick Start

The A2 assignment demonstrates the most common use case: students receive a class template and implement specific methods.

### Option 1: Auto-Detection (Recommended for most cases)

```bash
./submission-similarity \
  --root-dir sample-submissions \
  --assignment A2 \
  --question shopping-cart \
  --cell-id shopping_cart \
  --language python \
  --threshold 0.85 \
  --boilerplate-auto-threshold 0.9
```

**Results:**
- Detects 229 boilerplate 3-grams automatically
- Only 1 high-similarity pair (alice vs bob - actual copying)
- 5 false positives eliminated

### Option 2: Using the Boilerplate Template File

If you saved the original template you gave to students:

```bash
./submission-similarity \
  --root-dir sample-submissions \
  --assignment A2 \
  --question shopping-cart \
  --cell-id shopping_cart \
  --language python \
  --threshold 0.85 \
  --boilerplate-file sample-submissions/A2-boilerplate-template.ipynb \
  --boilerplate-cell-id shopping_cart_boilerplate
```

**Results:**
- Loads 227 boilerplate 3-grams from the template file
- Same outcome: only actual copying detected

### Option 3: Combined Approach (Most Thorough)

```bash
./submission-similarity \
  --root-dir sample-submissions \
  --assignment A2 \
  --question shopping-cart \
  --cell-id shopping_cart \
  --language python \
  --threshold 0.85 \
  --boilerplate-file sample-submissions/A2-boilerplate-template.ipynb \
  --boilerplate-cell-id shopping_cart_boilerplate \
  --boilerplate-auto-threshold 0.85
```

This combines both methods to catch:
- Everything in your template file
- Any additional common patterns students might have shared

## What Gets Filtered?

In this assignment, boilerplate detection removes:

1. **Pre-written methods** (4 methods provided to students):
   - `__init__()`
   - `add_item()`
   - `remove_item()`
   - `get_item_count()`

2. **Class structure**:
   - Class definition
   - Docstrings
   - Instance variables (`self.items`, `self.tax_rate`)

3. **Test code** (identical across all submissions):
   - Test function definition
   - All test assertions
   - Test data and expected values

4. **Method signatures** (for methods students implemented):
   - Function definitions
   - Parameter lists
   - Docstrings

## What Remains?

Only the actual implementation code students wrote:
- The logic inside `calculate_subtotal()`
- The logic inside `calculate_total()`
- Variable names they chose
- Control structures they used
- Expressions and calculations

## Impact on Results

### Without Boilerplate Detection
```
alice vs bob:   100.00% ← Actual copying
carla vs diego:  97.85% ← Different implementations! (FALSE POSITIVE)
alice vs carla:  97.43% ← Different implementations! (FALSE POSITIVE)
bob vs carla:    97.43% ← Different implementations! (FALSE POSITIVE)
alice vs diego:  95.82% ← Different implementations! (FALSE POSITIVE)
bob vs diego:    95.82% ← Different implementations! (FALSE POSITIVE)

HIGH flags: 6/6 = 100% of pairs
```

### With Boilerplate Detection (auto 90%)
```
alice vs bob:   100.00% ← Actual copying
alice vs carla:  22.09% ← Different (FIXED)
bob vs carla:    22.09% ← Different (FIXED)
alice vs diego:   3.19% ← Different (FIXED)
bob vs diego:     3.19% ← Different (FIXED)
carla vs diego:   0.00% ← Completely different (FIXED)

HIGH flags: 1/6 = 16.7% of pairs (only the actual copy)
```

## Choosing the Right Threshold

For auto-detection (`--boilerplate-auto-threshold`):

- **0.9 (90%)**: Conservative - only removes code in 90%+ of submissions
  - Use when: Students might have slightly different templates
  - Best for: Initial analysis

- **0.85 (85%)**: Moderate - removes code in 85%+ of submissions  
  - Use when: Most students have the same template
  - Best for: Typical classroom settings

- **0.75 (75%)**: Aggressive - removes code in 75%+ of submissions
  - Use when: You want to filter out common patterns
  - Best for: When students share code libraries or imports

## Verification

After running with boilerplate detection, check the warnings:

```
Warnings:
- auto-detected 229 boilerplate 3-gram(s) present in 90%+ of submissions
- removed 229 boilerplate 3-gram(s) from all submissions
```

Or check the JSON report:

```bash
jq '.config.boilerplate_grams_removed' similarity-report.json
```

A high number (200+) indicates significant boilerplate was present and removed.

## When to Use This Feature

Use boilerplate detection when:
- ✅ Students received template/starter code
- ✅ Assignments include provided test code
- ✅ Class structures or function signatures are identical
- ✅ You're getting too many high-similarity pairs
- ✅ Most pairs show 90%+ similarity

Don't use it when:
- ❌ Students wrote everything from scratch
- ❌ No common code was provided
- ❌ You want to check imports/boilerplate too

## Example: Real-World Workflow

1. Run without boilerplate detection first:
   ```bash
   ./submission-similarity --root-dir submissions --assignment A2 \
     --question shopping-cart --cell-id shopping_cart --language python
   ```
   
2. Notice too many high-similarity pairs (6 out of 6 = 100%)

3. Run with auto-detection:
   ```bash
   ./submission-similarity --root-dir submissions --assignment A2 \
     --question shopping-cart --cell-id shopping_cart --language python \
     --boilerplate-auto-threshold 0.9
   ```

4. Review results - now only 1 pair (the actual copy)

5. Investigate the flagged pair manually to confirm copying
