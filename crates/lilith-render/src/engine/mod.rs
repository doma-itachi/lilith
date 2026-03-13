use std::path::PathBuf;
use std::{collections::HashMap, ops::Range};

use thiserror::Error;
use tiny_skia::Pixmap;

use crate::{
    font::{FontCatalog, FontContext},
    layout::FrameSize,
    renderer::RenderFrame,
    timeline::{CommentPlacement, CommentSize, RenderComment, TimestampMs},
};

#[cfg(test)]
use crate::layout::PositionedComment;

#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub frame_size: FrameSize,
    pub default_font: FontCatalog,
    pub medium_font_size: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            frame_size: FrameSize::hd(),
            default_font: FontCatalog::default(),
            medium_font_size: 36.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RenderRequest {
    pub timestamp: TimestampMs,
    pub frame_size: FrameSize,
}

impl Default for RenderRequest {
    fn default() -> Self {
        Self {
            timestamp: TimestampMs(0),
            frame_size: FrameSize::hd(),
        }
    }
}

pub struct RenderEngine {
    font_context: FontContext,
    config: RenderConfig,
}

#[derive(Debug, Clone)]
pub struct PreparedComment {
    pub text: String,
    pub vpos_ms: u64,
    pub owner: bool,
    pub layer: i32,
    pub placement: CommentPlacement,
    pub lifetime_ms: u64,
    pub sprite: tiny_skia::Pixmap,
    pub width: f32,
    pub height: f32,
    pub line_height: f32,
}

#[derive(Debug, Clone)]
pub struct PreparedCommentSet {
    comments: Vec<PreparedComment>,
}

#[derive(Debug, Clone)]
struct PreparedLayoutItem {
    index: usize,
    #[allow(dead_code)]
    lane: usize,
    x: f32,
    y: f32,
}

impl RenderEngine {
    pub fn new(config: RenderConfig) -> Result<Self, RenderError> {
        Ok(Self {
            font_context: FontContext::new(config.default_font.clone())?,
            config,
        })
    }

