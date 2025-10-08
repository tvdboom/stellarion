use bevy::asset::AssetServer;
use bevy::prelude::*;
use bevy_kira_audio::AudioSource;
use std::collections::HashMap;

#[derive(Clone)]
pub struct TextureInfo {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
}

pub struct WorldAssets {
    pub audio: HashMap<&'static str, Handle<AudioSource>>,
    pub fonts: HashMap<&'static str, Handle<Font>>,
    pub images: HashMap<&'static str, Handle<Image>>,
    pub textures: HashMap<&'static str, TextureInfo>,
}

impl WorldAssets {
    fn get_asset<'a, T: Clone>(
        &self,
        map: &'a HashMap<&str, T>,
        name: &str,
        asset_type: &str,
    ) -> &'a T {
        map.get(name).expect(&format!("No asset for {asset_type} {name}"))
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
            ("turn", assets.load("images/icons/turn.png")),
            ("mute", assets.load("images/icons/mute.png")),
            ("no-music", assets.load("images/icons/no-music.png")),
            ("sound", assets.load("images/icons/sound.png")),
            ("attacked", assets.load("images/icons/attacked.png")),
            ("buildings", assets.load("images/icons/buildings.png")),
            ("fleet", assets.load("images/icons/fleet.png")),
            ("defense", assets.load("images/icons/defense.png")),
            ("transport", assets.load("images/icons/transport.png")),
            ("colonize", assets.load("images/icons/colonize.png")),
            ("attack", assets.load("images/icons/attack.png")),
            ("spy", assets.load("images/icons/spy.png")),
            ("bomb", assets.load("images/icons/bomb.png")),
            ("destroy", assets.load("images/icons/destroy.png")),
            // Backgrounds
            ("bg", assets.load("images/bg/bg.png")),
            ("menu", assets.load("images/bg/menu.png")),
            ("defeat", assets.load("images/bg/defeat.png")),
            ("victory", assets.load("images/bg/victory.png")),
            // Ui
            ("panel", assets.load("images/ui/panel.png")),
            ("thin_panel", assets.load("images/ui/thin_panel.png")),
            ("button", assets.load("images/ui/button.png")),
            // Planets
            ("planets", assets.load("images/planets/planets.png")),
            ("destroyed", assets.load("images/planets/destroyed.png")),
            // Resources
            ("metal", assets.load("images/resources/metal.png")),
            ("crystal", assets.load("images/resources/crystal.png")),
            ("deuterium", assets.load("images/resources/deuterium.png")),
            // Buildings
            ("mine", assets.load("images/buildings/mine.png")),
            ("shipyard", assets.load("images/buildings/shipyard.png")),
            ("factory", assets.load("images/buildings/factory.png")),
            ("missile silo", assets.load("images/buildings/missile silo.png")),
            ("planetary shield", assets.load("images/buildings/planetary shield.png")),
            ("jump gate", assets.load("images/buildings/jump gate.png")),
            ("sensor phalanx", assets.load("images/buildings/sensor phalanx.png")),
            // Defense
            ("rocket launcher", assets.load("images/defense/rocket launcher.png")),
            ("light laser", assets.load("images/defense/light laser.png")),
            ("heavy laser", assets.load("images/defense/heavy laser.png")),
            ("gauss cannon", assets.load("images/defense/gauss cannon.png")),
            ("ion cannon", assets.load("images/defense/ion cannon.png")),
            ("plasma turret", assets.load("images/defense/plasma turret.png")),
            ("antiballistic missile", assets.load("images/defense/antiballistic missile.png")),
            ("interplanetary missile", assets.load("images/defense/interplanetary missile.png")),
            // Ships
            ("probe", assets.load("images/ships/probe.png")),
            ("colony ship", assets.load("images/ships/colony ship.png")),
            ("light fighter", assets.load("images/ships/light fighter.png")),
            ("heavy fighter", assets.load("images/ships/heavy fighter.png")),
            ("destroyer", assets.load("images/ships/destroyer.png")),
            ("cruiser", assets.load("images/ships/cruiser.png")),
            ("bomber", assets.load("images/ships/bomber.png")),
            ("battleship", assets.load("images/ships/battleship.png")),
            ("dreadnought", assets.load("images/ships/dreadnought.png")),
            ("war sun", assets.load("images/ships/war sun.png")),
        ]);

        let mut texture = world.get_resource_mut::<Assets<TextureAtlasLayout>>().unwrap();

        let button = TextureAtlasLayout::from_grid(UVec2::new(231, 25), 1, 2, None, None);
        let planets =
            TextureAtlasLayout::from_grid(UVec2::splat(450), 8, 8, Some(UVec2::splat(30)), None);

        let textures: HashMap<&'static str, TextureInfo> = HashMap::from([
            (
                "button",
                TextureInfo {
                    image: images["button"].clone_weak(),
                    layout: texture.add(button),
                },
            ),
            (
                "planets",
                TextureInfo {
                    image: images["planets"].clone_weak(),
                    layout: texture.add(planets),
                },
            ),
        ]);

        Self {
            audio,
            fonts,
            images,
            textures,
        }
    }
}
