use std::time::{Duration, Instant};
use std::net::Shutdown;

use mio::Poll;

use super::Server;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

pub fn handle_timeouts(server: &mut Server, poll: &Poll) {
    let now = Instant::now();

    server.connections.retain(|_token, conn| {
        if now.duration_since(conn.last_activity) > CLIENT_TIMEOUT {
            let _ = poll.registry().deregister(&mut conn.stream);
            let _ = conn.stream.shutdown(Shutdown::Both);
            false
        } else {
            true
        }
    });
}
