use std::path::Path;

use anyhow::{Context, Result};
use tokio::{
    fs,
    io::AsyncWriteExt,
    process::{Child, ChildStdin, Command},
};

use crate::command::{build_composition_command, build_pipe_composition_command, CompositionPlan};

#[derive(Debug, Default)]
pub struct RgbaPipe;

pub struct PipeComposer {
    child: Child,
    stdin: Option<ChildStdin>,
    program: String,
}

pub async fn write_raw_rgba_file(path: &Path, frames: &[u8]) -> Result<()> {
    fs::write(path, frames)
        .await
        .with_context(|| format!("failed to write raw rgba frames to {}", path.display()))
}

pub async fn compose_video(plan: &CompositionPlan) -> Result<()> {
    let command = build_composition_command(plan, "libx264");
    let output = Command::new(&command.program)
        .args(&command.args)
        .output()
        .await
        .with_context(|| format!("failed to spawn {}", command.program))?;

    if !output.status.success() {
        anyhow::bail!(
            "ffmpeg composition failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

pub async fn compose_video_from_pipe(
    plan: &CompositionPlan,
    encoder_name: &str,
    rgba_frames: &[u8],
) -> Result<()> {
    let mut composer = spawn_pipe_composer(plan, encoder_name).await?;
    composer.write_bytes(rgba_frames).await?;
    composer.finish().await
}

pub async fn spawn_pipe_composer(
    plan: &CompositionPlan,
    encoder_name: &str,
) -> Result<PipeComposer> {
    let command = build_pipe_composition_command(plan, encoder_name);
    let mut child = Command::new(&command.program)
        .args(&command.args)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn {}", command.program))?;
    let stdin = child.stdin.take().context("ffmpeg stdin was not available")?;

    Ok(PipeComposer {
        child,
        stdin: Some(stdin),
        program: command.program,
    })
}

impl PipeComposer {
    pub async fn write_bytes(&mut self, rgba_bytes: &[u8]) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .context("ffmpeg stdin was already closed")?;

        stdin
            .write_all(rgba_bytes)
            .await
            .context("failed to stream rgba frames to ffmpeg")
    }

    pub async fn finish(mut self) -> Result<()> {
        if let Some(mut stdin) = self.stdin.take() {
            stdin.shutdown().await.context("failed to close ffmpeg stdin")?;
        }

        let output = self
            .child
            .wait_with_output()
            .await
            .context("failed to wait for ffmpeg composition")?;

        if !output.status.success() {
            anyhow::bail!(
                "{} composition failed: {}",
                self.program,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }
}
