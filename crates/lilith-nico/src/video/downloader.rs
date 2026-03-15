use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use thiserror::Error;
use tokio::{fs, process::Command, time::sleep};

const DEFAULT_BINARY_NAME: &str = "yt-dlp";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRequest {
    pub watch_url: String,
    pub output_dir: PathBuf,
    pub output_template: PathBuf,
    pub cookies_from_browser: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedVideo {
    pub file_path: PathBuf,
    pub stdout: String,
    pub stderr: String,
    pub attempts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_retries: usize,
    pub retry_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 2,
            retry_delay: Duration::from_millis(300),
        }
    }
}

#[derive(Debug, Clone)]
pub struct YtDlpDownloader {
    binary: PathBuf,
    retry_policy: RetryPolicy,
}

impl Default for YtDlpDownloader {
    fn default() -> Self {
        Self {
            binary: PathBuf::from(DEFAULT_BINARY_NAME),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl YtDlpDownloader {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            ..Self::default()
        }
    }

    pub fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub async fn download(
        &self,
        request: &DownloadRequest,
    ) -> Result<DownloadedVideo, DownloadError> {
        let binary = resolve_binary(&self.binary)?;
        fs::create_dir_all(&request.output_dir).await?;

        let total_attempts = self.retry_policy.max_retries + 1;

        for attempt in 1..=total_attempts {
            match self.download_once(&binary, request).await {
                Ok(downloaded_video) => {
                    return Ok(DownloadedVideo {
                        attempts: attempt,
                        ..downloaded_video
                    });
                }
                Err(error) if error.is_retryable() && attempt < total_attempts => {
                    sleep(self.retry_policy.retry_delay).await;
                }
                Err(error) if error.is_retryable() => {
                    return Err(DownloadError::AttemptsExhausted {
                        attempts: attempt,
                        last_error: Box::new(error),
                    });
                }
                Err(error) => return Err(error),
            }
        }

        unreachable!("retry loop must return before exhaustion")
    }

    async fn download_once(
        &self,
        binary: &Path,
        request: &DownloadRequest,
    ) -> Result<DownloadedVideo, DownloadError> {
        let output = Command::new(binary)
            .arg("--no-progress")
            .arg("--newline")
            .arg("--no-simulate")
            .arg("--no-playlist")
            .arg("--merge-output-format")
            .arg("mp4")
            .arg("-o")
            .arg(&request.output_template)
            .args(cookies_from_browser_args(request.cookies_from_browser.as_deref()))
            .arg(&request.watch_url)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if !output.status.success() {
            return Err(DownloadError::CommandFailed {
                status: output.status.code(),
                stdout,
                stderr,
            });
        }

        let file_path = detect_downloaded_file(request).await?;

        Ok(DownloadedVideo {
            file_path,
            stdout,
            stderr,
            attempts: 1,
        })
    }
}

fn cookies_from_browser_args(spec: Option<&str>) -> Vec<String> {
    match spec {
        Some(spec) => vec!["--cookies-from-browser".to_string(), spec.to_string()],
        None => Vec::new(),
    }
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("`{binary}` was not found in PATH")]
    BinaryNotFound { binary: String },

    #[error("failed to prepare or inspect download files: {0}")]
    Io(#[from] std::io::Error),

    #[error("yt-dlp failed with status {status:?}: {stderr}")]
    CommandFailed {
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },

    #[error("yt-dlp completed but no downloaded file was found in {output_dir}")]
    OutputNotFound { output_dir: PathBuf },

    #[error("yt-dlp failed after {attempts} attempts: {last_error}")]
    AttemptsExhausted {
        attempts: usize,
        last_error: Box<DownloadError>,
    },
}

impl DownloadError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::CommandFailed { .. } | Self::OutputNotFound { .. })
    }
}

