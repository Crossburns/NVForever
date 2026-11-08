# JIP LN NVSE, batch 9 (2026-11-08, #1006)

The HUD cursor and the mouse, camera and menu switches, the hotkeys' inventory
references, the Pip-Boy radio, the game volumes, fonts and interface colours, the enemy
health bar's actor, the player's immortal mode, interaction by form type, GetPosEx, and
the NIF block values (material properties; translation and rotation into script
variables). The owner's mod lists call ToggleHUDCursor (20 calls) and
SetMaterialPropertyValue (21) most of what JIP had left (xnvse-remaining.md §3).

Game side: `crates/fnv-game/src/nvse_jip9.rs` (the switches, `Jip9State`) and
`crates/fnv-game/src/nif_blocks.rs` (the NIF block values, on the scene mirror). Engine
side: `engine/nvf-play/src/script_switches.rs` (the switches reach the controls, the
engine tells the cursor, the window, the volumes and the health bar's actor) and
`engine/nvforever/src/nif_mirror.rs` (material values read from the model file, drawn
when a script changes them). Sources: the GECK wiki export, each function's page
[documented: the page named]. Not probed yet: the probe set `jip9`
(`tools/capture/jip_probes.py`, lab `L5.ext.jip9`) asks the original what the
[hypothesis] values below leave open. Nothing here is saved [hypothesis: the pages say
nothing of saves].

23 functions; JIP 201 → 178.

## 1. The cursor, the mouse, the camera

| Function | NVForever |
|---|---|
| ToggleHUDCursor `[toggle]` | the state after it; on, the pointer shows over the world, mouse look stops and the HUD takes the pointer as a menu does (clicks, mouseover) [documented]. Any menu opening turns it off [documented: "switched off upon opening any game menu"]; it cannot be turned on while a menu is open [hypothesis] |
| GetCursorPos `axis [useXML]` | `X` or `Y`: the window's pixels from the top left [documented: "(0, 0) is at the top-left corner"], or (useXML) the interface's units, 960 to the window's height, the tile tree's pointer. Outside MenuMode and the HUD cursor, 0 [hypothesis: the page gives no value there] |
| SetCursorPos `x y [useXML]` | the engine moves the pointer there, in MenuMode or with the HUD cursor only [documented]; it reads back at once |
| ToggleMouseMovement `[flags]` | 0 both axes off, 1 horizontal only, 2 vertical only, 3 both [documented]; the state after it; over 3, 3 [hypothesis]. The mouse look drops the axes turned off; a controller's stick is not held [hypothesis: the page names the mouse] |
| ToggleVanityWheel `[toggle]` | the wheel zooms the third-person camera, or not [documented]; the state after it |
| ToggleCameraCollision `[toggle]` | the third-person camera stops in front of walls, or goes through them [documented: "the collision box of the player's camera"]; the state after it |
| CloseActiveMenu `[closeAll]` | the open menu closes as with the player's OK (`Game::close_menu`), or every open one [documented] |

## 2. Hotkeys, radio, volumes, fonts, colours, the health bar

| Function | NVForever |
|---|---|
| GetHotkeyItemRef `key` | an inventory reference of the key's item in the player's inventory (the same eight keys GetHotkeyItem reads); 0 for an empty key or an item no longer carried [documented: "an Inventory Reference"; hypothesis: not carried] |
| SetHotkeyItemRef `key` on an inventory reference | keys 1 and 3-8 [documented: the page's values; 2 is Ammo Swap]; the player's items only [hypothesis]; 1 when set |
| GetPipboyRadio | the station's placed reference while the Pip-Boy radio plays, else 0 [documented] |
| GetGameVolume `type` | 0 master, 1 footsteps, 2 voice, 3 effects, 4 music, 5 radio, 0-100 [documented], as the engine plays them; past 5, 0 |
| SetGameVolume `type [level]` | clamped to 0-100 [hypothesis]; without a level the default: master 100, footsteps 50, voice 60, effects 100, music 60, radio 50 [documented: the page's "the value defined in the game's INI file"; observed: these are the original's sliders on an untouched install, `audio_out::DEFAULT_VOLUMES`]. The engine plays the new level at once; it is not written into the settings [hypothesis] |
| SetFontFile `type path` | types 1-8 and 10-89, a file under Data [documented]; 1 when the file is there [hypothesis: the page's "on success"]; GetFontFile reads it after |
| SetSystemColor `interface r g b` | 1 HUD, 2 HUD alt, 3 terminal, 4 Pip-Boy, 5 main menu [documented]; each part rounded into 0-255 [hypothesis]; the HUD's and the Pip-Boy's colours change at once |
| GetEnemyHealthTarget | the actor whose health bar the HUD shows, else none [documented], as the engine's `hud_target` decided this frame |

## 3. Immortal mode, interaction, GetPosEx, the NIF block values

| Function | NVForever |
|---|---|
| ToggleImmortalMode `[mode]` | 0 off, 1 the player's Health never below 1 HP, 2 no Health or limb damage [documented]; the mode after it; other values change nothing [hypothesis]. Mode 1 holds the player at 1 HP for hits and effects, with no death and no knockout; mode 2 deals hits no Health or limb damage and effects no Health damage. Script functions that set or damage the value (DamageActorValue, SetActorValue) are not held [hypothesis: the page speaks of taking damage] |
| Set/GetInteractionDisabledType `disable type` / `type` | the player cannot activate objects of the type by hand and sees no rollover text; scripted Activate still works [documented]. The type codes: GetIsUsedItemType's and RemoveAllTypedItems' (`functions::furniture`), with NPC_ 42 and CREA 43 [hypothesis: the same numbering]. SetInteractionDisabled's references (built before) now count too |
| GetPosEx `x y z` | the reference's position into the three variables [documented] |
| GetMaterialPropertyValue `block id` | the block's NiMaterialProperty: 0-2 specular R G B, 3-5 emissive R G B, 6 glossiness, 7 alpha, 8 emissive mult [documented: the value IDs]; read from the reference's model file (the emissive mult 1 in files without it); 0 for a block without one, a missing block, an ID past 8 or before the 3D is told [hypothesis: the page names no failure value] |
| SetMaterialPropertyValue `block id value` | the value reads back at once; the engine draws that shape with its own copy of its material: the specular tint, the emitted light (colour × mult), the roughness of the glossiness, the alpha [hypothesis: the spawn's own tuning of the emitted light is not redone]. This reference's only [hypothesis]. 0 [hypothesis: as the other setters, though the page names a result] |
| GetNifBlockTranslationAlt `block x y z [getWorld] [pcModel]` | 1 and the translation to the parent, or (getWorld) in the world through the nodes above and the reference's place; a missing block 0, the variables kept [documented] |
| GetNifBlockRotationAlt `block x y z [which] [pcModel]` | 0 the angles about the parent's fixed axes (GetNifBlockRotation's), 1 the same in the world, 2 about the turning axes (X, then Y, then Z) to the parent, 3 the same in the world [documented: the table]; a missing block 0 |

The NIF block values use the scene mirror (docs/design/nif-blocks.md): the tree is the
engine's, told once a script asks, so the first call on a reference reads 0. pcModel is
not read: the mirror has the drawn model. The world place turns the model by the
reference's angles as a node's are turned [hypothesis]. A block name with a `:n` suffix
the mirror gave a repeated name reads the material of its unsuffixed name in the file
[hypothesis].

## 4. Probe questions (set jip9)

- The switches without an argument, after one and put back; ToggleMouseMovement 9 and
  ToggleImmortalMode 5 (out-of-range values).
- GetCursorPos outside MenuMode (0?).
- GetGameVolume 0-6 on the probe install; SetGameVolume 4 150 (clamped?), SetGameVolume 4
  without a level (the INI's 60?).
- SetFontFile with type 9 and with a missing file; GetFontFile 2 after.
- The hotkeys' references against GetHotkeyItem; SetHotkeyItemRef's result on key 1 and on
  key 2.
- GetPipboyRadio and GetEnemyHealthTarget at the save.
- GetPosEx against GetPos; GetNifBlockTranslationAlt and GetNifBlockRotationAlt on the
  player's Bip01 Head (all modes, the world place against GetPos); a missing block (the
  variables kept?).
- The player's UpperBody material values 0-9, SetMaterialPropertyValue's result and the
  value read back.

## 5. Not in this batch

The text input and quantity menus (ShowTextInputMenu, SetTextInputString,
SetTextInputExtendedProps, ShowQuantityMenu) need a script-opened menu with a UDF
callback; GetMenuItemFilter and GetMenuItemListRefs the engine's inventory, container and
barter list state told to the game; the barter and recipe menu getters likewise. They are
the next UI batch.

## 6. Follow-ups

- SetFontFile: the engine loads its fonts at start; a font a script swaps is read by
  GetFontFile only until the engine reloads fonts from `Game::font_file`.
- SetSystemColor: NVForever draws the HUD's alt colour, the terminal and the main menu in
  fixed colours (`reticle.rs`, `terminal/screen.rs`, `main_menu.rs`); interfaces 2, 3 and 5
  are kept by the game (`Jip9State::colours`) for when those read it.
- The hotkeys: the engine's hotkey slots (`nvf_play::hotkeys`) and the scripts' table
  (GetHotkeyItem, SetHotkeyItem, this batch) are not one yet.
- SetMaterialPropertyValue on actors: the actor meshes are spawned by `nvf_assets::actor`,
  which the mirror's material table (read from the reference's MODL) does not cover; an
  actor's body parts read 0.
