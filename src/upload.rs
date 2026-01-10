use crate::prelude::*;

#[derive(Debug)]
pub enum UploadState {
    InProgress,
    Done,
    Error(u16),
}

impl Upload {
    pub fn new(path: PathBuf, boundary: &str) -> Self {
        Self {
            state: UploadState::InProgress,
            multi_part_state: MultiPartState::Start,
            path,
            boundary: boundary.to_string(),
            buffer: Vec::new(),
            current_pos: 0,
            saved_filenames: Vec::new(),
            files_saved: 0,
            part_info: PartInfo::default(),
            current_file_path: None,
        }
    }
}

#[derive(Debug)]
pub struct Upload {
    pub state: UploadState,
    pub multi_part_state: MultiPartState,
    pub path: PathBuf,
    pub boundary: String,
    pub buffer: Vec<u8>,
    pub current_pos: usize,
    pub saved_filenames: Vec<String>,
    pub files_saved: usize,
    pub part_info: PartInfo,
    pub current_file_path: Option<PathBuf>,
}

#[derive(Debug)]
pub enum MultiPartState {
    Start,
    HeaderSep,
    NextBoundary(usize),
}

impl Upload {
    pub fn upload_simple_body(&mut self, req: &HttpRequest, chunk: &[u8]) {
        let target_path = if let Some(ref path) = self.current_file_path {
            path.clone()
        } else {
            let upload_path = &self.path;
            let mut file_name = req.extract_filename();
            file_name.push_str(get_ext_from_content_type(
                req.headers.get("content-type").map_or("", |v| v),
            ));
            let full_path = upload_path.join(&file_name);
            self.current_file_path = Some(full_path.clone());
            full_path
        };

        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&target_path)
        {
            Ok(mut file) => match file.write_all(chunk) {
                Ok(_) => {}
                Err(_) => {
                    self.state = UploadState::Error(HTTP_INTERNAL_SERVER_ERROR);
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                self.state = UploadState::Error(HTTP_FORBIDDEN);
            }
            Err(_) => {
                self.state = UploadState::Error(HTTP_INTERNAL_SERVER_ERROR);
            }
        }
    }

    pub fn upload_body_with_boundry(&mut self, req: &HttpRequest, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);

        let boundary_str = format!("--{}", self.boundary);
        let boundary_bytes = boundary_str.as_bytes();
        let header_sep = b"\r\n\r\n";

