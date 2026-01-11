#!/usr/bin/env python3
import os
import secrets

# Simulate Session Creation
# Rust server should pass 'HTTP_COOKIE' via env
cookie = os.environ.get("HTTP_COOKIE", "")

print("Content-Type: text/html")
if "session_id" not in cookie:
    # We tell the user to refresh so Rust can set the cookie
    # or let Rust handle the Set-Cookie header logic you wrote
    print("") 
    print("<h1>No Session Found</h1>")
    print("<p>Refresh to let the Rust server generate a session.</p>")
else:
    print("")
    print(f"<h1>Welcome!</h1>")
    print(f"<p>Your Cookie: {cookie}</p>")
    print(f"<p>Path Info: {os.environ.get('PATH_INFO', 'N/A')}</p>")