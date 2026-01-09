#!/usr/bin/env python3
import sys
import os

# 1. Read headers from environment (optional, for debugging)
content_length = int(os.environ.get('CONTENT_LENGTH', 0))

# 2. Read the body from stdin
# This is what your server's in_stream.write() feeds
body = sys.stdin.read(content_length)

# 3. Print standard CGI response
print("Content-Type: text/plain")
print("")  # Critical empty line
print(f"CGI received a POST request!")
print(f"Body Length: {len(body)}")
print(f"Body Content: {body}")