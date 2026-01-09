pub use crate::prelude::*;

// pub fn handle_get(
//     request: &HttpRequest,
//     r_cfg: &RouteConfig,
//     s_cfg: &Arc<ServerConfig>,
// ) -> (HttpResponse, ActiveAction) {
//     let root = &r_cfg.root;
//     let relative_path = request
//         .url
//         .strip_prefix(&r_cfg.path)
//         .unwrap_or(&request.url);
//     let mut path = PathBuf::from(root);
//     path.push(relative_path.trim_start_matches('/'));

//     if path.is_dir() {
//         if r_cfg.default_file != "" {
//             path.push(&r_cfg.default_file);
//         } else if r_cfg.autoindex {
//             return (generate_autoindex(&path, &request.url), ActiveAction::None);
//         } else {
//             return (
//                 HttpResponse::new(403, "Forbidden").set_body(
//                     b"403 Forbidden: Directory listing denied".to_vec(),
//                     "text/plain",
//                 ),
//                 ActiveAction::None,
//             );
//         }
//     }

//     match File::open(&path) {
//         Ok(file) => {
//             let Ok(metadata) = file.metadata() else {
//                 return (
//                     handle_error(HTTP_INTERNAL_SERVER_ERROR, Some(s_cfg)),
//                     ActiveAction::None,
//                 );
//             };
//             let file_size = metadata.size() as usize;
//             let mime_type = get_mime_type(path.extension().and_then(|s| s.to_str()));
//             // conn.action = Some();

//             let mut res = HttpResponse::new(200, "OK");
//             res.headers
//                 .insert("Content-Length".to_string(), file_size.to_string());
//             res.headers
//                 .insert("Content-Type".to_string(), mime_type.to_string());
//             (res, ActiveAction::FileDownload(file, file_size))
//         }
//         Err(e) => match e.kind() {
//             std::io::ErrorKind::NotFound => (
//                 handle_error(HTTP_NOT_FOUND, Some(s_cfg)),
//                 ActiveAction::None,
//             ),
//             std::io::ErrorKind::PermissionDenied => (
//                 handle_error(HTTP_FORBIDDEN, Some(s_cfg)),
//                 ActiveAction::None,
//             ),
//             _ => (
//                 handle_error(HTTP_INTERNAL_SERVER_ERROR, Some(s_cfg)),
//                 ActiveAction::None,
//             ),
//         },
//     }
// }

pub fn execute_active_action<'a>(
    request: &HttpRequest,
    upload_manager: &mut Option<Upload>,
    action: &mut ActiveAction,
    start: usize,
    to_process: usize,
    boundary: &str,
) -> std::result::Result<(), ParseError> {
    let chunk = &request.buffer[start..start + to_process];
    match action {
        ActiveAction::Upload(upload_path) => {
            if upload_manager.is_none() {
                let upload_path = upload_path.clone();
                *upload_manager = Some(Upload::new(upload_path, boundary));
            }

            if let Some(mgr) = upload_manager {
                if !boundary.is_empty() {
                    mgr.upload_body_with_boundry(&request, chunk);
                } else {
                    mgr.upload_simple_body(&request, chunk);
                }
                if let UploadState::Error(code) = mgr.state {
                    return Err(ParseError::Error(code));
                }
            }
        }
        _ => {}
    }

    Ok(())
}

