# Terminals and hacking

How terminals work in NVForever: the record, the hacking game, and the terminal's own
menu. Rules: `crates/fnv-game/src/hacking.rs` (the game), `crates/fnv-game/src/terminal.rs`
(opening, items, choosing). Engine: `engine/nvf-play/src/terminal.rs` and
`terminal/screen.rs`. Survey: `fnvtool world terminals`; one game laid out:
`fnvtool world hack` (docs/cli-contract.md).

Sources: xEdit `wbDefinitionsFNV.pas` (dev-4.1.6) for the record; the GECK wiki (the
owner's export, `C:\fnv-private\geckwiki`) for the word count and lock levels; the game's
own interface files `menus\hacking_menu.xml` and `menus\computers_menu.xml` and its word
list `menus\falloutdict.txt` (base archive `Fallout - Misc.bsa`, extracted privately to
`C:\fnv-private\menus-extract\hacking`); the owner's seven screenshots of an Easy terminal
(2026-10-11, in the chat of session "hacking"); the owner's description of the rules
(same day).

## 1. The record (TERM)

[documented: xEdit TERM; schema `schemas/fnv/objects.json`]

| Field | What |
|---|---|
| FULL | name ("Terminal" on 156 of 344) |
| DESC | welcome text under the header |
| SNAM | looping sound |
| PNAM | password note (NOTE) |
| DNAM | base hacking difficulty (u8: 0 Very Easy … 4 Very Hard, 5 Requires Key), flags (Leveled, Unlocked, Alternate Colors, Hide Welcome Text when displaying Image), server type (0–9: "-Server 1-" … "-Server 10-"), one unused byte |
| ITXT… | menu items, each: ITXT item text, RNAM result text, ANAM flags (Add Note, Force Redraw), INAM display note, TNAM sub menu (TERM), an embedded result script (SCHR …), CTDA conditions |

[observed: `fnvtool world terminals`, FalloutNV.esm]: 344 terminals: 237 Very Easy, 19
Easy, 44 Average, 21 Hard, 18 Very Hard, 5 Requires Key; 193 Unlocked, 11 Leveled; 23 with
a password note. 895 items: 193 with a script, 542 with a note (19 Add Note), 86 with a
sub menu (72 terminals are sub menus), 277 with conditions (244 run on the subject, 115
on the linked reference, 46 on a reference). 219 placed; the 77 not Unlocked: 10 Very
Easy, 17 Easy, 23 Average, 12 Hard, 11 Very Hard, 4 Requires Key. No placed terminal has
lock data (XLOC). 342 DNAM are 4 bytes, 2 are 3 (no unused byte).

NOTE: TNAM is the text of a text note (DATA 1) or a topic of a voice note [documented:
xEdit union]; kept as bytes in the schema and read by type in `GameData::notes`.

## 2. The hacking screen

[documented: hacking_menu.xml] Font 5 (`fixedsys_comp_uniform_width`, every character
17 wide [observed: its glyphs], line height 25), the terminal system colour, the Pip-Boy
background. The area `hacking_depth_rect` 920 × 630 at (32, 48); a header 150 high; under
it the dump: two columns 342 apart, 19 characters a column, 17 rows of 28; the log right
of the dump (from 665), its prompt at 17, the entry 21 right of the prompt; log entries
stand 3 + n rows up from the bottom (n = 0 the newest) and vanish above the top; attempt
blocks 17 × 17, one block apart, starting one block after the text; a cursor block that
blinks every 400 ms.

[observed: the owner's screenshots]

- Header: the title line, the password prompt line, a blank line, the attempts line
  (the count, then a label) and four blocks. Lines at 1, 32 and 93 under the header's
  top. None of the header's words is in the plugins or the menu file (§8); NVForever's
  are `screen::words::HACK_TITLE`, `HACK_PROMPT` and `attempts(n)`.
- Rows: an address "0xF650" (four hex digits), a space, 12 characters. Addresses go up
  12 a row; column 2 goes on from column 1 (0xF710, then 0xF71C). The dump is one run of
  408 characters: words run on across rows and from the last row of column 1 into the
  first of column 2 ("GE" + "TTING").
- The symbols are `! " # $ % ' ( ) * + , - . / : ; < = > ? @ [ \ ] ^ _ { | }` (no `&`, `~`
  or backquote seen). Words are capitals.
- 20 words of 7 letters; every dud shares 1 to 5 letters in place with the password
  (MEETING): 3 share 5, 4 share 4, 4 share 3, 5 share 2, 4 share 1.
- Picking LOOTING logs ">LOOTING", then a line saying the entry was refused, then its
  likeness as 4 of 7, and takes a block.
  Likeness is letters equal at the same place (LOOTING / MEETING: T, I, N, G).
- The pointer on a bracket pair lights the pair ("{[_}" in "|{[_})": the inner `[` has no
  `]`), and the prompt shows it; picking it logged ">{[_}", then a two-line message that the attempts are back,
  and the four blocks came back. The pair's characters stay in the dump afterwards.
- The pointer on a word lights the whole word, both parts when it runs on (ME + ETING),
  and the prompt shows the word. With the pointer off the dump, nothing is lit and the
  prompt shows only ">" (and the cursor).
- The password logs ">MEETING", a line saying it matched, then a wait message on three
  lines: log lines are at most 12 characters after the ">", broken between words.
- One blank row between the newest log line and the prompt.
- Then the terminal's menu (§6).
- Final displayed colours on the screen: blocks and lit cells 63, 255, 152;
  text strokes up to 88, 255, 160; the screen behind 16–23, 38–52, 15–32.

[observed: owner's original-game JPEGs, 2026-10-31, 1920 × 1080, private
`pr-pictures/terminal-bugs/vanilla/20261031210905_1.jpg` (hacking) and
`20261031210855_1.jpg` (welcome)] PIL/numpy RGB medians, 10 × 10 half-open pixel
bounds, x right and y down. The welcome frame exposes the empty glass's gradient.

| Patch (x bounds; y bounds) | Hacking RGB | Welcome RGB |
|---|---|---|
| Below header (695–705; 290–300) | 21.5, 43, 28 | 21, 43, 28 |
| Away from streak (1345–1355; 695–705) | 18, 39, 24 | 18, 39, 24 |
| Top-left corner (505–515; 140–150) | 7, 10, 3 | 8, 10, 5 |
| Empty middle (1195–1205; 645–655) | 21, 44, 26 | 21, 44, 26 |
| Streak peak (1195–1205; 235–245) | 107.5, 129, 96 | 107, 129, 95 |
| Streak lower down (1195–1205; 295–305) | 99, 124, 92 | 99, 124, 92 |
| Bottom middle (955–965; 815–825) | 10, 29, 16 | 10, 27.5, 16 |

[observed: same hacking JPEG] For ink, take the 40 brightest pixels by Rec.709 RGB
weights (0.2126, 0.7152, 0.0722) in each patch: the first attempt block
(808–829; 251–272) has median (65, 255, 151.5), mean (64.7, 254.55, 151.2);
the header stroke patch (455–550; 152–180) has median (63, 246.5, 141), mean
(63.55, 246.325, 140.9). JPEG/patch variation explains small differences from the
earlier measurements [hypothesis]. [design, superseded] The retained block target
(63, 255, 152) was replaced by the 2026-11-01 fit below.

[observed: same JPEG, 2026-11-01] Picked on screen, the attempt blocks show (61, 253,
152); the brightest 30% of each block's pixels have medians (61-62, 255, 149-151), all
its pixels (57-59, 253, 139-141). Through the scanlines' troughs their green stays 253
to 255 while red and blue dim by about 15%, so they read solid. [hypothesis] The fills
are clipped in green. [observed: in game, 2026-11-01] Our glare adds about (6, 0, 2)
display units over the blocks (drawn (63, 255, 152), shown (69, 255, 154)). [design] So the canvas draws
the shown colour less that add in linear light, (54, 252, 150) (`screen::bright`), and
the fills (blocks, lit cells, the cursor, the separator, the chosen row's box) lie above
the scanlines, the text on them above the fills. Flat, without the glare, the blocks
show the drawn colour. [design] Keeping fills above the lines is an approximation:
it also removes the observed red and blue dip, leaving all three channels constant.

[observed: same hacking JPEG against the branch's `hacking.png` at 1280 × 720,
2026-11-01; the JPEG's sums divided by 2.25 for its larger pixels] The original's text
strokes are soft, about 3 pixels across at 1080 with peaks of 200 to 250 green, where
ours were 2.5 pixels of full green at 720. On the same lines of text (the password
prompt line and the attempts line, §2), the summed green above the glass beside the line
was 0.79 and 0.77 of ours in display values, 0.60 and 0.51 in linear light, with hard-edged text in
the block colour. Fine horizontal scanlines cross the glass and the text: 7.8 pixels
apart at 1080, which is 7.33 units (the dump's rows 29.7 pixels apart for 28 units), 131
down the texture's 960 units; the blocks' blue dims to about 0.84 at 1080 and 0.82 when
the frame is scaled to 720, the empty glass by about 4 display units [observed]. The
streak dims by about the glass's units, not its own share [observed]. [hypothesis]
So the lines lie under the glare. Fallout_default.ini has `bDoRenderedTerminalScanlines=1`,
`fRenderedTerminalScanlineScale=130` [documented]; the only scanline texture in the
archives is `textures\pipboy3000\pipboyscanlines.dds` [observed]. [hypothesis] The
scale is the number of lines down the screen's texture.

[design] So NVForever draws both, and the text keeps the terminal colour of the blocks:
- Soft strokes (`screen::soften`): the terminal's own copy of font 5, each glyph in a
  cell with 3 texels of room, its coverage blurred by a Gaussian of 0.8 texels (canvas
  units) and bent (`display_blend`) so that blending in linear light over the glass gives
  what blending the display values would; the glyphs keep their advances, so nothing
  moves. Blurs of 0.6 and 0.7 left the strokes harder than the original's.
- Scanlines (`screen::scanline_image`): the band at 960/130 units, baked into one column
  a texel per canvas row and stretched over the whole canvas above the text, under the
  glare; it approximately scales the display values (in the calibration run, before the
  fills moved above the lines, a block's blue dimmed to 0.81 at 720; scaling the light
  instead gave 0.94; the final blocks draw above the lines and keep their blue). This fit was calibrated on the block dip
  and glass banding. `sRGB(linear(C) * linear(band))` is darker than a display-value
  multiply in dark tones: glass (12,38,20) at band 0.8 gives about (7,28,14), rather
  than (10,30,16). The undercoat and the background's tint are raised by what
  the band takes away on average; the glass's 10 × 10 medians then move by 2 units at
  most [observed: in game, 2026-11-01].
- [observed: in game, 1280 × 720, 2026-11-01] With both and the blocks' colour below,
  the original's header lines give off 1.01 and 1.00 of our light in display values,
  0.97 and 0.90 in linear light;
  the empty glass's banding is about 4 display units, as the original's. An intermediate
  fit, hard text at 0.6 of the block colour's light, matched the sums but not the look.

[design] Keep the Pip-Boy background beneath the ink and the monitor's glare above the
screen (§7). The glare is additive and its dark texels add a little over the whole glass
(a few display units in linear light; §7), so the glass beneath it is fitted to the
original's middle less that, (12, 38, 20) [design: from the in-game measurements of
2026-10-31; the first fit, to (19, 41, 25), with the glare through the engine's
display-value add read (33, 46, 31) in game]. With the glare switched off the glass
alone read (13, 35, 17) in the middle, (4, 8, 2) in the corners and the blocks exactly
(63, 255, 152) [observed: 1280 x 720, 2026-10-31]. The background's alpha is 0.4
(as on main), its independent sRGB tint (90, 202, 131). [observed: private
`textures/interface/shared/background/pipboy.dds`, PIL, 2026-10-31] Its centre texel
(256,256) is (76,77,76,255), corner (0,0) is (5,6,5,255), and RGB maxima are (99,99,99);
it is a dark texture, not white. [design] In linear light, solve `undercoat = (middle -
tinted_centre × alpha) / (1-alpha)`, approximately sRGB (4, 10, 2): the image carries the
vignette, and undercoat + background reaches the middle instead of brightening it twice. The image retains its texture
and vignette; the same composition is used in monitor and flat modes. If unavailable,
the image falls back to opaque middle glass.

[design] `screen::compose_screen` mirrors undercoat → texture × tint → foreground
coverage in linear light. Its test predicts middle (12,38,20), dark texture corner
(3,8,2), dim texel (49,49,49) → (7,24,11), brightest texel → (17,50,28), opaque
blocks (54,252,150), and half glyph coverage at the middle (39,187,110). It checks
undercoat + background (the §2 ranges hold for the middle with the glare's floor on
top, not for the glass alone), blocks within 3 units and opacity over both world
shades. The text is the block colour; the original's softer, dimmer-looking strokes
come from soft edges and scanlines (above), not from a different colour in the files.
Glare's final screen-space contribution remains an in-game check (§7).

## 3. The words

[observed] `menus\falloutdict.txt` is one line of 4,710 upper-case words (4,709
different), letters only: 4 letters 555, 5: 765, 6: 770, 7: 785, 8: 613, 9: 434, 10: 282,
11: 141, 12: 73, 13: 46, 14: 12, 15: 3 (and a few shorter and one of 17). Every word of the
owner's screenshots is in it. [hypothesis] The game draws its words from it; nothing else
in the data looks like a word list.

## 4. How many words, how long

- **Count** [documented: GECK wiki "Hacking Word Count"]: `(1 - (Science - lock) / (100 -
  lock)) × (iHackingMaxWords - iHackingMinWords) + iHackingMinWords`, lock = difficulty ×
  `fHackLevelMult` × 100 = 0, 25, 50, 75, 100; where `100 - lock` is 0 (Very Hard) the
  fraction is 0.5; never more than 20. Defaults 20, 5, 0.25 (none of the three is in the
  plugins). Rounded down [hypothesis]. The owner's screenshot (20 words) fits Science
  equal to the lock level.
- **Science below the level** [hypothesis]: the player cannot hack; the terminal says so
  (wording §8). Requires Key cannot be hacked at all.
- **Length** [hypothesis, the owner: "the difficulty of the word is determined by the lock
  strength"]: Very Easy 4–5, Easy 6–8, Average 9–10, Hard 11–12, Very Hard 13–15 (the
  lengths players report for Fallout 3 and New Vegas). The owner's Easy terminal had 7.
  NVForever draws a length among those with enough words (more than twice the count).
- **Leveled** flag: not applied; how the level follows the player's level is not
  documented.

## 5. Picking

- A word: the password opens the terminal; another costs an attempt and logs its
  likeness [observed]. Four attempts [observed]. The last one gone: locked for good
  [the owner; documented: GECK wiki Lock, "if player failed to hack terminal … will still
  be locked out"]; the screen then shows `sHackingLockout3` / `sHackingLockout4`
  [documented: hacking_menu.xml; their texts come from the strings table, §8].
- A bracket pair [the owner; observed]: an opening bracket `( [ { <` and the first matching
  closing bracket later in the same row, with no letter between [hypothesis: the row
  limit and the no-letter rule]. Picked at its opening bracket, once. It removes a dud
  (the dud's letters become dots [hypothesis]) and logs "Dud removed." [hypothesis: the
  text], or gives the attempts back (logged [observed]). Which of the
  two: one pair in five gives the attempts back, and every pair does once no dud is left
  [hypothesis]. A used pair is a lone symbol afterwards.
- A lone symbol: logged, then ">Error"; no attempt lost [hypothesis].
- Leaving the game (Tab) before the end: nothing is kept; the next visit is a new dump
  and password with all attempts [the owner].
- Duds are spread evenly over 1 to length − 1 shared letters, never 0 unless too few
  words [design, fitted to §2]. Words are laid one in each equal stretch of the dump, at a
  random place in it, with a symbol either side [design, fitted to §2]. The first address
  is random, four digits [design].

## 6. Opening a terminal; its menu

- Activating a placed terminal: an Unlocked terminal, one hacked before [documented: GECK
  wiki Lock], or one whose password note the player holds (among the notes or carried)
  [hypothesis: the note skips hacking, even after a lockout] opens at its menu; one locked
  out shows the lockout screen; otherwise the hacking game, or a refusal (§4).
- The activate prompt of a terminal that does not open at its menu reads as a lock's,
  "Locked (Easy)" (Very Easy … Very Hard, Requires Key, by its difficulty) [the owner,
  2026-10-22]; one that opens at its menu reads "Access" (§8). When someone else owns the
  terminal (its own owner, else its cell's) the prompt is in the crime colour, and
  winning the hacking game is the crime of opening an owned lock (crime-karma.md §3.2).
- The menu [documented: computers_menu.xml; observed: the owner's screenshot]: centred
  "ROBCO INDUSTRIES UNIFIED OPERATING SYSTEM", "COPYRIGHT 2075-2077 ROBCO INDUSTRIES", the
  server line ("-Server 6-" for server type 5); the welcome text (DESC) at x 100, wrapped
  at 600, 20 under the header; a line 10 under it as long as the text is wide; the list 10
  under that, 700 wide, 12 rows, each "> " and the item's text; at the bottom 30 above the
  edge a ">" prompt and, 34 right of it, the last result text.
- Items listed: those whose conditions hold, run on the terminal reference with the
  player as target [hypothesis: 244 of the conditions run on the subject, 115 on the
  linked reference, which only makes sense from the terminal]. The list is read again
  after every choice [design; "Force Redraw" says the game redraws on some].
- Choosing an item: its result script runs on the terminal reference [hypothesis; the
  default unlock terminals' script is `set myLink to getLinkedRef` / `myLink.unlock`], its
  result text goes to the bottom line, an Add Note item gives its note, a note item shows
  the note's text in the list's place (16 lines at most [documented]; Up / Down scroll),
  a sub menu item lists the sub menu's items. Back (Tab) goes from a note to the list,
  from a sub menu to its parent, else out [hypothesis].
- Scripts see menu 1055 while hacking and 1057 in the menu [documented: GECK wiki
  GetMenuTargetRef]. The world stands still.
- Not built: a terminal whose script has an OnActivate block (20 terminals have a
  script) runs the block instead of opening; ShowHackingMiniGame; Lock / Unlock and
  GetLocked / GetLockLevel on terminals (they use the reference's lock data, which
  terminals do not have); the hacking XP (`iXPRewardHackComputer*` are in the plugins);
  the typing-out of text and its sounds; image notes; the Alternate Colors and Hide Welcome
  flags; saving which terminals were hacked.

## 7. Drawing and keys

- The original draws both screens into a texture shown on the monitor model
  `meshes\terminals\terminalinterface01.nif` [documented: the XML's
  `FORenderedTerminal` reference; Fallout_default.ini `[RenderedTerminal]`: FOV 0.15,
  zoom 36, scanlines, VPos 0.38, a screen light]. So does NVForever (2026-10-16): the
  canvas, 960 × 720 interface units, is drawn into a picture put on the model's glass
  `screen:0`, whose texture coordinates run to 0.767 across and 0.755 down [observed];
  the texture spans 1350 × 960 units of the canvas, the case hiding the glass's edge
  [observed: fitted to the owner's screenshot]. The model is drawn over the world
  darkened to 0.4 [design: the original blurs the room, not drawn], seen from the front
  with a field of view of 0.15 radians [hypothesis: the INI's FOV, vertical], looking
  0.38 below the model's origin [hypothesis: the INI's VPos in model units; observed: the
  owner's screenshot puts the origin 0.37 above the middle] from 38.5 units away
  [observed: fitted, the glass 0.735 of the view's height; the INI's zoom 36 is not that
  distance]. [design] Keep the glare sheet `glare:0` with an opaque base colour
  `GLARE_LINEAR_SCALE` and additive blending in linear light (below), to retain the
  original's top-right streak [observed: §2]. Scanlines are drawn over the canvas (§2); the screen light is not drawn. Without the model the picture is drawn flat over the darkened
  world, its top 83 down [observed: the screen's scale on the owner's screenshots is 0.99
  of the interface's]. [design] World shade alpha is 0.4 with the monitor and 0.88 in
  flat mode; both stay behind the opaque screen, whose colours compose as in §2. The
  model's glare is available only in monitor mode.
- [observed: code at main 53fd575b] The previous canvas drew the measured glass then a
  Pip-Boy image tinted with the block colour at alpha 0.4; the monitor retained
  `glare:0` above the screen. [observed: main's private `before-terminal.png`, 1280 × 720,
  PIL/numpy 10 × 10 medians] Glass away from the streak (895–905; 315–325) is
  (38,73,50); the streak (815–825; 140–150) is (114,169,147). [hypothesis] Excess
  background/glare strength caused the bright glass and washed-out blocks; removing
  them lost the original's texture, vignette and streak. [design] Restore both at
  derived strengths. The screen material remains unlit with inherited emissive cleared,
  so its texels are not tinted again. Keep the monitor's case, camera and power button.
- [observed: the model's blocks, `fnvtool nif decode`, 2026-10-31] `glare:0` has
  NiAlphaProperty flags 4109 (blend on, SRC_ALPHA / ONE: additive), a
  BSShaderNoLightingProperty with `TerminalScreenGlare.dds` (DXT1, alpha 255
  throughout; texels of 4 to 15 in its dark parts, (90, 110, 112) at the streak's peak)
  and a black emissive; no vertex colours.
- [observed: in game, 1280 x 720, 2026-10-31] Through the engine's display-value add
  for additive shapes (`nif_material.wesl` `add_display`, nif.md §6) those dark texels
  put a floor over the whole glass: with the glare off and on, the glass read (13, 35,
  17) and (30-39, 43-62, 26-39) in the middle, (4, 8, 2) and (17, 21, 9) in the corners,
  the blocks (63, 255, 152) and (93, 255, 177); the display-value add turns a texel of
  12 into about 20 display units. The original shows no such floor: corners (8, 11, 5)
  and blocks (59-65, 255, 148-152) under a streak of about +80. Added in linear light,
  the same texels put 1 to 4 units on the glass and the predicted corner (9, 12, 6),
  blocks (69, 255, 153) and middle (21, 41, 25) fall on the original's.
- [design] `terminal_view.rs` therefore draws the glare on Bevy's own unlit additive
  material with the model's texture, base colour `GLARE_LINEAR_SCALE` (1.19, 1.14,
  0.64) in linear light: the original's peak (100, 125, 95) less its glass (21, 44, 26)
  over the texture's peak, per channel. [hypothesis] A counter-example, for this one
  shape, to the display-value add of nif.md §6; every other additive shape keeps the
  engine's rule. The monitor's case, camera, lights and power button are unchanged.

[design] One-line tuning recipes on the `hacking` smoke shot (`hacking.png`), using
10 × 10 medians on empty glass/solid blocks, brightest 40 pixels for ink; remeasure
after each adjustment:

- `GLASS_MIDDLE`: away from ink/streak, add the original's (21,44,26) minus measured
  RGB to its channels, one sRGB unit per unit error (the constant stands below the
  target by the glare's floor); the undercoat is derived automatically.
- `BACKGROUND_ALPHA`: with the middle matched, increase by 0.0056 per excess green
  unit in the dark top-left corner (target about 11, the original's corner), decrease for a deficit;
  this holds the middle fixed while deepening/lightening the vignette.
- `BACKGROUND_TINT`: hold fixed unless the corner hue needs correction; increase
  R/G/B by 4.2/1.9/2.8 sRGB tint units per excess corner channel unit, reverse for
  deficits (middle held fixed). Alpha and tint both affect contrast: tune one at a time.
- `GLARE_LINEAR_SCALE`: on the streak peak, scale a channel by `(linear(target) -
  linear(glass beside it)) / (linear(measured) - linear(glass beside it))`; then
  recheck blocks and text outside the streak (they should move by a few units at most).

- The monitor's power button (`PowerButton:0`, lower right under the screen) leaves the
  terminal when clicked [the owner, 2026-10-16].
- Keys [hypothesis]: arrows or W A S D move the hacking cursor a character or a row (a
  word at once; from the end of a row of column 1 into column 2), Enter or E enters; Up /
  Down choose a menu item, Enter or E takes it; Tab or Backspace go back; Esc leaves. The
  pointer lights what it is on and a click enters it. The first key or the pointer shows
  the cursor.
- Times [design]: 2 s of the wait message before the menu; 1.5 s before the lockout screen.

## 8. Words not in the plugins

The header's password prompt line and attempts line (§2) are not in the masters
[observed, 2026-11-01]: FalloutNV.esm has no string game setting for them (its only
hacking or terminal settings are the `iXPRewardHackComputer*` numbers and
`sTerminalServerText*`), the add-on masters have none either, and `menus\hacking_menu.xml`
holds no text for them (its prompt and header tiles are filled by the game's code; the
only names it reads from the strings table are `sHackingLockout3` and `sHackingLockout4`,
menu-xml.md §2.4). The log's lines (§2: the refusal, the likeness, the attempts coming
back, the match, the wait) are in the same position. So none of them is quoted here, and
the strings table has no entry to give them: NVForever writes its own (LEGAL.md),
in `screen::words` (`HACK_PROMPT`, `attempts(n)`, `HACK_WARNING`) and
`fnv_game::hacking::words` (the log's lines). Only where each line stands, how long the
log's lines are and when each appears follow the original [observed].

The lockout screen's two lines are `sHackingLockout3` and `sHackingLockout4`, read from
the strings table when the screen opens: the load order's string settings, else
NVForever's labels in `fnv_ui::strings::BUILT_IN` (menu-xml.md §2.4).

NVForever's own wording in place of the original's (owner decision 2026-10-30, LEGAL.md):
the hacking title and the menu's two header lines, which in the original name the game's
fictional maker (§2, §6), read "TERMINAL ACCESS PROTOCOL", "TERMINAL OPERATING SYSTEM"
and "VERSION 1.0". The layout is unchanged.

Also NVForever's, for lines the original has not been seen showing [hypothesis]: the
dud removed and error lines of the log, the warning on the last attempt, the refusals
"You need a Science skill of N to hack this terminal." and "This terminal requires a
password.", and the prompt word "Access".

## 9. Checks

`NVF_PLAY_TEST_SCRIPT_MENUS=0.3` and
`NVF_PLAY_TEST_TERMINAL=30:science:25,31:open:000D74D5,33:wrong,34:brackets,35:symbol,36:wrong,37:right,41:item:0,43:power`:
`nvforever --data <Data> --cell BisonSteve01 --play --no-voice --run-seconds 48` → "hacking,
Easy (Science 25): 20 words", the entries, "the password, access granted", "menu of
DefaultUnlockTermDESK1Easy (1 of 1 items listed): "Disengage Lock"", its result script
"terminal view: the monitor drawn", "terminal: the power button pressed" and "terminal
summary: … 1 hacked …". The lockout: `29:open:000D74D5,30:wrong,30.5:exit,
31.5:open:000D74D5,32:wrong,32.5:wrong,33:wrong,33.5:wrong,36:back,37:open:000D74D5` →
a new game after leaving, "locked for good", and "locked out for good" on return.

## Sounds (list 5 item 47)

[observed: FalloutNV.esm SOUN, files in `Fallout - Sound.bsa` under
`sound\fx\ui\hacking\`; hypothesis: which event each is for, by its name; code
`nvf_play::ui_sounds::sound_for`, the `terminal` menu]

| Event | SOUN |
|---|---|
| the cursor onto another character, or the menu's selection onto another row | `UIHackingCharScroll` (`…\char\ui_hacking_charscroll.wav`) |
| a word, bracket pair or symbol entered; a menu row taken | `UIHackingCharEnter` (`…\char\enter\`, 3 takes) |
| the password right | `UIHackingPassGood` |
| a wrong word, the last one locking the terminal | `UIHackingPassBad` |

Not played: `UIHackingCharSingle` / `…Multiple` (`…\char\single\`, `…\multiple\`,
[hypothesis: the text typing out, which is not built]) and the fan hum loop
`UIHackingFanHumLP`.
