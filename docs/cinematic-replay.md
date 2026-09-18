# Cinematic combat replay

Choose **Schematic** or **Cinematic** above the battle list, then select a battle.
Schematic remains the default. Cinematic uses individual isometric ships and defenses,
with a planet for ground combat and open space for fauna encounters. Space or the
Play/Pause button pauses the entire scene; Left/Right or the −/+ buttons change
speed from 0.25× to 64×. The combat settings gear shares the same speed setting.
Close or Escape returns to battle selection. A completed movie offers Replay.

The movie consumes `MissionReport` without modifying it or running combat again.
Stable combatant IDs retain each ship through hits, repairs, and casualties. Both
sides' salvos overlap, but target hit order is preserved and a lethal hit waits for
the victim's final recorded launch. Shared planetary shield hits precede hull damage
to protected defenses. Repair trucks heal the recorded targets and amounts; the
report does not identify which particular truck supplied each repair, so surviving
trucks are assigned only for presentation. Repairs can overlap fire at other targets.
Retreats, returning probes, bombing, planet destruction, and draws follow the report.
Noncombat orbital structures have no invented attacks.

The renderer uses a single clock for trajectories, sprite banking and recoil, engine
glows, navigation lights, shield impacts, explosion atlas frames, particles, stars,
comets, and parallax. It caches actor layouts and indexes shot time ranges rather
than repeatedly walking the entire shot history. Every combatant remains represented;
larger fleets scale down to fit the viewport. Source artwork and generation prompts
are documented in [cinematic-art.md](cinematic-art.md). Runtime textures are generated
by the existing asset pipeline and included by normal native and web packaging.

Visual pacing was informed by [Stellaris battle screenshots](https://forum.paradoxplaza.com/forum/threads/obligatory-space-battle-screenshot-thread.927576/).
All ship and structure artwork is derived from Stellarion's own shop references.

## Local verification

Run these commands from the isolated checkout:

```text
just assets
cargo test --lib core::combat::cinematic -j6
cargo test --lib core::combat:: -j6
just lint
just check-wasm
just assets-check
```

On Windows with a GPU, this opt-in test renders the real painter and controls to
images without opening a window or connecting to a multiplayer backend:

```text
cargo test --lib render_cinematic_preview -j6 -- --ignored --nocapture
```

The frames are written under ignored `target/cinematic-preview/`. They include
entrance, active combat, shield impact, repair, destruction, the outcome banner,
and a 640×480 viewport. It uses the generated runtime textures with the same
filtering and alpha settings as the game; run `just assets` first.
