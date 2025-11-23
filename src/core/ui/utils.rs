use std::collections::HashMap;

use bevy::prelude::Resource;
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::egui::*;

use crate::core::constants::BG_COLOR;

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

        response.on_hover_cursor(CursorIcon::PointingHand)
    }
}

pub trait CustomResponse {
    fn on_hover_small(self, text: impl Into<RichText>) -> Self;
    fn on_hover_small_ext(self, text: impl Into<RichText>) -> Self;
    fn on_disabled_hover_small(self, text: impl Into<RichText>) -> Self;
    fn on_disabled_hover_small_ext(self, text: impl Into<RichText>) -> Self;
}

impl CustomResponse for Response {
    fn on_hover_small(self, text: impl Into<RichText>) -> Self {
        self.on_hover_ui(|ui| {
            ui.small(text);
        })
    }

    fn on_hover_small_ext(self, text: impl Into<RichText>) -> Self {
        self.on_hover_ui(|ui| {
            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
            ui.small(text);
        })
    }

    fn on_disabled_hover_small(self, text: impl Into<RichText>) -> Self {
        self.on_disabled_hover_ui(|ui| {
            ui.small(text);
        })
    }

    fn on_disabled_hover_small_ext(self, text: impl Into<RichText>) -> Self {
        self.on_disabled_hover_ui(|ui| {
            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
            ui.small(text);
        })
    }
}

pub trait CustomUi {
    fn add_image(&mut self, texture: impl Into<TextureId>, size: impl Into<Vec2>) -> Response;
    fn add_image_button(
        &mut self,
        texture: impl Into<TextureId>,
        size: impl Into<Vec2>,
    ) -> Response;
    fn add_custom_button(&mut self, text: impl ToString, images: &ImageIds) -> Response;
    fn add_image_painter(&mut self, image: TextureId, rect: Rect);
    fn add_icon_on_image(&mut self, id: impl Into<TextureId>, rect: Rect) -> Response;
    fn add_text_on_image(
        &mut self,
        text: String,
        color: Color32,
        style: TextStyle,
        pos: Pos2,
        align: Align2,
    ) -> Rect;
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

    fn add_custom_button(&mut self, text: impl ToString, images: &ImageIds) -> Response {
        let (rect, mut response) = self.allocate_exact_size([180., 50.].into(), Sense::click());

        response = response.on_hover_cursor(CursorIcon::PointingHand);

        let image = if response.hovered() && !response.is_pointer_button_down_on() {
            images.get("button hover")
        } else {
            images.get("button")
        };

        self.add_image_painter(image, rect);

        self.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            TextStyle::Button.resolve(self.style()),
            Color32::WHITE,
        );

        response
    }

    fn add_image_painter(&mut self, image: TextureId, rect: Rect) {
        self.painter().rect_filled(rect, 0.0, BG_COLOR);

        self.painter().image(
            image,
            rect,
            Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
            Color32::WHITE,
        );
    }

    fn add_icon_on_image(&mut self, id: impl Into<TextureId>, rect: Rect) -> Response {
        let size = [20., 20.];
        let pos = rect.right_top() - vec2(size[0] + 5., -5.);

        self.put(Rect::from_min_size(pos, size.into()), Image::new(SizedTexture::new(id, size)))
    }

    fn add_text_on_image(
        &mut self,
        text: String,
        color: Color32,
        style: TextStyle,
        mut pos: Pos2,
        align: Align2,
    ) -> Rect {
        let (margin, cr) = match style {
            TextStyle::Small => (Vec2::new(1., 0.), 2.),
            TextStyle::Body => (Vec2::new(2.5, -1.5), 4.),
            _ => (Vec2::new(5., -5.), 6.),
        };

        let galley = self.painter().layout_no_wrap(text, style.resolve(self.style()), color);
        let size = galley.size();

        if style == TextStyle::Heading {
            pos = pos + vec2(3., -3.);
        }

        let top_left = match align {
            Align2::LEFT_TOP => pos,
            Align2::LEFT_BOTTOM => pos - vec2(0., size.y + 2. * margin.y),
            Align2::RIGHT_TOP => pos - vec2(size.x + 2. * margin.x, 0.),
            Align2::RIGHT_BOTTOM => pos - vec2(size.x + 2. * margin.x, size.y + 2. * margin.y),
            Align2::CENTER_TOP => pos - vec2(size.x / 2., 0.),
            Align2::CENTER_BOTTOM => pos - vec2(size.x / 2., size.y + 2. * margin.y),
            Align2::CENTER_CENTER => pos - size / 2.,
            Align2::LEFT_CENTER => pos - vec2(0., size.y / 2.),
            Align2::RIGHT_CENTER => pos - vec2(size.x + 2. * margin.x, size.y / 2.),
        } + margin;

        // Adjust heading since it has more y padding
        let bg_rect = Rect::from_min_size(top_left - margin, size + 2. * margin);

        // Draw semi-transparent background
        self.painter().rect_filled(bg_rect, cr, Color32::from_rgba_premultiplied(0, 0, 0, 128));

        // Draw galley
        self.painter().galley(top_left, galley, Color32::WHITE);

        bg_rect
    }

    fn cell<R>(&mut self, width: f32, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
        self.centered_and_justified(|ui| {
            ui.set_min_size([width, 70.].into());
            add_contents(ui)
        })
        .inner
    }
}
