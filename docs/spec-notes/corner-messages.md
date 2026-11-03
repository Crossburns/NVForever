# Spec note: the messages at the top left (corner messages)

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/corner_messages.rs` (the
queue, the layout, the drawing), `interface.rs` (fonts, textures and the atlas index,
shared with the message box), `script_events.rs` (`Note`: what a line carries),
`crates/fnv-game/src/data.rs` (the record fields), `crates/fnv-font` (`fonts.md`).

## 1. Sources

- **A screenshot from the owner**, 2026-10-11 (kept privately): the original game at
  1080 x 607 showing "You are now Well Rested!" at the top left. Every "[observed:
  screenshot]" is measured on it.
- **The game's interface files** in the base archive `Fallout - Misc.bsa`:
  `menus\main\hud_main_menu.xml`, `menus\prefabs\hudtemplates.xml` (which the first
  includes) and `menus\globals.xml`. Described here in plain words; nothing of them is
  shipped.
- **Textures** in the base archive `Fallout - Textures2.bsa`: the interface's atlas
  `textures\interface\interfaceshared0.dds` and its index `interfaceshared.tai`, the
  icons in `textures\interface\icons\message icons\`, the bracket
  `textures\interface\icons\mood\glow_messages_seperator_left.dds`, and font 7.
- **The records of `FalloutNV.esm`**: all MESG (Message) and MICN (Menu Icon) records,
  read with a throwaway reader and `fnvtool esm decode`.
- **The GECK wiki's "Message" and "MenuIcon" pages**, from the owner's export.
- **The owner's rule**, 2026-10-11: the messages are shown one at a time, each for the
  time its record gives.

## 2. What the files say

### 2.1 The HUD menu file

[documented: `menus\main\hud_main_menu.xml`] The HUD has an area named "Messages", 460
wide and 90 high in the interface's units (960 to the height of the view), in the HUD's
colour. The file gives it no place and no content: both are left to the program.

[documented: `menus\prefabs\hudtemplates.xml`] Three patterns are there for the program
to fill the area with:

- **The bracket**: a picture 470 wide and 192 high, 20 above the area's top, named
  `Interface\icons\MOOD\glow_messages_seperator_left.dds`, from the texture atlas
  `Interface\InterfaceShared.tai`, stretched to that size, invisible at first (opacity
  0).
- **The icon**: a picture 160 by 160, 25 above the area's top, at the area's left edge,
  drawn over the bracket (depth 2), from the same atlas, stretched to that size,
  invisible at first. The file name written there (`Interface\Icons\Mood\
  glow_messages_happy.dds`) is in neither the atlas nor the archive [observed]; the
  program puts the message's icon in its place [hypothesis].
- **Left-aligned text**: font 7, wrap width 2048, invisible at first. The same pattern
  serves other HUD texts; where the message's text stands and where it is broken is
  left to the program.

Nothing in either file says how many messages are shown at once, how long, or how they
come and go. [documented] The menu as a whole fades over 0.25 seconds (`menufade`).

[documented: `Fallout_default.ini`, `sFontFile_7`] Font 7 is
`Textures\Fonts\Baked-in_Monofonto_Large.fnt`. [observed] Its lines are 36 high; its
letters' pictures carry their own glow, so each picture is about 10 wider than the room
the letter takes (a capital: picture 27 wide, 10 of it overlapping the next letter;
a space takes 13); a capital's picture starts 30 above the line the letters stand on.

### 2.2 The atlas

[documented: the heading of `textures\interface\interfaceshared.tai`] The index is a
text file. Each line has a picture's file name (no folder), then, separated by commas,
the atlas's file name, an atlas number, a type, the picture's left and top offsets in
the atlas, a depth offset, and its width and height, all as parts of the atlas's size
from 0 to 1. Lines starting with `#` are comments. The heading says the offsets are
already moved in by half a texel.

