# Spec note: the dialogue menu

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/dialogue_menu.rs` (the
menu), `dialogue.rs` (what it holds, its keys; docs/design/dialogue-play.md),
`interface.rs` (fonts, pictures, units; `message-menu.md`, `fonts.md`).

## 1. Sources

- **Three pictures from the owner**, 2026-10-11 (kept privately in
  `C:\fnv-private\references\dialogue_menu\`): Trudy's choices on an 800 x 600 screen
  (three choices, the second framed in red, the third dim, a scroll bar at the left);
  Veronica's choices on a 1200 x 675 screen (six choices, the fifth framed, the fourth
  dim, the list scrolled to its end); Bruce Isaac's line being said on a 1525 x 857
  screen (two lines on a dark band without a frame, the name at the top right). The owner
  said the third is what the actor's line looks like.
- **The game's interface files** in `Fallout - Misc.bsa`: `menus\dialog\dialog_menu.xml`
  and the pieces it includes (`menus\prefabs\list_box.xml`, `list_box_template.xml`,
  `box.xml`, `top_bracket.xml`, `bottom_bracket.xml`, `scrollbar_vert.xml`,
  `menus\globals.xml`). Described here in plain words; nothing of them is shipped.
- **The pictures they name**: `textures\interface\shared\background\solid_black.dds`,
  and from the atlas `textures\interface\interfaceshared.tai` the line ends
  `fade_to_bottom.dds`, `fade_to_top.dds`, the scroll bar's `arrow_up.dds`,
  `arrow_down.dds` and `vert_marker.dds`.
- **The fonts**: font 6 `Glow_Monofonto_VL_dialogs` (the line and the choices), font 7
  `Baked-in_Monofonto_Large` (the name) [documented: `Fallout_default.ini`, `fonts.md` §4].

## 2. What the files say

[documented: `dialog_menu.xml` and its pieces] In the interface's units (960 to the
screen's height; the width is the screen's aspect times 960):

- **The band.** A black picture (`solid_black.dds`) 1080 wide, centred across the
  screen, at brightness 128 and the interface's background alpha (32 in `globals.xml`).
  While the actor speaks it runs from 45 above the line's top and is the line's height
  and 80 more; while the player chooses it runs from 30 above the top bracket's line to
  30 under the bottom one's. It is shown only when subtitles are on or the choices are
  shown.
- **The click area.** While the actor speaks the whole screen takes a click (to skip).
- **The name.** Font 7, right-justified, ending 50 left of the safe zone's right edge,
  its top 170 under the safe zone's top; the file subtracts the safe zone twice from the
  screen's width and adds it twice to the top.
- **The centring line** is at 0.8 of the height inside the safe zone. The line and the
  list are each centred on it, but kept 40 above the safe zone's bottom.
- **The line** (font 6) starts 60 into the band and wraps 120 narrower than it (960).
  It is shown while the actor speaks, with subtitles on.
- **The list** starts 20 into the band and is 70 narrower (1010), at least 110 high; its
  height is otherwise "set in code". It is a list box: a scroll bar 32 wide at its left
  (arrows 32 x 16 at its ends, a marker 6 wide at least 32 long between them), shown
  only when the choices do not all fit; the choices start right of the scroll bar and
  end 5 short of the list's right. A scroll step is 26.
- **A choice's row** is its text's height and 20 more; its text (font 6) is 25 into the
  row and 15 down, wrapped 40 narrower than the row. The selected row has a box one unit
  less high than the row: a line all round at brightness 200 and a fill at alpha 40 and
  brightness 128. A choice's text has its own alpha "set in code". Rows play no click
  sound.
- **The brackets**: a line 15 right of the list and as wide as it, 20 above the list,
  with an upright at each end 40 long that fades downwards (`fade_to_bottom`); and one
  10 under the list with uprights fading upwards. Lines are 2 thick on a PC.
- All parts are in the interface's colour.

## 3. What the pictures add

Measured in units of each picture (1 unit = 0.625, 0.703 and 0.893 pixels). The
safe zone is 15 at the top and bottom [documented: `iSafeZoneY` in
`Fallout_default.ini`, hud.md §3], so the centring line is at 744.

| What | Measured | Reading |
|---|---|---|
| Trudy: the brackets' lines | 600 and 880 | a list of 250 centred on 744: its top at 619, lines 20 above, 10 below. 250 is two rows of two lines and one of one, each row 20 more than its text, a line 38 high |
| Trudy: the first line's letters stand at | 651 | 38.2 under the top of the first row as the frame shows it |
| Trudy: the red frame | 709 to 803.5 | the second row, 96 high, one unit short |
| Trudy: the band | 0.87 dark (by the light let through) | |
| Bruce: the line's top, its letters, the next line | 706, 729.3, 767.4 | two lines (76 high) centred on 744; letters 23.3 under the text's top; lines 38 apart |
| Bruce: the band's soft edges | half dark at 675 and 803, wholly dark from about 684 to 796, from 334 to 1389 across | the picture is soft all round; the band from 661, 156 high, 1080 wide |
| Bruce: the name | its capitals from 198 to 219.5, ending at 1637.7 | font 7's letters 19.5 under its top; see the safe zone below |
| Trudy: the name | ending at 1201.6 | 1280 - 30 - 50 = 1200 |
| Trudy and Veronica: the scroll bar's up arrow | its middle 31 under the list's top, 136 across (the list's left and 16) | |
| Trudy and Veronica: the down arrow | its middle 35 above the list's bottom | |
| Trudy and Veronica: the marker | 683 to 712 (at the top, nothing scrolled); 815 to 858 (at the end) | |
| Veronica: the frame | its line about 0.78 as bright as the brackets' (200 of 255); its fill lifts what is behind by about a tenth of the light | |
| Trudy and Veronica: a dim choice | as bright as 128 of 255 | the file's disabled text brightness |
| Trudy: the red frame's line | (100, 18, 16) | the selected choice is a check the player falls short of ("[Sneak 17/25]"); the text stays amber |
| Trudy: tag and line | "[Sneak 17/25]  I could" | two spaces between |

Veronica's letters are smaller than the other two pictures' (a row of one line is 51,
not 58; 21 units from a capital's top to a descender's bottom, against 34): another font
or a changed interface. Its positions are used only where they agree with Trudy's.

From these, all [hypothesis] fitted to the pictures:

- A line of font 6 is 38 high (its tallest glyph; the font's own size says 34), and its
  letters stand 23.3 under its top.
- The rows stand 7.2 higher in the list than the file puts them (Trudy's frame and
  letters agree on it); the cause is not known. NVForever draws them so and cuts the
  rows at the brackets' lines.
- The list grows to 290 at most (Veronica's list) and then shows as many whole choices
  as fit (Trudy's 250 with a hidden fourth); the file gives no maximum.
- The safe zone at the sides is 15 at 4:3 and narrower in units on a wider screen:
  15 x 1280 / the width in units (11.25 at 16:9, which puts Bruce's name within 2 units
  of the picture; 15 would put it 7 too far left).
- The band is about 0.87 opaque, much more than the file's alpha of 32 of 255 would
  give; how the game uses that alpha is not known.
- The frame of a selected check the player falls short of is red, its fill dark red;
  other frames are the interface's colour. Seen once (Trudy); what makes the game choose
  red is not in the files.
- The marker's length: about 30 and 43 in the two pictures; NVForever draws it 40 long
  on a track from 60 under the list's top to 52 above its bottom, placed by the first
  choice shown.

## 4. What NVForever does

- Draws the menu by §2 and §3 with the game's fonts 6 and 7 and the pictures of §1, read
  from the owner's archives at run time. Without them the same layout is drawn with
  Bevy's font and plain lines.
- While the actor speaks: the band and the line, no frame; the name. The log keeps the
  `subtitle: Speaker: text` line for it. While the player chooses: the band, the
  brackets, the choices, the frame on the selected one, the scroll bar when needed.
- A choice reads as the runtime gives it, with a check's tag and two spaces before it;
  no numbers. A choice answered before is drawn at brightness 128; the frame of a check
  the player falls short of is red.
- The line and the choices are shown without the acting notes in braces
  (`dialogue::shown_text`: "{questioning}You know how to play Caravan?" reads "You know
  how to play Caravan?") [owner statement 2026-10-11: the original shows no `{…}`
  content; observed: 5,634 of FalloutNV.esm's response texts carry such a note, 4,617 at
  the start, 4 closed by ")" instead of "}", which NVForever ends there; design]. Square
  brackets stay ("[SUCCEEDED]", "[You hear static.]"): the statement names braces only.
  The subtitles and radio subtitles are shown the same way (corner-messages.md §8); the
  log's `dialogue:` lines keep the record's text.
- The list scrolls by whole choices to keep the selected one in sight [design].
- While a Goodbye choice is offered, "TAB) Goodbye" (font 6) stands left of the band,
  its right end 20 units from the band, level with the bottom bracket, on its own dark
  box at the band's shade; on a screen too narrow for it, at the safe zone's left edge
  over the band [owner, list 5 item 62; design: the place; dialog_menu.xml has no such
  hint, observed]. Tab picks that choice (dialogue-play.md §2).
- The keys are dialogue-play.md §2's; the old on-screen key hint and the "You cannot
  leave this conversation yet." note are gone (the original shows neither; a refused
  Tab is in the log).

Not built: the subtitles setting (the line is always shown), the click and focus sounds
(the file names none for a row), dragging the marker or clicking the arrows, the
player's own font and colour settings.

That is the default drawing. Drawn from the file instead, behind a switch: §6.

## 5. Looking at it

`NVF_PLAY_TEST_TALK="8:EasyPeteRef="` with `NVF_PLAY_TEST_TALK_DELAY=60` and
`--screenshot <png> --screenshot-delay 13` shows Easy Pete's four choices; delay 6 shows
his greeting. `SunnyRef` offers six choices of one and two lines: the list scrolls.

Checked this way on 2026-10-11 [observed]: at 1525 x 858 Easy Pete's greeting stands at
726 (its top centred on 744 as one line), the name ends on the same pixel column as
Bruce Isaac's in the owner's picture and its capitals start one pixel higher; the band
lets through about 0.11 of the light behind it. Sunny Smiles's list shows three choices
(two lines, one line, two lines) with both arrows and the marker at the top, as in
Trudy's picture.

## 6. The menu drawn from its file: what code owns

`[UI] TileMenus=DialogMenu` draws the menu from its file, off by default
(`engine/nvf-play/src/tiles/dialog_menu.rs`, ui-tile-tree.md §6.2 step 3). This section is
the contract between the file and the code. It says what code has to give the file, in
NVForever's words, not how the file is laid out: the file itself places the rest, read at
run time from the player's archives. Everything named below was read with `fnvtool ui tree
--menu menus\dialog\dialog_menu.xml` and `fnvtool bsa extract` [observed: the vanilla
files, by fnvtool, 2026-11-04]; **no run of the original was looked at for this section**,
so what the original's code does with these names is [hypothesis] throughout.

### 6.1 The conversation is the menu's lifetime

`Game::menu` stays empty during a conversation (it holds what stands *over* one); the tree
builds the menu class while `Game::dialogue` is `Some` (`fnv_game::ui_loading::sync`) and
drops it after [design]. So readiness follows the conversation, not `Game::menu`.

### 6.2 Inputs from code

Names the file reads and never sets a meaningful value for, or marks as code's:

| Tile | Name | What code gives it [hypothesis unless tagged] |
|---|---|---|
| the menu (`io()`) | `_DialogVisible` | the whole menu shown |
| the menu | `_ShowingText` | the actor's line is up, not the choices |
| the menu | `_ShowSubtitles` | the subtitles setting; NVForever has none yet, and also clears it for an empty line, so that no band stands behind nothing [design] |
| `DM_SpeakerNameLabel` | `string` | the speaker |
| `DM_SpeakerText` | `string` | the line |
| every text tile | `width`, `height` | measured with the fonts; no expression in the file sets them (`menus::write_text_sizes`, message-menu.md) [design] |
| `DM_TopicList` | `height` | the rows' height; the file only holds a least height (`_MinListHeight`) [observed: the vanilla files, by fnvtool: its comments call the rest code's] |
| `DM_TopicList` | `_highlight_y`, `_selected_height` | the selected row's place and height [observed: the vanilla files, by fnvtool: the list box prefab's comments] |
| `lb_scrollbar` | `_number_of_items`, `_number_of_visible_items` | counted in `_scroll_delta` steps, as the message box's list (message-menu.md §5a) |
| `lb_scrollbar` | `_current_value`, `_SetInCode` | the scroll, and the signal that code set it [observed: the vanilla files, by fnvtool: the prefab's comments] |
| a row (`DM_TopicTemplate`, under `DM_TopicList`) | `string`, `listindex`, `height`, `_y`, `_line_alpha` | the label; the choice's number (the file hides a row with a negative one); its text and `_VerticalSpacing`; its place with nothing scrolled; the text's strength |

Names NVForever reads from the file, not code's: `_scroll_delta`, `_VerticalSpacing`,
`_MinListHeight`. `id`s 0 to 3 on the click area, the name, the line and the list suggest
the original's code finds them by number; NVForever finds them by path [hypothesis]. The
click area's `target` follows `_ShowingText`, so a click on it skips only while the actor
speaks.

The file reads `screen()`'s `cropx` and `cropy`. No other vanilla menu with a tile switch
does (message, quantity, sleep/wait, tutorial, HUD) [observed: the vanilla files, by
fnvtool]. While `DialogMenu` is on the code writes them, in interface units: `cropy` is
`iSafeZoneY`'s 15 [documented: `Fallout_default.ini`, hud.md §3], `cropx` is §3's fitted
rule [hypothesis]. `dialog_menu.xml` has no hint for Tab, no red, and no fill picture for
the frame [observed: the vanilla files, by fnvtool]: those three are NVForever's own tiles
and writes (§6.4).

### 6.3 What stays NVForever's choice, not the file's

Kept from §4, with the rules still in `crate::dialogue`:

- the acting notes stripped, square brackets kept (`dialogue::shown_text`); the speaker or
  the talking activator's name; no list, brackets or hint during a response; an empty
  response leaves only the name;
- the list scrolls by **whole choices** and never shows a part of one [design]: rows outside
  the window take a `listindex` of -1, so the file's own rule hides them, and the scroll is
  written as the height of the choices above the first shown. `dialogue_menu::window` decides
  the window for both views. The file's `_scroll_delta` suits a list that scrolls by rows of
  a set height, which this one does not [hypothesis: nothing says which the original does];
- the list's greatest height is §3's fitted number, kept as NVForever's policy for now in
  `dialogue_menu::window` [design; hypothesis as to the original]. It is not worked out from
  the file's or the fonts' geometry: the file gives no maximum, and the two views should
  agree on what fits. A capture that finds the original's rule replaces it for both;
- a said-before choice is dim: `_line_alpha` 128 [hypothesis: the file gives the row's text
  a code-set alpha and §3 saw a dim choice as bright as 128 of 255]; the hand-built view
  says the same with brightness;
- skip, refused Tab, the keys, the wheel, the pause and an overlying menu act as in §4 and
  dialogue-play.md §2. The scroll bar's arrows and page areas move the selection, which
  moves the window, so they keep to choice boundaries; dragging its marker is not built
  [design: the hand-built bar takes no clicks at all].

Rows stand where the file puts them. §3's lift is **not** applied: the file's window clips
at the list's own top, so a lifted first row would lose its frame's top line. The hand-built
view keeps the lift and clips at the brackets. Which is the original's is for a capture.

### 6.4 NVForever's own tiles

Code-made under the menu, named `NVF_…` so nobody takes them for the file's, from templates
NVForever writes itself (`tiles/dialog_menu.rs`):

- `NVF_GoodbyeHint`: the Tab hint and its dark box, placed from the file's worked-out band
  and bottom bracket. Its wording and whether it shows come from the same model as the
  hand-built hint's (`dialogue_menu::GOODBYE_HINT`, `MenuView::goodbye`) [owner, list 5
  item 62].
- `NVF_SelectionFill`: the frame's fill. The file's highlight box is a hotrect with an alpha
  and a brightness, which the tile renderer does not paint (message-menu.md §2); the fill
  uses §3's look.

The scroll bar's pictures are marked to keep their own alpha: the bar above them is a picture
tile with an alpha of 0, and the tile renderer passes a picture's alpha to its children
[hypothesis: §3's pictures show them; how the original does is for a capture].

The red frame of a short check is a write on the highlight box's four lines and the fill
(§3's colours). The file names no such colour [observed: the vanilla files, by fnvtool].

### 6.5 Readiness

One decision (`DialogTiles`, worked out after the scripts of each frame) controls the tile
drawing, the hand-built view and the tile input [design]. It needs the switch, the menu files,
a live conversation, every path above with the frame's four lines and the bar's controls, a row
template that makes a row with its text, every row made, text that measures, boxes with a
worked-out size and something to click; and, each time the view is placed, what it has to show
as the renderer selects it and regions the pointer can reach as the input selects them. Anything
else: the whole hand-built menu, a warning
once per reason, the made rows unloaded, the safe zone given back, and the preparation tried
again next frame. The Tab hint and the frame's fill never stand alone.

### 6.5a The order in a frame

- At each transition of the conversation, inside the runtime's own work, before a result script
  of the line runs (`Game::ui_dialogue_hook`), and again before the frame's scripts: the model's
  view is written, each trait only when the model's value for it changed, so a script reads the
  phase, words and rows of the screen it runs in, and what it sets of them stands until the
  model changes that very thing. A new selection changes none of them. With no world at hand a
  new speaker's name can be the runtime's until the frame's own step puts it right.
- After the scripts: measure, place, decide. Placing runs again when the model, a text's size
  or an evaluated number it reads moved. These are read one frame late by a script [design].
- A click on the tiles is decided on what was drawn: every tile that can be hit, with its place,
  clip, depth and target, taken after the frame's last working out of the tree, animations stepped.
  An animation goes on under it and does not stop input. On a frame with a new window size the
  tile input takes nothing: that one press is lost.
- Every click carries the screen it was made on, a script's the one it was queued on: the
  conversation, the lines said, the response. A click from an earlier screen does nothing, and
  neither does a hand-built button's: those carry their screen too.
- The safe zone on `screen()` is held from the first write to the conversation's end or a
  fallback, then given back as it was, absent if it was; a script's own value stands.

### 6.6 Unknown until a capture of the original

Read these in a focused capture (private; identifiers and numbers only, no built-in wording):

- whether the list scrolls by choices or by `_scroll_delta`, and `_current_value` then;
- the list's `height` at the longest, and whether any maximum is in code;
- `_highlight_y` and the first row's `y`, against §3's lift;
- `lb_highlight_box`'s lines' colour on a short check, and what drives it;
- a said-before row's `_line_alpha` and its text's `brightness`;
- `DM_TextBackground`'s `alpha` as read while drawn (§3's opacity against the file's value);
- `screen()`'s `cropx` and `cropy` at two aspect ratios;
- what a click on the arrows, a page area and the marker do to the selection;
- `_scroll_delta` and `_number_of_items` with a list that overflows;
- the band and the name for a line with no text.
