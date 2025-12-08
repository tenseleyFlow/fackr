# LSP Test 4: Go to Definition (F12 or Ctrl+Click)
# =================================================
#
# Instructions:
# 1. Place cursor on a symbol (function, class, variable)
# 2. Press F12 to jump to its definition
# 3. This works for:
#    - Functions defined in this file
#    - Classes defined in this file
#    - Imported modules/functions
#    - Variables (jumps to assignment)
#
# Try going to definition on the marked symbols:


def helper_function(x: int) -> int:
    """A helper function defined at the top of the file."""
    return x * 2


class DataProcessor:
    """A class for processing data."""

    def __init__(self, data: list):
        self.data = data

    def process(self) -> list:
        """Process the data and return results."""
        return [self.transform(item) for item in self.data]

    def transform(self, item):
        """Transform a single item."""
        return item * 2


# Test 1: Go to function definition
# Place cursor on "helper_function" and press F12
result = helper_function(42)  # <- F12 on helper_function

# Test 2: Go to class definition
# Place cursor on "DataProcessor" and press F12
processor = DataProcessor([1, 2, 3])  # <- F12 on DataProcessor

# Test 3: Go to method definition
# Place cursor on "process" and press F12
output = processor.process()  # <- F12 on process

# Test 4: Go to variable definition
# Place cursor on "result" below and press F12
print(result)  # <- F12 on result (should go to line 31)

# Test 5: Go to imported function definition
# Place cursor on "path" and press F12
from os import path
exists = path.exists("/tmp")  # <- F12 on path or exists

# Test 6: Go to standard library
# Place cursor on "print" and press F12 (may open stdlib)
print("hello")  # <- F12 on print


# Nested definitions for testing
def outer_function():
    """Outer function containing nested definitions."""

    def inner_function():
        """Inner function."""
        return "inner"

    return inner_function()


# Test 7: Go to nested function
nested_result = outer_function()  # <- F12 on outer_function
