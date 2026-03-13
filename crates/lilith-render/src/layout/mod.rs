#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSize {
    pub width: u32,
    pub height: u32,
}

impl FrameSize {
    pub fn hd() -> Self {
        Self {
            width: 1280,
            height: 720,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionedComment {
    pub text: String,
    pub lane: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub color: [u8; 4],
}
