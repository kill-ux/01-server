# /rust-server
├── /src
│   ├── main.rs           # Entry point
│   ├── server.rs         # Handles server lifecycle
│   ├── router.rs         # Routes requests
│   ├── cgi.rs            # Manages CGI execution
│   ├── config.rs         # Parses configuration file
│   ├── error.rs          # Error responses
│   ├── utils/
│       ├── session.rs    # Session management
│       ├── cookie.rs     # Cookie utilities
├── config.yaml           # Server config
├── README.md             # Project overview and setup
├── error_pages/          # Static error page files