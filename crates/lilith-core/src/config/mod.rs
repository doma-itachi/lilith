use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HardwareAccel {
    #[default]
    Auto,
    None,
    VideoToolbox,
    Nvenc,
    Qsv,
    Amf,
}

impl HardwareAccel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::VideoToolbox => "videotoolbox",
            Self::Nvenc => "nvenc",
            Self::Qsv => "qsv",
            Self::Amf => "amf",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub output_dir: PathBuf,
    pub keep_temp: bool,
    pub hwaccel: HardwareAccel,
    pub font: Option<PathBuf>,
    pub quiet: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("."),
            keep_temp: false,
            hwaccel: HardwareAccel::Auto,
            font: None,
            quiet: false,
        }
    }
}
