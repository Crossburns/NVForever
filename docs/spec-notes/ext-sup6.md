# SUP NVSE batch 6: the mouse, the cursor, debug texts, the screen, the lockpick menu (2026-11-08, #1008)

22 SUP functions: the mouse (4), the cursor's look (3), the debug texts (6), the HUD's
visibility flags (2), the screen, fonts, plugin headers and the machine (4), the lockpick
menu (2) and the throw held (1). Code: `crates/fnv-game/src/nvse_sup6.rs` (the
functions and `Game::sup`, what the engine fills and takes), the engine's side in
`engine/nvf-play/src/sup_screen.rs` (the mouse, the window, the fonts, the lockpick
menu, the throw; the fake moves, the pin's health, the debug texts) and
`engine/nvf-play/src/cursor.rs` (the cursor's traits). Tests:
`crates/fnv-game/tests/sup6.rs` (the engine's part played by the test) and the unit tests
of `nvse_sup6.rs`.

Source: the GECK wiki export [documented: the page named]. Not probed yet: set `sup6`
of `tools/capture/jip_probes.py` (lab `L5.ext.sup6`) asks the questions of §7; until it
runs, every value a page leaves open is a [hypothesis].

## 1. The mouse

| Function | NVForever |
|---|---|
| GetMousePosition `xOut yOut` | the system's pointer [documented: "system mouse position"]: the window's pixels from its top left, the last place while the pointer is outside the window [hypothesis: the engine knows only its window] |
| GetMouseMovement `xOut yOut request` | this frame's movement (0) or the last frame's that moved (1), in pixels, only in a menu [documented]; 0 0 out of menus [hypothesis] |
| GetMouseWheelMovement `request` | the wheel's notches the same way [documented]; a wheel that gives pixels counts 40 a notch [hypothesis] |
| FakeMouseMovement `x y` | the engine adds the move to the next frame's mouse movement, so it turns the camera or moves the pointer as the mouse would [documented: "Will move cursor\camera by 1 pixel"] |

The `…Out` arguments are written by the VM (`fnv_script::vm` `out_params`), as
GetTFCPosEx.

## 2. The cursor's look

| Function | NVForever |
|---|---|
| GetCursorTrait, SetCursorTrait `trait [value]` | 0 alpha, 1 width, 2 height, 3-5 red, green, blue [documented]; at rest 255, the arrow's own size, 255 255 255 [hypothesis: 255 is the arrow as drawn] |
| SetCursorTraitGradual `trait [start [end [duration [mode]]]]` | the trait set to start, then to end in the duration, modes 0-3 as the page's table [documented]; the trait alone stops it where it is [documented]; a start alone sets it [hypothesis: as SetUIFloatGradual]; another mode reads 0 |

The values are worked out from the game time when read (no per-frame step). The engine
(`cursor.rs`) redraws the arrow when they change: scaled to width × height, its colours
times red/green/blue over 255, its alpha times alpha over 255 [hypothesis: the page
gives no scale].

## 3. The debug texts

| Function | NVForever |
|---|---|
| DebugTextCreate `name x y text` | a text at x, y in the window's pixels [hypothesis: the page's 100 100 and 600 700]; a name already there is moved and changed, not doubled [hypothesis] |
| DebugTextDestroy, DebugTextExists | gone; 0/1 [documented] |
| DebugTextSetPos, DebugTextSetString, DebugTextSetZoom | its place, its text, its size [documented]; the zoom is the letters' height in pixels, 16 until set [hypothesis: the page's example sets 20] |

The engine draws them as plain white text over everything [hypothesis: the page shows
no picture].

## 4. The HUD's visibility flags

GetHUDVisibilityFlags and SetHUDVisibilityFlags are "replaced by an equivalent function
in JohnnyGuitar NVSE" [documented]: they read and write the same flags as
Get/SetHUDVisibilityOverride (`crate::nvse_hud`, `Game::hud.visibility`).

## 5. The screen, fonts, plugins and the machine

| Function | NVForever |
|---|---|
| GetScreenTrait `trait` | 0, 1 the UI's width and height (the tile tree's `screen()`), 2, 3 the window's pixels, 4 `screen()`'s resolutionconverter [documented: the page's table] |
| GetFontTrait `font trait` | 0 the base height, 1 the line height [documented]; NVForever gives the font file's line height (its first number) as the base height and its tallest glyph as the line height [hypothesis: the page's "corresponding with Height UI trait" and "(presumably) real font height"]; fonts 1-9 as `nvf_play::interface::FONT_FILES`, another 0 |
| GetModTraitSTR `index trait` | the plugin's author (0) and description (1), TES4 CNAM and SNAM [documented]; "" for another index or trait (`GameData::plugin_texts`) |
| GetPCTrait | 0 [decision: no channel to the machine's Windows version, memory and time since it started (the 2026-11-08 brief); follow-up] |

## 6. The lockpick menu and the throw

| Function | NVForever |
|---|---|
| GetLockPickMenuValue `request` | 0 the pin's angle (0 left, 90 up, 180 right, as NVForever's menu, `fnv_game::lockpick::Picking::pin`), 1 the lock's level (0, 25, 50, 75, 100 by its class [hypothesis]), 2 the Lockpick skill; -1 with the menu shut [documented] |
| SetBobbyPinHealth `[health]` | sets the pin's health, above 1 too, and returns it; without a value returns it; a new pin starts at 1 [documented; `fnv_game::lockpick::PIN_HEALTH` is 1]; -1 with the menu shut [hypothesis] |
| GetGrenadeTimeHeld | the seconds the attack was held with a throwable weapon in hand (Grenade Throw, Thrown, Land Mine, Mine Drop), kept after the throw and reset at the next press [documented: the page's note] |

The engine writes the menu's state into `Game::sup.lockpick` before the scripts and takes
the new health after them (`sup_screen.rs`); the throw from `PlayerControl::trigger` and
the weapon's animation type (`Combat::player_weapon_is_throwable`).

## 7. Probe set `sup6` (not run yet)

Read-only rows and writes restored at the end (the cursor's traits, a debug text made
and destroyed, the HUD flags put back). The questions: the cursor traits at rest (alpha,
size, colours); SetCursorTraitGradual's value halfway; a debug text's default zoom; the
mouse functions out of menus; GetScreenTrait's five values; GetFontTrait of fonts 1-8;
GetModTraitSTR of FalloutNV.esm; GetPCTrait (for the record only: NVForever gives 0);
GetLockPickMenuValue and SetBobbyPinHealth with the menu shut; GetGrenadeTimeHeld at
rest. The lockpick menu's values while it is open and the throw held need a person at
the controls: not in the set.