        loop {
            match self.multi_part_state {
                MultiPartState::Start => {
                    if let Some(start_idx) =
                        find_subsequence(&self.buffer, boundary_bytes, self.current_pos)
                    {
                        let part_start = start_idx + boundary_bytes.len() + 2;

                        if self.buffer.len() < part_start {
                            break;
                        }

                        if self.buffer.get(part_start - 2..part_start) == Some(b"--") {
                            self.state = UploadState::Done;
                            break;
                        }

                        self.current_pos = part_start;
                        self.multi_part_state = MultiPartState::HeaderSep;
                    } else {
                        self.trim_buffer();
                        break;
                    }
                }

                MultiPartState::HeaderSep => {
                    if let Some(sep_idx) =
                        find_subsequence(&self.buffer, header_sep, self.current_pos)
                    {
                        let data_start = sep_idx + 4;
                        let headers_part =
                            String::from_utf8_lossy(&self.buffer[self.current_pos..data_start]);

                        self.part_info = parse_part_headers(&headers_part);
                        self.multi_part_state = MultiPartState::NextBoundary(data_start);
                        self.current_pos = data_start;
                    } else {
                        break;
                    }
                }

                MultiPartState::NextBoundary(data_start) => {
                    if let Some(next_boundary_idx) =
                        find_subsequence(&self.buffer, boundary_bytes, data_start)
                    {
                        // 1. Identify where the binary data actually ends (before the \r\n)
                        let mut data_end = next_boundary_idx;
                        if next_boundary_idx >= 2
                            && &self.buffer[next_boundary_idx - 2..next_boundary_idx] == b"\r\n"
                        {
                            data_end -= 2;
                        }

                        // 2. Save the final chunk of this file
                        if self.part_info.filename.is_some() {
                            self.save_file_part(req, data_start, data_end);
                        }

                        // 3. CLEANUP FOR NEXT PART
                        // Remove everything up to the boundary so the buffer is fresh
                        self.buffer.drain(..next_boundary_idx);
                        self.current_pos = 0;
                        self.current_file_path = None; // Reset so next file gets a new name
                        self.multi_part_state = MultiPartState::Start;
                    } else {
                        self.flush_partial_data(req, data_start);
                        break;
                    }
                }
            }
        }
    }

    fn flush_partial_data(&mut self, req: &HttpRequest, data_start: usize) {
        let safety_margin = self.boundary.len() + 10;

        if self.buffer.len() > (data_start + safety_margin) {
            let write_end = self.buffer.len() - safety_margin;
            let data_to_write = &self.buffer[data_start..write_end];

            let target_path = if let Some(ref path) = self.current_file_path {
                path.clone()
            } else {
                let path = self
                    .get_current_part_path(req)
                    .unwrap_or_else(|| PathBuf::from("tmp_upload"));
                let unique =
                    Self::get_unique_path(&self.path, path.file_name().unwrap().to_str().unwrap());
                self.current_file_path = Some(unique.clone());
                unique
            };

            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&target_path)
            {
                let _ = file.write_all(data_to_write);
            }

            self.buffer.drain(data_start..write_end);
            self.multi_part_state = MultiPartState::NextBoundary(data_start);
            self.current_pos = data_start;
        }
    }

    fn get_current_part_path(&self, req: &HttpRequest) -> Option<PathBuf> {
        // Use the part_info to generate the path, similar to your save_file_part logic
        if self.part_info.filename.is_none() {
            return None;
        }

        let raw_fname = self.part_info.filename.as_deref().unwrap_or("");
        let clean_name = if raw_fname.is_empty() {
            let mut n = req.extract_filename();
            n.push_str(get_ext_from_content_type(&self.part_info.content_type));
            n
        } else {
            Self::sanitize_filename(raw_fname)
        };

        Some(self.path.join(clean_name))
    }

    fn trim_buffer(&mut self) {
        let b_len = self.boundary.len() + 4;
        if self.buffer.len() > b_len {
            let drain_to = self.buffer.len() - b_len;
            self.buffer.drain(..drain_to);
            self.current_pos = 0;
        }
    }

    fn save_file_part(&mut self, req: &HttpRequest, data_start: usize, data_end: usize) {
        let data = &self.buffer[data_start..data_end];

        let final_path = if let Some(path) = self.current_file_path.take() {
            path
        } else {
            let raw_fname = self.part_info.filename.as_deref().unwrap_or("");
            let clean_name = if raw_fname.is_empty() {
                let mut n = req.extract_filename();
                n.push_str(get_ext_from_content_type(&self.part_info.content_type));
                n
            } else {
                Self::sanitize_filename(raw_fname)
            };
            Self::get_unique_path(&self.path, &clean_name)
        };

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&final_path)
        {
            if file.write_all(data).is_ok() {
                self.files_saved += 1;
                self.saved_filenames.push(
                    final_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }

        self.current_file_path = None;
    }

    fn get_unique_path(directory: &PathBuf, filename: &str) -> PathBuf {
        let mut full_path = directory.join(filename);
        let mut counter = 1;

        // While the file exists, append a (1), (2), etc.
        while full_path.exists() {
            let stem = Path::new(filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("file");
            let ext = Path::new(filename)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            let new_name = if ext.is_empty() {
                format!("{}_{}", stem, counter)
            } else {
                format!("{}_{}.{}", stem, counter, ext)
            };

            full_path = directory.join(new_name);
            counter += 1;
        }
        full_path
    }

    pub fn sanitize_filename(name: &str) -> String {
        // 1. Use Path to extract only the file_name component
        // This handles cases like "path/to/my_file.txt" -> "my_file.txt"
        let path = std::path::Path::new(name);
        let raw_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("default_upload");

        // 2. Filter characters: Allow only Alphanumeric, dots, underscores, and hyphens
        let sanitized: String = raw_name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' {
                    c
                } else {
                    '_' // Replace spaces or symbols with underscores
                }
            })
            .collect();

        // 3. Prevent hidden files or relative dots (e.g., "..", ".env") if desired
        // For many servers, we force the name to start with a standard character
        if sanitized.is_empty() || sanitized.starts_with('.') {
            format!("upload_{}", sanitized)
        } else {
            sanitized
        }
    }

    pub fn handel_upload_manager(
        response: &mut HttpResponse,
        upload_manager: &mut Upload,
        s_cfg: &Arc<ServerConfig>,
    ) {
        if upload_manager.boundary.is_empty() {
            if let Some(target_path) = &upload_manager.current_file_path {
                upload_manager.saved_filenames.push(
                    target_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
                upload_manager.files_saved += 1;
            }
        }
        if upload_manager.saved_filenames.len() > 0 {
            // let mut res = HttpResponse::new(HTTP_CREATED, "Created");
            response.set_status_code(HTTP_CREATED);
            if upload_manager.saved_filenames.len() == 1 {
                response.headers.insert(
                    "location".to_string(),
                    format!("/upload/{}", upload_manager.saved_filenames[0]),
                );
                response.set_body(
                    format!("File saved as {}", upload_manager.saved_filenames[0]).into_bytes(),
                    "text/plain",
                );
            } else {
                let body_msg =
                    format!("Saved files: {}", upload_manager.saved_filenames.join(", "));
                response.set_body(body_msg.into_bytes(), "text/plain");
            }
        } else {
            handle_error(response, HTTP_INTERNAL_SERVER_ERROR, Some(s_cfg));
        };
    }
}
