pub mod config;
pub mod job;
pub mod model;
pub mod pipeline;

pub use job::Job;

pub fn build_job(url: &str) -> Result<Job, String> {
    pipeline::build_job(url)
}
