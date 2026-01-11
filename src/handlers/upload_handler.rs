pub use crate::prelude::*;

pub fn execute_active_action(
    request: &HttpRequest,
    upload_manager: &mut Option<Upload>,
    action: &mut ActiveAction,
    start: usize,
    to_process: usize,
    boundary: &str,
) -> std::result::Result<(), ParseError> {
    let chunk = &request.buffer[start..start + to_process];
    if let ActiveAction::Upload(upload_path) = action {
        if upload_manager.is_none() {
            let upload_path = upload_path.clone();
            *upload_manager = Some(Upload::new(upload_path, boundary));
        }

        if let Some(mgr) = upload_manager {
            if !boundary.is_empty() {
                mgr.upload_body_with_boundry(request, chunk);
            } else {
                mgr.upload_simple_body(request, chunk);
            }
            if let UploadState::Error(code) = mgr.state {
                return Err(ParseError::Error(code));
            }
        }
    }
    Ok(())
}
