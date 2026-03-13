#[derive(Debug, Clone)]
pub struct AppConfig {
    pub output_dir: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            output_dir: "output".to_string(),
        }
    }
}
