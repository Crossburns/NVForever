# Design: the screen menu (the Pip-Boy's menus without the Pip-Boy)

Status: built (issue #1228, PR #1229, 2026-10-31). Owner's request: a second way to show the Pip-Boy's
menus that does not raise the arm. The same pages and tabs, drawn full screen over a
blacked-out world as the barter and container menus are, with the device's three buttons
(STATS, ITEMS, DATA) moved into a header along the top. The player switches between it and
the Pip-Boy on the arm with the Gameplay setting Use Pip-Boy, which also takes the Pip-Boy
and its glove off, or puts them back on (owner, 2026-10-31). **The goal behind it: the inventory never needs a Pip-Boy to be
equipped** (owner, 2026-10-31). With no Pip-Boy worn, the menus open as the screen menu
whatever the setting says (§2.1).

Everything here is [design] unless tagged otherwise: the original has no such menu.

## 1. What stays the same

The screen menu is a second *frame* for the Pip-Boy's menus, not a second set of menus. It
reuses, unchanged:

- **The model and the pause.** Tab (the control Menu Mode) opens the inventory model
  (`menu::open_menus`, `Mode::Inventory`) and pauses the game (`PlayState::Menu`) exactly as
  now. A script that has turned the Pip-Boy off (`DisablePlayerControls` with its Pip-Boy
  flag, `menu.rs` `locks.pipboy`) [documented: GECK wiki DisablePlayerControls] keeps the
  screen menu shut too. That is a quest locking the menus for a scene, not a question of
  whether the device is worn, so it is honoured in both frames [design: lifting it would
  let the player open menus mid-scene that the quest assumes are shut].
- **The pages.** `pipboy::Page`, each page's five tabs, the tab a page remembers
  (`PipBoy::tabs`), the page modules (`stats`, `items`, `data`), their sub-screens (repair,
  item mods, the hotkey panel, fast travel, the radio, the maps) and their keys. Nothing in
  a page module learns which frame it is in, except where §4 says so.
- **The image.** The menus are still drawn by the 2D camera into `PipBoyScreen::image` on
  the 1024 × 768 canvas [documented: `menus\globals.xml` `_pipboy_width`/`_pipboy_height`,
  docs/spec-notes/pipboy.md §3], and the pointer is still mapped onto the canvas and fed to
  Bevy's picking as `PIPBOY_POINTER`. The screen menu is the flat-image path that already
  exists (`pipboy.rs` `flat_screen`, used when nothing shows the model's screen), made the
  chosen path instead of the fallback.
- **What scripts see.** `MenuMode` codes (`pipboy::menu_code`: 1002 STATS, 1003 ITEMS, 1023
  DATA, repair and mod codes) are reported the same in both frames, so quest scripts and
  extender functions that test for the Pip-Boy's menus work unchanged.
- **The Pip-Boy light.** Holding Menu Mode while playing still toggles the light
  (`flashlight.rs` `MenuKeyHold`); a tap opens whichever frame is chosen.

## 2. What changes

| | Pip-Boy (now) | Screen menu |
|---|---|---|
| Opening | `pipboy.kf` raise clip to `Hit` (0.33 s) [observed, pipboy.md §1] | at once [design] |
| Closing | the clip plays on to its end (0.73 s) | at once |
| View | forced to first person, given back after (`saved_pov`) | untouched; third person stays third person |
| Body | left arm and device drawn, screen textured with the image | nothing drawn; the arm never moves |
| Behind the menu | the world, through the device's glass | the whole screen blacked out evenly, 0.98, square to its edges (§3.3) |
| Page choice | three buttons on the device (`PipBoyInput::Page` from the body's hit test) | a header on the heading's line, in place of the page's title (§3.1) |
| Size | the device's screen, about half the view [observed: the owner's screenshots] | the header and the canvas 92% of the view's height, centred (§3.3) |
| Background picture | `Interface\Shared\Background\pipboy.dds`, 90 in the menu file and a fitted 129 here (`device_screen.rs`, pipboy.md §3), scanlines over it | neither: the shade shows through, as behind the barter menu; scanlines off [design: they belong to the device's glass] |
| Raise/lower sounds | the Pip-Boy's own (`pipboy/sounds.rs`) | the sounds the container and barter menus play on opening and closing (`ui_sounds.rs`); tab, page and hover sounds unchanged |

The colour stays the interface's (`pipboy_amber`, or whatever HUD colour the player set), so
the screen menu matches the barter, container and message menus beside it.

### 2.1 Without a Pip-Boy

The inventory, and every other page, must not depend on a Pip-Boy being equipped.

- **The frame in use** is `Frame::Screen` when the setting says so, *or* when the player
  wears no Pip-Boy, whatever the setting (`pipboy::frame_for`). "Wears a Pip-Boy" is an
  equipped item in the Pip-Boy's body slot (`pipboy::wears_pipboy`: ARMO biped flag 1 << 6,
  the bit the body draws on the arm, `fnv_actor::resolve::slot::PIPBOY`) [documented: ARMO
  BMDT]. It is chosen when the menus open and followed while they are up: taking the
  Pip-Boy off in ITEMS > Apparel moves the menus to the screen at once, putting it on (with
  the setting off) brings the arm up.
- So the player can unequip the Pip-Boy (from ITEMS > Apparel, a mod that removes or
  replaces it, or a script's `UnequipItem`) and Tab still opens the full inventory.
- All pages work without it: STATS, ITEMS, and DATA with the maps, quests, notes and the
  radio [design: the owner's goal is access to the menus; the original's tie of these to the
  device is lore, not a rule NVForever keeps]. Left open whether the radio and the maps
  should need the device (§7).
- Before this, the menus already opened with no Pip-Boy worn, as a bare flat image at 90%
  of the view (`pipboy.rs` `flat_screen`, the fallback for "nothing shows the model's
  screen"). The screen menu replaced that fallback, so there is one look for it, not two.
- Scripts: `MenuMode` codes are reported as before (§1), so quest scripts that wait for
  the Pip-Boy's menus to open still see them open.

## 3. The layout

### 3.1 The header

The device's three buttons become a header: **STATS  ITEMS  DATA** standing on the page
heading's top line, in place of the page's title, so the page's name is not shown twice
(owner, 2026-10-31; a first build had the names in a strip above the image and the title
under them).

- The title is left out by `frame::heading_title`, which every page heading draws its title
  through (ITEMS and STATS by `frame::heading`, the repair list and the mod menu by
  `heading_named`, DATA by its own heading); in the screen menu (`Kit::screen`) it draws
  nothing and the line runs on from the lead line to the first card.
- The names stand above the line, their boxes' feet on it (`frame::header_boxes`), from
  where the title starts (x 70). The cards of figures hang below the line (from x 215 on
  ITEMS, 270 on STATS) [documented: `inventory_menu.xml`, `card_info.xml`], so the names,
  about 485 across, pass over them without touching. They lie in the canvas's top margin of
  50, which the screen menu shows.
- The names are in the title's font (font 4), left to right in the device's order. Each
  sits in a box as wide as a tab's (its text and 35) and as high (its text and 15), the
  boxes 40 apart; each is centred in its box by its letters: across by the line's width,
  down by the tallest letter's rise (the font's own baseline left them high; owner,
  2026-10-31).
- The page shown is boxed and filled as the tab shown is, its name at `TEXT` (255); the
  other two at `DIMMED` (128). A name under the pointer is boxed, unfilled, and plays the
  tab hover sound (`sounds::TabHovered`).
- A click on a name writes `PipBoyInput::Page(p)`, the same message the device's buttons
  write (`Clickable`, `on_click`); the names are drawn into the image with the rest, so the
  image's pointer picks them as it picks the tabs.
- No key hint in the header: the switch is a setting, not a key in the menus (owner,
  2026-10-31).
- The header is drawn by `frame::draw` with the frame, again whenever the page changes.

### 3.2 The tab line

Unchanged: five tabs along the foot at y 650 (`frame.rs` `TABLINE_Y`), clicked or stepped
with A/D and Left/Right. The header is page choice, the foot is tab choice, the same split
as on the device.

### 3.3 Size and sharpness

- The canvas's image is drawn 884 interface units high, 92% of the view's height, centred
  [design: about the barter menu's footprint, its 800 of 960 is 83%, plus the Pip-Boy
  canvas's own empty margins of 50 round `MAIN`] (`pipboy::SCREEN_IMAGE_H`). On a 16:9 window that leaves bars at the sides, the same black shade
  as the rest of the world behind the menu.
- The shade is a full-window node (`pipboy::ScreenShade`) that holds the image, spawned and despawned with them. It blacks out the whole screen evenly,
  0.98 black square to its edges (`pipboy::SHADE`) [design: the owner, 2026-10-31: "black
  out the entire background screen", then "black should extend the entire screen" and
  "square up against the edge of the screen" after an oval fade; the barter menu's 0.93
  let the world show through the dimmed hints and the map].
- The image itself is cleared to nothing, not black, so the menus lie on the shade and not
  on a box; it shows the canvas's part `pipboy::SCREEN_CROP` (955 × 750: `MAIN` and its
  margin of 50 on every side), so the menus sit centred.
- The HUD's corner and quest messages are hidden while the screen menu is up, as behind
  any menu (`corner_messages::held`: every state but play, issue #1239).
- **Sharpness.** The image is 1280 × 960 (`pipboy::TARGET`, one pixel per interface unit
  at 960 units per view height). Drawn at 85% of a 1440-line window it would be magnified
  1.6×. In the screen menu the image is made again larger (`pipboy::image_scale`): by the
  window's physical height × 884 / 960 / 750, in steps of an eighth, from 1 up to 3, and
  the screen camera's render target is given the same scale factor, so the menus still
  lay out in the canvas's units and the pointer (logical units) is unchanged. It goes back
  to 1 on the device. The game's bitmap fonts are still magnified when the window is high,
  but the picture is sampled once, not twice.

### 3.4 Wider screens (not in the first build)

A canvas widened to the window's shape (1365 × 768 at 16:9) would give the lists and the
item cards more room, but every page lays out on the menu files' fixed numbers, so each page
would need a widened layout. Left open (§7); the first build keeps 4:3 with side bars.

## 4. Code shape

- **The choice.** The Gameplay setting Use Pip-Boy (`pipboy::UsePipBoy`, key `UsePipBoy`
  in `NVForever.ini` `[Settings]`, `settings::add_setting`, group `pause::GROUP_MENUS`). On
  by default: a new player sees the original's way first [design; owner may flip it, §7].
  `pipboy::apply_use` makes the player follow it (`pipboy::use_changes`):
  - Off, at all times: whatever is worn in the Pip-Boy's slot and the Pip-Boy Glove (ARMO
    00025B83 `PipBoyGlove`) are taken off, so a loaded save or a script that equips them
    (Doc Mitchell's, a console `equipitem`) does not bring them back while it is off.
  - On, once when it changes (the settings page, or the INI read at start): a carried
    Pip-Boy is put on if none is worn (the Pip-Boy 3000, 00015038 `PipBoy`, first), and
    the glove with it if carried; never the glove alone. Afterwards the player may take
    them off by hand.
  - A change the game refuses (a script's equip lock) is logged once
  [observed: the two FormIDs, FalloutNV.esm; design: owner, 2026-10-31, and the merge
  coordinator's review of #1229].
- **`PipBoy::frame`** is the frame in use (`pipboy::Frame { Device, Screen }`).
  `follow_menu` sets it when the menus open; `follow_frame` follows the setting and the
  Pip-Boy worn while they are up. With `Frame::Screen`, `raise` and `lower` jump straight to
  `Phase::Up` and `Phase::Down`, and the view is not saved or forced to first person.
- **`PipBoy::on_arm`** (shown and on the device) is what the body (`player_body.rs`: the
  clip and the arms' field of view), the device (`pipboy_arm.rs`: the screen texture, the
  glows) and the blur behind it (`imagespace.rs`) follow in place of `shown`. So in the
  screen menu none of them moves, and `PipBoyScreen::on_model` stays false.
- **`flat_screen`** is the screen menu's presenter: wanted when `phase == Up` and (the
  frame is `Screen` or nothing shows the model's screen); it spawns the shade and the
  image, maps the window's pointer onto the canvas as before, and
  spawns them again when the image's scale changed (its crop is in the image's pixels).
- **`frame::draw`** leaves out the device's background picture and scanlines in the
  screen menu; the page modules do not branch.
- No fade: the menus are there at once, as the barter menu is [design].

## 5. Switching

- Only in the settings page's Gameplay tab: Use Pip-Boy (§4). No key in the menus [owner,
  2026-10-31: "not on the Pip-Boy screen itself"]. With the menus up the frame still
  follows the Pip-Boy being taken off or put on in ITEMS > Apparel (`follow_frame`).
- Not done: a key outside the menus that opens the other frame directly. Left open (§7).

## 6. Checking it

- **Unit tests** in `pipboy.rs`: the frame for each setting and Pip-Boy worn; in the screen
  menu `raise` reaches `Up` and `lower` reaches `Down` in one step and nothing is on the
  arm; the page and tab survive closing and a frame change.
- **Header layout test** in `frame.rs` (`header_boxes`, pure): the names' boxes are apart,
  stand on the heading's line inside the canvas's top margin, start where the title does
  and end before the right quarter of the menus.
- **Smoke scenarios** (`tools/smoke/scenarios.toml`): `pipboy-screen-menu` turns Use
  Pip-Boy off (the Pip-Boy and glove taken off), opens the screen menu at 1920 × 1080 (the
  image made larger), changes page and tab, clicks a tab through the flat image, then turns
  it on (both put on) and opens the menus on the model's screen; `pipboy-screen-menu-unworn` unequips the Pip-Boy (`player.unequipitem
  00015038`, the player's Pip-Boy 3000) and opens the menus: the screen menu, the arm
  never raised. The test aid `NVF_PLAY_TEST_PIPBOY` gained `frame=screen|device` (Use
  Pip-Boy off or on, as the settings page sets it).
- **By eye** (owner): ITEMS, STATS and DATA (the maps drag and zoom; the radio list) in
  first and third person at 1080 and 1440 lines; text sharp; the world blacked out to the
  screen's edges; Use Pip-Boy both ways; the header's clicks and hover boxes.

## 7. Open for the owner

- Default for a new install: Use Pip-Boy on (built) or off.
- Widened canvas on wide windows (§3.4), page by page, or 4:3 with bars for good.
- A key outside the menus that opens the other frame directly (e.g. I for the screen
  menu on ITEMS while Tab keeps the Pip-Boy), or the one setting only.
- Without a Pip-Boy worn, whether the DATA page's maps and radio still work (proposed: yes,
  §2.1).
- Whether the screen menu keeps the Pip-Boy's scanlines and `pipboy.dds` picture as an
  option (proposed: no, plain like the barter menu).
