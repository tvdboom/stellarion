use bevy::asset::AssetServer;
use bevy::prelude::*;
use bevy_kira_audio::AudioSource;
use std::collections::HashMap;

#[derive(Clone)]
pub struct TextureInfo {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
}

#[derive(Clone)]
pub struct AtlasInfo {
    pub image: Handle<Image>,
    pub texture: TextureAtlas,
    pub last_index: usize,
}

pub struct WorldAssets {
    pub audio: HashMap<&'static str, Handle<AudioSource>>,
    pub fonts: HashMap<&'static str, Handle<Font>>,
    pub images: HashMap<&'static str, Handle<Image>>,
    pub textures: HashMap<&'static str, TextureInfo>,
    pub atlas: HashMap<&'static str, AtlasInfo>,
}

impl WorldAssets {
    fn get_asset<'a, T: Clone>(
        &self,
        map: &'a HashMap<&str, T>,
        name: &str,
        asset_type: &str,
    ) -> &'a T {
        map.get(name)
            .expect(&format!("No asset for {asset_type} {name}"))
    }

    pub fn audio(&self, name: &str) -> Handle<AudioSource> {
        self.get_asset(&self.audio, name, "audio").clone_weak()
    }

    pub fn font(&self, name: &str) -> Handle<Font> {
        self.get_asset(&self.fonts, name, "font").clone_weak()
    }

    pub fn image(&self, name: &str) -> Handle<Image> {
        self.get_asset(&self.images, name, "image").clone_weak()
    }

    pub fn texture(&self, name: &str) -> TextureInfo {
        self.get_asset(&self.textures, name, "texture").clone()
    }

    pub fn atlas(&self, name: &str) -> AtlasInfo {
        self.get_asset(&self.atlas, name, "atlas").clone()
    }
}

impl FromWorld for WorldAssets {
    fn from_world(world: &mut World) -> Self {
        let assets = world.get_resource::<AssetServer>().unwrap();

        let audio = HashMap::from([
            ("button", assets.load("audio/button.ogg")),
            ("message", assets.load("audio/message.ogg")),
            ("warning", assets.load("audio/warning.ogg")),
            ("error", assets.load("audio/error.ogg")),
            ("defeat", assets.load("audio/defeat.ogg")),
            ("music", assets.load("audio/music.ogg")),
        ]);

        let fonts = HashMap::from([
            ("bold", assets.load("fonts/FiraSans-Bold.ttf")),
            ("medium", assets.load("fonts/FiraMono-Medium.ttf")),
        ]);

        let images: HashMap<&'static str, Handle<Image>> = HashMap::from([
            // Icons
            ("mute", assets.load("images/icons/mute.png")),
            ("no-music", assets.load("images/icons/no-music.png")),
            ("sound", assets.load("images/icons/sound.png")),
            // Backgrounds
            ("bg", assets.load("images/bg.png")),
            // Planets
            // ("desert", assets.load("images/planets/desert.png")),
            // ("gas", assets.load("images/planets/gas.png")),
            // ("ice", assets.load("images/planets/ice.png")),
            // ("normal", assets.load("images/planets/normal.png")),
            ("planets", assets.load("images/planets.png")),
        ]);

        let mut texture = world
            .get_resource_mut::<Assets<TextureAtlasLayout>>()
            .unwrap();

        let planets =
            TextureAtlasLayout::from_grid(UVec2::splat(450), 8, 8, Some(UVec2::splat(30)), None);
        let textures: HashMap<&'static str, TextureInfo> = HashMap::from([(
            "planets",
            TextureInfo {
                image: images["planets"].clone_weak(),
                layout: texture.add(planets),
            },
        )]);

        let atlas = HashMap::new();

        Self {
            audio,
            fonts,
            images,
            textures,
            atlas,
        }
    }
}
