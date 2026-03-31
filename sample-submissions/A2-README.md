# Example: Assignment A2 - Shopping Cart Class

## Scenario

This example demonstrates a realistic academic assignment where students receive substantial boilerplate code and must implement only a small portion.

**Assignment Setup:**
- Students receive a `ShoppingCart` class with 4 pre-written methods
- Students must implement only 2 methods: `calculate_subtotal()` and `calculate_total()`
- All submissions include identical test code (provided by instructor)
- Result: ~85% of the code is identical boilerplate across all submissions

## Student Submissions

### Alice's Implementation (Reference Solution)
```python
def calculate_subtotal(self):
    total = 0.0
    for item in self.items.values():
        total += item['price'] * item['quantity']
    return total

def calculate_total(self):
    subtotal = self.calculate_subtotal()
    return subtotal * (1 + self.tax_rate)
```

### Bob's Implementation (Copied from Alice)
```python
def calculate_subtotal(self):
    # Bob copied from Alice with minor style changes
    total=0.0
    for item in self.items.values():
        total+=item['price']*item['quantity']
    return total

def calculate_total(self):
    subtotal=self.calculate_subtotal()
    return subtotal*(1+self.tax_rate)
```

### Carla's Implementation (Original - Generator)
```python
def calculate_subtotal(self):
    # Carla's different approach using sum with generator
    return sum(item['price'] * item['quantity'] for item in self.items.values())

def calculate_total(self):
    return self.calculate_subtotal() * (1 + self.tax_rate)
```

### Diego's Implementation (Original - List Comprehension)
```python
def calculate_subtotal(self):
    # Diego's compact approach with list comprehension
    return sum([self.items[name]['price'] * self.items[name]['quantity'] for name in self.items])

def calculate_total(self):
    st = self.calculate_subtotal()
    return st + (st * self.tax_rate)
```

## Results Comparison

### WITHOUT Boilerplate Detection

```
┌───────────┬───────────┬────────┬──────┐
│ Student A │ Student B │ Score  │ Flag │
├───────────┼───────────┼────────┼──────┤
│ alice     │ bob       │ 1.0000 │ HIGH │ ← CORRECT: Bob copied
│ carla     │ diego     │ 0.9785 │ HIGH │ ← FALSE POSITIVE!
│ alice     │ carla     │ 0.9743 │ HIGH │ ← FALSE POSITIVE!
│ bob       │ carla     │ 0.9743 │ HIGH │ ← FALSE POSITIVE!
│ alice     │ diego     │ 0.9582 │ HIGH │ ← FALSE POSITIVE!
│ bob       │ diego     │ 0.9582 │ HIGH │ ← FALSE POSITIVE!
└───────────┴───────────┴────────┴──────┘

High-similarity pairs: 6/6 (100% flagged!)
```

**Problem:** All pairs show 95%+ similarity because they share:
- Identical class structure (4 pre-written methods)
- Identical test code
- Similar method signatures for the 2 implemented methods

Even completely different implementations (Carla vs Diego) show 97.85% similarity!

### WITH Boilerplate Detection (90% threshold)

```
┌───────────┬───────────┬────────┬──────┐
│ Student A │ Student B │ Score  │ Flag │
├───────────┼───────────┼────────┼──────┤
│ alice     │ bob       │ 1.0000 │ HIGH │ ← CORRECT: Bob copied
│ alice     │ carla     │ 0.2209 │      │ ← Fixed!
│ bob       │ carla     │ 0.2209 │      │ ← Fixed!
│ alice     │ diego     │ 0.0319 │      │ ← Fixed!
│ bob       │ diego     │ 0.0319 │      │ ← Fixed!
│ carla     │ diego     │ 0.0000 │      │ ← Fixed!
└───────────┴───────────┴────────┴──────┘

High-similarity pairs: 1/6 (only the actual copy!)
Boilerplate 3-grams removed: 229
```

**Results:**
- Alice vs Bob: Still 100% similar (correctly identified as copying)
- Carla vs Diego: 97.85% → 0.00% (now correctly identified as different)
- All other pairs: Dramatically reduced to reflect actual code differences

## Running the Analysis

```bash
# Without boilerplate detection - lots of false positives
./submission-similarity \
  --root-dir sample-submissions \
  --assignment A2 \
  --question shopping-cart \
  --cell-id shopping_cart \
  --language python \
  --threshold 0.85

# With auto-detection - only true positives
./submission-similarity \
  --root-dir sample-submissions \
  --assignment A2 \
  --question shopping-cart \
  --cell-id shopping_cart \
  --language python \
  --threshold 0.85 \
  --boilerplate-auto-threshold 0.9
```

## Key Takeaway

In assignments with significant boilerplate:
- **Without boilerplate detection:** Nearly everything flags as similar
- **With boilerplate detection:** Only actual copying is detected

The 229 boilerplate 3-grams removed represent:
- The entire class structure (`__init__`, `add_item`, `remove_item`, `get_item_count`)
- All the test code
- Shared docstrings and method signatures
- Common patterns like `self.items`, `self.tax_rate`, etc.

This allows the tool to focus on what students actually wrote, not what was provided to them.
