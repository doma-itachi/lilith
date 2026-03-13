#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimestampMs(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderComment {
    pub text: String,
    pub vpos_ms: u64,
    pub mail: Vec<String>,
    pub owner: bool,
    pub layer: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommentPlacement {
    Scroll,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentSize {
    Small,
    Medium,
    Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentStyle {
    pub placement: CommentPlacement,
    pub size: CommentSize,
    pub color: CommentColor,
    pub lifetime_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ActiveComment<'a> {
    pub comment: &'a RenderComment,
    pub style: CommentStyle,
}

pub fn active_comments<'a>(
    comments: &'a [RenderComment],
    timestamp: TimestampMs,
) -> Vec<ActiveComment<'a>> {
    let mut active = comments
        .iter()
        .filter_map(|comment| {
            let style = resolve_style(comment);
            if is_active(comment, timestamp, style) {
                Some(ActiveComment { comment, style })
            } else {
                None
            }
        })
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

pub fn resolve_style(comment: &RenderComment) -> CommentStyle {
    let mut placement = CommentPlacement::Scroll;
    let mut size = CommentSize::Medium;
    let mut color = CommentColor {
        r: 0xff,
        g: 0xff,
        b: 0xff,
        a: 0xff,
    };

    for command in comment.mail.iter().map(|item| item.as_str()) {
        match command {
            "ue" => placement = CommentPlacement::Top,
            "shita" => placement = CommentPlacement::Bottom,
            "big" => size = CommentSize::Big,
            "small" => size = CommentSize::Small,
            "red" => color = rgb(0xff, 0x44, 0x44),
            "pink" => color = rgb(0xff, 0x80, 0xbf),
            "orange" => color = rgb(0xff, 0x99, 0x33),
            "yellow" => color = rgb(0xff, 0xee, 0x58),
            "green" => color = rgb(0x7c, 0xe3, 0x73),
            "cyan" => color = rgb(0x56, 0xe0, 0xff),
            "blue" => color = rgb(0x6f, 0x95, 0xff),
            "purple" => color = rgb(0xc0, 0x7b, 0xff),
            "black" => color = rgb(0x22, 0x22, 0x22),
            "white" => color = rgb(0xff, 0xff, 0xff),
            _ => {}
        }
    }

    CommentStyle {
        placement,
        size,
        color,
        lifetime_ms: 3_000,
    }
}

fn is_active(comment: &RenderComment, timestamp: TimestampMs, style: CommentStyle) -> bool {
    if comment.text.trim().is_empty() || comment.mail.iter().any(|item| item == "invisible") {
        return false;
    }

    let end = comment.vpos_ms.saturating_add(style.lifetime_ms);
    comment.vpos_ms <= timestamp.0 && timestamp.0 < end
}

const fn rgb(r: u8, g: u8, b: u8) -> CommentColor {
    CommentColor { r, g, b, a: 0xff }
}

#[cfg(test)]
mod tests {
    use super::{
        active_comments, resolve_style, CommentPlacement, CommentSize, RenderComment, TimestampMs,
    };

    #[test]
    fn resolves_basic_mail_commands() {
        let comment = RenderComment {
            text: "hello".to_string(),
            vpos_ms: 100,
            mail: vec!["ue".to_string(), "big".to_string(), "red".to_string()],
            owner: false,
            layer: 1,
        };

        let style = resolve_style(&comment);

        assert_eq!(style.placement, CommentPlacement::Top);
        assert_eq!(style.size, CommentSize::Big);
        assert_eq!(style.color.r, 0xff);
        assert_eq!(style.color.g, 0x44);
    }

    #[test]
    fn filters_only_active_visible_comments() {
        let comments = vec![
            RenderComment {
                text: "visible".to_string(),
                vpos_ms: 1_000,
                mail: Vec::new(),
                owner: false,
                layer: 1,
            },
            RenderComment {
                text: "hidden".to_string(),
                vpos_ms: 1_000,
                mail: vec!["invisible".to_string()],
                owner: true,
                layer: 0,
            },
            RenderComment {
                text: "future".to_string(),
                vpos_ms: 5_000,
                mail: Vec::new(),
                owner: false,
                layer: 1,
            },
        ];

        let active = active_comments(&comments, TimestampMs(1_500));

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].comment.text, "visible");
    }
}
