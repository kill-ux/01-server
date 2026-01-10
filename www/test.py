import sys

# 1. Define the data you want to save to the session
username = "Ruby_User"
theme = "dark"

# 2. Print the standard CGI headers
print("Content-Type: text/html")

# 3. Print your custom session update headers
# Format: Key=Value
print(f"X-Session-Update: username={username}")
print(f"X-Session-Update: theme={theme}")

# 4. End of headers
print("")

# 5. Body
print(f"<h1>Session Updated!</h1>")
print(f"<p>Set username to: {username}</p>")