[observed] The index has 91 pictures, all in `InterfaceShared0.dds`
(1024 x 1024, DXT5, in `textures\interface\`). Among them are the bracket
`glow_messages_seperator_left.dds` (255 x 127 texels) and thirteen of the message icons
(127 x 127 each), the default one included. The faction icons and the Wild Wasteland
icon are not in it.

[observed] **The atlas's bracket is not the bracket's own file made smaller.** In the
own file (512 x 256) the two lines meet in a square corner. In the atlas the corner is
cut: the upright line stops short of the top, the line across starts further right,
and a slanted piece joins them. The owner's screenshot shows the cut corner. So the
game draws the atlas's picture where the pattern names the atlas [observed for the
bracket], and NVForever does the same: a picture whose file name is in the index is
taken from the atlas, any other from its own file [hypothesis for the icons; the
default icon looks the same in both].

## 3. What the records say

[documented: GECK wiki, Message] Without the "Message Box" flag a message is "briefly
displayed at the top left of the screen". Icon is the image shown with such a message;
with none, `glow_message_vaultboy_neutral` is shown. Display Time is the number of
seconds it is shown, used only for messages that are not boxes.

[documented: xEdit's field names, in `schemas/fnv/gameplay.json`] In a MESG record,
INAM is the icon (a form), DNAM the flags (bit 0 "Message Box"), TNAM the Display Time
(a whole number).

[observed: `FalloutNV.esm`]

- INAM points at a **MICN (Menu Icon)** record. [documented: GECK wiki, MenuIcon] A
  Menu Icon has an Editor ID and an icon file name. [observed] That is all its record
  has: EDID and ICON, the file's path under the textures folder. There are 12: nine
  faction faces, the Wild Wasteland face and "Vault Boy in pain" under
  `interface\icons\message icons\`, and two weapon type icons used elsewhere.
- Of 1144 messages, 308 are not boxes (no flag, no buttons). 294 of those have no icon;
  five have the face in pain, and one each has a faction's or the Wild Wasteland's.
- Display Time among those 308: 2 seconds (94 messages), 10 (85), 4 (41), 5 (36),
  3 (12), 6 (9), 1 (5), 15 (4), 8 (2), 7 (1). None has 0. Nineteen have no TNAM at all
  (and no DNAM): leftovers from Fallout 3. Boxes never have TNAM.
- "You are now Well Rested!" is `AbilityWellRestedStartMsg`: no icon, 10 seconds.
- The game setting `fDefaultNoticeTextDisplayTime` is 0.5. What it applies to is not
  known; it is not used.

## 4. What the screenshot shows

[observed: screenshot; lengths in the interface's units, 0.632 pixels each at this
size]

- **The pattern's sizes hold.** The atlas's bracket laid over the screenshot fits at
  470 by 192 (the line across is 237 pixels long, the upright one 65), and the default
  icon at 160 by 160.
- **Where the area is.** With the bracket 20 and the icon 25 above the area's top, as
  the patterns say, both fit when the area's corner is 25 from the view's left edge and
  55 from its top.
- **The icon** is `glow_message_vaultboy_neutral`, as the wiki says for a message
  without one.
- **The text** is font 7 at its own size (one unit of the font is one of the
  interface). It starts 130 right of the area's corner; a capital's picture starts at
  the area's top, so the first line's letters stand 30 below it. The second line is 38
  lower: the font's 36 and 2.
- **Where the text is broken.** "You are now Well" (228 wide) is one line and "Rested!"
  the next; the whole text would be 336. So the wrap width is at least 228 and under
  336. NVForever uses 330, what is left of the area's 460 after the text's 130
  [hypothesis].
- **Brightness.** The icon and the text are at the interface's full colour. The
  bracket's lines are fainter, about 0.65 of it.
- The bracket's size does not follow the text in this one screenshot (two lines);
  whether it grows for a long text is not known. NVForever's stays as it is, and a long
  text runs on below it.

NVForever's frame of the same message at the same size agrees with the screenshot to
within a pixel in every one of these (§7).

## 5. NVForever's rules

- **One at a time, each for its own time** [owner's rule, 2026-10-11]. Messages wait in
  the order they came. The first is shown for its record's Display Time; only when it
  has gone is the next shown.
- **Coming and going** [design]. A message fades in over 0.25 seconds and, after its
  time, fades out over 0.25 seconds (the HUD menu's own fade time; that the game uses
  it here is a guess). Its time counts from when it starts to appear.
- **No Display Time** [design]. A message without TNAM, or with 0, and every line that
  is not a message record (below) stays 2 seconds, the commonest time in the game's own
  messages.
- **Said twice running** [design]. A message that says exactly what the one before it
  in the queue says is left out, so a script that shows the same message every frame
  does not fill the queue. At most 64 wait; more are left out.
- **Behind a message box or a script's menu** [design]. The area is hidden and its clock
  stands still while one is open, so a message is not used up unseen.
- **The other short lines** [design; nothing documents where the original shows them].
  NVForever's own lines for items a script added or removed and a default activation's
  message ("Gate is locked") go through the same area, with the default icon and the 2
  seconds. The wording of these lines is NVForever's. So do the refusals of the
  lockpicking menu, of a terminal and of the Wait and Sleep menu ("You cannot sleep when
  enemies are nearby.", "You need a Lockpick skill of 50 to pick this lock.") [owner,
  list 5 items 44, 45]. The engine's own placeholder lines that were drawn white under
  the crosshair (an item picked up, a door opened or closed, a kill, a crippled limb, a
  control not built yet such as F5) are no longer drawn at all, only logged
  (`activate::HudMessage`) [owner, list 5 items 43, 50, 53]. Quests added, completed or failed
  and objectives shown or completed have their own place, as in the original
  (`quest-messages.md`); they no longer come here.
- **Notes** [owner's request, issue #1163]. A note (NOTE) the player gets for the first
  time, by any way (picked up, taken from a container, AddItem, AddNote, a terminal's Add
  Note item), shows "Note: " and its name (FULL, else its editor ID), with the default
  icon and the 2 seconds. No string setting in FalloutNV.esm or the add-on masters names
  notes [observed: search of their GMST records, 2026-10-31], so the words are
  NVForever's (the owner's example). That the original shows it for script-given notes
  too is a hypothesis (observer-checks.md item 67). No sound: none in the masters is
  named for it [observed: SOUN editor IDs, only the holotape player's].
- **Icons.** A message's icon is its Menu Icon's file, from the atlas if the index has
  its file name, else the file itself, stretched to 160 by 160. Without an icon, or if
  the file cannot be read, the default icon.
- **Colour.** The amber of the owner's screenshots (`message-menu.md` §3). The player's
  own HUD colour is not read.
- **Without the game's files** the text is drawn in Bevy's font beside two plain lines,
  with no icon.

## 6. Not built

- A sound when a message appears (none is named in the files read).
- The original's exact coming and going (a slide, a different fade), and whether its
  bracket grows with the text.
- `MessageBoxIcon`-style script functions that choose an icon by themselves, and the
  message-shaped texts the game's own code shows (experience, karma, "You cannot fast
  travel", item pick-ups by the player's hand): only what reaches `ScriptEffect::Notify`
  is shown.
- The original's other HUD text areas (location and region names, the experience
  bar). Quests and objectives: `quest-messages.md`.
- The format arguments of `ShowMessage` are whatever the script runtime already fills
  in; nothing was added here.

## 7. Checking

- Unit tests: the queue (one at a time, own time, twice running), the text's layout,
  the atlas index (`cargo test -p nvf-play` in `engine`).
- By eye and by measure, at the screenshot's size:
  `NVF_PLAY_TEST_SCRIPT_MENUS=0.3 NVF_PLAY_TEST_SCRIPT=3:message:FactionReminderNCR,3.1:message:AbilityWellRestedStartMsg`
  `nvforever --data <Data> --cell GSHouseInterior03 --play --no-voice --no-overlay --size 1080x607 --run-seconds 20`
  with `NVF_PLAY_TEST_SHOTS=8:<file.png>`. The log has, in this order,
  `corner message: "You are still dressed as a member of the NCR." for 2 s, icon glow_message_vaultboy_ncr.dds`
  and `corner message: "You are now Well Rested!" for 10 s, icon default`.
  Measured on the frame and on the owner's screenshot (pixels): the line across at
  height 28.5 against 29.1, ending at 280 against 279; the upright line at 35.4 against
  35.7, ending at 101 in both; the icon's face from 40 to 91 across and 34 to 100 down
  in both; the first line of text from 102 to 246 against 103 to 246.
- `--no-overlay` matters for looking: nvforever's own status text is at the top left
  too and lies over the message.

## 8. The subtitles

Lines said out loud that are not the conversation with the player (SayTo, Say, a radio's
talk; `engine/nvf-play/src/script_hud.rs`) are drawn at the bottom centre:

- In the interface's colour (§2.1's `&hudmain;`) [documented: menus\main\hud_main_menu.xml,
  the `Subtitles` rect has `systemcolor &hudmain;`, width 800, height 110].
- In font 7, the font of the HUD's text templates [documented:
  menus\prefabs\hudtemplates.xml]; centred, broken at the rect's width of 800.
- The text alone, without the speaker's name [observed: the owner's screenshot,
  2026-10-11]; the log's `subtitle:` lines keep "Speaker: text". Acting notes in braces
  are left out ("{Sadly}Okay." shows "Okay."; dialogue-menu.md §4) [owner statement
  2026-10-11].
- Where: the first line's base 633 units under the top of the view, the lines 38 apart,
  centred across [observed: the same screenshot; it shows the G.O.A.T. classroom, so it
  may be Fallout 3's HUD, taken as the same layout — hypothesis]. Where a single line
  stands, and whether the text starts at the rect's top or ends at its bottom, is not
  known [hypothesis: from the top].
