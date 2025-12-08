# Multi-Cursor Test: Ctrl+D Select Next Match
# =============================================
#
# Instructions:
# 1. Place cursor on any word (e.g., "value" below)
# 2. Press Ctrl+D to select that word
# 3. Press Ctrl+D again to select the next occurrence and add a cursor
# 4. Keep pressing Ctrl+D to select all occurrences
# 5. Now type to replace all selected occurrences at once!
# 6. Press Escape to collapse back to a single cursor
#
# Other multi-cursor shortcuts:
# - Ctrl+Alt+Up: Add cursor above
# - Ctrl+Alt+Down: Add cursor below
# - Ctrl+Click: Add/remove cursor at click position
# - Escape: Collapse to single cursor
#
# Test Area - Try selecting "value" with repeated Ctrl+D:

value = 10
another_value = value * 2
third_value = value + another_value
print(f"The value is {value}")

def process_value(value):
    """Process the value and return a new value."""
    return value * value

result = process_value(value)
final_value = result + value


# Test with "item" - has many occurrences:

items = ["apple", "banana", "cherry"]

for item in items:
    print(item)
    process_item(item)
    if item == "banana":
        special_item = item

def process_item(item):
    """Process a single item."""
    return item.upper()

first_item = items[0]
last_item = items[-1]


# Test with "count" - various contexts:

count = 0
max_count = 100

while count < max_count:
    count += 1
    if count % 10 == 0:
        print(f"count = {count}")

final_count = count
print(f"Final count: {final_count}")


# Test with "data" - in a class:

class DataProcessor:
    def __init__(self, data):
        self.data = data
        self.processed_data = None

    def process(self):
        self.processed_data = self.transform_data(self.data)
        return self.processed_data

    def transform_data(self, data):
        return [x * 2 for x in data]

    def get_data(self):
        return self.data

processor = DataProcessor([1, 2, 3])
data = processor.get_data()
new_data = processor.process()


# Test with "name" - strings and variables:

name = "Alice"
user_name = name
full_name = f"{name} Smith"

def greet_by_name(name):
    print(f"Hello, {name}!")
    return f"Greeted {name}"

greet_by_name(name)
greet_by_name("Bob")  # Different name

names = [name, "Bob", "Charlie"]
for n in names:
    print(f"Name: {n}")


# Test edge cases:

# Same word at start and end of line
test test test test test

# Word appears in comments too
# The word appears here and in code: word = "word"
word = "word"
print(word)  # prints the word


# Numbers and underscores in identifiers:
var_1 = 100
var_2 = var_1 + var_1
var_3 = var_1 * 2

my_var_1 = var_1
