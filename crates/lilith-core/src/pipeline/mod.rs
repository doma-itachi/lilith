use std::path::Path;

use url::Url;

use crate::{
    config::AppConfig,
    error::BuildJobError,
    job::{Job, JobPaths},
    model::VideoId,
};

pub fn build_job(url: &str, config: AppConfig) -> Result<Job, BuildJobError> {
    let watch_url = normalize_url(url)?;
    validate_host(&watch_url)?;
    let video_id = parse_video_id(&watch_url)?;
    let paths = build_paths(&config.output_dir, &video_id);

    Ok(Job {
        watch_url: watch_url.to_string(),
        video_id,
        config,
        paths,
    })
}

fn normalize_url(raw_url: &str) -> Result<Url, BuildJobError> {
    let mut url = Url::parse(raw_url)?;
    url.set_fragment(None);
    Ok(url)
}

fn validate_host(url: &Url) -> Result<(), BuildJobError> {
    match url.host_str() {
        Some("www.nicovideo.jp") | Some("nicovideo.jp") | Some("sp.nicovideo.jp") => Ok(()),
        _ => Err(BuildJobError::UnsupportedHost),
    }
}

fn parse_video_id(url: &Url) -> Result<VideoId, BuildJobError> {
    let mut segments = url.path_segments().ok_or(BuildJobError::InvalidWatchPath)?;

    match segments.next() {
        Some("watch") => {}
        _ => return Err(BuildJobError::InvalidWatchPath),
    }

    let video_id = segments.next().ok_or(BuildJobError::MissingVideoId)?.trim();

    if video_id.is_empty() {
        return Err(BuildJobError::MissingVideoId);
    }

    Ok(VideoId::new(video_id))
}

fn build_paths(output_dir: &Path, video_id: &VideoId) -> JobPaths {
    let temp_dir = output_dir.join(".lilith").join(video_id.as_str());

    JobPaths {
        source_video: temp_dir.join("source.mp4"),
        comments_json: temp_dir.join("comments.json"),
        overlay_rgba: temp_dir.join("overlay.rgba"),
        output_video: output_dir.join(format!("{}.mp4", video_id.as_str())),
        temp_dir,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use url::Url;

    use crate::{config::AppConfig, error::BuildJobError};

    use super::{build_job, parse_video_id};

    #[test]
    fn parses_watch_url() {
        let url = Url::parse("https://www.nicovideo.jp/watch/sm45174902?ref=garage").unwrap();
        let actual = parse_video_id(&url).unwrap();

        assert_eq!(actual.as_str(), "sm45174902");
    }

    #[test]
    fn rejects_non_watch_url() {
        let error = build_job("https://www.nicovideo.jp/user/1", AppConfig::default()).unwrap_err();
        assert!(matches!(error, BuildJobError::InvalidWatchPath));
    }

    #[test]
    fn rejects_non_nicovideo_host() {
        let error = build_job("https://example.com/watch/sm9", AppConfig::default()).unwrap_err();
        assert!(matches!(error, BuildJobError::UnsupportedHost));
    }

    #[test]
    fn builds_expected_paths() {
        let job = build_job(
            "https://www.nicovideo.jp/watch/sm9",
            AppConfig {
                output_dir: PathBuf::from("dist"),
                ..AppConfig::default()
            },
        )
        .unwrap();

        assert_eq!(job.paths.temp_dir, PathBuf::from("dist/.lilith/sm9"));
        assert_eq!(
            job.paths.source_video,
            PathBuf::from("dist/.lilith/sm9/source.mp4")
        );
        assert_eq!(job.paths.output_video, PathBuf::from("dist/sm9.mp4"));
    }
}
