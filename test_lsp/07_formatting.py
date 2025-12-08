# LSP Test 7: Code Formatting (Ctrl+Shift+F)
# ==========================================
#
# Instructions:
# 1. Press Ctrl+Shift+F to format the entire file
# 2. The LSP server (ruff or black via pylsp) will reformat the code
# 3. Observe the changes in whitespace, line breaks, etc.
#
# This file has intentionally poor formatting for testing:

x=1+2+3
y    =    4    *    5
z=x+y

def badly_formatted(   a,b,c   ):
    return a+b+c

result=badly_formatted(1,2,3)

my_list=[1,2,3,4,5,6,7,8,9,10]
my_dict={"key1":"value1","key2":"value2","key3":"value3"}

class BadlyFormatted:
    def __init__(self,x,y):
        self.x=x
        self.y=y
    def method(self):
        return self.x+self.y

if x>0:
    print("positive")
elif x<0:
    print("negative")
else:
    print("zero")

for i in range(10):
    if i%2==0:
        print(i)

data=[{"name":"alice","age":30},{"name":"bob","age":25},{"name":"charlie","age":35}]

long_string="this is a very long string that should probably be wrapped or reformatted in some way by the formatter"

# Inconsistent quotes
single = 'single quotes'
double = "double quotes"
mixed = 'mixed' + "quotes"

# Extra blank lines



# and missing blank lines
def func1():
    pass
def func2():
    pass
def func3():
    pass

# Trailing whitespace on next line (may not be visible):
x = 1

# Long function call
result = some_function_with_long_name(argument1, argument2, argument3, argument4, argument5, argument6)

# Lambda
f=lambda x:x*2

# Comprehension
squares=[x**2 for x in range(10)if x%2==0]
