pub mod config;
pub mod error;
pub mod job;
pub mod model;
pub mod pipeline;

pub use config::{AppConfig, HardwareAccel};
pub use error::BuildJobError;
pub use job::Job;

pub fn build_job(url: &str, config: AppConfig) -> Result<Job, BuildJobError> {
    pipeline::build_job(url, config)
}
