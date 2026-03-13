use std::path::Path;

use tiny_skia::Pixmap;

use crate::engine::RenderError;

pub struct RenderFrame {
    pixmap: Pixmap,
}

impl RenderFrame {
    pub fn new(pixmap: Pixmap) -> Self {
        Self { pixmap }
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    pub fn rgba(&self) -> &[u8] {
        self.pixmap.data()
    }

    pub fn into_rgba(self) -> Vec<u8> {
        self.pixmap.take()
    }

    pub fn save_png(&self, path: &Path) -> Result<(), RenderError> {
        self.pixmap
            .save_png(path)
            .map_err(|error| RenderError::PngEncode {
                path: path.to_path_buf(),
                message: error.to_string(),
            })
    }
}
