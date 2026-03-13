pub mod downloader;

pub use downloader::{DownloadError, DownloadRequest, DownloadedVideo, RetryPolicy, YtDlpDownloader};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoMetadata {
    pub id: String,
    pub title: String,
    pub duration_seconds: u64,
    pub registered_at: Option<String>,
}
