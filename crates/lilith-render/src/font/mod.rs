use std::path::PathBuf;

use cosmic_text::{
    Attrs, AttrsOwned, Buffer, Color, Family, FamilyOwned, FontSystem, Metrics, Shaping,
    SwashCache, Weight, Wrap,
};
use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::engine::RenderError;

const BUNDLED_NOTO_SANS_JP: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/fonts/NotoSansJP-VariableFont_wght.ttf"
));
const BUNDLED_NOTO_SANS_JP_FAMILY: &str = "Noto Sans JP";
const MACOS_PREFERRED_FAMILIES: &[&str] = &[
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    "Hiragino Kaku Gothic Pro",
];

#[derive(Debug, Clone)]
pub struct FontCatalog {
    pub default_family: String,
    pub default_weight: Weight,
    pub custom_font: Option<PathBuf>,
}

impl Default for FontCatalog {
    fn default() -> Self {
        Self {
            default_family: preferred_default_family().to_string(),
            default_weight: Weight::SEMIBOLD,
            custom_font: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub line_height: f32,
}

pub struct FontContext {
    font_system: FontSystem,
    swash_cache: SwashCache,
    family: FamilyOwned,
    weight: Weight,
}

impl FontContext {
    pub fn new(catalog: FontCatalog) -> Result<Self, RenderError> {
        let mut font_system = FontSystem::new();
        font_system
            .db_mut()
            .load_font_data(BUNDLED_NOTO_SANS_JP.to_vec());
        let default_family = if let Some(custom_font) = &catalog.custom_font {
            font_system
                .db_mut()
                .load_font_file(custom_font)
                .map_err(|error| RenderError::FontLoad {
                    path: custom_font.clone(),
                    message: error.to_string(),
                })?;

            custom_font
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(&catalog.default_family)
                .to_string()
        } else if is_macos() {
            pick_first_available_family(font_system.db_mut(), MACOS_PREFERRED_FAMILIES)
                .unwrap_or_else(|| catalog.default_family.clone())
        } else {
            catalog.default_family.clone()
        };

        Ok(Self {
            font_system,
            swash_cache: SwashCache::new(),
            family: family_owned(&default_family),
            weight: catalog.default_weight,
        })
    }

    pub fn measure_text(&mut self, text: &str, font_size: f32) -> TextMetrics {
        let buffer = self.make_buffer(text, font_size);
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        let mut line_height = font_size * 1.2;

        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            height = run.line_top + run.line_height;
            line_height = run.line_height;
        }

        TextMetrics {
            width,
            height: height.max(line_height),
            line_height,
        }
    }

    pub fn draw_text(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        font_size: f32,
        x: f32,
        y: f32,
        rgba: [u8; 4],
        with_stroke: bool,
    ) {
        if with_stroke {
            for (dx, dy) in [
                (-1.5_f32, 0.0_f32),
                (1.5_f32, 0.0_f32),
                (0.0_f32, -1.5_f32),
                (0.0_f32, 1.5_f32),
                (-1.0_f32, -1.0_f32),
                (-1.0_f32, 1.0_f32),
                (1.0_f32, -1.0_f32),
                (1.0_f32, 1.0_f32),
            ] {
                let buffer = self.make_buffer(text, font_size);
                self.draw_buffer(pixmap, &buffer, x + dx, y + dy, [0, 0, 0, rgba[3]]);
            }
        }

        let buffer = self.make_buffer(text, font_size);
        self.draw_buffer(pixmap, &buffer, x, y, rgba);
    }

    pub fn render_text_sprite(
        &mut self,
        text: &str,
        font_size: f32,
        rgba: [u8; 4],
        with_stroke: bool,
    ) -> Result<Pixmap, RenderError> {
        let metrics = self.measure_text(text, font_size);
        let width = (metrics.width.ceil() as u32).saturating_add(8).max(1);
        let height = (metrics.height.ceil() as u32).saturating_add(8).max(1);
        let mut pixmap =
            Pixmap::new(width, height).ok_or(RenderError::PixmapAllocation { width, height })?;

        self.draw_text(&mut pixmap, text, font_size, 4.0, 4.0, rgba, with_stroke);

        Ok(pixmap)
    }

    fn make_buffer(&mut self, text: &str, font_size: f32) -> Buffer {
        let line_height = font_size * 1.2;
        let metrics = Metrics::new(font_size, line_height);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        let attrs = self.attrs();

        buffer.set_wrap(&mut self.font_system, Wrap::None);
        buffer.set_size(
            &mut self.font_system,
            Some(4_096.0),
            Some(line_height * 2.0),
        );
        buffer.set_text(
            &mut self.font_system,
            text,
            &attrs.as_attrs(),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        buffer
    }

    fn draw_buffer(
        &mut self,
        pixmap: &mut Pixmap,
        buffer: &Buffer,
        offset_x: f32,
        offset_y: f32,
        rgba: [u8; 4],
    ) {
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            Color::rgba(rgba[0], rgba[1], rgba[2], rgba[3]),
            |x, y, width, height, color| {
                let (r, g, b, a) = color.as_rgba_tuple();
                let mut paint = Paint::default();
                paint.set_color_rgba8(r, g, b, a);
                paint.anti_alias = false;

                if let Some(rect) = Rect::from_xywh(
                    offset_x + x as f32,
                    offset_y + y as f32,
                    width as f32,
                    height as f32,
                ) {
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                }
            },
        );
    }

    fn attrs(&self) -> AttrsOwned {
        AttrsOwned::new(
            &Attrs::new()
                .family(self.family.as_family())
                .weight(self.weight),
        )
    }
}

fn family_owned(name: &str) -> FamilyOwned {
    match name {
        "sans-serif" => FamilyOwned::new(Family::SansSerif),
        "serif" => FamilyOwned::new(Family::Serif),
        family => FamilyOwned::new(Family::Name(family)),
    }
}

fn preferred_default_family() -> &'static str {
    if is_macos() {
        MACOS_PREFERRED_FAMILIES[0]
    } else {
        BUNDLED_NOTO_SANS_JP_FAMILY
    }
}

fn pick_first_available_family(
    db: &mut cosmic_text::fontdb::Database,
    families: &[&str],
) -> Option<String> {
    for family in families {
        if db
            .faces()
            .any(|face| face.families.iter().any(|entry| entry.0 == *family))
        {
            return Some((*family).to_string());
        }
    }

    None
}

const fn is_macos() -> bool {
    cfg!(target_os = "macos")
}
