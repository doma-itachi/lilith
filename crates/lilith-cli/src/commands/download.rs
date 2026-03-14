use anyhow::{Context, Result};
use lilith_core::Job;
use lilith_ffmpeg::{
    probe_video, resolve_video_encoder, spawn_pipe_composer, CompositionPlan,
    HardwareAccelMode,
};
use lilith_nico::api::NicoApiClient;
use lilith_nico::parser;
use lilith_render::{
    PreparedCommentSet, RenderComment, RenderConfig, RenderEngine, RenderRequest, TimestampMs,
};
use lilith_nico::video::{DownloadRequest, YtDlpDownloader};
use tokio::fs;

pub async fn run(mut job: Job) -> Result<()> {
    if !job.config.quiet {
        println!("queued job for {}", job.watch_url);
        println!("video id: {}", job.video_id);
        println!("temp dir: {}", job.paths.temp_dir.display());
        println!("hwaccel: {}", job.config.hwaccel.as_str());

        if let Some(font) = &job.config.font {
            println!("font: {}", font.display());
        }

        if job.config.keep_temp {
            println!("temp files: preserved");
        }
    }

    let api_client = NicoApiClient::default();
    let metadata = api_client
        .fetch_watch_metadata(&job.watch_url)
        .await
        .with_context(|| format!("failed to fetch watch metadata for {}", job.video_id))?;

    if !job.config.quiet {
        println!("title: {}", metadata.video.title);
        println!("duration: {}s", metadata.video.duration_seconds);
        println!("comment threads: {}", metadata.comment.threads.len());
        println!(
            "output: {}",
            output_video_path(&job.config.output_dir, &metadata.video.title, job.video_id.as_str())
                .display()
        );

        if let Some(nv_comment) = &metadata.comment.nv_comment {
            println!("nvcomment server: {}", nv_comment.server);
        }
    }

    job.paths.output_video = output_video_path(
        &job.config.output_dir,
        &metadata.video.title,
        job.video_id.as_str(),
    );

    let raw_comments = api_client
        .fetch_comments(&metadata.comment)
        .await
        .with_context(|| format!("failed to fetch comments for {}", job.video_id))?;
    let comments = parser::normalize(&raw_comments, &metadata.comment.threads);

    fs::create_dir_all(&job.paths.temp_dir)
        .await
        .with_context(|| format!("failed to create temp dir {}", job.paths.temp_dir.display()))?;
    fs::write(
        &job.paths.comments_json,
        serde_json::to_vec_pretty(&comments).context("failed to serialize normalized comments")?,
    )
    .await
    .with_context(|| format!("failed to write comments to {}", job.paths.comments_json.display()))?;

    if !job.config.quiet {
        println!("normalized comments: {}", comments.len());
        println!("comments json: {}", job.paths.comments_json.display());
    }

    let all_render_comments = comments
        .iter()
        .map(|comment| RenderComment {
            text: comment.body.clone(),
            vpos_ms: comment.vpos_ms,
            mail: comment.mail.clone(),
            owner: comment.owner,
            layer: comment.layer,
        })
        .collect::<Vec<_>>();
    let downloader = YtDlpDownloader::default();
    let request = DownloadRequest {
        watch_url: job.watch_url.clone(),
        output_dir: job.paths.temp_dir.clone(),
        output_template: job.paths.source_download_template(),
    };
    let downloaded_video = downloader
        .download(&request)
        .await
        .with_context(|| format!("failed to download source video for {}", job.video_id))?;
    let video_info = probe_video(&downloaded_video.file_path)
        .await
        .with_context(|| format!("failed to probe source video {}", downloaded_video.file_path.display()))?;

    if !job.config.quiet {
        println!("downloaded video: {}", downloaded_video.file_path.display());
        println!(
            "source video: {}x{} @ {}/{} fps",
            video_info.width, video_info.height, video_info.fps_num, video_info.fps_den
        );
    }

    let mut render_config = RenderConfig::default();
    render_config.frame_size.width = video_info.width;
    render_config.frame_size.height = video_info.height;
    render_config.medium_font_size = 70.0;
    let mut render_engine = RenderEngine::new(render_config.clone())
        .context("failed to initialize render engine")?;
    let prepared_all_comments = render_engine
        .prepare_comments(&all_render_comments)
        .context("failed to prepare full comment set")?;
    let resolved_encoder = resolve_video_encoder(hardware_accel_mode(job.config.hwaccel))
        .await
        .context("failed to resolve ffmpeg video encoder")?;

    let final_duration_seconds = video_info.duration_seconds as f32;
    let final_fps_num = video_info.fps_num.max(1);
    let final_fps_den = video_info.fps_den.max(1);
    let final_plan = build_composition_plan(
        &job,
        &downloaded_video.file_path,
        video_info.width,
        video_info.height,
        final_fps_num,
        final_fps_den,
        final_duration_seconds,
        job.paths.output_video.clone(),
    );
    stream_rendered_video(
        &mut render_engine,
        &prepared_all_comments,
        render_config.frame_size,
        0,
        &final_plan,
        &resolved_encoder.codec_name,
    )
        .await
        .with_context(|| format!("failed to compose output video {}", job.paths.output_video.display()))?;

    if !job.config.keep_temp {
        cleanup_temp_dir(&job.paths.temp_dir)
            .await
            .with_context(|| format!("failed to clean temp dir {}", job.paths.temp_dir.display()))?;
    }

    if !job.config.quiet {
        println!("output video: {}", job.paths.output_video.display());
        println!("encoder: {}", resolved_encoder.codec_name);
        if resolved_encoder.used_fallback {
            println!(
                "encoder fallback: {} -> {}",
                job.config.hwaccel.as_str(),
                resolved_encoder.selected.as_str()
            );
        }
    }

    if !job.config.quiet {
        println!("yt-dlp attempts: {}", downloaded_video.attempts);
        println!("status: ffmpeg output scaffold ready");
    }

    Ok(())
}

