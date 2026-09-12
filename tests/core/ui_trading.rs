use super::*;
use crate::core::simulation::{GameModel, GameRules};
use std::collections::BTreeMap;

fn trading_panel_frame(
    context: &egui::Context,
    world: &mut World,
    state: &mut UiState,
    model: &GameModel,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<MultiplayerRequest>,
        MessageWriter<MessageMsg>,
    )>::new(world);
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..default()
        },
        |_| {
            let (mut requests, mut messages) = params.get_mut(world).unwrap();
            draw_trade_panel(
                context,
                state,
                &model.map,
                &model.players[0],
                &MultiplayerSession::default(),
                &mut requests,
                &mut messages,
                &ImageIds::default(),
            );
        },
    );
    output.textures_delta.clear();
    output
}

fn panel_labels(output: &egui::FullOutput) -> BTreeMap<String, egui::Rect> {
    output
        .shapes
        .iter()
        .filter_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape {
                Some((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                ))
            } else {
                None
            }
        })
        .collect()
}

fn trading_panel_game() -> GameModel {
    let mut model = GameModel::new([31; 32], GameRules::default()).unwrap();
    let home = model.players[0].home_planet;
    let enemy = model.players[1].home_planet;
    model.map.get_mut(home).position = Vec2::ZERO;
    model.map.get_mut(enemy).position = Vec2::X * Planet::SIZE * 3.0;
    model.map.get_mut(enemy).army.insert(Unit::Building(Building::TradingPost), 1);
    model
}

#[test]
fn missing_trading_post_panel_explains_the_requirement_and_closes_at_small_sizes() {
    for size in [egui::vec2(1280.0, 800.0), egui::vec2(640.0, 480.0), egui::vec2(360.0, 640.0)] {
        let model = trading_panel_game();
        let enemy = model.players[1].home_planet;
        let mut state = UiState {
            trading_post_open: Some(enemy),
            ..default()
        };
        let context = egui::Context::default();
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<MessageMsg>>();
        trading_panel_frame(&context, &mut world, &mut state, &model, size, Vec::new());
        let output =
            trading_panel_frame(&context, &mut world, &mut state, &model, size, Vec::new());
        let labels = panel_labels(&output);
        let message =
            "You need a completed Trading Post of your own within 3 AU of this post to trade.";
        assert!(labels.contains_key(message));
        assert!(!labels.contains_key("Send offer"));
        assert!(!labels.contains_key("You offer"));
        assert_eq!(state.trading_post_open, Some(enemy));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        for text in ["Trading Post", message, "Close"] {
            assert!(screen.contains_rect(labels[text]), "{text} must fit at {size:?}");
        }
        assert!(labels["Trading Post"].bottom() < labels[message].top());
        assert!(labels[message].bottom() < labels["Close"].top());

        let position = labels["Close"].center();
        for pressed in [true, false] {
            trading_panel_frame(
                &context,
                &mut world,
                &mut state,
                &model,
                size,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: default(),
                    },
                ],
            );
        }
        assert_eq!(state.trading_post_open, None);
        assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    }
}

#[test]
fn trading_panel_requires_a_completed_owned_post_within_three_au() {
    let mut model = trading_panel_game();
    let home = model.players[0].home_planet;
    let enemy = model.players[1].home_planet;
    let mut state = UiState {
        trading_post_open: Some(enemy),
        ..default()
    };
    let context = egui::Context::default();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    for (distance, completed, owned, can_trade) in [
        (3.0, false, true, false),
        (3.01, true, true, false),
        (3.0, true, false, false),
        (3.0, true, true, true),
    ] {
        model.map.get_mut(enemy).position = Vec2::X * Planet::SIZE * distance;
        let local = model.map.get_mut(home);
        local.owned = owned.then_some(1);
        local.army.insert(Unit::Building(Building::TradingPost), usize::from(completed));
        local.buy = if completed {
            Vec::new()
        } else {
            vec![Unit::Building(Building::TradingPost)]
        };
        trading_panel_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            egui::vec2(640.0, 480.0),
            Vec::new(),
        );
        let output = trading_panel_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            egui::vec2(640.0, 480.0),
            Vec::new(),
        );
        let labels = panel_labels(&output);
        assert_eq!(labels.contains_key("You offer"), can_trade);
        assert_eq!(labels.contains_key("Send offer"), can_trade);
        assert_eq!(
            labels.keys().any(|text| text.starts_with("You need a completed Trading Post")),
            !can_trade
        );
        if can_trade {
            assert!(labels["Trading Post"].bottom() < labels["You offer"].top());
        }
        assert_eq!(state.trading_post_open, Some(enemy));
    }
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
}
