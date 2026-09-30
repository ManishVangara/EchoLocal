//! On-disk model storage and verified, resumable downloads.

use echolocal_core::{ModelId, ModelSpec};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("download cancelled")]
    Cancelled,
    #[error("downloaded file failed verification ({0}); it was deleted, please retry")]
    Verification(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct ModelStore {
    dir: PathBuf,
}

impl ModelStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self, id: ModelId) -> PathBuf {
        self.dir.join(id.spec().filename)
    }

    fn partial_path(&self, id: ModelId) -> PathBuf {
        self.dir.join(format!("{}.part", id.spec().filename))
    }

    /// A model counts as downloaded when the final file exists with the exact
    /// expected size. The hash is checked once, when the download completes;
    /// only verified files are ever renamed into place.
    pub fn is_downloaded(&self, id: ModelId) -> bool {
        std::fs::metadata(self.path(id)).is_ok_and(|m| m.len() == id.spec().size_bytes)
    }

    /// Bytes already fetched by an interrupted download.
    pub fn partial_bytes(&self, id: ModelId) -> u64 {
        std::fs::metadata(self.partial_path(id)).map_or(0, |m| m.len())
    }

    pub fn delete(&self, id: ModelId) -> std::io::Result<()> {
        for path in [self.path(id), self.partial_path(id)] {
            match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    /// Download (or resume) a model and verify its size and SHA-256.
    /// `progress` receives `(downloaded_bytes, total_bytes)`.
    pub fn download(
        &self,
        id: ModelId,
        cancel: &AtomicBool,
        progress: impl FnMut(u64, u64),
    ) -> Result<PathBuf, DownloadError> {
        let spec: &ModelSpec = id.spec();
        std::fs::create_dir_all(&self.dir)?;
        let partial = self.partial_path(id);
        download_verified(
            &spec.download_url(),
            &partial,
            spec.size_bytes,
            spec.sha256,
            cancel,
            progress,
        )?;
        let dest = self.path(id);
        std::fs::rename(&partial, &dest)?;
        Ok(dest)
    }
}

/// Stream `url` into `partial`, resuming from any bytes already there, then
/// verify the total size and hash. On verification failure the partial file
/// is removed so the next attempt starts clean.
pub fn download_verified(
    url: &str,
    partial: &Path,
    expected_size: u64,
    expected_sha256: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), DownloadError> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        // No overall timeout: a large file on a slow link is fine. A stalled
        // transfer is cancelled by the user and resumed later.
        .timeout(None::<Duration>)
        .user_agent(concat!("EchoLocal/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| DownloadError::Network(e.to_string()))?;

    let mut hasher = Sha256::new();
    let mut have = hash_existing(partial, &mut hasher)?;
    if have > expected_size {
        std::fs::remove_file(partial)?;
        hasher = Sha256::new();
        have = 0;
    }

    if have < expected_size {
        let mut request = client.get(url);
        if have > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut response = request
            .send()
            .map_err(|e| DownloadError::Network(e.to_string()))?;
        let status = response.status();
        let append = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            true
        } else if status.is_success() {
            // Server ignored the range: start over.
            hasher = Sha256::new();
            have = 0;
            false
        } else {
            return Err(DownloadError::Network(format!("server responded {status}")));
        };

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(partial)?;
        let mut buf = vec![0u8; 256 * 1024];
        progress(have, expected_size);
        loop {
            if cancel.load(Ordering::Relaxed) {
                file.flush()?;
                return Err(DownloadError::Cancelled);
            }
            let n = response
                .read(&mut buf)
                .map_err(|e| DownloadError::Network(e.to_string()))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            have += n as u64;
            progress(have, expected_size);
            if have > expected_size {
                break;
            }
        }
        file.sync_all()?;
    }

    if have != expected_size {
        if have > expected_size {
            let _ = std::fs::remove_file(partial);
            return Err(DownloadError::Verification(format!(
                "expected {expected_size} bytes, got {have}"
            )));
        }
        // Connection closed early; keep the partial file for resuming.
        return Err(DownloadError::Network(format!(
            "connection closed after {have} of {expected_size} bytes"
        )));
    }
    let digest = to_hex(&hasher.finalize());
    if !digest.eq_ignore_ascii_case(expected_sha256) {
        let _ = std::fs::remove_file(partial);
        return Err(DownloadError::Verification("checksum mismatch".into()));
    }
    Ok(())
}

fn hash_existing(path: &Path, hasher: &mut Sha256) -> std::io::Result<u64> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    let mut buf = vec![0u8; 1024 * 1024];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(total);
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::Arc;

    /// Minimal HTTP/1.1 server with optional `Range` support, serving `body`
    /// for each accepted connection. Returns the base URL.
    fn serve(body: Vec<u8>, honor_range: bool, connections: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = Arc::new(body);
        std::thread::spawn(move || {
            for stream in listener.incoming().take(connections) {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut start = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(rest) = lower.strip_prefix("range: bytes=") {
                        start = rest.trim().trim_end_matches('-').parse().unwrap();
                    }
                }
                let (status, slice) = if honor_range && start > 0 {
                    ("206 Partial Content", &body[start..])
                } else {
                    ("200 OK", &body[..])
                };
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    slice.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                stream.write_all(slice).unwrap();
            }
        });
        format!("http://{addr}/model.gguf")
    }

    fn body() -> Vec<u8> {
        (0..300_000u32).map(|i| (i % 251) as u8).collect()
    }

    fn sha(data: &[u8]) -> String {
        to_hex(&Sha256::digest(data))
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("echolocal-dl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("model.gguf.part")
    }

    #[test]
    fn downloads_and_verifies() {
        let data = body();
        let url = serve(data.clone(), true, 1);
        let path = temp_file("fresh");
        let mut last = (0, 0);
        download_verified(
            &url,
            &path,
            data.len() as u64,
            &sha(&data),
            &AtomicBool::new(false),
            |a, b| last = (a, b),
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), data);
        assert_eq!(last, (data.len() as u64, data.len() as u64));
    }

    #[test]
    fn resumes_from_partial_file() {
        let data = body();
        let path = temp_file("resume");
        std::fs::write(&path, &data[..100_000]).unwrap();
        let url = serve(data.clone(), true, 1);
        download_verified(
            &url,
            &path,
            data.len() as u64,
            &sha(&data),
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), data);
    }

    #[test]
    fn restarts_when_server_ignores_range() {
        let data = body();
        let path = temp_file("norange");
        std::fs::write(&path, &data[..100_000]).unwrap();
        let url = serve(data.clone(), false, 1);
        download_verified(
            &url,
            &path,
            data.len() as u64,
            &sha(&data),
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), data);
    }

    #[test]
    fn rejects_corrupt_download_and_deletes_it() {
        let data = body();
        let url = serve(data.clone(), true, 1);
        let path = temp_file("corrupt");
        let err = download_verified(
            &url,
            &path,
            data.len() as u64,
            &"0".repeat(64),
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap_err();
        assert!(matches!(err, DownloadError::Verification(_)));
        assert!(!path.exists());
    }

    #[test]
    fn cancellation_keeps_partial_file() {
        let data = body();
        let url = serve(data.clone(), true, 1);
        let path = temp_file("cancel");
        let cancel = AtomicBool::new(false);
        let err = download_verified(
            &url,
            &path,
            data.len() as u64,
            &sha(&data),
            &cancel,
            |have, _| {
                if have > 0 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap_err();
        assert!(matches!(err, DownloadError::Cancelled));
        assert!(path.exists());
    }

    #[test]
    fn store_paths_and_status() {
        let dir = temp_file("store").parent().unwrap().to_path_buf();
        let store = ModelStore::new(&dir);
        let id = ModelId::ParakeetTdtV2;
        assert!(!store.is_downloaded(id));
        assert!(store.path(id).ends_with(id.spec().filename));
        // A file of the wrong size doesn't count.
        std::fs::write(store.path(id), b"x").unwrap();
        assert!(!store.is_downloaded(id));
        store.delete(id).unwrap();
        assert!(!store.path(id).exists());
        store.delete(id).unwrap();
    }
}
