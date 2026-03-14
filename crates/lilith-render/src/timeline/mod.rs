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
    let mut lifetime_ms = 3_000_u64;
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
            _ if matches_color_command(command) => color = resolve_color(command),
            _ if command.starts_with('@') => {
                if let Some(seconds) = parse_long_seconds(command) {
                    lifetime_ms = seconds;
                }
            }
            _ => {}
        }
    }

    CommentStyle {
        placement,
        size,
        color,
        lifetime_ms,
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

fn matches_color_command(command: &str) -> bool {
    matches!(
        command,
        "white"
            | "red"
            | "pink"
            | "orange"
            | "yellow"
            | "green"
            | "cyan"
            | "blue"
            | "purple"
            | "black"
            | "white2"
            | "niconicowhite"
            | "red2"
            | "truered"
            | "pink2"
            | "orange2"
            | "passionorange"
            | "yellow2"
            | "madyellow"
            | "green2"
            | "elementalgreen"
            | "cyan2"
            | "blue2"
            | "marinblue"
            | "purple2"
            | "nobleviolet"
            | "black2"
    )
}

fn resolve_color(command: &str) -> CommentColor {
    match command {
        "white" => rgb(0xff, 0xff, 0xff),
        "red" => rgb(0xff, 0x00, 0x00),
        "pink" => rgb(0xff, 0x80, 0x80),
        "orange" => rgb(0xff, 0xc0, 0x00),
        "yellow" => rgb(0xff, 0xff, 0x00),
        "green" => rgb(0x00, 0xff, 0x00),
        "cyan" => rgb(0x00, 0xff, 0xff),
        "blue" => rgb(0x00, 0x00, 0xff),
        "purple" => rgb(0xc0, 0x00, 0xff),
        "black" => rgb(0x00, 0x00, 0x00),
        "white2" | "niconicowhite" => rgb(0xcc, 0xcc, 0x99),
        "red2" | "truered" => rgb(0xcc, 0x00, 0x33),
        "pink2" => rgb(0xff, 0x33, 0xcc),
        "orange2" | "passionorange" => rgb(0xff, 0x66, 0x00),
        "yellow2" | "madyellow" => rgb(0x99, 0x99, 0x00),
        "green2" | "elementalgreen" => rgb(0x00, 0xcc, 0x66),
        "cyan2" => rgb(0x00, 0xcc, 0xcc),
        "blue2" | "marinblue" => rgb(0x33, 0x99, 0xff),
        "purple2" | "nobleviolet" => rgb(0x66, 0x33, 0xcc),
        "black2" => rgb(0x66, 0x66, 0x66),
        _ => rgb(0xff, 0xff, 0xff),
    }
}

fn parse_long_seconds(command: &str) -> Option<u64> {
    let seconds = command.strip_prefix('@')?.parse::<f32>().ok()?;
    if seconds.is_sign_negative() {
        return None;
    }

    Some((seconds * 1_000.0).floor() as u64)
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
        assert_eq!(style.color.g, 0x00);
        assert_eq!(style.color.b, 0x00);
    }

    #[test]
    fn resolves_extended_niconico_color_commands() {
        let comment = RenderComment {
            text: "hello".to_string(),
            vpos_ms: 100,
            mail: vec!["niconicowhite".to_string(), "passionorange".to_string()],
            owner: false,
            layer: 1,
        };

        let style = resolve_style(&comment);

        assert_eq!(style.color.r, 0xff);
        assert_eq!(style.color.g, 0x66);
        assert_eq!(style.color.b, 0x00);
    }

    #[test]
    fn resolves_aliases_to_same_color() {
        let left = RenderComment {
            text: "left".to_string(),
            vpos_ms: 0,
            mail: vec!["truered".to_string()],
            owner: false,
            layer: 0,
        };
        let right = RenderComment {
            text: "right".to_string(),
            vpos_ms: 0,
            mail: vec!["red2".to_string()],
            owner: false,
            layer: 0,
        };

        assert_eq!(resolve_style(&left).color, resolve_style(&right).color);
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

    #[test]
    fn parses_long_command_into_lifetime() {
        let comment = RenderComment {
            text: "slow".to_string(),
            vpos_ms: 0,
            mail: vec!["@5.5".to_string()],
            owner: false,
            layer: 1,
        };

        let style = resolve_style(&comment);

        assert_eq!(style.lifetime_ms, 5_500);
    }
}
