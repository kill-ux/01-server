pub use crate::prelude::*;

pub fn handle_get(
    request: &HttpRequest,
    response: &mut HttpResponse,
    r_cfg: &RouteConfig,
    s_cfg: &Arc<ServerConfig>,
) -> ActiveAction {
    let root = &r_cfg.root;
    let relative_path = request
        .url
        .strip_prefix(&r_cfg.path)
        .unwrap_or(&request.url);
    let mut path = PathBuf::from(root);
    path.push(relative_path.trim_start_matches('/'));

    if path.is_dir() {
        if r_cfg.default_file != "" {
            path.push(&r_cfg.default_file);
        } else if r_cfg.autoindex {
            generate_autoindex(response, &path, &request.url);
            return ActiveAction::None;
        } else {
            response.set_status_code(403);
            response.set_body(
                b"403 Forbidden: Directory listing denied".to_vec(),
                "text/plain",
            );
            return ActiveAction::None;
        }
    }

    match File::open(&path) {
        Ok(file) => {
            let Ok(metadata) = file.metadata() else {
                handle_error(response, HTTP_INTERNAL_SERVER_ERROR, Some(s_cfg));
                return ActiveAction::None;
            };
            let file_size = metadata.size() as usize;
            let mime_type = get_mime_type(path.extension().and_then(|s| s.to_str()));
            // conn.action = Some();

            response.set_status_code(HTTP_OK);
            response
                .headers
                .insert("Content-Length".to_string(), file_size.to_string());
            response
                .headers
                .insert("Content-Type".to_string(), mime_type.to_string());

            ActiveAction::FileDownload(file, file_size)
        }
        Err(e) => {
            match e.kind() {
                std::io::ErrorKind::NotFound => handle_error(response, HTTP_NOT_FOUND, Some(s_cfg)),
                std::io::ErrorKind::PermissionDenied => {
                    handle_error(response, HTTP_FORBIDDEN, Some(s_cfg))
                }
                _ => handle_error(response, HTTP_INTERNAL_SERVER_ERROR, Some(s_cfg)),
            };
            ActiveAction::None
        }
    }
}