async fn detect_downloaded_file(request: &DownloadRequest) -> Result<PathBuf, DownloadError> {
    let expected_file_name = request
        .output_template
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("source.%(ext)s")
        .replace("%(ext)s", "mp4");
    let expected_path = request.output_dir.join(&expected_file_name);

    if file_exists(&expected_path).await? {
        return Ok(expected_path);
    }

    let mut directory = fs::read_dir(&request.output_dir).await?;
    let target_stem = request
        .output_template
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("source");
    let mut candidates = Vec::new();

    while let Some(entry) = directory.next_entry().await? {
        let file_type = entry.file_type().await?;
        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if !file_name.starts_with(&format!("{target_stem}.")) || is_temporary_file(&path) {
            continue;
        }

        candidates.push(path);
    }

    candidates.sort();
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| DownloadError::OutputNotFound {
            output_dir: request.output_dir.clone(),
        })
}

async fn file_exists(path: &Path) -> Result<bool, std::io::Error> {
    match fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn is_temporary_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("part" | "ytdl" | "temp")
    )
}

fn resolve_binary(binary: &Path) -> Result<PathBuf, DownloadError> {
    if is_explicit_path(binary) {
        return find_binary_candidate(binary).ok_or_else(|| DownloadError::BinaryNotFound {
            binary: binary.display().to_string(),
        });
    }

    let Some(path) = env::var_os("PATH") else {
        return Err(DownloadError::BinaryNotFound {
            binary: binary.display().to_string(),
        });
    };

    for entry in env::split_paths(&path) {
        let candidate = entry.join(binary);
        if let Some(candidate) = find_binary_candidate(&candidate) {
            return Ok(candidate);
        }
    }

    Err(DownloadError::BinaryNotFound {
        binary: binary.display().to_string(),
    })
}

fn is_explicit_path(path: &Path) -> bool {
    path.is_absolute() || path.components().count() > 1
}

