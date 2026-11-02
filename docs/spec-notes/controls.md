# Spec note: the player's controls (Layer 4 play)

Tags as in `nif.md`. The game's controls and their default bindings, as `nvf-play` binds
them (`engine/nvf-play/src/bindings.rs`, `control.rs`, `player.rs`); the player body's
use of them: docs/design/player-body.md.

## 1. Where the game keeps its bindings [observed 2026-10-09]

The `[Controls]` section of the owner's `Documents\My Games\FalloutNV\FalloutPrefs.ini`
(read once for this note; NVForever never reads that folder at run time,
docs/design/install-layout.md) holds one line per control, `Name=XXXXXXXX`. The
install's `Fallout_default.ini` `[Controls]` holds only `bAlwaysRunByDefault`,
`bBackground Mouse` and `bBackground Keyboard`, not the bindings.

Each value is four bytes, written as eight hex digits `00KKMMJJ`:

- `KK`: a keyboard key as a DirectInput scan code [documented: Microsoft DirectInput
  `DIK_*` keyboard constants; observed: `Forward` 0x11 = W, `Back` 0x1F = S, `Jump` 0x39 =
  Space, `QuickSave` 0x3F = F5 and `QuickLoad` 0x43 = F9 match the game's known keys];
  `FF` none.
- `MM`: a mouse button, `FF` none; 0 is the left button (`Use`, attack), 1 the right
  (`Block`), 2 the middle (`Toggle POV`) [hypothesis: from the controls they sit on].
- `JJ`: a controller button, `FF` none (§2.1).

## 2. The bindings [observed: owner's FalloutPrefs.ini; hypothesis: the game's defaults]

| Control | Value | Key | Mouse |
|---|---|---|---|
| Forward | 0011FF13 | W | |
| Back | 001FFF14 | S | |
| Slide Left | 001EFF17 | A | |
| Slide Right | 0020FF16 | D | |
| Use | 00FF0011 | | left |
| Activate | 0012FF0A | E | |
| Block | 00380110 | Left Alt | right |
| Ready Item | 0013FF0C | R | |
| Crouch/Sneak | 001DFF08 | Left Ctrl | |
| Run | 002AFFFF | Left Shift | |
| Always Run | 003AFFFF | Caps Lock | |
| Auto Move | 0010FFFF | Q | |
| Jump | 0039FF0D | Space | |
| Toggle POV | 0021020F | F | middle |
| Menu Mode | 000FFF0B | Tab | |
| Rest | 0014FF07 | T | |
| Vats | 002FFF0E | V | |
| Hotkey1, Ammo Swap, Hotkey3–8 | 0002FFFF … 0009FFFF | 1–8 | |
| QuickSave | 003FFFFF | F5 | |
| QuickLoad | 0043FFFF | F9 | |
| Grab | 002CFF09 | Z | |

Other `[Controls]` values: `bAlwaysRunByDefault=1`, `bInvertYValues=0`,
`fMouseSensitivity=0.0020` (used as radians of turn per mouse count [hypothesis: the
unit]), `fXenonVertLookSpeed=1200`, `fXenonHorizLookSpeed=1500`, `bGamePadRumble=1`.
The second hotkey's line is named `Ammo Swap`.

### 2.1 The controller byte `JJ`

What the `JJ` values are [observed]: `0A` Activate, `0B` Menu Mode, `0C` Ready Item,
`0D` Jump, `0E` Vats, `0F` Toggle POV, `10` Block, `11` Use, `07` Rest, `08`
Crouch/Sneak, `09` Grab, `01` Ammo Swap, `13` Forward, `14` Back, `16` Slide Right, `17`
Slide Left; `FF` for Run, Always Run, Auto Move, Hotkey1, Hotkey3–8, QuickSave, QuickLoad.

The console version's layout [documented: published Xbox 360 control lists for the
game, e.g. magicgameworld.com "Fallout: New Vegas Controls"]: left stick moves, right
stick looks, A activates, B opens the Pip-Boy, X readies and reloads, Y jumps, LT aims,
RT attacks, RB is V.A.T.S., Back waits, a press of the left stick crouches, the D-pad
holds the quick keys.

