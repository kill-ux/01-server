import os
import sys

# 1. READ existing session data from Environment Variables
# Your Rust server should prefix these with 'SESSION_'
current_user = os.environ.get('SESSION_USERNAME', 'Guest')
visit_count = int(os.environ.get('SESSION_VISITS', '0'))


# 2. LOGIC: Increment the counter
new_count = visit_count + 1

# 3. OUTPUT HEADERS
print("Content-Type: text/html")

# Send updates back to Rust via your custom header
print(f"X-Session-Update: visits={new_count}")

# Let's say we want to set a 'last_visit' timestamp too
import time
print(f"X-Session-Update: last_active={int(time.time())}")

# 4. END HEADERS
print("")

print(f"<h1>Hello, {current_user}!</h1>")
print(f"<p>This is your visit number: <b>{new_count}</b></p>")
