# Loading screens in `--play`

Owner request 2026-10-10: when the player changes cell through a door, pause the game and
show a loading screen while the destination loads; everything is on hold until loading
is done; the picture is one of the game's loading-screen textures, as the plugins list
them. Records and what is known about them: docs/spec-notes/loading-screens.md.

Code: `engine/nvf-play/src/loading.rs` (state, screen), `crates/fnv-world/src/loadscreen.rs`
(records, choice), `engine/nvforever/src/world.rs` (`loading_gate`, faster streaming
behind the screen).

## 1. What happens

1. A door (activation) or a script moving the player sends `TravelRequest` instead of
   `Travel`.
2. The same frame the loading screen is spawned, `PlayState` goes to `Loading` and Bevy's
   virtual clock is paused.
3. Two frames later (`SHOW_FRAMES`), when the screen has been drawn, the request goes on
   as `Travel`. Physics rebuilds its cells and places the player; the renderer drops its
   cells and streams the destination's. These are the long frames, and they now pass
   behind the picture instead of in front of a frozen view of the old place.
4. Behind the screen, streaming is faster than in play: the renderer loads cells for
   200 ms each frame instead of one cell a frame, physics takes up to 8 prepared cells a
   frame instead of 1.
   Until then the renderer loads the cells around the arrival's grid square
   (`Streamer::arrival`), not around the camera: in the frame of the travel the camera
   still has its transform of the place left, and around it (an interior's coordinates,
   far out in the worldspace) empty cells loaded at once and counted as the
   destination's; the screen then came down with the arrival's own cells still to load
   one a frame, and they faded in in front of the player (playtest list 4, items 9 and
   10, 2026-10-13) [observed: `--cell GSGasStation` through its door: 26 cell loads in
   all after the fix, no fade in the frame after the screen].
5. The screen comes down when physics has every cell it asked for around the player
   (`PhysState::loading_done`), the renderer has all the cells around the camera
   (`Streamer::settled`, reported through `Loading::render_busy`) and no actor in view is
   still waiting for its parts to be read (`readahead::actors_waiting`, reported through
   `Loading::actors_busy`; owner 2026-10-24: NPCs were seen being drawn in after the
   screen), the pipelines they are drawn with are ready (`pipeline_gate.rs`, reported
   through `Loading::pipelines_busy`, #1520), and that has been so
   for 0.25 s and 3 frames (`SETTLE_SECS`, `SETTLE_FRAMES`: time for textures and meshes
   to reach the graphics card). `PlayState` returns to `Playing`, the clock runs again.
6. If loading has not finished after 120 s (`TIMEOUT_SECS`) the screen comes down with a
   warning, so an unattended run cannot hang there.
   Pipelines have a shorter limit of their own: 15 s (`PIPELINE_CAP_SECS`) once they alone
   are waited for, with a warning naming those still waiting. One that has failed for good,
   or is still being retried 15 s after it was asked for, is logged with its label and not
   waited for again.

   Without this wait the view stayed flat under the HUD for 1-2 s after a start's screen
   came down, then the world appeared in one frame, with 104-125 pipelines still waiting as
   it came down [observed: Goodsprings, main e2517687 and the same with logging, all 19 starts with shots, across
   pool sizes]. A task holds Bevy's shader cache lock while it composes a shader [design:
   bevy_render 0.20.1 `pipeline_cache.rs`], and more threads did not shorten the wait
   [observed: 1 to 16 threads, about 3.7-4.1 s to clear in all but one batch]. Objects first seen after the screen is down are not covered.

The player arrives standing: physics puts the character on the support the arrival rule
found and counts it as on the ground until its first step, so the body does not play the
jump loop behind the screen and a landing when it comes down (`PhysState::arrive_player`).

## 1a. The start (owner 2026-10-24)

A `--play` run that starts in the world (not on the main menu, `--menu`) is held behind
the same screen from its first frame (`nvf_play::loading::StartCover`, asked for by
nvforever). Before, the world was in view while the script data still loaded on its
thread (4–9 s on an unbusy machine, up to 21 s with three smoke games starting at once),
and NPCs were drawn in as their parts were read. The screen comes down when what step 5
waits for is in, and the scripts are made (and a `--load`ed save put back into them).
A travel asked for under it goes out behind it and is waited for too.

