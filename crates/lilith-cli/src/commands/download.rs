pub fn run(url: &str) -> Result<(), String> {
    let job = lilith_core::build_job(url)?;

    println!("queued job for {}", job.watch_url);
    println!("video id: {}", job.video_id);
    println!("status: scaffold only");

    Ok(())
}
