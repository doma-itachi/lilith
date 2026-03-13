use std::collections::HashMap;
use std::path::PathBuf;

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
    pub end_ms: u64,
    pub owner: bool,
    pub layer: i32,
    pub placement: CommentPlacement,
    pub lifetime_ms: u64,
    pub lane: usize,
    pub y: f32,
    pub sprite: tiny_skia::Pixmap,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub line_height: f32,
}

#[derive(Debug, Clone)]
pub struct PreparedCommentSet {
    comments: Vec<PreparedComment>,
}

pub struct PreparedFrameSequence<'a> {
    comments: &'a [PreparedComment],
    next_index: usize,
    active_indices: Vec<usize>,
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
        let mut prepared = comments
            .iter()
            .map(|comment| self.prepare_comment(comment))
            .collect::<Result<Vec<_>, _>>()?;

        prepared.sort_by(|left, right| {
            left.vpos_ms
                .cmp(&right.vpos_ms)
                .then_with(|| left.layer.cmp(&right.layer))
                .then_with(|| left.text.cmp(&right.text))
        });
        self.assign_lanes(&mut prepared);

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
        let mut sequence = comments.sequence();
        self.render_prepared_frame_with_sequence(&mut sequence, request)
    }

    pub fn render_prepared_frame_with_sequence(
        &mut self,
        sequence: &mut PreparedFrameSequence<'_>,
        request: RenderRequest,
    ) -> Result<RenderFrame, RenderError> {
        let mut pixmap = Pixmap::new(request.frame_size.width, request.frame_size.height).ok_or(
            RenderError::PixmapAllocation {
                width: request.frame_size.width,
                height: request.frame_size.height,
            },
        )?;
        let active = sequence.advance_to(request.timestamp);
        let draw_items = self.layout_prepared_comments(&active, request);

        for item in draw_items {
            let mut paint = tiny_skia::PixmapPaint::default();
            paint.opacity = 1.0;
            pixmap.draw_pixmap(
                item.x.round() as i32,
                item.y.round() as i32,
                sequence.comment(item.index).sprite.as_ref(),
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
        let mut prepared = comments
            .iter()
            .filter_map(|comment| self.prepare_comment(comment).ok())
            .collect::<Vec<_>>();
        prepared.sort_by(|left, right| {
            left.vpos_ms
                .cmp(&right.vpos_ms)
                .then_with(|| left.layer.cmp(&right.layer))
                .then_with(|| left.text.cmp(&right.text))
        });
        self.assign_lanes(&mut prepared);

        let active = prepared
            .iter()
            .enumerate()
            .map(|(index, comment)| IndexedPreparedComment { index, comment })
            .collect::<Vec<_>>();

        self.layout_prepared_comments(&active, request)
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
        comments: &[IndexedPreparedComment<'_>],
        request: RenderRequest,
    ) -> Vec<PreparedLayoutItem> {
        let side_padding = 24.0_f32;
        let frame_width = request.frame_size.width as f32;
        let mut positioned = Vec::with_capacity(comments.len());

        for item in comments {
            let comment = item.comment;
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

            positioned.push(PreparedLayoutItem {
                index: item.index,
                lane: comment.lane,
                x: base_x.max(-comment.width),
                y: comment.y,
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
            end_ms: comment.vpos_ms.saturating_add(style.lifetime_ms),
            owner: comment.owner,
            layer: comment.layer,
            placement: style.placement,
            lifetime_ms: style.lifetime_ms,
            lane: 0,
            y: 0.0,
            sprite,
            width: metrics.width,
            height: metrics.height,
            font_size,
            line_height: metrics.line_height,
        })
    }

    fn assign_lanes(&self, comments: &mut [PreparedComment]) {
        let frame_height = self.config.frame_size.height as f32;
        let lane_step = self.max_lane_step();
        let margin = 24.0_f32;
        let mut groups = HashMap::<CollisionGroup, LaneScheduler>::new();

        for comment in comments {
            let group = CollisionGroup {
                placement: comment.placement,
                owner: comment.owner,
                layer: comment.layer,
            };
            let scheduler = groups.entry(group).or_insert_with(|| {
                LaneScheduler::new(
                    comment.placement,
                    frame_height,
                    margin,
                    lane_step,
                    self.config.frame_size.width as f32,
                )
            });
            let assignment = scheduler.assign(comment);
            comment.lane = assignment.lane;
            comment.y = assignment.y;
        }
    }

    fn max_lane_step(&self) -> f32 {
        self.config.medium_font_size * 1.45 * 1.2 + 8.0
    }
}

#[derive(Clone, Copy)]
pub struct IndexedPreparedComment<'a> {
    index: usize,
    comment: &'a PreparedComment,
}

impl IndexedPreparedComment<'_> {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn comment(&self) -> &PreparedComment {
        self.comment
    }
}

impl PreparedCommentSet {
    pub fn sequence(&self) -> PreparedFrameSequence<'_> {
        PreparedFrameSequence {
            comments: &self.comments,
            next_index: 0,
            active_indices: Vec::new(),
        }
    }
}

