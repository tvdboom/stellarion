use bevy_egui::egui::load::SizedTexture;
use bevy_egui::egui::{Image, ImageButton, Response, TextureId, Ui, Vec2};

pub trait CustomUi {
    fn add_image(&mut self, texture: impl Into<TextureId>, size: impl Into<Vec2>) -> Response;
    fn add_image_button(
        &mut self,
        texture: impl Into<TextureId>,
        size: impl Into<Vec2>,
    ) -> Response;
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
        self.add(ImageButton::new(SizedTexture::new(texture, size)))
    }
}
