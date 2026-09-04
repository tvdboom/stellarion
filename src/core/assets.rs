//! Deferred menu/gameplay asset registry backed by reproducibly generated KTX2 textures.

use std::collections::HashMap;

use bevy::asset::{
    AssetLoadError, AssetServer, DependencyLoadState, LoadState, RecursiveDependencyLoadState,
    UntypedHandle,
};
use bevy::prelude::*;
use bevy_kira_audio::AudioSource;
use strum::IntoEnumIterator;

use crate::core::basis_texture::BasisTextureSettings;
use crate::core::map::planet::PlanetKind;
use crate::utils::NameFromEnum;

/// Image handle plus atlas metadata used by animated sprite systems.
#[derive(Clone)]
pub struct TextureInfo {
    /// Optimized source image.
    pub image: Handle<Image>,
    /// Atlas layout and current frame.
    pub atlas: TextureAtlas,
    /// Highest valid zero-based frame index.
    pub last_index: usize,
}

/// Logical lifecycle of deferred gameplay assets.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GameplayAssetState {
    /// Only menu assets have been requested.
    #[default]
    Deferred,
    /// World, unit, combat, effect, and audio assets are loading.
    Loading,
    /// Every gameplay handle and dependency is ready.
    Ready,
    /// At least one gameplay asset or dependency could not be loaded.
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HandleLoadStatus {
    Pending,
    Ready,
    Failed,
}

/// Deduplicated handles partitioned into a minimal menu group and a deferred gameplay group.
#[derive(Resource)]
pub struct WorldAssets {
    audio: HashMap<String, Handle<AudioSource>>,
    fonts: HashMap<String, Handle<Font>>,
    pub(crate) images: HashMap<String, Handle<Image>>,
    /// Premultiplied artwork overrides for images also used as straight-alpha map masks.
    pub(crate) ui_images: HashMap<String, Handle<Image>>,
    textures: HashMap<String, TextureInfo>,
    menu_handles: Vec<UntypedHandle>,
    gameplay_handles: Vec<UntypedHandle>,
    gameplay_state: GameplayAssetState,
    gameplay_error: Option<String>,
}

impl WorldAssets {
    /// Returns an audio handle or a harmless default while reporting a missing registry key.
    pub fn audio(&self, name: impl AsRef<str>) -> Handle<AudioSource> {
        self.audio.get(name.as_ref()).cloned().unwrap_or_else(|| {
            error!("missing audio asset key: {}", name.as_ref());
            Handle::default()
        })
    }

    /// Returns a font handle or a harmless default while reporting a missing registry key.
    pub fn font(&self, name: impl AsRef<str>) -> Handle<Font> {
        self.fonts.get(name.as_ref()).cloned().unwrap_or_else(|| {
            error!("missing font asset key: {}", name.as_ref());
            Handle::default()
        })
    }

    /// Returns an image handle or a harmless default while reporting a missing registry key.
    pub fn image(&self, name: impl AsRef<str>) -> Handle<Image> {
        self.images.get(name.as_ref()).cloned().unwrap_or_else(|| {
            error!("missing image asset key: {}", name.as_ref());
            Handle::default()
        })
    }

    /// Returns texture-atlas metadata or an empty fallback for a missing key.
    pub fn texture(&self, name: impl AsRef<str>) -> TextureInfo {
        self.textures.get(name.as_ref()).cloned().unwrap_or_else(|| {
            error!("missing texture asset key: {}", name.as_ref());
            TextureInfo {
                image: Handle::default(),
                atlas: TextureAtlas::default(),
                last_index: 0,
            }
        })
    }

    /// Returns whether every boot/menu asset and recursive dependency is loaded.
    pub fn menu_ready(&self, server: &AssetServer) -> bool {
        handles_ready(server, &self.menu_handles)
    }