    pub fn prepare_comments(
        &mut self,
        comments: &[RenderComment],
    ) -> Result<PreparedCommentSet, RenderError> {
        let prepared = comments
            .iter()
            .map(|comment| self.prepare_comment(comment))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PreparedCommentSet { comments: prepared })
    }

    pub fn render_frame(
        &mut self,
        comments: &[RenderComment],
        request: RenderRequest,
    ) -> Result<RenderFrame, RenderError> {
        let prepared = self.prepare_comments(comments)?;
        self.render_prepared_frame(&prepared, request)
    }

    pub fn render_prepared_frame(
        &mut self,
        comments: &PreparedCommentSet,
        request: RenderRequest,
    ) -> Result<RenderFrame, RenderError> {
        let mut pixmap = Pixmap::new(request.frame_size.width, request.frame_size.height).ok_or(
            RenderError::PixmapAllocation {
                width: request.frame_size.width,
                height: request.frame_size.height,
            },
        )?;
        let draw_items = self.layout_prepared_comments(comments.comments.as_slice(), request);

        for item in draw_items {
            let mut paint = tiny_skia::PixmapPaint::default();
            paint.opacity = 1.0;
            pixmap.draw_pixmap(
                item.x.round() as i32,
                item.y.round() as i32,
                comments.comments[item.index].sprite.as_ref(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            );
        }

        Ok(RenderFrame::new(pixmap))
    }

    #[cfg(test)]
    fn layout_comments(
        &mut self,
        comments: &[RenderComment],
        request: RenderRequest,
    ) -> Vec<PositionedComment> {
        let prepared = comments
            .iter()
            .filter_map(|comment| self.prepare_comment(comment).ok())
            .collect::<Vec<_>>();

        self.layout_prepared_comments(&prepared, request)
            .into_iter()
            .map(|item| PositionedComment {
                text: prepared[item.index].text.clone(),
                lane: item.lane,
                x: item.x,
                y: item.y,
                width: prepared[item.index].width,
                height: prepared[item.index].height,
                font_size: prepared[item.index].line_height / 1.2,
                color: [0xff, 0xff, 0xff, 0xff],
            })
            .collect()
    }

    fn layout_prepared_comments(
        &mut self,
        comments: &[PreparedComment],
        request: RenderRequest,
    ) -> Vec<PreparedLayoutItem> {
        let active = active_prepared_comments(comments, request.timestamp);
        let lane_padding = 8.0_f32;
        let side_padding = 24.0_f32;
        let horizontal_padding = 16.0_f32;
        let frame_width = request.frame_size.width as f32;
        let frame_height = request.frame_size.height as f32;
        let mut positioned = Vec::with_capacity(active.len());
        let mut lanes = HashMap::<CollisionGroup, LaneAllocator>::new();

        for item in active {
            let comment = item.comment;
            let lane_height = comment.line_height + lane_padding;
            let progress = (request.timestamp.0.saturating_sub(comment.vpos_ms) as f32)
                / comment.lifetime_ms as f32;
            let base_x = match comment.placement {
                CommentPlacement::Scroll => {
                    frame_width - progress * (frame_width + comment.width) - side_padding
                }
                CommentPlacement::Top | CommentPlacement::Bottom => {
                    ((frame_width - comment.width) / 2.0).max(side_padding)
                }
            };
            let span = base_x..(base_x + comment.width);
            let group = CollisionGroup {
                placement: comment.placement,
                owner: comment.owner,
                layer: comment.layer,
            };
            let lane_allocator = lanes.entry(group).or_insert_with(|| {
                LaneAllocator::new(comment.placement, frame_height, 24.0, horizontal_padding)
            });
            let (lane, y) = lane_allocator
                .place(span.clone(), lane_height)
                .unwrap_or_else(|| {
                    let fallback_lane = lane_allocator.fallback_lane();
                    let y = lane_allocator.lanes[fallback_lane].y;
                    lane_allocator.lanes[fallback_lane].spans.push(span.clone());
                    (fallback_lane, y)
                });

            positioned.push(PreparedLayoutItem {
                index: item.index,
                lane,
                x: base_x.max(-comment.width),
                y,
            });
        }

        positioned
    }

    fn prepare_comment(&mut self, comment: &RenderComment) -> Result<PreparedComment, RenderError> {
        let style = crate::timeline::resolve_style(comment);
        let font_size = match style.size {
            CommentSize::Small => self.config.medium_font_size * 0.72,
            CommentSize::Medium => self.config.medium_font_size,
            CommentSize::Big => self.config.medium_font_size * 1.45,
        };
        let metrics = self.font_context.measure_text(&comment.text, font_size);
        let sprite = self.font_context.render_text_sprite(
            &comment.text,
            font_size,
            [style.color.r, style.color.g, style.color.b, style.color.a],
            true,
        )?;

        Ok(PreparedComment {
            text: comment.text.clone(),
            vpos_ms: comment.vpos_ms,
            owner: comment.owner,
            layer: comment.layer,
            placement: style.placement,
            lifetime_ms: style.lifetime_ms,
            sprite,
            width: metrics.width,
            height: metrics.height,
            line_height: metrics.line_height,
        })
    }
}

fn active_prepared_comments<'a>(
    comments: &'a [PreparedComment],
    timestamp: TimestampMs,
) -> Vec<IndexedPreparedComment<'a>> {
    let mut active = comments
        .iter()
        .enumerate()
        .filter(|comment| {
            let end = comment.1.vpos_ms.saturating_add(comment.1.lifetime_ms);
            comment.1.vpos_ms <= timestamp.0
                && timestamp.0 < end
                && !comment.1.text.trim().is_empty()
        })
        .map(|(index, comment)| IndexedPreparedComment { index, comment })
        .collect::<Vec<_>>();

    active.sort_by(|left, right| {
        left.comment
            .vpos_ms
            .cmp(&right.comment.vpos_ms)
            .then_with(|| left.comment.layer.cmp(&right.comment.layer))
            .then_with(|| left.comment.text.cmp(&right.comment.text))
    });

    active
}

struct IndexedPreparedComment<'a> {
    index: usize,
    comment: &'a PreparedComment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CollisionGroup {
    placement: CommentPlacement,
    owner: bool,
    layer: i32,
}

#[derive(Debug)]
struct LaneAllocator {
    placement: CommentPlacement,
    frame_height: f32,
    margin: f32,
    horizontal_padding: f32,
    lanes: Vec<LaneState>,
}

impl LaneAllocator {
    fn new(
        placement: CommentPlacement,
        frame_height: f32,
        margin: f32,
        horizontal_padding: f32,
    ) -> Self {
        Self {
            placement,
            frame_height,
            margin,
            horizontal_padding,
            lanes: Vec::new(),
        }
    }

    fn place(&mut self, span: Range<f32>, lane_height: f32) -> Option<(usize, f32)> {
        for (index, lane) in self.lanes.iter_mut().enumerate() {
            if !lane.overlaps(&span, self.horizontal_padding) {
                lane.spans.push(span);
                return Some((index, lane.y));
            }
        }

        let y = self.next_lane_y(lane_height)?;
        let lane = LaneState {
            y,
            spans: vec![span],
        };
        self.lanes.push(lane);
        Some((self.lanes.len() - 1, y))
    }