fn build_composition_plan(
    job: &Job,
    source_video: &std::path::Path,
    width: u32,
    height: u32,
    fps_num: u32,
    fps_den: u32,
    duration_seconds: f32,
    output_video: std::path::PathBuf,
) -> CompositionPlan {
    let frame_count = ((duration_seconds as f64) * fps_num as f64 / fps_den as f64)
        .round()
        .max(1.0) as usize;

    CompositionPlan {
        source_video: source_video.to_path_buf(),
        overlay_rgba: job.paths.overlay_rgba.clone(),
        output_video,
        frame_width: width,
        frame_height: height,
        fps_num,
        fps_den,
        frame_count,
        duration_seconds: Some(duration_seconds),
    }
}

async fn stream_rendered_video(
    render_engine: &mut RenderEngine,
    render_comments: &PreparedCommentSet,
    frame_size: lilith_render::layout::FrameSize,
    start_ms: u64,
    plan: &CompositionPlan,
    encoder_name: &str,
) -> Result<()> {
    let mut composer = spawn_pipe_composer(plan, encoder_name)
        .await
        .with_context(|| format!("failed to start ffmpeg pipe for {}", plan.output_video.display()))?;
    let progress_step = (plan.frame_count / 20).max(1);
    let mut stderr = std::io::stderr().lock();
    let mut sequence = render_comments.sequence();

    for frame_index in 0..plan.frame_count {
        let timestamp_ms = start_ms + ((frame_index as u64) * 1_000 * plan.fps_den as u64) / plan.fps_num as u64;
        let frame = render_engine
            .render_prepared_frame_with_sequence(
                &mut sequence,
                RenderRequest {
                    timestamp: TimestampMs(timestamp_ms),
                    frame_size,
                },
            )
            .with_context(|| format!("failed to render streamed frame {}", frame_index))?;
        composer
            .write_bytes(frame.rgba())
            .await
            .with_context(|| format!("failed to stream frame {} to ffmpeg", frame_index))?;

        if frame_index % progress_step == 0 || frame_index + 1 == plan.frame_count {
            use std::io::Write as _;
            let _ = writeln!(
                stderr,
                "rendering {}: {}/{} frames",
                plan.output_video.display(),
                frame_index + 1,
                plan.frame_count
            );
        }
    }

    composer.finish().await
}

