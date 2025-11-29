// // ============================================================================
// // data_provider.rs
// // ============================================================================

use std::path::{Path, PathBuf};
use std::fs;

/// Provides data/content for HTTP responses
pub struct DataProvider {
    root_dir: PathBuf,
}

impl DataProvider {
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        Self {
            root_dir: root_dir.as_ref().to_path_buf(),
        }
    }

    /// Read a file from the filesystem
    pub fn read_file(&self, path: &str) -> std::io::Result<Vec<u8>> {
        let safe_path = self.sanitize_path(path);
        let full_path = self.root_dir.join(&safe_path);
        
        // Security: prevent directory traversal
        if !full_path.starts_with(&self.root_dir) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Invalid path",
            ));
        }

        fs::read(full_path)
    }

    /// Check if a file exists
    pub fn file_exists(&self, path: &str) -> bool {
        let safe_path = self.sanitize_path(path);
        let full_path = self.root_dir.join(&safe_path);
        full_path.exists() && full_path.is_file()
    }

    ////////// Get MIME type for a file
    pub fn get_mime_type(&self, path: &str) -> &'static str {
        let extension = Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        match extension {
            "html" | "htm" => "text/html",
            "css" => "text/css",
            "js" => "application/javascript",
            "json" => "application/json",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "svg" => "image/svg+xml",
            "txt" => "text/plain",
            _ => "application/octet-stream",
        }
    }

    /// Sanitize path to prevent directory traversal
    fn sanitize_path(&self, path: &str) -> PathBuf {
        let path = path.trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };
        PathBuf::from(path)
    }

    /// Get directory listing (optional, for directory browsing)
    pub fn get_directory_listing(&self, path: &str) -> std::io::Result<Vec<String>> {
        let safe_path = self.sanitize_path(path);
        let full_path = self.root_dir.join(&safe_path);
        
        if !full_path.starts_with(&self.root_dir) || !full_path.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Directory not found",
            ));
        }

        let mut entries = Vec::new();
        for entry in fs::read_dir(full_path)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                entries.push(name.to_string());
            }
        }
        Ok(entries)
    }
}