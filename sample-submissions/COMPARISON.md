# Visual Comparison: A1 vs A2 Boilerplate Detection

## The Problem

When students share identical boilerplate/template code, similarity detection tools report false positives.

## Assignment A1: Fibonacci (Low Boilerplate)

**What students received:**
```python
# Minimal - just function signature hint in instructions
```

**What students wrote:**
```python
# ~100% of the code is student work
def build_sequence(n):
    a, b = 0, 1
    out = []
    for _ in range(n):
        out.append(a)
        a, b = b, a + b
    return out

print(build_sequence(10))
```

**Impact of boilerplate detection:** Small
- Only 11 3-grams removed (function signature, print pattern)
- Similarity scores reduced slightly but appropriately

## Assignment A2: Shopping Cart (High Boilerplate)

**What students received:**
```python
class ShoppingCart:
    """A shopping cart that manages items and calculates totals."""
    
    def __init__(self):
        """Initialize an empty shopping cart."""
        self.items = {}
        self.tax_rate = 0.08
    
    def add_item(self, name, price, quantity=1):
        """Add an item to the cart or update quantity if it exists."""
        if name in self.items:
            self.items[name]['quantity'] += quantity
        else:
            self.items[name] = {'price': price, 'quantity': quantity}
    
    def remove_item(self, name):
        """Remove an item from the cart."""
        if name in self.items:
            del self.items[name]
    
    def get_item_count(self):
        """Return the total number of items in the cart."""
        return sum(item['quantity'] for item in self.items.values())
    
    def calculate_subtotal(self):
        # TODO: Students implement this
        pass
    
    def calculate_total(self):
        # TODO: Students implement this
        pass

# Complete test function provided (40+ lines)
def test_shopping_cart():
    cart = ShoppingCart()
    cart.add_item("Apple", 1.50, 3)
    cart.add_item("Banana", 0.75, 2)
    # ... many more lines ...
```

**What students wrote:**
```python
# Only ~15% of the code is student work!
def calculate_subtotal(self):
    total = 0.0
    for item in self.items.values():
        total += item['price'] * item['quantity']
    return total

def calculate_total(self):
    subtotal = self.calculate_subtotal()
    return subtotal * (1 + self.tax_rate)
```

**Impact of boilerplate detection:** CRITICAL
- 229 3-grams removed (entire class structure + test code)
- False positives eliminated (6 pairs → 1 pair)
- Focus shifts to actual student work

## Side-by-Side Results

### Carla vs Diego (Different Implementations)

**Carla's code:**
```python
def calculate_subtotal(self):
    return sum(item['price'] * item['quantity'] 
               for item in self.items.values())
```

**Diego's code:**
```python
def calculate_subtotal(self):
    return sum([self.items[name]['price'] * self.items[name]['quantity'] 
                for name in self.items])
```

**Similarity scores:**
- **Without boilerplate:** 97.85% (FALSE POSITIVE!)
- **With boilerplate:** 0.00% (Correct!)

### Alice vs Bob (Actual Copying)

**Alice's code:**
```python
def calculate_subtotal(self):
    total = 0.0
    for item in self.items.values():
        total += item['price'] * item['quantity']
    return total
```

**Bob's code:**
```python
def calculate_subtotal(self):
    total=0.0  # Minor formatting
    for item in self.items.values():
        total+=item['price']*item['quantity']  # No spaces
    return total
```

**Similarity scores:**
- **Without boilerplate:** 100.00% (True positive)
- **With boilerplate:** 100.00% (Still detected - Good!)

## Key Insight

Boilerplate detection doesn't just reduce numbers—it transforms the tool from unusable to invaluable:

### A2 Without Boilerplate
```
ALL 6 PAIRS FLAGGED AS HIGH SIMILARITY
→ Instructor must manually review every pair
→ Too many false positives to be useful
→ Real copying hidden in the noise
```

### A2 With Boilerplate
```
ONLY 1 PAIR FLAGGED AS HIGH SIMILARITY
→ Immediate focus on the actual copy
→ No false positives to waste time on
→ Tool becomes actionable
```

## When to Use Boilerplate Detection

| Assignment Type | Boilerplate Level | Use Detection? | Expected Impact |
|----------------|-------------------|----------------|-----------------|
| Write function from scratch | Low (~10%) | Optional | Small reduction |
| Fill in 1-2 methods in class | High (~80%) | **REQUIRED** | Dramatic reduction |
| Complete small program | Medium (~30%) | Recommended | Moderate reduction |
| Implement interface/abstract class | High (~70%) | **REQUIRED** | Significant reduction |

## Recommendation

**Always try boilerplate detection** when:
1. You provided template/starter code
2. Assignment includes test code
3. You see >80% of pairs flagged as similar

The worst case is you remove a few common patterns. The best case is you eliminate all false positives and find the actual copying.
