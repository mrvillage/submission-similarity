# Sample Submissions for Testing

This directory contains sample student submissions demonstrating different scenarios for similarity detection and boilerplate filtering.

## Assignments

### A1: Fibonacci Sequence (Minimal Boilerplate)

**Scenario:** Students write a complete function from scratch to generate Fibonacci numbers.

**Submissions:**
- **alice**: Reference implementation using loop with tuple unpacking
- **bob**: Copied from Alice with minor formatting changes (HIGH similarity expected)
- **carla**: Different problem - squares using loop
- **diego**: Different problem - squares using list comprehension  
- **erin**: Different problem - factorial using loop
- **frank**: Different problem - factorial using recursion

**Boilerplate:** Minimal (function signature, print statement)
- Auto-detection (90%): Removes ~11 3-grams
- Impact: Small reduction in similarity scores

**Key pairs:**
- alice vs bob: 97.93% → 97.62% (still HIGH - correctly detected)
- carla vs diego: 68.87% → 59.04% (different solutions, reduced)

### A2: Shopping Cart Class (Heavy Boilerplate)

**Scenario:** Students receive a class with 4 pre-written methods and must implement only 2 methods. Includes identical test code.

**Submissions:**
- **alice**: Reference implementation using loop accumulator
- **bob**: Copied from Alice with minor formatting changes (HIGH similarity expected)
- **carla**: Original implementation using generator expression
- **diego**: Original implementation using list comprehension

**Boilerplate:** Extensive (~85% of code)
- Pre-written methods: `__init__`, `add_item`, `remove_item`, `get_item_count`
- Complete test function with assertions
- Class structure and docstrings

**Auto-detection (90%):** Removes ~229 3-grams
**Impact:** Dramatic reduction in false positives

**Key pairs:**
- alice vs bob: 100.00% → 100.00% (still HIGH - correctly detected)
- carla vs diego: 97.85% → 0.00% (different solutions, now correctly identified)
- alice vs carla: 97.43% → 22.09% (false positive eliminated)

## Running the Samples

### Without Boilerplate Detection
```bash
# A1 - Fibonacci
./submission-similarity \
  --root-dir sample-submissions \
  --assignment A1 \
  --question fibonacci \
  --cell-id cell42 \
  --language python \
  --threshold 0.85

# A2 - Shopping Cart
./submission-similarity \
  --root-dir sample-submissions \
  --assignment A2 \
  --question shopping-cart \
  --cell-id shopping_cart \
  --language python \
  --threshold 0.85
```

### With Auto-Detection
```bash
# A1 - Fibonacci (90% threshold)
./submission-similarity \
  --root-dir sample-submissions \
  --assignment A1 \
  --question fibonacci \
  --cell-id cell42 \
  --language python \
  --threshold 0.85 \
  --boilerplate-auto-threshold 0.9

# A2 - Shopping Cart (90% threshold)
./submission-similarity \
  --root-dir sample-submissions \
  --assignment A2 \
  --question shopping-cart \
  --cell-id shopping_cart \
  --language python \
  --threshold 0.85 \
  --boilerplate-auto-threshold 0.9
```

### With Boilerplate Template File
```bash
# A2 using the template file
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

## Expected Results

### A1 Without Boilerplate Detection
```
High-similarity pairs: 1 (alice vs bob)
False positives: None (minimal shared code)
```

### A1 With Boilerplate Detection  
```
High-similarity pairs: 1 (alice vs bob)
Boilerplate removed: 11 3-grams
Impact: Slight reduction in similarity scores
```

### A2 Without Boilerplate Detection
```
High-similarity pairs: 6 out of 6 pairs!
False positives: 5 (everything flags due to shared class structure)
This demonstrates the problem boilerplate detection solves.
```

### A2 With Boilerplate Detection
```
High-similarity pairs: 1 (alice vs bob)
Boilerplate removed: 229 3-grams
False positives eliminated: 5
Impact: Dramatic - only actual copying detected
```

## Files

### Assignment A1
- `sample-solution/A1/question-fibonacci.ipynb` - Reference solution
- `sample-submissions/{student}/A1/question-fibonacci.ipynb` - Student submissions

### Assignment A2
- `sample-solution/A2/question-shopping-cart.ipynb` - Reference solution
- `sample-submissions/{student}/A2/question-shopping-cart.ipynb` - Student submissions
- `sample-submissions/A2-boilerplate-template.ipynb` - Template file (starter code)
- `sample-submissions/A2-README.md` - Detailed A2 analysis
- `sample-submissions/A2-USAGE.md` - Usage guide for A2

## Documentation

- `BOILERPLATE.md` - Complete feature documentation
- `sample-submissions/A2-README.md` - A2 scenario analysis
- `sample-submissions/A2-USAGE.md` - A2 usage examples

## Use Cases

**A1 demonstrates:**
- Assignments where students write most code from scratch
- Minimal boilerplate (function signatures, print statements)
- When boilerplate detection has minimal but helpful impact

**A2 demonstrates:**
- Real-world academic assignments with template code
- Heavy boilerplate (class structure, pre-written methods, tests)
- When boilerplate detection is essential to avoid false positives
- The difference between auto-detection and template file methods

## Testing the Feature

Run both assignments with and without boilerplate detection to see:
1. How boilerplate inflates similarity scores
2. How auto-detection identifies common patterns
3. How template files provide precise filtering
4. How actual copying is still detected correctly
