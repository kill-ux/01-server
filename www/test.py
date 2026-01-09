# #!/usr/bin/env python3
# import sys
# import os

# # 1. Read headers from environment (optional, for debugging)
# content_length = int(os.environ.get('CONTENT_LENGTH', 0))

# # 2. Read the body from stdin
# # This is what your server's in_stream.write() feeds
# body = sys.stdin.read(content_length)

# # 3. Print standard CGI response
# print("Content-Type: text/plain")
# print("")  # Critical empty line
# print(f"CGI received a POST request!")
# print(f"Body Length: {len(body)}")
# print(f"Body Content: {body}")

#!/usr/bin/env python3
# import sys
# import os

# # Read Content-Length
# content_length = int(os.environ.get('CONTENT_LENGTH', 0))

# # Read the body as RAW BYTES
# # sys.stdin.buffer is the binary stream
# body_bytes = sys.stdin.buffer.read(content_length)

# print("Content-Type: text/plain")
# print("")
# print(f"CGI Success!")
# print(f"Received Bytes: {len(body_bytes)}")
# # We only print the first 10 bytes to avoid filling the terminal with junk
# print(f"First 10 bytes (hex): {body_bytes[:10].hex()}")

import sys

# Read in chunks to avoid Python OOM
total_received = 0
while True:
    chunk = sys.stdin.buffer.read(32768)
    if not chunk:
        break
    total_received += len(chunk)
    # Don't print too much, but flush if you do
    # print(f"Received {total_received}", file=sys.stderr, flush=True)

print("Status: 200 OK")
print("Content-Type: text/plain\r\n")
print(f"CGI Success! Received Bytes: {total_received}")