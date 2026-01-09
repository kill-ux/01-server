pub use crate::prelude::*;

pub fn handle_delete(
    request: &HttpRequest,
    r_cfg: &RouteConfig,
    s_cfg: &Arc<ServerConfig>,
) -> HttpResponse {
    let upload_base = PathBuf::from(&r_cfg.root).join(&r_cfg.upload_dir);

    // e.g., /upload/test.txt -> test.txt
    let relative_path = request.url.strip_prefix(&r_cfg.path).unwrap_or("");
    let target_path = upload_base.join(relative_path.trim_start_matches('/'));

    // 3. Security: Canonicalize and Path Traversal Check
    // This prevents DELETE /upload/../../etc/passwd
    let absolute_upload_base = match upload_base.canonicalize() {
        Ok(path) => path,
        Err(_) => return handle_error(HTTP_NOT_FOUND, Some(s_cfg)),
    };

    let absolute_target = match target_path.canonicalize() {
        Ok(path) => path,
        Err(e) => {
            return match e.kind() {
                ErrorKind::NotFound => handle_error(HTTP_NOT_FOUND, Some(s_cfg)),
                _ => handle_error(HTTP_FORBIDDEN, Some(s_cfg)),
            };
        }
    };

    if !absolute_target.starts_with(&absolute_upload_base) {
        return handle_error(HTTP_FORBIDDEN, Some(s_cfg));
    }

    if absolute_target.is_dir() {
        return handle_error(HTTP_FORBIDDEN, Some(s_cfg));
    }

    match fs::remove_file(&absolute_target) {
        Ok(_) => HttpResponse::new(204, "No Content"),
        Err(e) => match e.kind() {
            ErrorKind::PermissionDenied => handle_error(HTTP_FORBIDDEN, Some(s_cfg)),
            _ => handle_error(HTTP_INTERNAL_SERVER_ERROR, Some(s_cfg)),
        },
    }
}
