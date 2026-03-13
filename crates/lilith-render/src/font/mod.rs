#[derive(Debug, Clone)]
pub struct FontCatalog {
    pub default_family: String,
}

impl Default for FontCatalog {
    fn default() -> Self {
        Self {
            default_family: "sans-serif".to_string(),
        }
    }
}
