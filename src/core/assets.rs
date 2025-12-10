use std::collections::HashMap;

use bevy::asset::AssetServer;
use bevy::prelude::*;
use bevy_kira_audio::AudioSource;
use strum::IntoEnumIterator;

use crate::core::map::planet::PlanetKind;
use crate::utils::NameFromEnum;

#[derive(Clone)]
pub struct TextureInfo {
    pub image: Handle<Image>,
    pub atlas: TextureAtlas,
    pub last_index: usize,
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
        name: impl Into<String>,
        asset_type: &str,
    ) -> &'a T {
        let name = name.into().clone();
        map.get(name.as_str()).expect(&format!("No asset for {asset_type} {name}"))
    }

    pub fn audio(&self, name: impl Into<String>) -> Handle<AudioSource> {
        self.get_asset(&self.audio, name, "audio").clone()
    }

    pub fn font(&self, name: impl Into<String>) -> Handle<Font> {
        self.get_asset(&self.fonts, name, "font").clone()
    }

    pub fn image(&self, name: impl Into<String>) -> Handle<Image> {
        self.get_asset(&self.images, name, "image").clone()
    }

    pub fn texture(&self, name: impl Into<String>) -> TextureInfo {
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
            ("victory", assets.load("audio/victory.ogg")),
            ("draw", assets.load("audio/draw.ogg")),
            ("defeat", assets.load("audio/defeat.ogg")),
            ("music", assets.load("audio/music.ogg")),
            ("horn", assets.load("audio/horn.ogg")),
            ("drums", assets.load("audio/drums.ogg")),
            ("repair", assets.load("audio/repair.ogg")),
            ("explosion", assets.load("audio/explosion.ogg")),
            ("short explosion", assets.load("audio/short explosion.ogg")),
            ("large explosion", assets.load("audio/large explosion.ogg")),
            ("death ray", assets.load("audio/death ray.ogg")),
        ]);

        let fonts = HashMap::from([
            ("bold", assets.load("fonts/FiraSans-Bold.ttf")),
            ("medium", assets.load("fonts/FiraMono-Medium.ttf")),
        ]);

        let mut images: HashMap<&'static str, Handle<Image>> = HashMap::from([
            // Icons
            ("mute", assets.load("images/icons/mute.png")),
            ("no-music", assets.load("images/icons/no-music.png")),
            ("sound", assets.load("images/icons/sound.png")),
            ("user", assets.load("images/icons/user.png")),
            ("info", assets.load("images/icons/info.png")),
            ("message", assets.load("images/icons/message.png")),
            ("won", assets.load("images/icons/won.png")),
            ("lost", assets.load("images/icons/lost.png")),
            ("eye", assets.load("images/icons/eye.png")),
            ("missile", assets.load("images/icons/missile.png")),
            ("logs", assets.load("images/icons/logs.png")),
            ("repair", assets.load("images/icons/repair.png")),
            ("convert", assets.load("images/icons/convert.png")),
            ("convert hover", assets.load("images/icons/convert hover.png")),
            ("dock", assets.load("images/icons/dock.png")),
            ("dock enemy", assets.load("images/icons/dock enemy.png")),
            ("mission", assets.load("images/icons/mission.png")),
            ("mission jump", assets.load("images/icons/mission jump.png")),
            ("mission enemy", assets.load("images/icons/mission enemy.png")),
            ("mission hover", assets.load("images/icons/mission hover.png")),
            ("mission jump hover", assets.load("images/icons/mission jump hover.png")),
            ("mission enemy hover", assets.load("images/icons/mission enemy hover.png")),
            // Backgrounds
            ("bg", assets.load("images/bg/bg.png")),
            ("menu", assets.load("images/bg/menu.png")),
            ("combat", assets.load("images/bg/combat.png")),
            ("defeat", assets.load("images/bg/defeat.png")),
            ("defeat bg", assets.load("images/bg/defeat bg.png")),
            ("draw", assets.load("images/bg/draw.png")),
            ("victory", assets.load("images/bg/victory.png")),
            ("victory bg", assets.load("images/bg/victory bg.png")),
            // Ui
            ("panel", assets.load("images/ui/panel.png")),
            ("thin panel", assets.load("images/ui/thin panel.png")),
            ("long button", assets.load("images/ui/long button.png")),
            ("button", assets.load("images/ui/button.png")),
            ("button hover", assets.load("images/ui/button hover.png")),
            // Resources
            ("turn", assets.load("images/resources/turn.png")),
            ("owned", assets.load("images/resources/owned.png")),
            ("metal", assets.load("images/resources/metal.png")),
            ("crystal", assets.load("images/resources/crystal.png")),
            ("deuterium", assets.load("images/resources/deuterium.png")),
            // Buildings
            ("lunar base", assets.load("images/buildings/lunar base.png")),
            ("demolition nexus", assets.load("images/buildings/demolition nexus.png")),
            ("metal mine", assets.load("images/buildings/metal mine.png")),
            ("crystal mine", assets.load("images/buildings/crystal mine.png")),
            ("deuterium synthesizer", assets.load("images/buildings/deuterium synthesizer.png")),
            ("shipyard", assets.load("images/buildings/shipyard.png")),
            ("factory", assets.load("images/buildings/factory.png")),
            ("missile silo", assets.load("images/buildings/missile silo.png")),
            ("planetary shield", assets.load("images/buildings/planetary shield.png")),
            ("reactor", assets.load("images/buildings/reactor.png")),
            ("jump gate", assets.load("images/buildings/jump gate.png")),
            ("sensor phalanx", assets.load("images/buildings/sensor phalanx.png")),
            ("laboratory", assets.load("images/buildings/laboratory.png")),
            ("orbital radar", assets.load("images/buildings/orbital radar.png")),
            // Defense
            ("crawler", assets.load("images/defense/crawler.png")),
            ("rocket launcher", assets.load("images/defense/rocket launcher.png")),
            ("light laser", assets.load("images/defense/light laser.png")),
            ("heavy laser", assets.load("images/defense/heavy laser.png")),
            ("gauss cannon", assets.load("images/defense/gauss cannon.png")),
            ("ion cannon", assets.load("images/defense/ion cannon.png")),
            ("plasma turret", assets.load("images/defense/plasma turret.png")),
            ("space dock", assets.load("images/defense/space dock.png")),
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
            // Mission
            ("overview", assets.load("images/mission/overview.png")),
            ("abandon", assets.load("images/mission/abandon.png")),
            ("attacked", assets.load("images/mission/attacked.png")),
            ("buildings", assets.load("images/mission/buildings.png")),
            ("fleet", assets.load("images/mission/fleet.png")),
            ("defenses", assets.load("images/mission/defenses.png")),
            ("deploy", assets.load("images/mission/deploy.png")),
            ("deploy cover", assets.load("images/mission/deploy cover.png")),
            ("colonize", assets.load("images/mission/colonize.png")),
            ("colonize cover", assets.load("images/mission/colonize cover.png")),
            ("attack", assets.load("images/mission/attack.png")),
            ("attack cover", assets.load("images/mission/attack cover.png")),
            ("spy", assets.load("images/mission/spy.png")),
            ("spy cover", assets.load("images/mission/spy cover.png")),
            ("missile strike", assets.load("images/mission/missile strike.png")),
            ("missile strike cover", assets.load("images/mission/missile strike cover.png")),
            ("destroy", assets.load("images/mission/destroy.png")),
            ("destroy cover", assets.load("images/mission/destroy cover.png")),
            // Combat
            ("hull", assets.load("images/combat/hull.png")),
            ("shield", assets.load("images/combat/shield.png")),
            ("damage", assets.load("images/combat/damage.png")),
            ("production", assets.load("images/combat/production.png")),
            ("speed", assets.load("images/combat/speed.png")),
            ("fuel consumption", assets.load("images/combat/fuel consumption.png")),
            ("rapid fire", assets.load("images/combat/rapid fire.png")),
            // Planets
            ("unknown", assets.load("images/planets/unknown.png")),
            ("destroyed bg", assets.load("images/planets/destroyed bg.png")),
            // Animations
            ("explosion", assets.load("images/animations/explosion.png")),
            ("short explosion", assets.load("images/animations/short explosion.png")),
            ("flame", assets.load("images/animations/flame.png")),
            ("death ray", assets.load("images/animations/death ray.png")),
        ]);

        for i in 0..65 {
            let name = Box::leak(Box::new(format!("planet{}", i))).as_str();
            images.insert(&name, assets.load(&format!("images/planets/planet{i}.png")));
        }

        for i in 0..6 {
            let name = Box::leak(Box::new(format!("moon{}", i))).as_str();
            images.insert(&name, assets.load(&format!("images/planets/moon{i}.png")));
        }

        for kind in PlanetKind::iter() {
            let name = Box::leak(Box::new(kind.to_lowername())).as_str();
            images.insert(&name, assets.load(&format!("images/planets/{name}.png")));

            let name = Box::leak(Box::new(format!("{} large", kind.to_lowername()))).as_str();
            images.insert(&name, assets.load(&format!("images/planets/{name}.png")));
        }

        let mut texture = world.get_resource_mut::<Assets<TextureAtlasLayout>>().unwrap();

        let long_button = TextureAtlasLayout::from_grid(UVec2::new(231, 25), 1, 2, None, None);
        let explosion = TextureAtlasLayout::from_grid(UVec2::new(256, 256), 8, 6, None, None);
        let short_explosion = TextureAtlasLayout::from_grid(UVec2::new(256, 251), 8, 4, None, None);
        let flame = TextureAtlasLayout::from_grid(UVec2::new(124, 54), 1, 12, None, None);
        let death_ray =
            TextureAtlasLayout::from_grid(UVec2::new(190, 474), 9, 1, Some(UVec2::splat(2)), None);
        let textures: HashMap<&'static str, TextureInfo> = HashMap::from([
            (
                "long button",
                TextureInfo {
                    image: images["long button"].clone(),
                    atlas: TextureAtlas {
                        layout: texture.add(long_button),
                        index: 1,
                    },
                    last_index: 1,
                },
            ),
            (
                "explosion",
                TextureInfo {
                    image: images["explosion"].clone(),
                    atlas: TextureAtlas {
                        layout: texture.add(explosion),
                        index: 1,
                    },
                    last_index: 48,
                },
            ),
            (
                "short explosion",
                TextureInfo {
                    image: images["short explosion"].clone(),
                    atlas: TextureAtlas {
                        layout: texture.add(short_explosion),
                        index: 1,
                    },
                    last_index: 32,
                },
            ),
            (
                "flame",
                TextureInfo {
                    image: images["flame"].clone(),
                    atlas: TextureAtlas {
                        layout: texture.add(flame),
                        index: 1,
                    },
                    last_index: 8,
                },
            ),
            (
                "death ray",
                TextureInfo {
                    image: images["death ray"].clone(),
                    atlas: TextureAtlas {
                        layout: texture.add(death_ray),
                        index: 1,
                    },
                    last_index: 9,
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
