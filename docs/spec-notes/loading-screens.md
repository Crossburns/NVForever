# Spec notes: loading screens

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was read from the owner's install (FNV 1.4.0.525) with `fnvtool`,
**[hypothesis]** needs verification. Only §4's layout was captured in the running game
(2026-11-06, marked there).

Code: `fnv_world::loadscreen` (records, choice), `nvf_play::loading` (the screen in the
game), `nvf_play::loading_look` (its tip and emblem). Design: docs/design/loading-screen.md.
Schema: `schemas/fnv/gameplay.json` (LSCR, LSCT).

## 1. The records

**LSCR, "Load Screen"** [documented: fopdoc FalloutNV/Records/LSCR,
tes5edit.github.io/fopdoc, generated from xEdit's definitions]:

| Subrecord | Meaning |
|---|---|
| EDID | Editor ID |
| ICON | the picture, a texture path under `textures\` |
| DESC | the text shown with it |
| LNAM | a location the screen belongs to; none, one or several |
| WMI1 | the screen's type, an LSCT record |

The record header's bit 10 is "Displays In Main Menu" [documented: xEdit
wbDefinitionsFNV.pas LSCR flag list; schemas/fnv/gameplay.json `header_flags`]: 33 of
the 208 LSCR of FalloutNV.esm carry it; no add-on's does (Dead Money's 75 have it
clear, the other add-ons have none) [observed 2026-10-24, private survey script, not
committed].

LNAM is 12 bytes [documented: same page; observed: every LNAM is 12 bytes]:

| Field | Type | Meaning |
|---|---|---|
| Direct | FormID | a cell or a worldspace, or null |
| Indirect: world | FormID | a worldspace, or null |
| Indirect: grid Y | 16-bit signed | |
| Indirect: grid X | 16-bit signed | |

**LSCT, "Load Screen Type"** [documented: fopdoc FalloutNV/Records/LSCT]: one DATA
subrecord of 88 bytes: type (0 none, 1 XP progress, 2 objective, 3 tip, 4 stats), the
text box (x, y, width, height, orientation), font 1 with its colour and alignment (1 left,
2 centre, 4 right), 20 unknown bytes, font 2 with its colour, 4 unknown bytes and a stats
field. NVForever reads the box, font 1, its colour and its alignment of an LSCT of the tip
type (§4). The colours are floats of 0 to 255 [observed: below].

## 2. What FalloutNV.esm holds

[observed: `fnvtool esm survey`, `esm inventory` and `esm decode` over FalloutNV.esm]

- 208 LSCR records and one LSCT (`StandardLoadingTip`, 0x00166D58), which every LSCR
  names in WMI1. Its DATA: type 3 (tip), box x 210, y 100, width 850, height 400,
  orientation 0, font 6 in (255, 180, 53), alignment 2; font 2 is 6 in white, the second
  unknown field reads 4, stats 3. No DLC master adds an LSCT [observed: `esm coverage`
  counts this one DATA over the ten masters].
- Every LSCR has EDID, ICON, DESC and WMI1, in that order, with the LNAMs between DESC
  and WMI1. None has a small icon (MICO).
- The 208 records use 35 different pictures, each 2 to 9 times. All are in
  `textures\interface\loading\`, named `loading_<subject><nn>.dds` with six subjects:
  billboard (9), building (3), bulletinboard (9), desktop (8), magazines (3) and
  posters (3).
- That folder holds 52 textures [observed: `fnvtool dds list` over
  `Fallout - Textures2.bsa`]. The other 17 are not loading-screen pictures: parts of the
  screen (`load_roulette_wheel`, `load_roulette_ball`, `load_roulette_bars`,
  `loading_glare`, `loading_text_bar`, `loading_swipe01`, `loading_brighten01`,
  `loading_vdsgsymbol`, the two `glow_` textures), the start-up screens
  (`loading_screen_legal`, `_legal_ps3`, `_bgs`, `_bethsoft`), and pictures no record
  names (`loading_background`, `loading_missing01`, `loading_screen01`). So the records,
  not the folder, say which pictures are loading screens.
- The pictures are 1024 x 512 texels, DXT1 or DXT5, one mip level. The four of
  `textures\dlc04\interface\loading\` (in `Fallout - Textures.bsa`) are the same size.
- 12 records have locations, 23 LNAMs in all (1 to 4 per record). 19 are direct and all
  of those name cells, interiors by their names (casinos of the Strip, Primm's buildings, Vault 11, the
  King's School, the Silver Rush, the Atomic Wrangler, Michael Angelo's). 4 are
  indirect, all in WastelandNV (0x000DA726):

  | Screen | Grid X | Grid Y |
  |---|---|---|
  | GoodspringsLoadScreen01 | -17 | -2 |
  | PrimmLoadScreen01 | -14 | -14 |
  | BoulderCityLoadScreen01 | 12 | 5 |
  | Vault11LoadScreen01 | 6 | 5 |

  The stored order is Y then X: read that way, three of the four are the grid squares
  of the exterior cells named `Primm` (-14, -14), `BoulderCity` (12, 5) and `Vault11`
  (6, 5) [observed: `fnvtool world cells --world WastelandNV`]. The Goodsprings square
  is an unnamed cell; the cell named `Goodsprings` is at (-18, 0), one square west and
  two north of it.
- The DESC texts are game tips and background, 56 to 248 characters.

The DLC masters' loading screens were not surveyed for this note; the game reads them
through the same code and logs the total at the start (`loading screens: N in the
plugins`).

## 3. Which screen is shown

- A screen with locations belongs to those places; a screen with none can be shown
  anywhere [hypothesis: the editor's wording of the field, "location"].
- The script function `SetLocationSpecificLoadScreensOnly 1` "restricts load screens to
  those tied specifically to the current cell" [documented: GECK wiki,
  SetLocationSpecificLoadScreensOnly]. So without it, screens for anywhere are shown at
  such places too.
- How often a place's own screen is taken against one for anywhere is not known.
  **[hypothesis]** NVForever takes the place's own every other time.
- How far from its grid square an indirect location counts is not known.
  **[hypothesis]** NVForever counts the squares within 2 of it, the squares loaded
  around a player standing there (uGridsToLoad 5). An exact match would seldom
  apply: the Goodsprings square is not the one the town's doors open onto.
- A direct location naming a worldspace would cover all of it [hypothesis; none exists
  in FalloutNV.esm].

To settle these: enter Goodsprings and the Prospector Saloon's neighbours many times in
the original and count the screens (docs/spec-notes/layer4-captures.md style).

## 4. The layout

[observed: frames of the original at 1280x720 and 1280x1024, 2026-11-06, private and not
shipped; a hudtest save at Doc Mitchell's house. Numbers are read off the frames, about
+-5 px, unless marked computed. The model and textures are read from the archives.]

- **The picture is stretched to the window**, not cropped: `loading_building03` stretched
  matches the 1280x720 frame at correlation 0.991, other fits lower (computed). The
  1280x1024 frame's picture was not matched; `menus\loading_menu.xml` gives the picture
  tile the screen's width and height [documented].
- **The tip stands at the top in LSCT's box.** x and width follow the window's width, as
  pixels of a window 1280 wide; y and the text's size follow its height, as parts of 960:
  y 100 is 75 at 720 and 107 at 1024, x 210 is 210 at both, and the lines' centre is 635-636
  at both, the box's. Lines 28 px apart at 720 and 40 at 1024, font 6's pitch of 38 units
  (fonts.md §3.1). The colour is amber in both.
- **Font 6 or 7.** LSCT says font 6; the tip tiles of `loading_menu.xml` say font 7
  [documented: the file]. NVForever takes LSCT's. Three lines of the frames set in each
  font's advances: font 6 within 0.3 % of the frames' widths, font 7 11 % narrower (computed).
  The glyphs themselves have not been compared side by side: check by eye.
- **The band** is `Interface\Loading\loading_text_bar.dds` (`tip_matte_01`) [documented],
  as wide as the screen in the file: the texture's sides are clear and its dark part (alpha up to 192)
  comes out at x about 205-1075 on a window 1280 wide, as in the frames. Down, its dark part
  lies at about y 70-135 at 720 and 99-191 at 1024 for two lines, so the whole texture
  stands from 10 units over the box for two pitches and 25 units [fitted to these two;
  hypothesis: the same for other numbers of lines]. A screen with no text has none [design].
- **The emblem**, bottom centre: a roulette wheel with bars either side, a ball going round
  it. The wheel's centre at (0.5 W, 0.794 H); the wheel about 125 px across at 720 and 175 at
  1024, so 0.172 H; with the bars about 300 and 430, 0.42 H. The bars come out about
  right by the model's own proportions with the wheel at 0.172 H.
- **The emblem's model** is `meshes\interface\circular loading\loading01.nif`, sequence
  `Idle`, 5.333 s, cycle type 0 (`loading_pinwheel` in the menu file) [documented: the
  file]. Flat squares with `TileShaderProperty` textures, in the model's x-y plane, x to the
  right and y down, seen from below z: the normals point down z, and the wheel's numbers
  read the right way round so in the frames. `Plane01`'s `loading_brighten01` plate is many
  times the wheel's size, in another plane, with bit 0 of its object flags set; the frames
  show no such plate. NVForever treats the bit as hidden, as for actors' shapes.
- **It turns.** Rotation keys about z: `main_timer` (the wheel) one turn over the sequence,
  `Pointer` (the ball) two, the other way. On the screen the wheel goes clockwise and the
  ball anticlockwise, at those rates (frames 0.12 s apart) [observed]. The textures are white
  and alpha, so the amber comes from the tile; NVForever draws them in the interface's colour.
- **No word** like "Loading" and no objective, stats or level bar in any screen of this
  save.

Not built, though the original has them:

- The picture stage: `meshes\interface\loading\loadinganim01.nif` (Background, Slide01/02,
  strips, Glare, brighten; sequences Forward, Backward, Left; the sounds
  `UILoadScreenInitial` and `UILoadScreenMicroFiche01` in its text keys). Its motion is not
  measured. So no picture comes in over about 0.6 s and there is no black screen with only
  the emblem for a short load or the start of one: NVForever shows picture and emblem from
  the screen's first frame.
- Several tips for one load (`tile_slide`, `tip_text_01/02` in the file); not timed.
- The objective, stats and level bar, and with them LSCT's other types, its font 2 and
  stats field. Orientation is read by no one (0 in the one LSCT).

## 4a. Not known

- LSCT types other than "tip" (no LSCR of the base game uses another).
- The tile values while a screen is up: the capture's samples read -999, taken before or
  after a screen, not during it.
- When the original shows a loading screen at all (it does for doors, fast travel and
  loading a save [observed by playing]; the shortest wait is not known).

## 5. Behind the main menu

The main menu fades through the pictures of the screens flagged Displays In Main Menu
(§1) [documented: the flag's name] (`fnv_world::loadscreen::main_menu_pictures`,
`nvf_play::main_menu`). Before 2026-10-24 it took the 196 screens with no location
instead (ESM full audit gap 25). A load order where no screen carries the flag keeps
that rule [design]. Whether the flagged screens are also shown while loading is not
documented; NVForever keeps them in the loading pool by their locations as before
[hypothesis]. The `main menu:` log line counts the flagged screens.