- Its time counts on no clock: the test aids' clock (`pause::PlayClock`) and
  `--run-seconds` count from when it comes down, as if the run began then. A step at
  14 s comes 14 s after the world can be played, with the world and the scripts run for
  14 s; the world aids' late-start rule (`pause::WorldAidClock`, `SCRIPTS_START_SECONDS`)
  then never holds them back. A run lasts its seconds plus its start.
- **The picture first.** The first render frame blocks on a blit pipeline [design:
  bevy_core_pipeline 0.20.1 `prepare_view_upscaling_pipelines`], compiled on the pool that
  also reads models and cells, so the world's reads delayed the screen: its picture showed
  in at most one capture of three starts [observed]. So under the start's screen cells and
  actors start loading only once the screen has had `PICTURE_FRAMES` render frames with the
  pipelines it is drawn with ready, or after `PICTURE_CAP_SECS` (`Loading::world_held`). Log: `loading
  screen: the world starts loading <s> s after the screen (drawn)`.
- Log: `loading screen: the start is held until the world can be played`, `loading
  screen down after <s> s (the start: cells, actors and scripts ready); …`, or after
  120 s `the start still waits for <what> after <s> s; going on without it`.
- A run taking over from another (`--handoff`, loading a save while playing) shows its
  window once this screen is down.
- [hypothesis] The original never shows the world before its scripts run: it reads the
  plugins, scripts and all, before the main menu.

## 2. What is on hold

- Everything that runs only in `PlayState::Playing`: the player's input, movement and
  physics steps, activation, combat, the game clock, opening menus and the pause menu.
- Scripts: `run_scripts` returns while a loading screen is up, as it does under the
  pause menu. Loading-screen time does not count for scripts in the original either
  [observed: layer5-captures.md §8.3 e]. The two frozen frames after a change of place
  that stood in for a loading screen remain (they now follow the real one).
- Everything driven by `Time`: animations, doors, travelling NPCs, the sky's clock,
  on-screen messages, the `NVF_PLAY_TEST_*` aids and `--run-seconds` (a run lasts its
  seconds of game, plus the loading).
- All input (owner 2026-10-27: New Game could be clicked and the arrow keys moved the
  main menu while a save loaded): `loading::hold_input` lets go of every key and mouse
  button in `PreUpdate`, after the controller's menu keys and the test aids' presses and
  before the interface reads the pointer, and the screen blocks the pointer from what
  is under it. A key held through the screen counts only once pressed again.
- Not on hold: streaming (render and physics), distant LOD generation, and a voice line
  already playing (as under the pause menu; voice.rs).

## 3. The screen

- The record: `fnv_world::loadscreen::choose` over every LSCR of the load order. Screens
  that belong to the destination (its interior cell, or a grid square within 2 of the
  arrival square in its worldspace) and screens for anywhere are both allowed; when the
  destination has its own, one of them is taken every other time. Both rules are
  hypotheses (spec note §3).
- The picture: the record's ICON, read through the texture cache like any other
  texture. Scaled to cover the window without changing its shape; what does not fit is
  cropped evenly (`loading::cover`). A record without a picture, or one whose texture
  is missing, gives a black screen with the text.
- The text: the record's DESC in a band along the bottom, with "Loading...".
- The layout is NVForever's own. The original's roulette wheel, level progress and text
  box (LSCT) are not drawn.

## 4. Scope and open points

- Travel and a world start (§1a) show a loading screen. A New Game chosen on the main
  menu still loads behind the menu and the intro film.
- Travel without `--play` (the free camera following `--npc-travel` NPCs) is unchanged:
  no loading screen.
- The game's rules for choosing a screen and its layout are open (spec note §3, §4).
- Test aid: `NVF_PLAY_TEST_LOADING_SHOT=<png>` saves the window when the travel goes out
  (the screen is up); later screens of the run go to `<name>-2.png`, and so on.

## 5. Checks

- `fnv-world`: `loadscreen::tests` (locations, choice).
- `nvf-play`: `loading::tests` (cover, destination, and the sequence: screen up and
  clock paused before the travel goes out, held while the renderer is busy, down and
  playing again after; held while actors are read; the start's screen and its clock;
  a travel under the start's screen).
- In the game (owner's data): Doc Mitchell's front door, both ways; see the pull request.
