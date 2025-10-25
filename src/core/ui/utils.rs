use std::collections::HashMap;

use bevy::prelude::Resource;
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::egui::*;

#[derive(Resource, Default)]
pub struct ImageIds(pub HashMap<&'static str, TextureId>);

impl ImageIds {
    pub fn get(&self, key: impl Into<String>) -> TextureId {
        let key = key.into().clone();
        *self.0.get(key.as_str()).expect(format!("No image found with name: {}", key).as_str())
    }
}

/// Custom IOS style toggle for UI
pub fn toggle(on: &mut bool) -> impl Widget + '_ {
    move |ui: &mut Ui| {
        let desired_size = ui.spacing().interact_size.y * Vec2::new(2.0, 1.0);
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }

        response
            .widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, ui.is_enabled(), *on, ""));

        if ui.is_rect_visible(rect) {
            let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
            let visuals = ui.style().interact_selectable(&response, *on);
            let rect = rect.expand(visuals.expansion);
            let radius = 0.5 * rect.height();
            ui.painter().rect(
                rect,
                radius,
                visuals.bg_fill,
                visuals.bg_stroke,
                StrokeKind::Outside,
            );
            let circle_x = lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
            let center = Pos2::new(circle_x, rect.center().y);
            ui.painter().circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
        }

        response
    }
}

pub trait CustomUi {
    fn add_image(&mut self, texture: impl Into<TextureId>, size: impl Into<Vec2>) -> Response;
    fn add_image_button(
        &mut self,
        texture: impl Into<TextureId>,
        size: impl Into<Vec2>,
    ) -> Response;
    fn add_image_painter(&mut self, image: TextureId, rect: Rect);

    fn cell<R>(&mut self, width: f32, add_contents: impl FnOnce(&mut Ui) -> R) -> R;
}

impl CustomUi for Ui {
    fn add_image(&mut self, texture: impl Into<TextureId>, size: impl Into<Vec2>) -> Response {
        self.add(Image::new(SizedTexture::new(texture, size)))
    }

    fn add_image_button(
        &mut self,
        texture: impl Into<TextureId>,
        size: impl Into<Vec2>,
    ) -> Response {
        self.add(Button::image(SizedTexture::new(texture, size)))
    }

    fn add_image_painter(&mut self, image: TextureId, rect: Rect) {
        self.painter().image(
            image,
            rect,
            Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
            Color32::WHITE,
        );
    }

    fn cell<R>(&mut self, width: f32, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        self.centered_and_justified(|ui| {
            ui.set_min_size([width, 70.].into());
            add_contents(ui)
        })
        .inner
    }
}
