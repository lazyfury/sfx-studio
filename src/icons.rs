//! Small Lucide-style icon set copied from `classic-game-box/assets/icons`.
//!
//! `igui_svg` flattens the paths and strokes them into the backend-neutral draw
//! list, so icons stay vector-crisp and take the current theme colour.

use std::sync::OnceLock;

use igui_core::{Color, Rect, Size, Vec2};
use igui_render::PaintContext;
use igui_svg::SvgDocument;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Library,
    Sparkles,
    Star,
    Save,
    Settings,
    FolderSearch,
    Trash,
    Pencil,
    LayoutGrid,
    ImagePlus,
    Tag,
    ChevronDown,
    Check,
    X,
    ArrowUp,
    ArrowDown,
    ChevronRight,
    ChevronLeft,
    Camera,
    Pin,
}

impl Icon {
    const ALL: [Icon; 20] = [
        Icon::Library,
        Icon::Sparkles,
        Icon::Star,
        Icon::Save,
        Icon::Settings,
        Icon::FolderSearch,
        Icon::Trash,
        Icon::Pencil,
        Icon::LayoutGrid,
        Icon::ImagePlus,
        Icon::Tag,
        Icon::ChevronDown,
        Icon::Check,
        Icon::X,
        Icon::ArrowUp,
        Icon::ArrowDown,
        Icon::ChevronRight,
        Icon::ChevronLeft,
        Icon::Camera,
        Icon::Pin,
    ];

    fn svg(self) -> &'static str {
        match self {
            Icon::Library => include_str!("../assets/icons/library.svg"),
            Icon::Sparkles => include_str!("../assets/icons/sparkles.svg"),
            Icon::Star => include_str!("../assets/icons/star.svg"),
            Icon::Save => include_str!("../assets/icons/save.svg"),
            Icon::Settings => include_str!("../assets/icons/settings-2.svg"),
            Icon::FolderSearch => include_str!("../assets/icons/folder-search.svg"),
            Icon::Trash => include_str!("../assets/icons/trash-2.svg"),
            Icon::Pencil => include_str!("../assets/icons/pencil.svg"),
            Icon::LayoutGrid => include_str!("../assets/icons/layout-grid.svg"),
            Icon::ImagePlus => include_str!("../assets/icons/image-plus.svg"),
            Icon::Tag => include_str!("../assets/icons/tag.svg"),
            Icon::ChevronDown => include_str!("../assets/icons/chevron-down.svg"),
            Icon::Check => include_str!("../assets/icons/check.svg"),
            Icon::X => include_str!("../assets/icons/x.svg"),
            Icon::ArrowUp => include_str!("../assets/icons/arrow-up.svg"),
            Icon::ArrowDown => include_str!("../assets/icons/arrow-down.svg"),
            Icon::ChevronRight => include_str!("../assets/icons/chevron-right.svg"),
            Icon::ChevronLeft => include_str!("../assets/icons/chevron-left.svg"),
            Icon::Camera => include_str!("../assets/icons/camera.svg"),
            Icon::Pin => include_str!("../assets/icons/pin.svg"),
        }
    }

    const fn index(self) -> usize {
        match self {
            Icon::Library => 0,
            Icon::Sparkles => 1,
            Icon::Star => 2,
            Icon::Save => 3,
            Icon::Settings => 4,
            Icon::FolderSearch => 5,
            Icon::Trash => 6,
            Icon::Pencil => 7,
            Icon::LayoutGrid => 8,
            Icon::ImagePlus => 9,
            Icon::Tag => 10,
            Icon::ChevronDown => 11,
            Icon::Check => 12,
            Icon::X => 13,
            Icon::ArrowUp => 14,
            Icon::ArrowDown => 15,
            Icon::ChevronRight => 16,
            Icon::ChevronLeft => 17,
            Icon::Camera => 18,
            Icon::Pin => 19,
        }
    }
}

pub fn document(icon: Icon) -> Option<&'static SvgDocument> {
    static CACHE: OnceLock<Vec<Option<SvgDocument>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        Icon::ALL
            .iter()
            .map(|icon| SvgDocument::parse(icon.svg()).ok())
            .collect()
    });
    cache[icon.index()].as_ref()
}

/// Paints `icon` centred in `rect` using `color` for `currentColor`.
pub fn paint(ctx: &mut PaintContext, icon: Icon, rect: Rect, color: Color) {
    let Some(document) = document(icon) else {
        return;
    };
    let side = rect.size.width.min(rect.size.height);
    if side <= 0.0 {
        return;
    }
    let target = Rect::from_min_size(
        Vec2::new(rect.center().x - side * 0.5, rect.center().y - side * 0.5),
        Size::new(side, side),
    );
    document.draw(ctx, target, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use igui_render::DrawCommand;

    #[test]
    fn every_icon_parses_and_draws() {
        for icon in Icon::ALL {
            let document = document(icon).expect("icon parses");
            let mut ctx = PaintContext::new();
            document.draw(
                &mut ctx,
                Rect::from_min_size(Vec2::ZERO, Size::splat(16.0)),
                Color::WHITE,
            );
            assert!(
                ctx.into_draw_list()
                    .commands()
                    .iter()
                    .any(|command| matches!(command, DrawCommand::Line { .. })),
                "{icon:?} drew no lines"
            );
        }
    }
}