Reading [hypothesis]: `0A`–`0D` are A, B, X, Y and `10`, `11` are LT, RT, `0E` is RB,
`07` Back and `08` the left stick's press, as those controls sit in the console layout.
A to Y and Back, left stick press, right stick press follow the order of XInput's
button bits [documented: Microsoft `XINPUT_GAMEPAD` `wButtons`], so `09` (Grab) is read
as the right stick's press and `0F` (Toggle POV) as LB, the one shoulder left. `13`,
`14`, `16`, `17` are read as the left stick pushed up, down, right, left. `01` is not
placed.

The menu files name controller buttons for their hotkeys and hints [observed:
`_xbox_button` traits and `<xbuttona>`…`<xbuttonrt>` elements in the interface XMLs,
e.g. `&xbuttona;` on the HUD's activate prompt, `hud_main_menu.xml`], and
`prefabs/button_xbox_hint.xml` lists the button pictures in this order:
`Interface\Shared\Button\general_button_a.dds`, `_b`, `_x`, `_y`, `_lt`, `_rt`, `_lb`,
`_rb`, `_ls`, `_rs`. With a controller in use the hint shows the picture in place of the
key text [observed: the prefab's `_xbox` switch]. `FalloutPrefs.ini` also holds
`bDisable360Controller=0` (another section) [observed].

## 3. Behaviour [hypothesis unless tagged]

- Running is the default while Always Run is on; Run held walks (and runs when Always Run
  is off). Auto Move walks forward until Forward or Back is pressed.
- Ready Item: with the weapon away, draws it; with it out, a tap reloads and a hold of
  0.5 s holsters it. Use with the weapon away draws it.
- Block: guns aim down the sights; fists and melee weapons block. While Block is held
  with a weapon out the player moves at walking speed and the body walks
  (`player::aiming_walks`) [design: the owner's wish, 2026-10-11; not captured].
  Aiming a gun zooms the view to its Sight FOV (camera.md §4).
- Toggle POV: a tap switches first and third person; held while the mouse moves, the
  camera circles the player without turning them, and swings back when let go; begun
  in first person, the view stays third person afterwards (the owner, 2026-10-19). The
  mouse wheel zooms the third-person camera.
- Rest: the Wait menu, or the Sleep menu while lying on a bed
  (docs/spec-notes/wait-menu.md).
- Grab (Z) held picks up and carries what is under the crosshair; E held does too
  (docs/spec-notes/grab.md §2).
- Dropping from the Pip-Boy's ITEMS page is a right click on the row (Shift + right click
  the stack); X is the mod menu's, the original's PC key for Mod; the controller's X
  drops, as the original's `IM_DropButton` (`&xbuttonx;` [documented:
  inventory_menu.xml]) [design: owner 2026-11-02, #1393; a deliberate difference, the
  original's PC drop key is not in the menu file] (pipboy.md §6).
- Not built yet (a line on screen says so): attacking, reloading, VATS, the
  hotkeys and Ammo Swap, Quick Save, Quick Load.

## 4. Captures wanted

- The hold time that holsters with Ready Item, and whether a tap with the weapon out
  reloads only when the clip is not full.
- What Toggle POV held does in first person and in third person, and whether the
  camera stays where it was circled to after letting go.
- Whether the bindings above are the defaults (a fresh `FalloutPrefs.ini`, or the
  game's Controls menu "Default" button).
- The open points of §5.4 (controls turned off by scripts).
- With a controller in the original: which button each control answers to (§2.1 is
  read from the INI and the console layout), how fast the right stick turns at
  `fXenonHorizLookSpeed=1500`, and whether a light push of the left stick walks.

## 5. Controls turned off by scripts

Scripts take groups of controls away from the player and give them back:
`DisablePlayerControls`, `EnablePlayerControls`, and `GetPlayerControlsDisabled` to ask.
The state is the script runtime's (`fnv_game::functions::player`,
`PlayerState::controls_disabled`); `nvforever --play` applies it since 2026-10-10
(`nvf-play` `control::ControlLocks`, §5.3).

### 5.1 The functions [documented: GECK wiki pages DisablePlayerControls, EnablePlayerControls, GetPlayerControlsDisabled]

- All three take up to seven optional 0/1 parameters, one per group, in this order:
  movement, Pip-Boy, fighting, POV, looking, rollover text, sneaking. Opcodes 0x1061
  (disable), 0x1060 (enable), 0x1062 (get) [observed: script-functions-catalogue.md].
- `DisablePlayerControls` turns off the groups whose parameter is 1 and leaves the others
  as they are. With no parameters it is `1 1 1 1 0 0 0`: movement, Pip-Boy, fighting and
  POV go; looking, rollover text and sneaking stay.
- `EnablePlayerControls` turns the groups whose parameter is 1 back on. With no
  parameters it is `1 1 1 1 1 1 1`: everything.
- `GetPlayerControlsDisabled` is 1 when any of the groups picked by its parameters is
  off; with no parameters, when any group is.
- What each group does while it is off:

  | Group | Effect |
  |---|---|
  | movement | the player cannot move; world objects cannot be activated; part of the HUD is hidden: action points, health, crosshair |
  | Pip-Boy | the Pip-Boy cannot be brought up, nor the Wait menu |
  | fighting | no weapon can be brought up; a weapon that is out is holstered |
  | POV | no third-person view; a third-person view is forced into first person |
  | looking | the view is locked where it is |
  | rollover text | no rollover text on world objects |
  | sneaking | the player cannot sneak; a sneaking player is forced out of it |

- `DisablePlayerControls` also resets image-space modifiers, and replaced the older
  `ForcePlayerTo1stPerson`.
- An xNVSE plugin (JohnnyGuitar, `SetDisablePlayerControlsHUDVisibilityFlags`) can choose
  which HUD parts are hidden while controls are off; its list of parts includes the
  three above and, separately, messages, subtitles, the quest reminder and the "info"
  part [documented: its GECK wiki page]. Not a vanilla function; noted for the HUD parts'
  names.

### 5.2 Use in the game [observed]

- FalloutNV.esm calls `DisablePlayerControls` 151 times in 126 script units (119 times
  with no parameters), `EnablePlayerControls` 137 times in 112 units (125 with no
  parameters) and `GetPlayerControlsDisabled` 3 times in 2 units (`fnvtool script
  decompile`). The parameter lists used, most common first: disable `0 0 0 1 0 0 0`
  (8; first person only), `1 1 1 1 1 1 1` (6), `1 1 1 1 0 1 1` (5), `1 1 1 0 1 1 1` (3);
  enable `0 1 1` (3), `1 1 1 1 1 1 1` (2), `1 0 0 1 1 1 0` (2), `0 0 0 0 1` (2).
- A new game (save `nvfnew`, before any input): `GetPlayerControlsDisabled` asked one
  group at a time reads `1 1 1 0 1 0 0`: movement, Pip-Boy, fighting and looking off;
  POV, rollover text and sneaking on. After VCG01 stage 55 moving and looking are on
  again (layer5-captures.md §10.2, §10.3).
- The opening's calls, in order: VCG00 stage 0 `DisablePlayerControls 1 1` then
  `DisablePlayerControls 0 0 0 0 1 0 0`; VCG01 stage 0 `EnablePlayerControls 0 0 0 0 1`
  then `DisablePlayerControls 1 1 1 1 0 0 1`; stage 55 `DisablePlayerControls 0 1 1 0 0
  0 1` then `EnablePlayerControls 1 0 0 1 1 1 0`; stage 110 `DisablePlayerControls 1 1 1
  1 1 1 1` then `EnablePlayerControls 1 0 0 1 1 1 0`.
- **Short parameter lists are not understood.** After VCG00 stage 0's two calls the
  game read fighting off (the `1 1 1 0 1 0 0` above); the runtime, which takes a
  left-out parameter as 0, has `1 1 0 0 1 0 0` there (`--play --new-game`, 2026-10-10).
  So `DisablePlayerControls 1 1` seems to turn fighting off too, yet not POV, which the
  form without parameters turns off [hypothesis: a left-out parameter is not simply 0;
  the rule is not known, and the save may hold a state from before the scripts]. The
  runtime is unchanged for `DisablePlayerControls`; `pc_controls` (§5.4) asks the game.
  In the opening the difference lasts until VCG01 stage 0, three seconds later, which
  turns fighting off.
- **A left-out parameter of `EnablePlayerControls` enables its group** (since
  2026-10-11) [hypothesis]. The opening turns sneaking off (VCG01 stages 0, 55 and 110,
  each with all seven parameters); Doc's farewell line gives back `1 1 0 1 1 1 0`, and
  the trigger outside his door, VCG01RemoveDocCarebearModeTriggerSCRIPT ("Doc never gets
  mad at the player for fighting or stealing... until the player steps outside his
  house", its source), gives back `EnablePlayerControls 1 1 1 1 1 1`: six parameters
  [observed: the SCTX of FalloutNV.esm 0x0011E65E]. No other script of FalloutNV.esm
  gives sneaking back after that (`fnvtool script decompile`: every
  `EnablePlayerControls` whose seventh parameter is 1, or with no parameters, belongs to
  another quest). Read with a left-out 0, the player could never sneak again after the
  opening, which the playtest of 2026-10-11 met ("can no longer crouch with ctrl");
  read as the form without parameters reads it, the trigger gives back everything. The
  same rule makes `VULEnablePipBoyFailsafeTriggerSCRIPT`'s `0 1 1` give back the last
  four groups too. `GetPlayerControlsDisabled`'s short lists keep a left-out 0.
- The capture tools use `DisablePlayerControls 0 0 0 1 0 0 0` to force first person for
  reference screenshots; `IsPC1stPerson` read 1 with every picture taken that way
  (`tools/capture/capture_frames.py`; the view before the call was not recorded).

### 5.3 In `nvforever --play`

After each frame's scripts the runtime's seven flags are copied to `ControlLocks`
(`script::sync_locks`); the next frame's input obeys them. A change is logged ("player
controls: movement, Pip-Boy, fighting, looking disabled by scripts" / "all enabled") and
written to the script trace as a `controls` line.

| Group | While it is off | Tag |
|---|---|---|
| movement | Forward, Back, the slides, Jump and Auto Move do nothing; Activate does nothing; the crosshair and the player's health line are hidden | [documented]; Jump and Auto Move [hypothesis] |
| Pip-Boy | Menu Mode does not open the inventory | [documented] |
| fighting | Ready Item and Use do not draw; a drawn weapon is put away (the holster clip plays); no firing, aiming, blocking, reloading or Ammo Swap | [documented]; aiming and blocking [hypothesis] |
| POV | Toggle POV does nothing; third person and the circling camera go back to first person | [documented]; the circling camera [hypothesis] |
| looking | the mouse does not turn the view (nor circle the camera) | [documented] |
| rollover text | the activation prompt is hidden | [documented] |
| sneaking | Crouch/Sneak does nothing; a sneaking player stands up | [documented] |

- The keys are read as always; `control::enforce_locks` strikes what is locked from the
  frame's requests before anything acts on them. So the test aids that stand in for keys
  and the mouse (`NVF_PLAY_TEST_MOVE`, `NVF_PLAY_TEST_CONTROL`, `NVF_PLAY_TEST_LOOK`,
  `NVF_PLAY_TEST_ACTIVATE`, and `NVF_PLAY_TEST_MENU` and `NVF_PLAY_TEST_DROP` where they
  open the inventory) are held too; the aids that place the player or act for someone
  else are not (`GOTO`, `FACE`, the `aim…` presses, the script aids `walk:` and
  `activate:`, `NVF_PLAY_TEST_TALK`, `--auto-walk`).
- Nothing comes back by itself when a group is enabled again: the weapon stays away, the
  view stays in first person, the player stays standing [hypothesis].
- With movement off, a movement key does not get the player up from furniture
  [hypothesis].
- Not held: the pause menu (Esc), conversations and menus that scripts or actors open,
  what scripts do to the player (`MoveTo`, a script's `Activate`).
- `--new-game`: every group is held from the first frame until the opening's first
  script frame, which turns the controls off itself (the scripts take about a second to
  load) [design].
- The Wait menu (Rest) stays shut with the Pip-Boy group off, as the inventory does
  (docs/spec-notes/wait-menu.md §6); each refused request counts under "Pip-Boy" in the
  summary below.
- Not built: the action-point part of the HUD (NVForever has none yet); the image-space
  reset.
- At exit, when scripts changed the controls at all: "controls summary: scripts changed
  the player's controls N times; disabled at exit: …; held back: …" (per group, the
  frames or presses in which something the player asked for was struck).

**Checks (2026-10-10)**, unattended, `RUST_LOG=info`, each exit 0 with "run finished":

- `--play --new-game --no-voice --run-seconds 140` with boxes and menus answered after
  1 s, Forward held from the first second (`NVF_PLAY_TEST_MOVE=1:1:0`), Activate at 14
  and 15 s, and Crouch/Sneak, Ready Item and Toggle POV pressed at 16 to 18 s. The log:
  "all held until the new game's opening starts"; on the first script frame "movement,
  Pip-Boy, looking disabled by scripts" (VCG00 stage 0); 3.4 s of game time later
  "movement, Pip-Boy, fighting, POV, sneaking" (VCG01 stage 0); at game time 76.8, with
  VCG01 stage 55, "Pip-Boy, fighting, sneaking". The player's status line reads 0
  units/s commanded every second until stage 55 and 344 from then on (the character
  still moved up to 40 units in that time with nothing commanded, its status "sliding"
  and touching the bed 00103E22 it starts beside [hypothesis: collision settling or Doc
  Mitchell walking past; not the controls]). Both Activates were ignored; the three presses were struck (sneak,
  weapon and view unchanged a frame later). Pictures: no crosshair and no health line
  at 20 s, both back at 135 s. Summary: "scripts changed the player's controls 3 times;
  disabled at exit: Pip-Boy, fighting, sneaking; held back: movement 1341, fighting 1,
  POV 1, sneaking 1".
- The same for 30 s with `NVF_PLAY_TEST_LOOK=0.1:90:0` (the smoke test in CLAUDE.md):
  the look is ignored; "held back: movement 418, fighting 1, POV 1, looking 1, sneaking
  1". A look at 14 s, when VCG01 stage 0 has given looking back, turns the view.
- `--cell Goodsprings --play --run-seconds 45` with the same presses and Forward held:
  no control is ever off (no "player controls" change, no summary, the trace's final
  `controls_disabled` all false); the player sneaks, draws, goes to third person and
  runs north until a fence.
- The force-greet smoke test of CLAUDE.md (a script aid's `activate:`, then Sunny's
  conversation): as before.

### 5.4 Open [hypothesis until captured]

1. With only movement off, is the rollover text still shown on an object the player
   looks at (ours: yes, since rollover text is its own group)?
2. Does Jump work with movement off? Auto Move?
3. After `EnablePlayerControls`: does a weapon holstered by the fighting flag come back
   out, a view forced to first person go back to third, a player forced out of sneak
   crouch again?
4. With looking off, does Toggle POV held still circle the camera?
5. With movement off, can the player get up from a chair?
6. With fighting off, can the player still aim or block (Block held)?

Capture job `pc_controls` (`tools/capture/jobs/make_controls_jobs.py`, save `nvfdoc`,
about 3 minutes, no input; not run yet) asks the game: the seven flags after each form
of the two functions (the defaults of §5.1, one group at a time, all seven with two given
back, and the short parameter lists of §5.2: `1`, `1 1`, `0 1 1`, `0 0 0 0 1`); with
movement off, the position while Forward is held, after Jump and after Auto
Move (point 2); a drawn weapon, third person and sneaking each before the lock, under it,
after the control is tapped under it, and after `EnablePlayerControls` (point 3; the
[documented] forcing too); Menu Mode with the Pip-Boy off; and at the house door,
pictures with the HUD on and `GetCrosshairRef` with nothing off, movement off and
rollover text off, then Activate with movement off and with everything on (point 1 and
the HUD parts). Points 4 to 6 need a mouse, furniture or a way to read blocking and are
not in it.
