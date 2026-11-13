//! SFX LAB — Minimal Studio tokens and compact paint helpers.
//!
//! Dark near-black surfaces, one lime accent, hairline borders, and a darker
//! "playback" surface so the waveform stays the visual anchor.

use igui_core::{Color, Rect, Vec2};
use igui_render::{PaintContext, TextAlign};
use igui_svg::SvgDocument;
use igui_ui::TextMeasurer;

pub const BG: Color = hex(0x141714);
pub const PANEL: Color = hex(0x1C201C);
pub const RAISED: Color = hex(0x242A24);
pub const BORDER: Color = hex(0x303830);
pub const BORDER_HI: Color = hex(0x44503F);
pub const TEXT: Color = hex(0xE6ECE3);
pub const DIM: Color = hex(0x9AA393);
pub const FAINT: Color = hex(0x6E776A);
pub const ACCENT: Color = hex(0xA8CE86);
pub const ACCENT_DARK: Color = hex(0x7FA35F);
/// Muted accent washes for active states: a hint of color instead of a solid
/// brand block, so the waveform and timeline stay the visual anchors.
pub const ACCENT_TINT: Color = Color::new(0.659, 0.808, 0.525, 0.14);
pub const ACCENT_LINE: Color = Color::new(0.659, 0.808, 0.525, 0.5);
pub const ACCENT2: Color = hex(0x8DBBEA);
pub const WARN: Color = hex(0xE4B45A);
pub const HOVER: Color = hex(0x232A22);
pub const ACTIVE: Color = hex(0x2C3529);
pub const DARK: Color = hex(0x0D100D);
pub const DARK_DIM: Color = hex(0x8F9A8A);
/// Slightly lifted surface for the timeline body so clips read brighter.
pub const TRACK_BG: Color = hex(0x212621);

pub const VIEW_W: f32 = 1440.0;
pub const VIEW_H: f32 = 900.0;
pub const FONT: f32 = 12.0;
pub const SMALL: f32 = 10.0;
pub const TITLE: f32 = 13.0;
pub const TOP_H: f32 = 32.0;
pub const STATUS_H: f32 = 24.0;
pub const LEFT_W: f32 = 240.0;
pub const RIGHT_W: f32 = 360.0;
pub const HEADER_H: f32 = 34.0;
pub const AUTOMATION_H: f32 = 62.0;

pub const fn hex(v: u32) -> Color {
    Color::new(
        ((v >> 16) & 0xFF) as f32 / 255.0,
        ((v >> 8) & 0xFF) as f32 / 255.0,
        (v & 0xFF) as f32 / 255.0,
        1.0,
    )
}

pub fn fill(ctx: &mut PaintContext, rect: Rect, color: Color) {
    if rect.size.width > 0.0 && rect.size.height > 0.0 {
        ctx.fill_rect(rect, color);
    }
}

pub fn stroke(ctx: &mut PaintContext, rect: Rect, color: Color) {
    ctx.stroke_rect(rect, 1.0, color);
}

pub fn hline(ctx: &mut PaintContext, x0: f32, x1: f32, y: f32, color: Color) {
    ctx.draw_line(Vec2::new(x0, y), Vec2::new(x1, y), 1.0, color);
}

pub fn vline(ctx: &mut PaintContext, x: f32, y0: f32, y1: f32, color: Color) {
    ctx.draw_line(Vec2::new(x, y0), Vec2::new(x, y1), 1.0, color);
}

pub fn baseline(measurer: &dyn TextMeasurer, rect: Rect, size: f32) -> f32 {
    rect.center().y + measurer.ascent(size) * 0.5 - measurer.line_height(size) * 0.1
}

pub fn text(ctx: &mut PaintContext, text: &str, position: Vec2, size: f32, color: Color) {
    ctx.draw_text(text, position, size, TextAlign::Left, color);
}

pub fn text_top(
    ctx: &mut PaintContext,
    measurer: &dyn TextMeasurer,
    label: &str,
    x: f32,
    top: f32,
    size: f32,
    color: Color,
) {
    let y = top + measurer.ascent(size);
    text(ctx, label, Vec2::new(x, y), size, color);
}

pub fn text_center(
    ctx: &mut PaintContext,
    measurer: &dyn TextMeasurer,
    label: &str,
    rect: Rect,
    size: f32,
    color: Color,
) {
    let width = measurer.measure_run(label, size);
    let x = rect.center().x - width * 0.5;
    let y = baseline(measurer, rect, size);
    text(ctx, label, Vec2::new(x, y), size, color);
}

pub fn text_right(
    ctx: &mut PaintContext,
    measurer: &dyn TextMeasurer,
    label: &str,
    right: f32,
    center_y: f32,
    size: f32,
    color: Color,
) {
    let width = measurer.measure_run(label, size);
    let y = center_y + measurer.ascent(size) * 0.5 - measurer.line_height(size) * 0.1;
    text(ctx, label, Vec2::new(right - width, y), size, color);
}

pub fn ellipsis(measurer: &dyn TextMeasurer, text: &str, size: f32, width: f32) -> String {
    if measurer.measure_run(text, size) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let next = format!("{out}{ch}…");
        if measurer.measure_run(&next, size) > width {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

pub fn button(
    ctx: &mut PaintContext,
    measurer: &dyn TextMeasurer,
    rect: Rect,
    label: &str,
    hovered: bool,
    active: bool,
) {
    button_icon(ctx, measurer, rect, label, None, hovered, active);
}

pub fn button_icon(
    ctx: &mut PaintContext,
    measurer: &dyn TextMeasurer,
    rect: Rect,
    label: &str,
    icon: Option<&SvgDocument>,
    hovered: bool,
    active: bool,
) {
    let bg = if active {
        ACCENT_TINT
    } else if hovered {
        HOVER
    } else {
        RAISED
    };
    ctx.fill_rounded_rect(rect, 6.0, bg);
    ctx.stroke_rounded_rect(
        rect,
        6.0,
        1.0,
        if active {
            ACCENT_LINE
        } else if hovered {
            BORDER_HI
        } else {
            BORDER
        },
    );
    let fg = if active { ACCENT } else { TEXT };
    if let Some(icon) = icon {
        let side = (rect.size.height - 8.0).clamp(10.0, 14.0);
        let label_w = if label.is_empty() {
            0.0
        } else {
            measurer.measure_run(label, SMALL)
        };
        let gap = if label.is_empty() { 0.0 } else { 5.0 };
        // Center icon + label as one group so wide buttons do not leave a
        // lonely glyph at the far left.
        let start = rect.center().x - (side + gap + label_w) * 0.5;
        let icon_rect = Rect::from_min_size(
            Vec2::new(start, rect.center().y - side * 0.5),
            igui_core::Size::splat(side),
        );
        icon.draw(ctx, icon_rect, if active { ACCENT } else { DIM });
        if !label.is_empty() {
            text(
                ctx,
                label,
                Vec2::new(start + side + gap, baseline(measurer, rect, SMALL)),
                SMALL,
                fg,
            );
        }
    } else {
        text_center(ctx, measurer, label, rect, FONT, fg);
    }
}