    /// Starts loading gameplay groups once, preserving shared menu handles.
    pub fn begin_gameplay_loading(
        &mut self,
        server: &AssetServer,
        layouts: &mut Assets<TextureAtlasLayout>,
    ) {
        if self.gameplay_state != GameplayAssetState::Deferred {
            return;
        }
        self.gameplay_state = GameplayAssetState::Loading;
        self.gameplay_error = None;

        for name in [
            "warning",
            "victory",
            "draw",
            "defeat",
            "music",
            "horn",
            "drums",
            "repair",
            "explosion",
            "short explosion",
            "large explosion",
            "death ray",
            "construction",
            "booster",
            "launch",
            "star ambience",
        ] {
            load_audio(server, &mut self.audio, &mut self.gameplay_handles, name);
        }

        load_category(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "icons",
            &[
                "user",
                "info",
                "message",
                "won",
                "lost",
                "eye",
                "missile",
                "logs",
                "repair",
                "convert",
                "convert hover",
            ],
        );
        // Faction-colored map artwork keeps neutral blue-silver relief under the sprite tint.
        // The matching UI variants premultiply that same shaded artwork for egui.
        for name in [
            "dock",
            "jump gate marker",
            "mission",
            "mission colonize",
            "mission destroy",
            "mission jump",
            "mission missile",
            "mission spy",
        ] {
            let path = format!("images/icons/{name}.basisu.ktx2");
            let handle: Handle<Image> = server
                .load_builder()
                .with_settings(|settings: &mut BasisTextureSettings| {
                    settings.ui_variant = true;
                    settings.linear_filtering = true;
                })
                .load(path.clone());
            let ui_handle: Handle<Image> = server
                .load_builder()
                .with_settings(|settings: &mut BasisTextureSettings| {
                    settings.ui_variant = true;
                    settings.linear_filtering = true;
                })
                .load(format!("{path}#ui"));
            self.gameplay_handles.push(ui_handle.clone().untyped());
            self.ui_images.insert(name.to_string(), ui_handle);
            self.gameplay_handles.push(handle.clone().untyped());
            self.images.insert(name.to_string(), handle);
        }
        load_category(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "bg",
            &["bg", "combat", "defeat bg", "victory bg"],
        );
        // Result lettering scales throughout its entrance animation; nearest sampling aliases
        // the gold outlines even though the source artwork has enough resolution.
        for name in ["victory", "defeat", "draw"] {
            let handle: Handle<Image> = server
                .load_builder()
                .with_settings(|settings: &mut BasisTextureSettings| {
                    settings.linear_filtering = true;
                })
                .load(format!("images/bg/{name}.basisu.ktx2"));
            self.gameplay_handles.push(handle.clone().untyped());
            self.images.insert(name.to_string(), handle);
        }
        load_category(server, &mut self.images, &mut self.gameplay_handles, "ui", &["panel"]);
        load_category(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "resources",
            &["turn", "owned", "metal", "crystal", "deuterium"],
        );
        load_category(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "buildings",
            &[
                "lunar base",
                "demolition nexus",
                "metal mine",
                "crystal mine",
                "deuterium synthesizer",
                "shipyard",
                "factory",
                "missile silo",
                "planetary shield",
                "reactor",
                "jump gate",
                "sensor phalanx",
                "laboratory",
                "orbital radar",
            ],
        );
        load_category(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "defense",
            &[
                "crawler",
                "rocket launcher",
                "light laser",
                "heavy laser",
                "gauss cannon",
                "ion cannon",
                "plasma turret",
                "space dock",
                "antiballistic missile",
                "interplanetary missile",
            ],
        );
        load_category(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "ships",
            &[
                "probe",
                "colony ship",
                "light fighter",
                "heavy fighter",
                "destroyer",
                "cruiser",
                "bomber",
                "battleship",
                "dreadnought",
                "war sun",
            ],
        );
        load_category(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "mission",
            &[
                "overview",
                "abandon",
                "attacked",
                "buildings",
                "fleet",
                "defenses",
                "deploy",
                "deploy cover",
                "colonize",
                "colonize cover",
                "attack",
                "attack cover",
                "spy",
                "spy cover",
                "missile strike",
                "missile strike cover",
                "destroy",
                "destroy cover",
            ],
        );
        load_category(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "combat",
            &["hull", "shield", "damage", "production", "speed", "fuel consumption", "rapid fire"],
        );
        load_category(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "planets",
            &["unknown", "destroyed bg"],
        );
        load_category(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "animations",
            &["explosion", "short explosion", "flame", "death ray"],
        );
        for index in 1..=4 {
            let name = format!("solar star {index}");
            load_linear_image(
                server,
                &mut self.images,
                &mut self.gameplay_handles,
                &name,
                &format!("images/animations/{name}.basisu.ktx2"),
            );
        }
        load_linear_image(
            server,
            &mut self.images,
            &mut self.gameplay_handles,
            "nebula",
            "images/ambient/nebula.basisu.ktx2",
        );
        for index in 1..=66 {
            let name = format!("black hole {index}");
            load_linear_image(
                server,
                &mut self.images,
                &mut self.gameplay_handles,
                &name,
                &format!("images/ambient/{name}.basisu.ktx2"),
            );
        }

        for index in 0..65 {
            let name = format!("planet{index}");
            load_image(
                server,
                &mut self.images,
                &mut self.gameplay_handles,
                &name,
                &format!("images/planets/{name}.basisu.ktx2"),
            );
        }
        for index in 0..6 {
            let name = format!("moon{index}");
            load_image(
                server,
                &mut self.images,
                &mut self.gameplay_handles,
                &name,
                &format!("images/planets/{name}.basisu.ktx2"),
            );
        }
        for kind in PlanetKind::iter() {
            let name = kind.to_lowername();
            load_image(
                server,
                &mut self.images,
                &mut self.gameplay_handles,
                &name,
                &format!("images/planets/{name}.basisu.ktx2"),
            );
            let large = format!("{name} large");
            load_image(
                server,
                &mut self.images,
                &mut self.gameplay_handles,
                &large,
                &format!("images/planets/{large}.basisu.ktx2"),
            );
        }

        self.add_texture(
            "explosion",
            TextureAtlasLayout::from_grid(UVec2::new(256, 256), 8, 6, None, None),
            47,
            layouts,
        );
        self.add_texture(
            "short explosion",
            TextureAtlasLayout::from_grid(UVec2::new(256, 251), 8, 4, None, None),
            31,
            layouts,
        );
        self.add_texture(
            "flame",
            TextureAtlasLayout::from_grid(UVec2::new(124, 54), 1, 12, None, None),
            11,
            layouts,
        );
        self.add_texture(
            "death ray",
            TextureAtlasLayout::from_grid(UVec2::new(190, 474), 9, 1, Some(UVec2::splat(2)), None),
            8,
            layouts,
        );
    }