    fn next_lane_y(&self, lane_height: f32) -> Option<f32> {
        let y = match self.placement {
            CommentPlacement::Scroll | CommentPlacement::Top => self
                .lanes
                .last()
                .map(|lane| lane.y + lane_height)
                .unwrap_or(self.margin),
            CommentPlacement::Bottom => self
                .lanes
                .last()
                .map(|lane| lane.y - lane_height)
                .unwrap_or(self.frame_height - self.margin - lane_height),
        };

        if y < 0.0 || y + lane_height > self.frame_height {
            None
        } else {
            Some(y)
        }
    }

    fn fallback_lane(&self) -> usize {
        0
    }
}

#[derive(Debug)]
struct LaneState {
    y: f32,
    spans: Vec<Range<f32>>,
}

impl LaneState {
    fn overlaps(&self, span: &Range<f32>, padding: f32) -> bool {
        self.spans.iter().any(|existing| {
            span.start < existing.end + padding && span.end + padding > existing.start
        })
    }
}

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("failed to load font `{path}`: {message}")]
    FontLoad { path: PathBuf, message: String },

    #[error("failed to allocate {width}x{height} render surface")]
    PixmapAllocation { width: u32, height: u32 },

    #[error("failed to encode png `{path}`: {message}")]
    PngEncode { path: PathBuf, message: String },
}

#[cfg(test)]
mod tests {
    use crate::{
        engine::{RenderConfig, RenderEngine, RenderRequest},
        timeline::{RenderComment, TimestampMs},
    };

    #[test]
    fn renders_visible_pixels_for_active_comments() {
        let mut engine = RenderEngine::new(RenderConfig::default()).unwrap();
        let comments = vec![
            RenderComment {
                text: "hello".to_string(),
                vpos_ms: 1_000,
                mail: vec!["ue".to_string(), "red".to_string()],
                owner: false,
                layer: 1,
            },
            RenderComment {
                text: "world".to_string(),
                vpos_ms: 1_200,
                mail: Vec::new(),
                owner: false,
                layer: 1,
            },
        ];

        let frame = engine
            .render_frame(
                &comments,
                RenderRequest {
                    timestamp: TimestampMs(1_500),
                    frame_size: RenderConfig::default().frame_size,
                },
            )
            .unwrap();

        assert_eq!(frame.width(), 1280);
        assert_eq!(frame.height(), 720);
        assert!(frame.rgba().chunks_exact(4).any(|pixel| pixel[3] != 0));
    }

    #[test]
    fn reuses_lane_for_non_overlapping_scroll_comments() {
        let mut engine = RenderEngine::new(RenderConfig::default()).unwrap();
        let positioned = engine.layout_comments(
            &[
                RenderComment {
                    text: "earlier comment".to_string(),
                    vpos_ms: 0,
                    mail: Vec::new(),
                    owner: false,
                    layer: 1,
                },
                RenderComment {
                    text: "later comment".to_string(),
                    vpos_ms: 1_500,
                    mail: Vec::new(),
                    owner: false,
                    layer: 1,
                },
            ],
            RenderRequest {
                timestamp: TimestampMs(2_000),
                frame_size: RenderConfig::default().frame_size,
            },
        );

        assert_eq!(positioned.len(), 2);
        assert_eq!(positioned[0].lane, 0);
        assert_eq!(positioned[1].lane, 0);
        assert!(positioned[0].x + positioned[0].width < positioned[1].x);
    }

    #[test]
    fn separates_overlapping_scroll_comments_into_different_lanes() {
        let mut engine = RenderEngine::new(RenderConfig::default()).unwrap();
        let positioned = engine.layout_comments(
            &[
                RenderComment {
                    text: "first".to_string(),
                    vpos_ms: 1_000,
                    mail: Vec::new(),
                    owner: false,
                    layer: 1,
                },
                RenderComment {
                    text: "second".to_string(),
                    vpos_ms: 1_100,
                    mail: Vec::new(),
                    owner: false,
                    layer: 1,
                },
            ],
            RenderRequest {
                timestamp: TimestampMs(1_200),
                frame_size: RenderConfig::default().frame_size,
            },
        );

        assert_eq!(positioned.len(), 2);
        assert_eq!(positioned[0].lane, 0);
        assert_eq!(positioned[1].lane, 1);
        assert!(positioned[1].y > positioned[0].y);
    }
}
