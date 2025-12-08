# LSP Test 1: Hover Information (F1)
# ===================================
#
# Instructions:
# 1. Place cursor on any symbol below
# 2. Press F1 to see hover information
# 3. Press Escape to dismiss the hover popup
#
# Try hovering over:
# - Function names (greet, calculate_area)
# - Variable names (message, radius)
# - Built-in functions (print, len, range)
# - Type names (str, int, float, list)

def greet(name: str) -> str:
    """Return a greeting message for the given name."""
    message = f"Hello, {name}!"
    return message


def calculate_area(radius: float) -> float:
    """Calculate the area of a circle given its radius."""
    import math
    return math.pi * radius ** 2


class Person:
    """A simple Person class for testing hover."""

    def __init__(self, name: str, age: int):
        self.name = name
        self.age = age

    def introduce(self) -> str:
        """Return an introduction string."""
        return f"I'm {self.name}, {self.age} years old."


# Test area - place cursor on these and press F1:
result = greet("World")
area = calculate_area(5.0)
person = Person("Alice", 30)
intro = person.introduce()

numbers = [1, 2, 3, 4, 5]
length = len(numbers)

for i in range(10):
    print(i)