    /// Advances the gameplay group to ready after all recursive dependencies load.
    pub fn refresh_gameplay_state(&mut self, server: &AssetServer) -> GameplayAssetState {
        if self.gameplay_state == GameplayAssetState::Loading {
            self.gameplay_state = classify_handle_group(
                self.gameplay_handles.iter().map(|handle| handle_status(server, handle)),
            );
            if self.gameplay_state == GameplayAssetState::Failed {
                let failure =
                    first_handle_failure(server, &self.gameplay_handles).unwrap_or_else(|| {
                        "A gameplay asset or one of its dependencies failed to load.".to_string()
                    });
                error!("gameplay asset loading failed: {failure}");
                self.gameplay_error = Some(failure);
            }
        }
        self.gameplay_state
    }

    /// Returns the current deferred-group lifecycle for UI and tests.
    pub fn gameplay_state(&self) -> GameplayAssetState {
        self.gameplay_state
    }

    /// Returns the terminal gameplay loading error, when one was observed.
    pub fn gameplay_error(&self) -> Option<&str> {
        self.gameplay_error.as_deref()
    }

    /// Inserts atlas metadata for an already requested image.
    fn add_texture(
        &mut self,
        name: &str,
        layout: TextureAtlasLayout,
        last_index: usize,
        layouts: &mut Assets<TextureAtlasLayout>,
    ) {
        self.textures.insert(
            name.to_string(),
            TextureInfo {
                image: self.image(name),
                atlas: TextureAtlas {
                    layout: layouts.add(layout),
                    index: 0,
                },
                last_index,
            },
        );
    }
}

impl FromWorld for WorldAssets {
    /// Requests only the fonts, background, controls, and audio needed before entering a game.
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>().clone();
        let mut layouts = world.resource_mut::<Assets<TextureAtlasLayout>>();
        let mut assets = Self {
            audio: HashMap::new(),
            fonts: HashMap::new(),
            images: HashMap::new(),
            ui_images: HashMap::new(),
            textures: HashMap::new(),
            menu_handles: Vec::new(),
            gameplay_handles: Vec::new(),
            gameplay_state: GameplayAssetState::Deferred,
            gameplay_error: None,
        };

        for name in ["ui-click", "message", "error", "music"] {
            load_audio(&server, &mut assets.audio, &mut assets.menu_handles, name);
        }
        for (name, path) in
            [("bold", "fonts/FiraSans-Bold.ttf"), ("medium", "fonts/FiraMono-Medium.ttf")]
        {
            let handle: Handle<Font> = server.load(path);
            assets.menu_handles.push(handle.clone().untyped());
            assets.fonts.insert(name.to_string(), handle);
        }
        load_category(
            &server,
            &mut assets.images,
            &mut assets.menu_handles,
            "icons",
            &["mute", "no-music", "sound"],
        );
        load_category(&server, &mut assets.images, &mut assets.menu_handles, "bg", &["menu"]);
        load_category(
            &server,
            &mut assets.images,
            &mut assets.menu_handles,
            "ui",
            &["long button", "button", "button hover"],
        );
        assets.add_texture(
            "long button",
            TextureAtlasLayout::from_grid(UVec2::new(231, 25), 1, 2, None, None),
            1,
            &mut layouts,
        );
        assets
    }
}

