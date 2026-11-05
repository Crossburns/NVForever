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
  as fit (Trudy's 250 with a hidden fourth); the file gives no maximum. That height, the
  whole choices that fit from the top, is the list's at every scroll of it: a later window
  shows as many whole choices as fit the same room and leaves the rest of it empty, so the
  band neither grows nor moves as the selection goes down [owner's playtest 2026-11-05: with
  Sunny Smiles's six choices (96, 58, 96, 58, 58, 58) the band grew from 250 to 270 and
  shifted when the selection scrolled onto the wrapped third choice, as four whole choices
  from the second fit 290; design: whole choices only, none cut at the foot].
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
- While a Goodbye choice is offered, "[Tab] Goodbye" (font 6, the band's own text) stands
  left of the band. [owner, list 5 item 62: the request's words were "TAB) Goodbye", at
  the left of the dialogue box, shown by default. The words are changed to the bracket
  style of the tile menus' key hints, proposed to the maintainer in #1517; they are not
  the maintainer's words. design: the rest; dialog_menu.xml has no such hint, observed.]
  All its measures come from the pictures of its glyphs (their ascent above the baseline
  and height, so the ink reaches `up` above and `down` below it; 17 and 0 for the
  stand-in text when the game's font is not there), not from one assumed height [design].
  - **At the side**, when the safe zone leaves room (a screen about 16:9 or wider for the
    words in the game's font; it depends on the screen's width and the words' width, not
    on one aspect): the ink is centred on the bottom bracket's line, the words' right end
    10 units from the band's edge, on a backing in the band's shade (0.87) that reaches
    10 units left of the words and 8 above and below the ink and is flush against the
    band's edge, so the two read as one shape [design].
  - **Inside the band**, where it does not (4:3, 5:4 and any screen where the above does not
    fit; this is not outside-left but below-left): the band grows under its bottom bracket
    until a strip holds the ink with 15 units clear above and below (19 units more for the
    stand-in's 17; more for taller ink), and the words stand in it, 60 units in from the
    band's left edge (as the line of text starts; kept inside the safe zone), with no
    backing of their own. They are clear of the bracket's line and corner, under the
    choices and the scroll bar, inside the safe zone [design].
  - The grown band does not change the wrapping or how many choices fit, but the list is
    set up to keep the band inside the safe zone's bottom: **at the list's maximum height,
    turning the hint on moves the list up** (by up to the growth: 19 units for the stand-in ink), and turning it off
    moves it back. At the side, and for shorter lists, nothing else moves.
  Tab picks that choice (dialogue-play.md §2).
- The settings page's Gameplay group has a "Goodbye hint" switch (key `dialogue-goodbye-hint`,
  on by default): off removes the words, their backing and, inside the band, the strip made
  for them (so the band is its own height again); Tab says goodbye as before [design]. The
  menu is drawn again at once when it is flipped; the log says `[Tab] Goodbye shown at the
  left of the choices` or `… hidden by the Goodbye hint setting` when the hint changes.
- The keys are dialogue-play.md §2's; the old on-screen key hint and the "You cannot
  leave this conversation yet." note are gone (the original shows neither; a refused
  Tab is in the log).

Not built: the subtitles setting (the line is always shown), the click and focus sounds
(the file names none for a row), dragging the marker or clicking the arrows, the
player's own font and colour settings.

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
