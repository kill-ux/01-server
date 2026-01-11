pub use crate::prelude::*;

pub fn handle_delete(
    response: &mut HttpResponse,
    request: &HttpRequest,
    r_cfg: &RouteConfig,
    s_cfg: &Arc<ServerConfig>,
) {
    let upload_base = PathBuf::from(&r_cfg.root).join(&r_cfg.upload_dir);

    // e.g., /upload/test.txt -> test.txt
    let relative_path = request.url.strip_prefix(&r_cfg.path).unwrap_or("");
    let target_path = upload_base.join(relative_path.trim_start_matches('/'));

    // 3. Security: Canonicalize and Path Traversal Check
    // This prevents DELETE /upload/../../etc/passwd
    let absolute_upload_base = match upload_base.canonicalize() {
        Ok(path) => path,
        Err(_) => {
            handle_error(response, HTTP_NOT_FOUND, Some(s_cfg));
            return;
        }
    };

    let absolute_target = match target_path.canonicalize() {
        Ok(path) => path,
        Err(e) => {
            match e.kind() {
                ErrorKind::NotFound => handle_error(response, HTTP_NOT_FOUND, Some(s_cfg)),
                _ => handle_error(response, HTTP_FORBIDDEN, Some(s_cfg)),
            };
            return;
        }
    };

    if !absolute_target.starts_with(&absolute_upload_base) {
        handle_error(response, HTTP_FORBIDDEN, Some(s_cfg));
        return;
    }

    if absolute_target.is_dir() {
        handle_error(response, HTTP_FORBIDDEN, Some(s_cfg));
        return;
    }

    match fs::remove_file(&absolute_target) {
        Ok(_) => {
            response.set_status_code(204);
        },
        Err(e) => {
            match e.kind() {
                ErrorKind::PermissionDenied => handle_error(response, HTTP_FORBIDDEN, Some(s_cfg)),
                _ => handle_error(response, HTTP_INTERNAL_SERVER_ERROR, Some(s_cfg)),
            }
        }
    }
}
