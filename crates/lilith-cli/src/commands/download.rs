use anyhow::{Context, Result};
use lilith_core::Job;
use lilith_ffmpeg::{
    probe_video, resolve_video_encoder, spawn_pipe_composer, CompositionPlan,
    HardwareAccelMode,
};
use lilith_nico::api::NicoApiClient;
use lilith_nico::cookies::load_browser_cookies;
use lilith_nico::parser;
use lilith_render::{
    PreparedCommentSet, RenderComment, RenderConfig, RenderEngine, RenderRequest, TimestampMs,
};
use lilith_nico::video::{DownloadRequest, YtDlpDownloader};
use std::time::Instant;
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

    let browser_cookies = job
        .config
        .cookies_from_browser
        .as_deref()
        .map(load_browser_cookies)
        .transpose()
        .context("failed to load cookies from browser")?;
    let api_client = NicoApiClient::new(client_with_optional_cookies(browser_cookies.as_ref())?);
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
        cookies_from_browser: job.config.cookies_from_browser.clone(),
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
    let started_at = Instant::now();

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
            let _ = write!(
                stderr,
                "\rrendering {}: {}",
                plan.output_video.display(),
                format_render_progress(frame_index + 1, plan.frame_count, started_at.elapsed())
            );
            let _ = stderr.flush();
        }
    }

    {
        use std::io::Write as _;
        let _ = writeln!(stderr);
        let _ = stderr.flush();
    }

    composer.finish().await
}

fn format_render_progress(processed_frames: usize, total_frames: usize, elapsed: std::time::Duration) -> String {
    let eta = estimate_eta(processed_frames, total_frames, elapsed);
    format!(
        "{}/{} frames eta {}",
        processed_frames,
        total_frames,
        format_duration(eta)
    )
}

fn estimate_eta(
    processed_frames: usize,
    total_frames: usize,
    elapsed: std::time::Duration,
) -> std::time::Duration {
    if processed_frames == 0 || processed_frames >= total_frames {
        return std::time::Duration::ZERO;
    }

    let remaining_frames = (total_frames - processed_frames) as f64;
    let per_frame = elapsed.as_secs_f64() / processed_frames as f64;

    std::time::Duration::from_secs_f64((remaining_frames * per_frame).max(0.0))
}

fn format_duration(duration: std::time::Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
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

fn client_with_optional_cookies(
    cookies: Option<&lilith_nico::cookies::BrowserCookies>,
) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder().user_agent(format!("lilith/{}", env!("CARGO_PKG_VERSION")));

    if let Some(headers) = cookie_headers(cookies)? {
        builder = builder.default_headers(headers);
    }

    builder.build().context("failed to build reqwest client")
}

fn cookie_headers(
    cookies: Option<&lilith_nico::cookies::BrowserCookies>,
) -> Result<Option<reqwest::header::HeaderMap>> {
    let Some(cookies) = cookies else {
        return Ok(None);
    };

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::COOKIE,
        reqwest::header::HeaderValue::from_str(&cookies.header_value)
            .context("invalid cookie header value")?,
    );

    Ok(Some(headers))
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

    use super::{
        build_composition_plan, cookie_headers, estimate_eta, format_duration,
        format_render_progress, hardware_accel_mode, output_video_path, sanitize_filename,
    };

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

    #[test]
    fn formats_render_progress_with_eta() {
        let progress = format_render_progress(50, 100, std::time::Duration::from_secs(10));

        assert_eq!(progress, "50/100 frames eta 00:10");
    }

    #[test]
    fn formats_duration_compactly() {
        assert_eq!(format_duration(std::time::Duration::from_secs(65)), "01:05");
        assert_eq!(format_duration(std::time::Duration::from_secs(3661)), "01:01:01");
    }

    #[test]
    fn estimates_eta_from_average_frame_time() {
        let eta = estimate_eta(25, 100, std::time::Duration::from_secs(5));

        assert_eq!(eta.as_secs(), 15);
    }

    #[test]
    fn builds_cookie_header() {
        let headers = cookie_headers(Some(&lilith_nico::cookies::BrowserCookies {
            source: "chrome".to_string(),
            header_value: "user_session=abc".to_string(),
            yt_dlp_argument: "chrome".to_string(),
        }))
        .unwrap();

        assert_eq!(
            headers.unwrap().get(reqwest::header::COOKIE).unwrap(),
            "user_session=abc"
        );
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
