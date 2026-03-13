use std::path::PathBuf;

use crate::{config::AppConfig, model::VideoId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub watch_url: String,
    pub video_id: VideoId,
    pub config: AppConfig,
    pub paths: JobPaths,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobPaths {
    pub temp_dir: PathBuf,
    pub source_video: PathBuf,
    pub comments_json: PathBuf,
    pub overlay_rgba: PathBuf,
    pub output_video: PathBuf,
}
