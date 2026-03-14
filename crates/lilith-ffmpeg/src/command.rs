use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareAccelMode {
    Auto,
    None,
    VideoToolbox,
    Nvenc,
    Qsv,
    Amf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositionPlan {
    pub source_video: PathBuf,
    pub overlay_rgba: PathBuf,
    pub output_video: PathBuf,
    pub frame_width: u32,
    pub frame_height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub frame_count: usize,
    pub duration_seconds: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl FfmpegCommand {
    pub fn as_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FfprobeInfo {
    pub codec_name: String,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub duration_seconds: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEncoder {
    pub requested: HardwareAccelMode,
    pub selected: HardwareAccelMode,
    pub codec_name: String,
    pub used_fallback: bool,
}

pub fn binary_name() -> &'static str {
    "ffmpeg"
}

pub fn ffprobe_binary_name() -> &'static str {
    "ffprobe"
}

pub fn build_composition_command(plan: &CompositionPlan, encoder_name: &str) -> FfmpegCommand {
    build_command(plan, encoder_name, OverlayInput::File)
}

pub fn build_pipe_composition_command(
    plan: &CompositionPlan,
    encoder_name: &str,
) -> FfmpegCommand {
    build_command(plan, encoder_name, OverlayInput::Pipe)
}

fn build_command(
    plan: &CompositionPlan,
    encoder_name: &str,
    overlay_input: OverlayInput,
) -> FfmpegCommand {
    let fps = format!("{}/{}", plan.fps_num, plan.fps_den);
    let overlay_path = match overlay_input {
        OverlayInput::File => plan.overlay_rgba.display().to_string(),
        OverlayInput::Pipe => "pipe:0".to_string(),
    };
    let mut args = vec![
        "-y".to_string(),
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-i".to_string(),
        plan.source_video.display().to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s:v".to_string(),
        format!("{}x{}", plan.frame_width, plan.frame_height),
        "-r".to_string(),
        fps,
        "-i".to_string(),
        overlay_path,
        "-filter_complex".to_string(),
        crate::filter::overlay_filter().to_string(),
        "-map".to_string(),
        "[vout]".to_string(),
        "-map".to_string(),
        "0:a?".to_string(),
        "-c:v".to_string(),
        encoder_name.to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-c:a".to_string(),
        "copy".to_string(),
    ];

    if let Some(duration_seconds) = plan.duration_seconds {
        args.push("-t".to_string());
        args.push(format!("{duration_seconds:.3}"));
    }

    args.push("-movflags".to_string());
    args.push("+faststart".to_string());
    args.push(plan.output_video.display().to_string());

    FfmpegCommand {
        program: binary_name().to_string(),
        args,
    }
}

pub fn video_encoder_for(mode: HardwareAccelMode) -> &'static str {
    match mode {
        HardwareAccelMode::Auto | HardwareAccelMode::None => "libx264",
        HardwareAccelMode::VideoToolbox => "h264_videotoolbox",
        HardwareAccelMode::Nvenc => "h264_nvenc",
        HardwareAccelMode::Qsv => "h264_qsv",
        HardwareAccelMode::Amf => "h264_amf",
    }
}

pub async fn resolve_video_encoder(
    requested: HardwareAccelMode,
) -> Result<ResolvedEncoder, EncoderSelectionError> {
    let available = available_video_encoders().await?;
    let selected = match requested {
        HardwareAccelMode::Auto => preferred_auto_mode(&available),
        HardwareAccelMode::None => HardwareAccelMode::None,
        other if available.iter().any(|item| item == video_encoder_for(other)) => other,
        other => {
            return Ok(ResolvedEncoder {
                requested,
                selected: HardwareAccelMode::None,
                codec_name: video_encoder_for(HardwareAccelMode::None).to_string(),
                used_fallback: other != HardwareAccelMode::None,
            });
        }
    };

    Ok(ResolvedEncoder {
        requested,
        codec_name: video_encoder_for(selected).to_string(),
        selected,
        used_fallback: selected != requested && requested != HardwareAccelMode::Auto,
    })
}

pub async fn available_video_encoders() -> Result<Vec<String>, EncoderSelectionError> {
    let output = Command::new(binary_name())
        .arg("-hide_banner")
        .arg("-encoders")
        .output()
        .await?;

    if !output.status.success() {
        return Err(EncoderSelectionError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_available_video_encoders(&stdout))
}

pub async fn probe_video(path: &Path) -> Result<FfprobeInfo, ProbeError> {
    probe_video_with_binary(path, ffprobe_binary_name()).await
}

async fn probe_video_with_binary(path: &Path, binary: &str) -> Result<FfprobeInfo, ProbeError> {
    let output = Command::new(binary)
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=codec_name,width,height,avg_frame_rate,duration")
        .arg("-of")
        .arg("json")
        .arg(path)
        .output()
        .await?;

    if !output.status.success() {
        return Err(ProbeError::CommandFailed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    let response: RawFfprobeResponse = serde_json::from_slice(&output.stdout)?;
    let stream = response.streams.into_iter().next().ok_or(ProbeError::NoVideoStream)?;
    let (fps_num, fps_den) = parse_ratio(&stream.avg_frame_rate).ok_or_else(|| {
        ProbeError::InvalidFrameRate(stream.avg_frame_rate.clone())
    })?;

    Ok(FfprobeInfo {
        codec_name: stream.codec_name,
        width: stream.width,
        height: stream.height,
        fps_num,
        fps_den,
        duration_seconds: stream.duration.parse().unwrap_or(0.0),
    })
}

fn parse_ratio(value: &str) -> Option<(u32, u32)> {
    let (left, right) = value.split_once('/')?;
    Some((left.parse().ok()?, right.parse().ok()?))
}

fn parse_available_video_encoders(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.starts_with('V') {
                return None;
            }

            line.split_whitespace().nth(1).map(ToOwned::to_owned)
        })
        .collect()
}

fn preferred_auto_mode(available: &[String]) -> HardwareAccelMode {
    let preferred = if cfg!(target_os = "macos") {
        [HardwareAccelMode::VideoToolbox, HardwareAccelMode::None]
    } else {
        [HardwareAccelMode::None, HardwareAccelMode::VideoToolbox]
    };

    preferred
        .into_iter()
        .find(|mode| available.iter().any(|item| item == video_encoder_for(*mode)))
        .unwrap_or(HardwareAccelMode::None)
}

#[derive(Debug, Error)]
pub enum EncoderSelectionError {
    #[error("failed to inspect ffmpeg encoders: {0}")]
    Io(#[from] std::io::Error),

    #[error("ffmpeg encoder inspection failed: {0}")]
    CommandFailed(String),
}

enum OverlayInput {
    File,
    Pipe,
}

#[derive(Debug, Error)]
pub enum ProbeError {
    #[error("failed to run ffprobe: {0}")]
    Io(#[from] std::io::Error),

    #[error("ffprobe failed: {0}")]
    CommandFailed(String),

    #[error("failed to parse ffprobe output: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("ffprobe returned no video stream")]
    NoVideoStream,

    #[error("invalid frame rate `{0}`")]
    InvalidFrameRate(String),
}

#[derive(Debug, Deserialize)]
struct RawFfprobeResponse {
    streams: Vec<RawFfprobeStream>,
}

#[derive(Debug, Deserialize)]
struct RawFfprobeStream {
    codec_name: String,
    width: u32,
    height: u32,
    avg_frame_rate: String,
    duration: String,
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::{fs::Permissions, os::unix::fs::PermissionsExt};

    #[cfg(unix)]
    use tempfile::tempdir;

    use super::{
        build_composition_command, video_encoder_for, CompositionPlan, HardwareAccelMode,
    };

    #[test]
    fn builds_ffmpeg_command_for_raw_overlay() {
        let command = build_composition_command(&CompositionPlan {
            source_video: "source.mp4".into(),
            overlay_rgba: "overlay.rgba".into(),
            output_video: "out.mp4".into(),
            frame_width: 320,
            frame_height: 240,
            fps_num: 30000,
            fps_den: 1001,
            frame_count: 90,
            duration_seconds: Some(1.0),
        }, video_encoder_for(HardwareAccelMode::VideoToolbox));

        assert_eq!(command.program, "ffmpeg");
        assert!(command.args.contains(&"rawvideo".to_string()));
        assert!(command
            .args
            .contains(&"[0:v][1:v]overlay=shortest=1[vout]".to_string()));
        assert!(command.args.contains(&"[vout]".to_string()));
        assert!(command.args.contains(&"-t".to_string()));
        assert!(command.args.contains(&"h264_videotoolbox".to_string()));
    }

    #[test]
    fn resolves_encoder_names() {
        assert_eq!(video_encoder_for(HardwareAccelMode::Auto), "libx264");
        assert_eq!(video_encoder_for(HardwareAccelMode::VideoToolbox), "h264_videotoolbox");
    }

    #[test]
    fn parses_available_video_encoder_names() {
        let output = "V....D libx264 libx264 H.264\n A....D aac AAC\nV....D h264_videotoolbox VideoToolbox H.264 Encoder\n";
        let encoders = super::parse_available_video_encoders(output);

        assert_eq!(encoders, vec!["libx264", "h264_videotoolbox"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn probes_video_from_fake_ffprobe() {
        let temp_dir = tempdir().unwrap();
        let script = temp_dir.path().join("fake-ffprobe");
        std::fs::write(
            &script,
            r#"#!/bin/sh
printf '{"streams":[{"codec_name":"h264","width":320,"height":240,"avg_frame_rate":"30000/1001","duration":"3.003"}]}'
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, Permissions::from_mode(0o755)).unwrap();

        let info = super::probe_video_with_binary(std::path::Path::new("dummy.mp4"), script.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(info.width, 320);
        assert_eq!(info.height, 240);
        assert_eq!((info.fps_num, info.fps_den), (30000, 1001));
    }
}