impl PreparedFrameSequence<'_> {
    pub fn advance_to(&mut self, timestamp: TimestampMs) -> Vec<IndexedPreparedComment<'_>> {
        while self.next_index < self.comments.len()
            && self.comments[self.next_index].vpos_ms <= timestamp.0
        {
            self.active_indices.push(self.next_index);
            self.next_index += 1;
        }

        self.active_indices
            .retain(|index| timestamp.0 < self.comments[*index].end_ms);

        self.active_indices
            .iter()
            .copied()
            .filter(|index| !self.comments[*index].text.trim().is_empty())
            .map(|index| IndexedPreparedComment {
                index,
                comment: &self.comments[index],
            })
            .collect()
    }

    pub fn comment(&self, index: usize) -> &PreparedComment {
        &self.comments[index]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct CollisionGroup {
    placement: CommentPlacement,
    owner: bool,
    layer: i32,
}

#[derive(Debug)]
struct LaneAssignment {
    lane: usize,
    y: f32,
}

struct LaneScheduler {
    placement: CommentPlacement,
    frame_height: f32,
    margin: f32,
    lane_step: f32,
    frame_width: f32,
    lanes: Vec<ScheduledLane>,
}

impl LaneScheduler {
    fn new(
        placement: CommentPlacement,
        frame_height: f32,
        margin: f32,
        lane_step: f32,
        frame_width: f32,
    ) -> Self {
        Self {
            placement,
            frame_height,
            margin,
            lane_step,
            frame_width,
            lanes: Vec::new(),
        }
    }

    fn assign(&mut self, comment: &PreparedComment) -> LaneAssignment {
        for (lane_index, lane) in self.lanes.iter_mut().enumerate() {
            if lane.can_place(comment, self.placement, self.frame_width) {
                lane.push(comment, self.placement, self.frame_width);
                return LaneAssignment {
                    lane: lane_index,
                    y: self.lane_y(lane_index),
                };
            }
        }

        let lane_index = self.lanes.len();
        if self.lane_y(lane_index) + self.lane_step > self.frame_height
            && self.placement != CommentPlacement::Bottom
        {
            return LaneAssignment {
                lane: 0,
                y: self.lane_y(0),
            };
        }
        self.lanes.push(ScheduledLane::new(
            comment,
            self.placement,
            self.frame_width,
        ));

        LaneAssignment {
            lane: lane_index,
            y: self.lane_y(lane_index),
        }
    }

    fn lane_y(&self, lane_index: usize) -> f32 {
        match self.placement {
            CommentPlacement::Scroll | CommentPlacement::Top => {
                self.margin + lane_index as f32 * self.lane_step
            }
            CommentPlacement::Bottom => {
                self.frame_height - self.margin - (lane_index as f32 + 1.0) * self.lane_step
            }
        }
    }
}

struct ScheduledLane {
    last_start_ms: u64,
    last_end_ms: u64,
    last_width: f32,
    last_lifetime_ms: u64,
}

impl ScheduledLane {
    fn new(comment: &PreparedComment, placement: CommentPlacement, frame_width: f32) -> Self {
        let mut lane = Self {
            last_start_ms: 0,
            last_end_ms: 0,
            last_width: 0.0,
            last_lifetime_ms: 0,
        };
        lane.push(comment, placement, frame_width);
        lane
    }

    fn can_place(
        &self,
        comment: &PreparedComment,
        placement: CommentPlacement,
        frame_width: f32,
    ) -> bool {
        match placement {
            CommentPlacement::Top | CommentPlacement::Bottom => comment.vpos_ms >= self.last_end_ms,
            CommentPlacement::Scroll => self.can_place_scroll(comment, frame_width),
        }
    }

    fn push(&mut self, comment: &PreparedComment, _placement: CommentPlacement, _frame_width: f32) {
        self.last_start_ms = comment.vpos_ms;
        self.last_end_ms = comment.end_ms;
        self.last_width = comment.width;
        self.last_lifetime_ms = comment.lifetime_ms;
    }

    fn can_place_scroll(&self, comment: &PreparedComment, frame_width: f32) -> bool {
        if comment.vpos_ms >= self.last_end_ms {
            return true;
        }

        let overlap_window = self.last_start_ms.saturating_add(1_000);
        if comment.vpos_ms < overlap_window {
            return false;
        }

        let side_padding = 24.0_f32;
        let horizontal_padding = 16.0_f32;
        let previous_speed = (frame_width + self.last_width) / self.last_lifetime_ms as f32;
        let current_speed = (frame_width + comment.width) / comment.lifetime_ms as f32;
        let elapsed = comment.vpos_ms.saturating_sub(self.last_start_ms) as f32;
        let previous_x = frame_width
            - side_padding
            - (elapsed / self.last_lifetime_ms as f32) * (frame_width + self.last_width);
        let gap =
            (frame_width - side_padding) - (previous_x + self.last_width + horizontal_padding);

        if gap < 0.0 {
            return false;
        }
        if current_speed <= previous_speed {
            return true;
        }

        let catch_time = gap / (current_speed - previous_speed);
        comment.vpos_ms as f32 + catch_time >= self.last_end_ms as f32
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
    fn sequential_frame_sequence_matches_activation_order() {
        let mut engine = RenderEngine::new(RenderConfig::default()).unwrap();
        let prepared = engine
            .prepare_comments(&[
                RenderComment {
                    text: "a".to_string(),
                    vpos_ms: 0,
                    mail: Vec::new(),
                    owner: false,
                    layer: 1,
                },
                RenderComment {
                    text: "b".to_string(),
                    vpos_ms: 500,
                    mail: Vec::new(),
                    owner: false,
                    layer: 1,
                },
            ])
            .unwrap();
        let mut sequence = prepared.sequence();

        let first = sequence
            .advance_to(TimestampMs(0))
            .into_iter()
            .map(|item| item.comment().text.clone())
            .collect::<Vec<_>>();
        let second = sequence
            .advance_to(TimestampMs(600))
            .into_iter()
            .map(|item| item.comment().text.clone())
            .collect::<Vec<_>>();

        assert_eq!(first, vec!["a"]);
        assert_eq!(second, vec!["a", "b"]);
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
