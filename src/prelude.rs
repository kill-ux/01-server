pub use crate::config::{AppConfig, RouteConfig, ServerConfig};
pub use crate::error::Result;
pub use crate::http::*;

pub use crate::*;
pub use mio::{
    Events, Interest, Poll, Token,
    event::Event,
    net::{TcpListener, TcpStream},
};
pub use proxy_log::{info, trace};
pub use std::collections::HashMap;
pub use std::fs::{self, File, OpenOptions};
pub use std::io::{ErrorKind, Read, Write};
pub use std::net::SocketAddr;
pub use std::os::unix::fs::MetadataExt;
pub use std::path::{Path, PathBuf};
pub use std::sync::Arc;
pub use std::time::Instant;
pub use std::time::Duration;

pub use std::{
    fmt::{self, Display},
    io,
    os::{
        fd::{FromRawFd, IntoRawFd},
        unix::net::UnixStream,
    },
    process::{Command, Stdio},
    str::FromStr,
    time::SystemTime,
};

pub use mio::*;

pub use crate::{
    cgi::CgiParsingState,
    http::HttpResponse,
    router::RoutingError,
    server::Server,
    upload::{Upload, UploadState},
};

pub use crate::http::{HttpRequest, PartInfo, find_subsequence, parse_part_headers};

pub const READ_BUF_SIZE: usize = 4096;
// 4xx Client Errors
pub const HTTP_BAD_REQUEST: u16 = 400;
pub const HTTP_FORBIDDEN: u16 = 403;
pub const HTTP_NOT_FOUND: u16 = 404;
pub const HTTP_METHOD_NOT_ALLOWED: u16 = 405;
pub const HTTP_PAYLOAD_TOO_LARGE: u16 = 413;
pub const HTTP_URI_TOO_LONG: u16 = 414;

// 5xx Server Errors
pub const HTTP_INTERNAL_SERVER_ERROR: u16 = 500;
pub const HTTP_NOT_IMPLEMENTED: u16 = 501;
pub const GATEWAY_TIMEOUT: u16 = 504;

pub const HTTP_FOUND: u16 = 302;
pub const HTTP_CREATED: u16 = 201;

pub const _1MB: usize = 1_024 * 1024;
pub const MAX_READ_DATA: usize = u16::MAX as usize; // 64KB
