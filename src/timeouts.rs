use crate::prelude::*;

pub fn process(server: &mut Server, poll: &Poll) {
    let now = Instant::now();

    server.connections.retain(|token, conn| {
        // 1️⃣ Client inactivity timeout
        if now.duration_since(conn.last_activity) > CLIENT_TIMEOUT {
            dbg!("gg");
            cleanup_connection(conn, poll);
            force_cgi_timeout(
                conn,
                &mut server.cgi_to_client,
                &mut server.zombie_purgatory,
            );
            return false;
        }

        // CGI execution timeout
        if let ActiveAction::Cgi { start_time, .. } = &conn.action
            && start_time.elapsed().as_secs() > TIMEOUT_CGI
        {
            force_cgi_timeout(
                conn,
                &mut server.cgi_to_client,
                &mut server.zombie_purgatory,
            );

            poll.registry()
                .reregister(&mut conn.stream, *token, Interest::WRITABLE)
                .ok();
        }

        true
    });

    if server.session_store.last_cleanup.elapsed() > Duration::from_secs(CLEAN_UP) {
        server.session_store.cleanup();
    }

    server
        .zombie_purgatory
        .retain_mut(|child| match child.try_wait() {
            Ok(Some(_)) => false,
            Ok(None) => true,
            Err(_) => false,
        });
}
fn cleanup_connection(conn: &mut HttpConnection, poll: &Poll) {
    let _ = poll.registry().deregister(&mut conn.stream);
    let _ = conn.stream.shutdown(Shutdown::Both);
}
