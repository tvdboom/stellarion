//! Legacy Bevy menu widget construction helpers retained by the styled UI.

use bevy::prelude::*;

use crate::core::assets::WorldAssets;
use crate::core::ui::systems::UiCmp;

#[derive(Component)]
/// Marker component for menu text whose font size follows window scale.
pub struct TextSize(pub f32);

/// Add a root UI node that covers the whole screen
pub fn add_root_node(block: bool) -> (Node, Pickable, ZIndex, UiCmp) {
    (
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(105.),
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            align_content: AlignContent::Center,
            align_items: AlignItems::Center,
            align_self: AlignSelf::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        if block {
            Pickable {
                should_block_lower: true,
                is_hoverable: false,
            }
        } else {
            Pickable::IGNORE
        },
        ZIndex(if block {
            4 // On top of end turn but below audio button
        } else {
            -1 // Below everything
        }),
        UiCmp,
    )
}

/// Add a standard text component
pub fn add_text(
    text: impl Into<String>,
    font: &str,
    font_size: f32,
    assets: &WorldAssets,
    window: &Window,
) -> (Text, TextFont, TextSize) {
    (
        Text::new(text),
        TextFont {
            font: assets.font(font).into(),
            font_size: (font_size * window.height() / 460.).into(),
            ..default()
        },
        TextSize(font_size),
    )
}
