use anyhow::Result;
use lilith_core::Job;

pub fn run(job: Job) -> Result<()> {
    if job.config.quiet {
        return Ok(());
    }

    println!("queued job for {}", job.watch_url);
    println!("video id: {}", job.video_id);
    println!("output: {}", job.paths.output_video.display());
    println!("temp dir: {}", job.paths.temp_dir.display());
    println!("hwaccel: {}", job.config.hwaccel.as_str());

    if let Some(font) = &job.config.font {
        println!("font: {}", font.display());
    }

    if job.config.keep_temp {
        println!("temp files: preserved");
    }

    println!("status: CLI and job scaffold ready");

    Ok(())
}
