# LSP Test 3: Diagnostics (Automatic)
# ====================================
#
# Instructions:
# 1. Diagnostics appear automatically as you edit
# 2. Look for underlined/highlighted code indicating errors or warnings
# 3. Errors and warnings should appear in the editor
# 4. The status bar may show diagnostic counts
#
# This file contains intentional errors for testing:

# Error 1: Undefined variable
result = undefined_variable + 10

# Error 2: Type mismatch (if using type checker)
def add_numbers(a: int, b: int) -> int:
    return a + b

wrong_type = add_numbers("hello", "world")

# Error 3: Import error
from nonexistent_module import something

# Error 4: Syntax-like issues
def broken_function(x, y)  # Missing colon - uncomment to test
    return x + y

# Error 5: Unused variable (warning)
unused_var = 42

# Error 6: Undefined name in expression
value = some_undefined_function()

# Error 7: Wrong number of arguments
def takes_two(a, b):
    return a + b

takes_two(1)  # Missing argument
takes_two(1, 2, 3)  # Too many arguments

# Error 8: Attribute error
my_string = "hello"
my_string.nonexistent_method()

# Error 9: Invalid operation
result = "string" + 42

# Error 10: Redefinition (warning in some linters)
def duplicate():
    pass

def duplicate():  # Redefined function
    pass


# Working code for comparison - this should have no errors:
def working_function(name: str) -> str:
    """This function works correctly."""
    return f"Hello, {name}!"

greeting = working_function("World")
print(greeting)
