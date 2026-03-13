use thiserror::Error;

#[derive(Debug, Error)]
pub enum BuildJobError {
    #[error("invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    #[error("URL must point to nicovideo.jp")]
    UnsupportedHost,

    #[error("URL must be a NicoNico watch URL")]
    InvalidWatchPath,

    #[error("watch URL is missing a video id")]
    MissingVideoId,
}