fn output_video_path(output_dir: &std::path::Path, title: &str, video_id: &str) -> std::path::PathBuf {
    output_dir.join(format!(
        "{}[{}][コメ付き].mp4",
        sanitize_filename(title),
        video_id
    ))
}

fn sanitize_filename(title: &str) -> String {
    let sanitized = title
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            _ => ch,
        })
        .collect::<String>();
    let trimmed = sanitized.trim().trim_end_matches(['.', ' ']);
    let trimmed = trimmed.trim_end_matches('_');

    if trimmed.is_empty() {
        "video".to_string()
    } else {
        trimmed.to_string()
    }
}

async fn cleanup_temp_dir(temp_dir: &std::path::Path) -> Result<()> {
    match fs::remove_dir_all(temp_dir).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }

    if let Some(parent) = temp_dir.parent() {
        match fs::remove_dir(parent).await {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::DirectoryNotEmpty
                        | std::io::ErrorKind::Other
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }

    Ok(())
}

fn hardware_accel_mode(mode: lilith_core::HardwareAccel) -> HardwareAccelMode {
    match mode {
        lilith_core::HardwareAccel::Auto => HardwareAccelMode::Auto,
        lilith_core::HardwareAccel::None => HardwareAccelMode::None,
        lilith_core::HardwareAccel::VideoToolbox => HardwareAccelMode::VideoToolbox,
        lilith_core::HardwareAccel::Nvenc => HardwareAccelMode::Nvenc,
        lilith_core::HardwareAccel::Qsv => HardwareAccelMode::Qsv,
        lilith_core::HardwareAccel::Amf => HardwareAccelMode::Amf,
    }
}

trait HardwareAccelModeExt {
    fn as_str(self) -> &'static str;
}

impl HardwareAccelModeExt for HardwareAccelMode {
    fn as_str(self) -> &'static str {
        match self {
            HardwareAccelMode::Auto => "auto",
            HardwareAccelMode::None => "none",
            HardwareAccelMode::VideoToolbox => "videotoolbox",
            HardwareAccelMode::Nvenc => "nvenc",
            HardwareAccelMode::Qsv => "qsv",
            HardwareAccelMode::Amf => "amf",
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use lilith_core::{job::JobPaths, AppConfig, HardwareAccel, Job};

    use super::{build_composition_plan, hardware_accel_mode, output_video_path, sanitize_filename};

    #[test]
    fn builds_expected_frame_count() {
        let job = fake_job();
        let plan = build_composition_plan(
            &job,
            std::path::Path::new("source.mp4"),
            320,
            240,
            30000,
            1001,
            2.0,
            PathBuf::from("out.mp4"),
        );

        assert_eq!(plan.frame_count, 60);
        assert_eq!(plan.duration_seconds, Some(2.0));
    }

    #[test]
    fn maps_hwaccel_modes() {
        assert_eq!(hardware_accel_mode(HardwareAccel::Auto), lilith_ffmpeg::HardwareAccelMode::Auto);
        assert_eq!(hardware_accel_mode(HardwareAccel::VideoToolbox), lilith_ffmpeg::HardwareAccelMode::VideoToolbox);
    }

    #[test]
    fn builds_title_based_output_path() {
        let path = output_video_path(std::path::Path::new("."), "a:b/c", "sm9");

        assert_eq!(path, PathBuf::from("./a_b_c[sm9][コメ付き].mp4"));
    }

    #[test]
    fn sanitizes_invalid_filename_characters() {
        assert_eq!(sanitize_filename(" test<>:\\|?*./ "), "test_______.");
    }

    fn fake_job() -> Job {
        Job {
            watch_url: "https://www.nicovideo.jp/watch/sm9".to_string(),
            video_id: lilith_core::model::VideoId::new("sm9"),
            config: AppConfig {
                hwaccel: HardwareAccel::Auto,
                ..AppConfig::default()
            },
            paths: JobPaths {
                temp_dir: PathBuf::from("tmp"),
                source_video: PathBuf::from("tmp/source.mp4"),
                comments_json: PathBuf::from("tmp/comments.json"),
                overlay_rgba: PathBuf::from("tmp/overlay.rgba"),
                output_video: PathBuf::from("tmp/out.mp4"),
            },
        }
    }
}