fn find_binary_candidate(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return Some(path.to_path_buf());
    }

    #[cfg(windows)]
    {
        if path.extension().is_none() {
            for extension in windows_path_extensions() {
                let mut candidate = path.as_os_str().to_os_string();
                candidate.push(extension);
                let candidate = PathBuf::from(candidate);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

#[cfg(windows)]
fn windows_path_extensions() -> Vec<String> {
    env::var("PATHEXT")
        .ok()
        .map(|extensions| {
            extensions
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(|extension| extension.to_string())
                .collect::<Vec<_>>()
        })
        .filter(|extensions| !extensions.is_empty())
        .unwrap_or_else(|| {
            vec![
                ".COM".to_string(),
                ".EXE".to_string(),
                ".BAT".to_string(),
                ".CMD".to_string(),
            ]
        })
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::{fs::Permissions, os::unix::fs::PermissionsExt};

    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn reports_missing_binary() {
        let temp_dir = tempdir().unwrap();
        let downloader = YtDlpDownloader::new("missing-lilith-test-yt-dlp");
        let request = DownloadRequest {
            watch_url: "https://www.nicovideo.jp/watch/sm9".to_string(),
            output_dir: temp_dir.path().join("job"),
            output_template: temp_dir.path().join("job/source.%(ext)s"),
            cookies_from_browser: None,
        };

        let error = downloader.download(&request).await.unwrap_err();
        assert!(matches!(error, DownloadError::BinaryNotFound { .. }));
    }

    #[tokio::test]
    async fn downloads_video_and_detects_output() {
        let temp_dir = tempdir().unwrap();
        let script = write_script(
            temp_dir.path().join("fake-yt-dlp"),
            r#"#!/bin/sh
set -eu
output_template=""
url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output_template="$2"
      shift 2
      ;;
    *)
      url="$1"
      shift
      ;;
  esac
done
output_file=$(printf '%s' "$output_template" | sed 's/%(ext)s/mp4/')
mkdir -p "$(dirname "$output_file")"
printf 'video for %s' "$url" > "$output_file"
printf 'downloaded\n'
printf 'merged\n' >&2
"#,
        );
        let request = DownloadRequest {
            watch_url: "https://www.nicovideo.jp/watch/sm9".to_string(),
            output_dir: temp_dir.path().join("job"),
            output_template: temp_dir.path().join("job/source.%(ext)s"),
            cookies_from_browser: None,
        };
        let downloader = YtDlpDownloader::new(script);

        let downloaded = downloader.download(&request).await.unwrap();

        assert_eq!(downloaded.attempts, 1);
        assert_eq!(downloaded.file_path, temp_dir.path().join("job/source.mp4"));
        assert!(downloaded.stdout.contains("downloaded"));
        assert!(downloaded.stderr.contains("merged"));
    }

    #[tokio::test]
    async fn retries_transient_failures() {
        let temp_dir = tempdir().unwrap();
        let attempt_file = temp_dir.path().join("attempt.txt");
        let script = write_script(
            temp_dir.path().join("retry-yt-dlp"),
            format!(
                "#!/bin/sh\nset -eu\nattempt_file=\"{}\"\noutput_template=\"\"\ncount=0\nif [ -f \"$attempt_file\" ]; then\n  count=$(cat \"$attempt_file\")\nfi\ncount=$((count + 1))\nprintf '%s' \"$count\" > \"$attempt_file\"\nwhile [ \"$#\" -gt 0 ]; do\n  case \"$1\" in\n    -o)\n      output_template=\"$2\"\n      shift 2\n      ;;\n    *)\n      shift\n      ;;\n  esac\ndone\nif [ \"$count\" -lt 2 ]; then\n  printf 'temporary failure\\n' >&2\n  exit 1\nfi\noutput_file=$(printf '%s' \"$output_template\" | sed 's/%(ext)s/mp4/')\nmkdir -p \"$(dirname \"$output_file\")\"\nprintf 'ok' > \"$output_file\"\n",
                attempt_file.display()
            ),
        );
        let request = DownloadRequest {
            watch_url: "https://www.nicovideo.jp/watch/sm9".to_string(),
            output_dir: temp_dir.path().join("job"),
            output_template: temp_dir.path().join("job/source.%(ext)s"),
            cookies_from_browser: None,
        };
        let downloader = YtDlpDownloader::new(script).with_retry_policy(RetryPolicy {
            max_retries: 1,
            retry_delay: Duration::from_millis(10),
        });

        let downloaded = downloader.download(&request).await.unwrap();

        assert_eq!(downloaded.attempts, 2);
        assert_eq!(std::fs::read_to_string(attempt_file).unwrap(), "2");
    }

    fn write_script(path: PathBuf, contents: impl Into<String>) -> PathBuf {
        std::fs::write(&path, contents.into()).unwrap();
        std::fs::set_permissions(&path, Permissions::from_mode(0o755)).unwrap();
        path
    }
}

#[cfg(test)]
#[cfg(windows)]
mod windows_tests {
    use std::sync::{LazyLock, Mutex};

    use tempfile::tempdir;

    use super::*;

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn resolves_binary_from_pathext() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = tempdir().unwrap();
        let executable = temp_dir.path().join("yt-dlp.exe");
        std::fs::write(&executable, b"").unwrap();

        let previous_path = env::var_os("PATH");
        let previous_pathext = env::var_os("PATHEXT");

        unsafe {
            env::set_var("PATH", temp_dir.path());
            env::set_var("PATHEXT", ".EXE;.CMD");
        }

        let resolved = resolve_binary(Path::new("yt-dlp")).unwrap();

        unsafe {
            match previous_path {
                Some(path) => env::set_var("PATH", path),
                None => env::remove_var("PATH"),
            }
            match previous_pathext {
                Some(path_ext) => env::set_var("PATHEXT", path_ext),
                None => env::remove_var("PATHEXT"),
            }
        }

        assert_eq!(resolved.parent(), executable.parent());
        assert_eq!(
            resolved.file_name().and_then(|name| name.to_str()).unwrap().to_ascii_lowercase(),
            executable.file_name().and_then(|name| name.to_str()).unwrap().to_ascii_lowercase()
        );
    }

    #[test]
    fn resolves_explicit_binary_path_with_windows_extension() {
        let temp_dir = tempdir().unwrap();
        let executable = temp_dir.path().join("yt-dlp.exe");
        std::fs::write(&executable, b"").unwrap();

        let resolved = resolve_binary(&temp_dir.path().join("yt-dlp")).unwrap();

        assert_eq!(resolved.parent(), executable.parent());
        assert_eq!(
            resolved.file_name().and_then(|name| name.to_str()).unwrap().to_ascii_lowercase(),
            executable.file_name().and_then(|name| name.to_str()).unwrap().to_ascii_lowercase()
        );
    }
}
