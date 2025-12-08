# LSP Test 6: Rename Symbol (F2)
# ===============================
#
# Instructions:
# 1. Place cursor on a symbol you want to rename
# 2. Press F2 to start rename
# 3. Type the new name in the prompt
# 4. Press Enter to apply the rename
# 5. All references should be updated automatically
#
# WARNING: This modifies the file! You may want to save a backup first.
# Use Ctrl+Z to undo if needed.


# Test 1: Rename a function
# Place cursor on "old_function_name" and press F2
# Try renaming it to "new_function_name"
def old_function_name(x):
    """A function with a name that should be renamed."""
    return x + 1


result1 = old_function_name(10)
result2 = old_function_name(20)
result3 = old_function_name(old_function_name(5))


# Test 2: Rename a class
# Place cursor on "OldClassName" and press F2
class OldClassName:
    """A class that should be renamed."""

    def __init__(self, value):
        self.value = value


obj1 = OldClassName(100)
obj2 = OldClassName(200)
instances = [OldClassName(i) for i in range(5)]


# Test 3: Rename a variable
# Place cursor on "counter" and press F2
counter = 0
counter += 1
counter += 1
print(counter)
if counter > 0:
    counter = counter * 2


# Test 4: Rename a method
# Place cursor on "old_method" and press F2
class MyClass:
    def old_method(self):
        """Method to be renamed."""
        return "old"

    def caller(self):
        return self.old_method()


instance = MyClass()
instance.old_method()
result = instance.old_method()


# Test 5: Rename a parameter
# Place cursor on "param" and press F2
def function_with_param(param):
    """Function with a parameter to rename."""
    if param is None:
        return 0
    return param * 2 + param


function_with_param(10)
function_with_param(param=20)


# Test 6: Rename a constant
# Place cursor on "MAX_VALUE" and press F2
MAX_VALUE = 100

if result1 < MAX_VALUE:
    print("Under limit")

threshold = MAX_VALUE // 2
