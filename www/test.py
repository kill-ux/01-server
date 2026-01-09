import os
import sys

# Checking the trailer passed from your Rust server
integrity_hash = os.environ.get('HTTP_HOST')

print("Content-Type: text/plain")
print("")
print("Hello World")
sys.stdout.flush()

while True
    pass