/// Loads one Ogg handle and retains it in exactly one logical group.
fn load_audio(
    server: &AssetServer,
    audio: &mut HashMap<String, Handle<AudioSource>>,
    group: &mut Vec<UntypedHandle>,
    name: &str,
) {
    if audio.contains_key(name) {
        return;
    }
    let handle: Handle<AudioSource> = server.load(format!("audio/{name}.ogg"));
    group.push(handle.clone().untyped());
    audio.insert(name.to_string(), handle);
}

/// Loads same-named KTX2 images from one source/runtime category.
fn load_category(
    server: &AssetServer,
    images: &mut HashMap<String, Handle<Image>>,
    group: &mut Vec<UntypedHandle>,
    category: &str,
    names: &[&str],
) {
    for name in names {
        load_image(server, images, group, name, &format!("images/{category}/{name}.basisu.ktx2"));
    }
}

/// Loads one deduplicated image and retains its strong handle for the group's lifetime.
fn load_image(
    server: &AssetServer,
    images: &mut HashMap<String, Handle<Image>>,
    group: &mut Vec<UntypedHandle>,
    name: &str,
    path: &str,
) {
    if images.contains_key(name) {
        return;
    }
    let handle: Handle<Image> = server.load(path.to_string());
    group.push(handle.clone().untyped());
    images.insert(name.to_string(), handle);
}

/// Loads artwork that is continuously scaled or rotated with smooth sampling.
fn load_linear_image(
    server: &AssetServer,
    images: &mut HashMap<String, Handle<Image>>,
    group: &mut Vec<UntypedHandle>,
    name: &str,
    path: &str,
) {
    if images.contains_key(name) {
        return;
    }
    let handle: Handle<Image> = server
        .load_builder()
        .with_settings(|settings: &mut BasisTextureSettings| settings.linear_filtering = true)
        .load(path.to_string());
    group.push(handle.clone().untyped());
    images.insert(name.to_string(), handle);
}

/// Returns whether every strong handle and recursive dependency has completed loading.
fn handles_ready(server: &AssetServer, handles: &[UntypedHandle]) -> bool {
    !handles.is_empty()
        && handles.iter().all(|handle| server.is_loaded_with_dependencies(handle.id()))
}

/// Collapses per-handle load results while ensuring any failure wins over pending work.
fn classify_handle_group(
    statuses: impl IntoIterator<Item = HandleLoadStatus>,
) -> GameplayAssetState {
    let mut saw_handle = false;
    let mut all_ready = true;
    for status in statuses {
        saw_handle = true;
        match status {
            HandleLoadStatus::Failed => return GameplayAssetState::Failed,
            HandleLoadStatus::Pending => all_ready = false,
            HandleLoadStatus::Ready => {},
        }
    }
    if saw_handle && all_ready {
        GameplayAssetState::Ready
    } else {
        GameplayAssetState::Loading
    }
}

/// Converts Bevy's root and dependency states into the lifecycle used by the loading screen.
fn handle_status(server: &AssetServer, handle: &UntypedHandle) -> HandleLoadStatus {
    match server.get_load_states(handle.id()) {
        Some((LoadState::Failed(_), _, _))
        | Some((_, DependencyLoadState::Failed(_), _))
        | Some((_, _, RecursiveDependencyLoadState::Failed(_))) => HandleLoadStatus::Failed,
        Some((
            LoadState::Loaded,
            DependencyLoadState::Loaded,
            RecursiveDependencyLoadState::Loaded,
        )) => HandleLoadStatus::Ready,
        _ => HandleLoadStatus::Pending,
    }
}

/// Describes the first failed retained handle so missing packaged assets are actionable.
fn first_handle_failure(server: &AssetServer, handles: &[UntypedHandle]) -> Option<String> {
    handles.iter().find_map(|handle| {
        let (root, dependencies, recursive) = server.get_load_states(handle.id())?;
        let failure: &AssetLoadError = match (&root, &dependencies, &recursive) {
            (LoadState::Failed(error), _, _)
            | (_, DependencyLoadState::Failed(error), _)
            | (_, _, RecursiveDependencyLoadState::Failed(error)) => error,
            _ => return None,
        };
        let path = server
            .get_path(handle.id())
            .map_or_else(|| format!("{:?}", handle.id()), |path| path.to_string());
        Some(format!("Could not load gameplay asset `{path}`: {failure}"))
    })
}

#[cfg(test)]
#[path = "../../tests/core/assets.rs"]
mod tests;
