import os
import sys

# Checking the trailer passed from your Rust server
integrity_hash = os.environ.get('HTTP_HOST')

print("Content-Type: text/plain")
print("")
print(f"I received a host header: {integrity_hash}")