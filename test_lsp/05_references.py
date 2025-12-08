# LSP Test 5: Find References (Shift+F12)
# ========================================
#
# Instructions:
# 1. Place cursor on a symbol
# 2. Press Shift+F12 to find all references
# 3. A list of locations should appear
# 4. Navigate the list to jump to each reference
#
# This file has symbols used multiple times for testing:


# A function that is called multiple times
def calculate(value: int) -> int:
    """Calculate something with the value."""
    return value * 2 + 1


# Test: Find all references to "calculate"
# Place cursor on any "calculate" and press Shift+F12
result1 = calculate(10)
result2 = calculate(20)
result3 = calculate(30)
combined = calculate(result1) + calculate(result2)


# A class used in multiple places
class Counter:
    """A simple counter class."""

    def __init__(self):
        self.count = 0

    def increment(self):
        self.count += 1

    def get_count(self):
        return self.count


# Test: Find all references to "Counter"
counter1 = Counter()
counter2 = Counter()
counters = [Counter(), Counter(), Counter()]

# Test: Find all references to "increment"
counter1.increment()
counter1.increment()
counter2.increment()
for c in counters:
    c.increment()


# A variable used throughout
MULTIPLIER = 10

# Test: Find all references to "MULTIPLIER"
value1 = 5 * MULTIPLIER
value2 = 3 * MULTIPLIER
value3 = MULTIPLIER * MULTIPLIER


def use_multiplier(x):
    return x * MULTIPLIER


# A parameter used multiple times in a function
def process_data(data):
    """Process data in multiple ways."""
    # Test: Find references to "data" parameter
    if data is None:
        return None
    length = len(data)
    first = data[0] if data else None
    last = data[-1] if data else None
    return {"data": data, "length": length, "first": first, "last": last}


# Call the function
process_data([1, 2, 3])
process_data(["a", "b", "c"])
