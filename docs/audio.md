# Interface and action sounds

The menu click comes from Arcana's `assets-src/audio/button.ogg`. Arcana is
copyright (c) 2026 Mavs under the MIT notice in this repository's root `LICENSE`.

Construction uses Kenney's [Interface Sounds](https://kenney.nl/assets/interface-sounds)
pack. All three construction categories share one short ascending confirmation.
The Kenney recordings are CC0; the original notices are preserved in
`LICENSE-Kenney-Interface.txt` and `LICENSE-Kenney-Sci-fi.txt` beside this document.

| Game file | Recording | License | Duration | Trigger |
| --- | --- | --- | --- | --- |
| `ui-click.ogg` | Arcana: `button.ogg` | MIT | 0.119 s | Accepted main, pause, and settings menu clicks |
| `construction.ogg` | Kenney: `confirmation_001.ogg` from [Interface Sounds](https://kenney.nl/assets/interface-sounds) | CC0 | 0.290 s | Accepted ship, defense, missile, building construction, or building upgrade purchase |
| `booster.ogg` | Kenney: `thrusterFire_000.ogg` from [Sci-fi Sounds](https://kenney.nl/assets/sci-fi-sounds) | CC0 | 0.850 s | Repeats while hovering a visible mission on the map or in the mission list |
| `launch.ogg` | User-provided recording | Source/license not recorded | Original file | Accepted mission launch, once |

The Interface Sounds 1.0 archive was downloaded directly from Kenney on
September 2, 2026. Construction retains its full original duration and is
mono 44.1 kHz Ogg/Vorbis (FFmpeg 7.1, quality 5). A 60 Hz high-pass removes DC/rumble;
a 5 kHz low-pass softens the upper frequencies. Construction has 4 ms / 25 ms fades.

Construction is normalized to -25 LUFS, measured with silence padding to 400 ms
because the cue is shorter than the loudness measurement window. That padding
is used only for measurement, never stored in the game asset.
`SoundEffect::request` plays these pre-balanced files at 0 dB gain.
The existing menu and booster cues retain their -25 LUFS normalization.
The new launch recording is used unchanged.

The existing notification, battle, and result effects retain their
-25 LUFS target and 0 dB playback gain. Their original channels and
sample rates are preserved. Music and drums keep their background playback level.

Browsing fleet, shipyard, defense, or mission panels is silent, including
switching their category icons. Purchases play once per accepted transaction,
including accepted right-click bulk purchases. Disabled controls and rejected
commands do not play success cues. Launching a mission plays `launch.ogg` once;
its toast remains silent.

Opening or selecting a planet is silent, including navigation from a colony toast.
The mission booster uses one repeating effect, without overlapping copies or
restarts while moving directly between missions. It stops when hover ends, the
mission disappears, the game leaves active play, the window loses focus or its
pointer, or audio is muted. It plays in both Effects and Music modes.

The audio control (or Q) cycles Muted, Effects, and Music. Menu controls register
their click as part of accepting the interaction, so playback does not depend on
egui retaining the widget response until the audio collection system runs.
`just assets` regenerates `assets-runtime` for desktop and browser packages.
