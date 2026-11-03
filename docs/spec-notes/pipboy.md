# Spec note: the Pip-Boy

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/pipboy.rs` (state, keys,
pointer, the image the menus are drawn into), `pipboy/frame.rs` (what every page shares),
`pipboy/items.rs`, `stats/`, `data.rs` and `data/` (the pages), `pipboy/test_aid.rs`;
`engine/nvforever/src/pipboy_arm.rs` (the model's screen, glows and buttons),
`player_body.rs` (the clip and the arms' field of view), `imagespace.rs` (the blur behind
it), `engine/nvf-assets/src/texture.rs` (`pc_texture`).

## 1. Sources

- **Fifteen screenshots from the owner**, 2026-10-11, 1280 × 720 (kept privately in
  `C:\fnv-private\references\pipboy\original\`): every tab of STATS (Status with CND,
  RAD and EFF, S.P.E.C.I.A.L., Skills), ITEMS (Weapons, Apparel, Aid, Misc, Ammo) and DATA
  (Local Map, World Map, Quests, Misc, Radio), in Goodsprings in daylight.
- **The game's interface files** in `Fallout - Misc.bsa`: `menus\main\inventory_menu.xml`,
  `stats_menu.xml`, `map_menu.xml`, `menus\globals.xml` and the pieces they include
  (`card_info.xml`, `tabline.xml`, `tabline_template.xml`, `super_text_box.xml`,
  `text_box.xml`, `box.xml`, `line.xml`, `vertical_fade_line.xml`, `list_box.xml`).
  Described here in plain words; nothing of them is shipped.
- **The models and clips**: `meshes\pipboy3000\pipboyarm.nif` (and the female and NPC
  forms), `meshes\characters\_1stperson\locomotion\male\pipboy.kf` (and
  `female\pipboyfemale.kf`), textures under `textures\pipboy3000\`.
- **`Fallout_default.ini`** in the game folder, and the owner's `Fallout.ini` (the same
  values): `fPipboy1stPersonFOV=47.0`, `fDefault1stPersonFOV=55.0000`; `[Pipboy]`
  `fBlurRadiusPipboy=3.5`, `fBlurIntensityPipboy=0.25`, `fScanlineScalePipboy=50`,
  `bEnableFlickerPipboy=1`; `[InterfaceFX]` `fBrightenPipboy=1.3000`,
  `bEnableScanlinesPipboy=1`, `fVertHoldChance=0.08`, `fShudderChance=0.20` (under
  ";pipboy menu transitions"). NVForever never reads the INIs at run time; the values
  are built in where used.
- The menus' own words (`&-sInventoryItems;` and the like) are game settings that the
  masters do not hold (only 130 string settings are in `FalloutNV.esm`, none of these)
  [observed]; the labels are taken from the screenshots.

## 2. The device

- **The clip** [observed]: `pipboy.kf` is one sequence `Pipboy`, 0.7333 s, clamped, with
  text keys `start` 0, `Blend: 8` 0.033, `Hit` 0.333, `end` 0.733. It drives
  `Camera1st` (a small move, no turn) and both arms from the spine out. Measured on the
  arms rig, the screen's place before the eye through the clip:

  | seconds | screen against the eye |
  |---|---|
  | 0.03 | 115° down (the arm hangs) |
  | 0.23 | 30° down, turning to face the eye |
  | 0.31–0.40 | straight ahead, 11 to 13 units off, facing the eye (0.96–0.99) |
  | 0.73 | 32° down, edge on, going away |

  So the clip both brings the Pip-Boy up (to `Hit`) and puts it away (after `Hit`).
  NVForever holds it at `Hit` while the menus are open and plays on to the end when they
  close [hypothesis: the original does the same; the owner saw it come up and go
  straight back down when it was played to the end]. It fades in over its first 8
  frames and out over its last 8 [hypothesis: frames at 30 a second].
- **Field of view** [documented: the INI]: the arms are drawn as through a view of 47°
  (`fPipboy1stPersonFOV`) while the Pip-Boy is up. NVForever draws them bigger about the
  eye across the view, by the ratio of the two views' half-widths (1.76 at the default
  75°), eased with the clip, instead of with a camera of their own [design]. On the
  owner's ITEMS screenshot and ours the device then stands in the same place to a few
  pixels (buttons, dial, screen corners) [observed]. The normal first-person field of
  view (`fDefault1stPersonFOV=55`) is not applied: the arms and weapons are drawn with
  the world's (open question, §8).
- **Texture** [observed]: the models name `textures\pipboy3000\xbox\english\
  PipBoyArm01.dds`, whose buttons carry the console's LT and RT; `pc\`, `ps3\` and
  `ps3o\` forms lie beside it, and the PC screenshots show no such labels. A texture a
  model names under `\xbox\` is read from `\pc\` when that file exists [hypothesis].
- **Parts** [observed: shape names]: the screen `pipboyscreen:0`; buttons
  `PipBoyButton01`–`03` at the glows `StatsGlow`, `ItemsGlow`, `DataGlow` (STATS, ITEMS,
  DATA from the elbow out); the glass: `ScreenLit:8` (a lit, glossy copy of the screen in
  the same place, `screen.dds` with a normal map) and `glare:0` (`ScreenGlare.dds`, added
  over it, unlit). Also `RadNeedle`, `TabKnob`, `ScrollKnob`, `PipboyLightEffect`.
- **The page's button** glows alone while the Pip-Boy is up [observed: the screenshots];
  down, all three glow (owner, 2026-10-10).
- **The view** is first person while it is up and goes back to what it was [hypothesis].
- **The world behind** is blurred by `fBlurRadiusPipboy` (3.5) in the image space
  modifiers' blur units [hypothesis: the same units], eased with the clip; what is
  nearer than 40 units is the player's arms and is neither blurred nor blurred into the
  world [design]. `fBlurIntensityPipboy` is not used (what it scales is not known).

## 3. The screen

- **Canvas** [documented: globals.xml `_pipboy_width` 1024, `_pipboy_height` 768]: the
  menu files lay out the Pip-Boy menus in 1024 × 768.
- **Mapping** [observed]: the screen's texture coordinates run from 0 to about 0.75
  (u −0.010 to 0.753, v 0.002 to 0.762) over a 4:3 shape. Fitted to the owner's ITEMS
  screenshot, the canvas fills 0.8 of the texture each way (the menus were 1.07 times as
  wide and 1.10 times as high as a fill of 0.75 gave): a 1024 × 768 canvas in the top
  left of a 1280 × 960 image. So the screen shows the canvas to about 964 across and 731
  down. NVForever draws the menus into such an image (interface lengths are its pixels)
  and puts it on the screen as it is.
- **Colour** [observed: fitted]: the solid amber of the CND bar on the screenshot is
  about (220, 180, 64); unlit and unscaled our brightest text came out (237, 167, 53)
  through the view's tone mapping; multiplied by (0.83, 1.17, 1.27) it is
  (221, 179, 59). `fBrightenPipboy` is not applied as such.
- **The glass** is not drawn while the menus are on the screen: drawn, the glare lay
  over them as a pale grey-blue haze. The screenshots show only a faint lighter patch at
  the top right, and an empty screen of about (31, 27, 15) where ours is (15, 9, 0)
  [observed]: the original draws the glare far more weakly [hypothesis; open, §8].
- **Scanlines** [observed]: `pipboyscanlines.dds` is 64 × 64 DXT1, one soft dark band
  (white at the top to about 0.8 at the bottom). On the screenshot the solid bar's rows
  are bright and dim by about 15% every 6 pixels, which is 9.5 of the canvas. NVForever
  tiles the band over the menus at that pitch, darkening by its shortfall from white
  [hypothesis]. Flicker, vertical hold and shudder (the INI's transitions) are not built.
- **Background**: `Interface\Shared\Background\pipboy.dds` over the whole canvas at
  brightness 90 [documented: inventory_menu.xml].

## 4. What every page shares [documented unless marked]

- The menus' rectangle: 50 in and down, 855 × 650.
- Heading: a vertical line fading down at its left (60 long), a line of 50 to the title
  (font 4, 70 in), a line from 20 after the title to the first card; cards of figures
  (`card_info.xml`): a line along the top, one fading down the right side, the title 5
  in and the value right-aligned 10 from the right end, both 20 down in font 2. ITEMS:
  Wg at 215 (155 wide), HP 380 (155), DT 545 (110), Caps 665 (190). STATS: LVL 270
  (100), HP 380 (155), AP 545 (135), XP 690 (205).
- Tab line (`tabline.xml`): 650 down, as wide as the menus, a vertical line rising at
  each end; each tab is its name in font 2 with 35 across and 15 up and down
  (`tabline_template.xml`, `text_box.xml`); the tab shown is boxed (lines at brightness
  200) and filled (brightness 128, alpha 32); the lines between the tabs are as long as
  each other [hypothesis: the file leaves their length to code; on the screenshots they
  are even].
- Lines are 2 thick on a PC; lines at brightness 200, text 255.
- A text's letters stand 15/31 of the font's size under its top [hypothesis for every
  font but 2, where it was measured: message-menu.md §3].

## 5. The pages

- **ITEMS** (`items.rs`): the inventory model (`menu::model`, `Mode::Inventory`), the
  tab choosing the kind of item. Built: the heading (Wg, HP, DT and Caps), the list
  (name, count in brackets, the filled square of what is equipped, the selected row
  boxed), 9 rows at once (51 a row in a list 468 high). A row (its box) runs from the
  scroll bar to 5 short of the list's right edge, as the DATA lists' [design: it was the
  Repair page's 393, which ran over the item's figures; owner's screenshot 2026-11-01].
  - Added 2026-10-23 (owner's playtest list 7, items 5, 6, 15, 20, 33):
    - **HP** is the STATS page's health out of its whole; **DT** the threshold a hit on
      the player meets (worn armour at its condition plus the Damage Threshold actor
      value, `combat::target_armor`), one decimal [observed: "HP 205/205", "DT 8.0" on
      the owner's screenshots; hypothesis: the figure is combat's].
    - A name over two lines makes its row taller; the first row drawn moves on so the
      selection is always in the list, and the model keeps it as its scroll position (the
      last of many weapons could not be brought into view: the model counts nine rows a
      page) [design].
    - An aid item's EFFECTS: aid-items.md §14.
    - Skill books (BOOK with a skill to teach) are on the Aid tab, other books on Misc
      [owner: "Skill books ... should be showing in the Aid tab"; hypothesis: the
      others].
    - The Misc tab's folders [owner's wish; design]: the keys (KEYM) are one "Keyring"
      row and the Caravan cards one "Caravan Cards" row, at the top, each with its count;
      using one lists what it holds under a "Back" row (Backspace too; another tab closes
      the folder). The cards are those AddCardToPlayer gave (kept outside the inventory,
      caravan.md §2), and any CCRD item carried; they are neither used nor dropped.
  - The scroll bar at the list's left (32 wide), only while some rows do not show: the
    arrows at its ends and a line for the part shown [observed: the owner's scroll
    screenshot; the line's look as the DATA lists' (hypothesis)].
  - The selected item (the row under the pointer is selected) at the right
    [documented: places, `inventory_menu.xml`, `item_stats_display.xml`]: its picture
    (ICON) in `IM_ItemIcon` (450, 10, 380 square); its figures in `IM_ItemInfoRect`
    (385, 340, 446 × 150), cards a third of it wide and high, 10 apart: DAM (WEAP base
    damage) or DT (ARMO DNAM) at the left, WG and VAL; a weapon's STR (its Strength
    requirement) over VAL; the second row CND (a bar 62 in, 15 down, 20 high, 72 short
    of the card) and beside it, two cards and 10 wide, the ammunition ("20ga (2/30)":
    the AMMO short name, then what a clip holds of what is carried over what is
    carried, hypothesis) or armour's class ("Light", "Medium", "Heavy"); EFFECTS across
    the three cards on the first free row: an aid item's own effects, apparel's or a
    weapon's enchantment's (EITM), written as the STATS EFF list writes them ("HP +40",
    "AGL -1") [observed: the owner's ITEMS screenshots, all five tabs]. WG and VAL are
    the stack's (the Stimpak's VAL 675 is 9 × 75; WG "--" for nothing) [observed: VAL;
    hypothesis: WG].
  - Added 2026-10-19 (`items.rs`):
    - **DPS** [documented: `item_stats_display.xml` `DPSInfo`, `sInventoryDamagePerSecond`;
      hypothesis: formula]: the damage a shot does at the item's condition
      (combat-rules.md §17) times its shots a second (an automatic weapon's fire rate,
      else its attack shots a second, else one), rounded [hypothesis: the GECK wiki's
      Weapons page names damage and fire rate as the DPS calculator's inputs]. Shown in
      the container and barter figures only: the original's ITEMS page has no DPS card
      (2026-10-24, below) [observed: the owner's ITEMS screenshots].
    - **The weapon's skill badge** (2026-10-24; replaces the ammunition's picture and
      the DPS card guessed on 2026-10-19) [documented: `IM_ItemIconBadge` at (390, 260),
      64 square, and the text `IM_StrReq` at (440, 300), font 7, left-justified;
      `textures\interface\icons\typeicons\weap_skill_icon_*.dds` (sm_arms, big_guns,
      energy, explosives, melee, unarmed), 64 square, a boxed picture in the lower left
      48; observed: the owner's screenshots show the bullets picture with 0, 25 and 50
      for the Single Shotgun, Service Rifle and Sawed-Off Shotgun (their Guns skill
      requirements, WEAP DNAM) and the knife picture with 25 for the Throwing Spear
      (Melee Weapons 25)]: the picture of the skill the weapon uses and, beside it, the
      skill it needs, "0" too. A weapon of another skill (binoculars: Barter) shows
      neither [hypothesis].
    - **The buttons' text** (Repair/Maintain R), Mod X), the repair and mod lists'
      Cancel/Exit) [documented: `text_box.xml`: the text sits half the box's 15 spare
      height plus 10 under the button's y; observed: "Maintain R)" clear of the heading's
      cards on the owner's screenshot]. Before 2026-10-24 it was drawn at the button's y
      and touched the Caps card (the owner's report).
    - **"Mod X)"** [documented: `IM_ModButton`, 47 under the Repair hint, `sInventoryMod`,
      PC key X; hypothesis: shown for weapons with mod slots (WMI1–3), bright when the
      player carries a mod that fits a free slot, dim otherwise]. X on a bright hint opens
      the mod menu (2026-10-20, weapon-mods.md §4–§5); elsewhere X still drops an item
      [design].
    - The repair hint ("Repair R)" / "Maintain R)") is built (repair.md §5), and the
      repair list takes the pointer (repair.md §5).
  - The mod menu (`item_mod.rs`): weapon-mods.md §5.
  - The flat image (no model's screen) shows only once the Pip-Boy is all the way up:
    before, it flashed for a frame or more on every Tab while the body took the screen
    (the owner's list 5, item 2).
- **STATS** (`stats/`): §5.1.
- **DATA** (`data.rs`, `data/`): §5.3.

### 5.1 STATS

Implementation: `pipboy/stats/mod.rs` (layout, keys, pointer, test aid), `model.rs` (the
words and rules), `read.rs` (the records and the player's figures). Sources:
`menus\main\stats_menu.xml` and the prefabs it includes (`list_box.xml`,
`list_box_template.xml`, `super_text_box.xml`, `text_box.xml`, `meter.xml`,
`scrollbar_vert.xml`, `box.xml`) [documented]; the owner's screenshots 1–5 (Status CND,
RAD, EFF, S.P.E.C.I.A.L., Skills); none of Perks and General, which are laid out from the
file alone. Compared side by side at 1280 × 720 with a new run (default stats): every
tab with a screenshot stands within a few pixels of the original [observed].

**Images in the file** [hypothesis, supported by the file's comment "filewidth/fileheight
already adjusted for zoom"]: an image tile shows its file at the tile's zoom from the
top left, cut off at its width and height, not stretched. The Vault Boy's pieces then
fill exactly the parts of their files the file gives as their sizes (the head's outline
is the top left 85 × 92 of `head.dds`) [observed].

- **Heading** [documented: places, `stats_*_info`]: LVL (the player's level), HP and AP
  as current/maximum (actor values 16 and 12: base and permanent modifier, less damage),
  XP as experience/what the next level needs, (n − 1) × ((n − 2) / 2 × `iXPBumpBase` +
  `iXPBase`) [documented: GECK wiki Settings; 200 at level 1, as the screenshots].
  Without the scripts' game the figures are "--".
- **Status** (`stats_status_container`, 50, 110, 855 × 482):
  - CND, RAD, EFF down the left, 84 wide, 51 high (font 2 and 20), boxed when pointed
    at, boxed and filled when chosen [documented]. The hardcore buttons (H2O, FOD, SLP)
    under them are not built.
  - Hints at the top right, right-aligned at 900, a slot 51 high each: "(9) Stimpak S)",
    the controller's healing mode's slot empty, "(3) Doctor's Bag E)"; on RAD "RadAway
    A)", "Rad-X X)" [documented: places, `_PCButton_*`; observed: the words; hypothesis:
    the count is left out at 0 (the RAD screenshot has none), dim unless pointed at].
    The four items are the DOBJ list's Stimpak (place 0), Doctor's Bag (21), RadAway (3)
    and RadX (2) (default-objects.md §1), else the Editor IDs `Stimpak`, `NVDoctorBag`,
    `RadAway`, `RadX` [observed: FalloutNV.esm's list names those four, so nothing changes
    with the shipped data; ESM full audit 2026-10-24].
    **Using them on CND** (built 2026-10-23, owner's request) [design]: S or a click
    on the Stimpak hint uses one on the body, or on the limb chosen (keys 1-6: head,
    torso, left arm, right arm, left leg, right leg; the same key again the body; the
    pointer on a limb's bar chooses it while there, and a click on the bar uses a
    Stimpak on that limb); E or the Doctor's Bag hint uses one on every limb. An item is
    not used when it would not heal: a Stimpak on the body at full health, on a limb
    that is whole, a Stimpak on a limb in Hardcore mode (it mends no limb there
    [observed: FalloutNV.esm help text, "In Hardcore Mode, a Doctor's Bag or an actual
    doctor is required"]), a Doctor's Bag with no limb hurt, or none carried
    (`model::refusal`). The chosen or pointed-at limb's bracket is boxed. The item is
    taken, its consume sound plays and the runtime uses it as the inventory does
    (`stats::use_hints`); what it does to the limbs: aid-items.md §3. RadAway and Rad-X
    are not used from here (A also switches the tab; the keys are logged).
  - CND: the Vault Boy at zoom 145, head 40 down and centred, face, torso, arms and legs
    placed from it as the file gives; a bracket (`vats_*_divider.dds`, 38 high) with a
    solid bar (77 × 14 at 25 or 51, 24) at each of six body parts, the bar as long as the
    part's condition (actor values 25–30, current over permanent) [documented]. A part at
    0 shows "Crippled" in place of its bracket [documented: the text's visibility;
    hypothesis: the word] and its `*_broken.dds` piece [hypothesis: the name]. The face
    is `face_00.dds` always [hypothesis; when 01–04 and 10 show is not known]. Under him
    "<name> - Level <n>" centred on the area's foot [observed: "Loot - Level 1"].
    Limb damage comes from combat's actor value damage; a new run's parts are whole.
    The bars read the same actor values combat lowers and cripples by
    (`Game::damage_limb`, `PlayerLimbs`) [observed: smoke `pipboy-stats-heal`].
  - RAD (zoom 130): "RAD RESIST" and the value (actor value 20, "8%") in a window
    showing the right end of `rad_effects_bracket.dds`, 360 down; the meter
    (`rad_meter_bracket.dds`) beside it, its 0 at 186 and full at 482 of the picture,
    "500" and "1000" over it, the arrow (`pointer_tail.dds`) at the player's rads (actor
    value 54) [documented: places; observed: a maximum of 1000, and no "0" written at
    the zero mark or at the arrow, so a 0 is not written (hypothesis)]. The radiation
    effects list above (`stats_rad_effects_*`, hidden by the file) is not built.
  - EFF: one line per source, 56 high, the name 20 in and the changes 300 to its right,
    lines between [documented]; the list centred in the area [hypothesis: fitted, one
    line at the middle on the screenshot]. Sources: the enchantment (ARMO `EITM`, ENCH
    of type Apparel) of each worn piece of apparel, named by the enchantment's FULL
    ("Vault Jumpsuit"), its Value Modifier effects written "Speech +2, Melee Weap. +2"
    with the actor values' short names (AVIF `ANAM`) [observed: the screenshot and the
    records]; then aid items (ALCH) with active effects. A detrimental effect (MGEF flag)
    is written with a minus [hypothesis]. "No Effects" when there is none [hypothesis:
    the words].
- **Lists** (S.P.E.C.I.A.L., Skills, Perks): 35, 110, 325 / 350 / 420 wide (25 / 50 more
  when a row is marked), nine rows of 51; the row 52 in (scroll bar 32 and 20), its name
  15 in and 20 down, its number right-aligned 10 before a right buffer of 20 (55 when
  marked), the mark 10 after [documented]. Skills are in name order [observed: Guns
  between Explosives and Lockpick], Big Guns left out [observed: thirteen; its AVIF is
  "Big Guns - OBSOLETE"]. A value shown is the current value plus worn apparel's changes
  [design: the game does not apply worn enchantments to actor values yet], skills kept to
  0–100; "(+)" or "(-)" when it differs from the permanent value [observed: "34 (+)";
  hypothesis: "(-)"]. Perks: the player's perks, playable and not hidden by PERK DATA
  [hypothesis: its bytes as trait, minimum level, ranks, playable, hidden], traits
  included [hypothesis], in name order, the rank as the number when the perk has more
  than one [hypothesis].
  - Names, descriptions and pictures: AVIF FULL, DESC, ICON (the record of code c is
    FormID 0x514 + c, 0x3E8 + c − 5, 0x44C + c − 12, 0x4B0 + c − 32 or 0x5DC + c − 46 by
    range [observed in FalloutNV.esm]); PERK and REPU the same.
  - The selected row is boxed and filled (alpha 40) after a key moved the selection or
    while the pointer is on a row [hypothesis: neither screenshot shows a box, the
    pointer on the tab line or off the screen].
  - Scroll bar when the rows do not all show: arrows at the list's top and foot, the
    marker 6 wide, as high as the track over (hidden rows + 2) [hypothesis: fitted to
    the Skills screenshot; the file leaves these to code].
  - Beside the list (`stats_display_rect`, from the list's right side to 905): the
    picture at zoom 150 in 340 × 384, centred, 60 above; a line 22 in and 319 down with a
    fading line down from its end; the description (font 2, lines 35 apart) 22 in, 333
    down, 175 high, scrolled 20 a step with arrows at its right [documented]. It scrolls
    when its lines take more than 175 [observed: five lines fit on the S.P.E.C.I.A.L.
    screenshot, six do not on the Skills one].
- **General** (`stats_genrep_container`, 20, 50): the reputations (the file's first mode)
  or the statistics; R or the button "General R)" / "Reputation R)" at the top right
  switches [documented: `_PCButton_R`; hypothesis: the button names the other list].
  The list 500 wide from 40, 100, nine rows [hypothesis: the file's height, 542 from
  100, would reach the tab line]. Reputations: those with fame or infamy [hypothesis],
  the selected one's picture (REPU ICON, zoom 150) and its title by the levels of fame
  and infamy (Accepted, Liked, Mixed …) [documented: GECK wiki GetReputationThreshold;
  hypothesis: "Dark Hero" with more infamy, "Soft-Hearted Devil" with more fame], and
  "Fame n, Infamy n" [design]. Statistics: the 43 New Vegas names [documented: GECK wiki
  GetPCMiscStat] in the order of their codes [observed: the owner's screenshot of the
  original's list, 2026-10-14] with their counters (docs/spec-notes/misc-stats.md; "--"
  without the script runtime). Beside them the karma:
  "Karma: <alignment>", the picture, the karma title and the karma's number
  [hypothesis: alignments by the globals `KarmaVeryEvil`, `KarmaEvil`, `KarmaGood`,
  `KarmaVeryGood`; pictures `karma_evil`, `_bad`, `_neutral`, `_good`, `_saintly`].
  The karma title is the game setting `sKarmicTitle<Good|Neutral|Evil><level>` (levels
  1–30, 30 for 30 and over) [observed: FalloutNV.esm holds 50 of the 90, e.g.
  `sKarmicTitleNeutral1` "Drifter", `sKarmicTitleEvil29` "Demon Spawn"; the rest are
  presumably the executable's defaults], else the owner's table (`todo 5.txt`,
  `stats::model::KARMA_TITLES`); Good and Very Good take the good column, Evil and Very
  Evil the evil one [hypothesis]. The reputation list (the other mode) is the hook for
  the reputation workstream.
- **Keys** [design]: the arrows choose CND, RAD, EFF (S, E, A, X are the hints' keys
  there, logged only; A also steps the tab); Up/Down and W/S select in a list, Page
  Up/Down scroll the description; the wheel scrolls the list, or the description when
  the pointer is on it; R switches General. The pointer selects the row it moves onto
  (not a row drawn under a pointer that has not moved) and clicks the buttons and arrows.
- **Checking it**: `NVF_PLAY_TEST_PIPBOY_STATS=<seconds>:<step>,...` with `sub=<0-2>`,
  `down`, `up`, `mode`, `desc=down|up`, and `perk=<FormID>` / `fame=<FormID>=<n>` to give
  the player something to show; each step and each selection is logged ("pipboy stats:
  Skills: Science 15 selected"), and a summary when a tab is drawn ("pipboy stats: LVL 1,
  HP 200/200, AP 80/80, XP 0/200; 7 S.P.E.C.I.A.L., 13 skills, …").
- **Open**: the game settings the page's words come from (`sStatsCrippled`,
  `sStatsNoEffects`, `sPCMenuHint*`) are not in the masters; when each face shows;
  whether the hints count 0 and when they are bright; whether worn enchantments should
  change the actor values themselves (then the page needs no sum of its own); the
  highlight box's rule; General's list height, order and right buffer, the reputation
  list's rule and second line, the karma titles. (The misc statistics' codes: misc-stats.md §1.)

### 5.3 DATA

`data.rs` (the page: state, input, what is shown and its drawing), `data/model.rs` (plain
rules, tested), `data/read.rs` (records). Sources: `menus\main\map_menu.xml` (places, sizes,
fonts) and the owner's five DATA screenshots (§1), compared at 1280 × 720 with
`NVF_PLAY_TEST_SHOTS` runs; px below are those screenshots' pixels, about 0.62 to a unit.

- **Heading** [documented: map_menu.xml]: the title 70 in after a line of 50; a line from
  180, 470 long; the place right-aligned at 630 and the date and time at 845, both 20 down
  in font 2; lines fading down at 650 and at the end, a line from 660 to the end.
  - The date: month, day, the year's last two digits, then the time, "10.25.81, 9:29"
    [observed: the screenshots, on 25 October 2281]; no leading zeros and a 24-hour clock
    as the wait menu's [hypothesis, wait-menu.md §4].
  - The place: indoors the cell's name [hypothesis]; outdoors the map name (`REGN RDMP`)
    of the regions the player's cell is in (`CELL XCLR`), the highest priority first, else
    the worldspace's name [hypothesis: the screenshots show "Goodsprings", the map name
    of `VMapGoodspringsRegion` (observed)].
  - Right-aligned texts of the original end about 5 px further right than ours (the time,
    "Show Location", "Place Marker") [observed]; not corrected.
- **Lists** (Quests, Misc, Radio) [documented: map_menu.xml, list_box.xml]: 15 left of the
  menus, 75 down, 400 × 468, the scroll bar of 32 on the left; a row's square of 25 at 15,
  20 and its text 40 in and 20 down in font 2, broken at 310. A row is 54 high and a text's
  further lines 34 lower [observed: fitted; rows 33.5 px apart, two-line rows 54.5 px];
  the squares start at the same pixel as the original's [observed]. The selection box
  starts 9 left of the row and ends 5 short of the list's edge [observed: the Radio
  screenshot boxes from 448 px; documented: the 5]. The box shows while the pointer is
  on a row or after Up/Down (W/S); a new tab starts without it [design: the screenshots
  show it only under the pointer]. Hover selects, a click or Enter acts. The wheel scrolls
  the list. The scroll bar: the file's arrows (32 × 16) at the ends and a line for the part
  shown [hypothesis: the marker's look].
- **Buttons at the right** [documented: map_menu.xml, text_box.xml]: right-aligned at
  855, from 75 down, each its font-2 text with 20 across and 15 up and down, the text 10
  under the middle of the spare height, a PC key's label 5 after the box. Their words are
  game settings the masters do not hold (§1): "Place Marker", "Show Location", "Show
  Active Quest Notes", "Challenges" with "R)" [observed]; "Show All Notes" [hypothesis:
  the setting `sShowAllNotes`]. The hover box is not drawn.
- **Quests**: the quests with a name and an objective shown or done (quest-messages.md §5);
  running quests before those done (completed or failed), each part newest first, quests
  first seen together by FormID from the highest [hypothesis: fits the screenshot's
  order, the add-ons' quests (01…–04…) on top, "Back in the Saddle" (0010A214) before
  "They Went That-a-Way" (000842DD), the completed "Ain't That a Kick in the Head" last].
  "Newest" is when NVForever first saw the quest listed (looked at four times a second;
  not saved). Done quests are drawn at half strength [hypothesis: greyed on the
  screenshot]. The active quest's square is filled; a click or Enter on a running quest
  makes it active [hypothesis: the PC's form of "Make Active Quest"]; until the player
  picks one, the newest running quest is active [design]. When the page comes up the
  active quest is selected [hypothesis: the screenshot shows its objectives with the
  pointer elsewhere].
  - Objectives at the right: 400 wide at the menus' right edge, under the buttons' place
    (75 + 120) less a button's height, the scroll bar on the right [documented]; each
    behind a square, filled and at half strength when done [hypothesis]; those to do
    first, each part from the latest objective down [hypothesis].
  - "Show Location" shows the World Map [design: the quest's place is not centred].
- **Misc**: the player's notes (AddNote; `NOTE` records) by name [hypothesis]; "(6 of 6)"
  is part of the note's name [observed: `VMQ01CourierNote` FULL "Mojave Express Delivery
  Order (6 of 6)"]. A text note's text is `TNAM`, a NUL-ended string [observed; the schema
  does not read `TNAM` of `NOTE`]: font 1, broken at 400, in a window 17 left of the item
  data, 20 down, 400 × 385, scrolled 20 a wheel step [documented]; Page Up/Down scroll a
  window [design]; arrows at its right when it is longer [documented]. The window reaches
  10 over the first line so its letters are not cut [design]. Our lines break exactly as
  the screenshot's [observed]. An image note shows its picture (`XNAM`) 360 square at the
  right [documented]; sound and voice notes (holotapes) play when used (a click or
  Enter; again to pause, again to go on) and show the audio meter (time remaining and a
  marker moving along the tick meter)
  ([holotapes](holotapes.md) §3). "Show Active Quest Notes" keeps the notes whose quest (`ONAM`) is the active one.
- **Radio**: the stations the Pip-Boy receives where the player is, by name (`crate::radio`,
  radio.md); the one playing has its square filled; a click or Enter tunes to a station,
  or turns the radio off on the one playing [hypothesis: "Tune"]. The waveform panel
  [documented]: 500, 125, 350 square, the tick meters along its foot and right side, the
  flat line (2 high, 5 short, 8 over the middle) while nothing plays [observed: the
  screenshot, the radio off]. While one plays the original's trace moves [observed: a
  60 fps capture of the Radio tab, 2026-10-27]: about 8 waves across the panel stand
  nearly still while their envelope runs to the left, some 7 panel widths a second;
  its height follows the sound (a steady tone draws even bursts with flat gaps, music
  peaks of mixed size, a quiet stretch small ripples), up to about a third of the panel
  either side of the line. Here the waves are two sines drawn under an envelope taken
  from the playing file's loudness (root mean square over 2048 samples) with a slow
  rise and fall of its own [design: whether the original draws the sound itself or a
  model is not known].
- **World Map**: the worldspace's picture `WRLD ICON`
  (`interface\worldmap\wasteland_nv_1024_no_map.dds`), drawn 2048 square [documented:
  `filewidth` 2048; `MNAM` usable 2048] at alpha 128 [documented]; the menu file names
  `wasteland_1024_no_map.dds`, which the plugin does not use [observed]. Where cells fall on it: the cells from `MNAM`'s north-west cell to its
  south-east cell (−35, 35 to 25, −25: 61 a side) fill the middle 0.7876 of the picture
  from (0.108, 0.102) [observed: fitted to Goodsprings as the screenshot shows its roads,
  the arc of Hoover Dam and the ring of Jacobstown, 309.8 units a texel, within 4 texels
  each; corner to corner on the whole picture put Goodsprings 11,000 units off its
  road]. The menu file keeps the picture's edges at most to the window's middle; it opens
  centred on the player [hypothesis: the owner's view was elsewhere]; dragged it pans,
  the wheel zooms 1.25 a step from 1 to 4 times [design].
  - Map markers: references of the `MapMarker` STAT (0x10) with `XMRK`; name `FULL`, type
    `TNAM` (0 None … 14 Vault [documented: xEdit; observed: vaults 14, a cave 5, Camp
    McCarran 8, Hoover Dam 7]), flags `FNAM`, radius `XRDS`. Shown when the player has been
    within the radius (`XRDS`, noted four times a second, not saved) with its type's
    icon, or when ShowMap or the Visible flag made it known, with the undiscovered icon
    [hypothesis]. Icons `interface\icons\world map\icon_map_<type>.dds`, the inverted form
    under the highlight box [hypothesis: by the files' names]; 24 wide, 1.75 times and
    brightness 255 (else 220) under the box [documented: the template's 1.75 and
    brightness; observed: 24 fitted].
  - The player's arrow: `glow_cursor.dds` [documented], 40 wide, 1.28 times as high,
    turned by the heading [observed: fitted, the screenshot's triangle 25 px; hypothesis:
    the turn's sense]. Drawn over the markers and under the highlighted one [documented:
    depths 1, 7, 11].
  - Quest markers: the targets (`QSTA`) of the active quest's objectives shown and not done,
    where they are now; a target indoors at the door outdoors into its interior
    [hypothesis: the screenshots' quest marker for Sunny Smiles stands by the saloon's
    door]; the picture is the HUD compass's objective marker
    `glow_hud_compass_objective_marker.dds` [hypothesis: the shape on the screenshots],
    56 wide [observed: fitted], 1.25 times as high, 15 right and 5 down [documented]. The
    marker is for the targets whose conditions pass [documented: Quest Objectives Tab],
    evaluated four times a second [hypothesis]; a
    target indoors is marked at a door into it that is not locked unless the target ignores
    locks [hypothesis] (quests-dialogue.md §2, target conditions).
  - The highlight box [documented]: 160 square, corners of 40 with 15-long fading tips, a
    dot of 8 (`waypoint_node.dds`) at its middle, the name in font 1 centred 180 down. It
    follows the pointer over the map and takes a marker's place and name within 40
    [hypothesis: the screenshots show it away from the middle, once round a door with
    the door's place name]; at first it is at the window's middle. A World Map marker
    with a reputation (REFR `WMI1`, 36 in FalloutNV.esm [observed]) shows, on the line
    under its name, the reputation's name and the player's standing title with it,
    "Goodsprings: Neutral" ("The reputation that is shown when the player hovers over the
    icon on the world map" [documented: GECK wiki Reference, Map Marker]; the wording and
    the second line are a hypothesis, owner-review.md).
  - A click on a marker (the button pressed and let go without the pointer moving
    more than 6 units [design]) asks whether to travel there: fast travel,
    `fast-travel.md`.
- **Local Map**: the same window, markers, arrow, quest markers (kept inside the window
  [documented]) and highlight box, on an empty map at 28 game units to a unit [observed:
  fitted, the gas station's door and the saloon's door 168 px apart on the screenshot; the
  zoom then is not known]. Doors leading elsewhere (`XTEL`) in the 7 × 5 cells round the
  view (or the interior) show `interface\icons\local map\icon_local_door.dds`, named by the
  place their destination door stands in [observed: the screenshot's door is named
  "Goodsprings Gas Station", the cell `GSGasStation`]. Left out: a door reference flagged
  Hidden From Local Map (header bit 6, "The reference will not be displayed on the local
  map" [documented: GECK wiki Reference]; 4 teleport doors, all in Dead Money
  [observed]), a door whose base DOOR is flagged Hidden (`FNAM` bit 2 [documented: xEdit];
  3 in FalloutNV.esm, all secret doors, none a teleport door [observed]; what the flag does
  is a hypothesis, owner-review.md), and a door disabled now [hypothesis, as a disabled
  marker]. Under them an outline of the place
  (`data/outline.rs`) [design: the original renders the area from above, its screenshot
  dark save a few shapes; the owner asked for an outline]: a grid of rays straight down
  through the physics world's collision (not actors, triggers or loose clutter), 96
  units apart outdoors from 8000 over the feet, 28 apart indoors from 150 over the feet
  (under the ceiling), cast a few hundred a frame (6 ms) and made into one picture when
  done; outdoors the ground shaded by its slope lit from the north-west, indoors the
  floor (a ray landing within 120 below and 40 above the feet) filled faintly, what
  stands on it brighter, and the floor's edge to nothing (the walls) or a step of 64
  down to a neighbour drawn bright on the higher side; outdoors a step of 192 (walls,
  cliffs, not the hills' slopes at that spacing). The grid covers the window at a
  magnification of 1 and 15% more round its anchor, kept while a pan stays within a
  quarter of it, made again for another floor (128 up or down) or place. Indoors the
  map is 8 units to a canvas unit (outdoors 28) [design]. What the physics world has not
  loaded stays empty. Not built: which doors and markers the original shows (its
  screenshot shows only the gas station's door), Place Marker, fog of the unexplored.
- **Dragging** (both maps): the left button held down on the map and the pointer moved
  pans it [design]; the pointer's own moves are followed, not Bevy's drag events, which
  ended at once because the map's nodes are drawn again as the highlight box follows the
  pointer (the owner's list 5, item 52). Test aid: `drag=<dx>:<dy>`.
- Checking it: as §7 with `page=data` and `tab=<0-4>`; `key=down|up|use` steps move and
  act on the DATA lists too. A run with a note, quests and the radio on:
  `NVF_PLAY_TEST_SCRIPT=3:info:0x001057E8:DocMitchellREF,4:start:VCG02,4.5:objective:VCG02:3,5:objective_done:VCG02:3,5.1:objective:VCG02:5`
  (Doc Mitchell's line whose result script adds the courier's note) and
  `NVF_PLAY_TEST_RADIO=5:next`. Log: "pipboy data: N map markers", "map marker … found",
  "local map: N doors leading elsewhere", "active quest …", "radio tuned to …".
- Open: the quest list's order and the active quest's default; the objectives' order and
  how done ones look; the hollow square's size (ours draws smaller than the original's,
  about 9 px against 14); which doors and markers the local map shows and its zoom;
  where the World Map opens; how the original turns the arrow; the radio's waveform model;
  Place Marker, Challenges, the audio notes' panel, the buttons' hover boxes.

## 6. Keys and pointer [hypothesis unless marked]

- Tab opens and closes; Esc closes. F1, F2, F3 show STATS, ITEMS, DATA; A/D and
  Left/Right the tabs [design]. ITEMS: Up/Down (W/S), Page Up/Down, the wheel, Enter or
  Space to equip, unequip or use, X to drop one, Shift+X the stack [design].
- The pointer is followed onto the model: on the screen, it is passed to the menus as
  a pointer of their own (Bevy's picking, as its `render_ui_to_texture` example does); a
  click on a button on the device shows its page. A tab and a row take clicks; the
  pointer on a row selects it, a click on the selected row acts on it.
- The pointer is found on the shapes as the arm's bones pose them, from CPU copies of
  the screen's and the buttons' meshes (nvforever `pipboy_arm.rs`, `Posed`) [design].
  Until 2026-10-11 Bevy's mesh ray cast was used, which sees neither the skinning nor a
  mesh kept on the GPU only, so no click on the model did anything (the owner's
  playtest). The button under the pointer glows as the page shown's does [design].

## 6.1 Sounds [observed: the SOUN records, FalloutNV.esm; the owner's list 5]

At the listener: another tab `UIPipBoyTab` (`fx\ui\pipboy\ui_pipboy_tab.wav`); another
page (the device's buttons, F1–F3) `UIPipBoyMode` (`ui_pipboy_mode.wav`); the pointer
onto another tab of the tab line `UIMenuFocus` (`fx\ui\menu\ui_menu_focus.wav`), that
tab then boxed without a fill [hypothesis: the look]; the ITEMS rows sound as before
(`UIPipBoyScroll`, the same file); the Pip-Boy light (`crate::flashlight`)
`UIPipBoyLightOn` / `UIPipBoyLightOff`; while up, a static crackle `UIPipBoyStatic` (the
folder `fx\ui\pipboy\static\`, a file at random) 0.5–2 s after it is up and then every
4–10 s [hypothesis: how often the original plays them is not captured]. Coming up and
going down keep `UIPipBoyAccessUp` / `UIPipBoyAccessDown` (`crate::ui_sounds`). Each is
logged "pipboy sound: <EditorID>" (the focus sound at debug level).

## 7. Checking it

`NVF_PLAY_TEST_PIPBOY=<seconds>:<step>,...` with steps `open`, `close`,
`page=stats|items|data`, `tab=<0-4>`, `key=down|up|use|drop`, and for the pointer
`point=<x>:<y>` (the pointer on that canvas point, through the model's screen),
`point=off`, `click=<x>:<y>` and `click=stats|items|data` (a click on the device's
button; logged "pipboy: the STATS button pressed"); screenshots with
`NVF_PLAY_TEST_SHOTS`. A new game's add-on message boxes come first: answer them with
`NVF_PLAY_TEST_SCRIPT_MENUS=0.05` and open after about 12 s:

```
NVF_PLAY_TEST_SCRIPT_MENUS=0.05 NVF_PLAY_TEST_PIPBOY="12:open,15:page=stats,17:close" NVF_PLAY_TEST_SHOTS="14,16.5" nvforever --data <Data> --cell Goodsprings --play --no-voice --hour 12 --size 1280x720 --run-seconds 19
```

Expect "pipboy: up, page ITEMS tab Weapons", "pipboy: the menus are on the model's
screen", "pipboy: STATS Status", "pipboy: down". Compare at 1280 × 720 with the owner's
pictures.

The mouse (2026-10-11): `NVF_PLAY_TEST_PIPBOY="12:open,14:click=320:650,16:click=stats,18:click=600:650"`
gives "pipboy: ITEMS Apparel" (the second tab), "pipboy: the STATS button pressed",
"pipboy: STATS Status" and "pipboy: STATS Perks"; `point=200:330` on ITEMS boxes the
fourth row, and a click on a row equips what it selects.

## 8. Open

- Whether the original holds the clip at `Hit` (the measurements say the screen faces
  the eye there).
- The keys of the PC game for the pages and tabs.
- The glare: how strongly the original draws it.
- `fDefault1stPersonFOV=55` for the arms outside the Pip-Boy.
- What `fBlurIntensityPipboy`, `fScanlineScalePipboy` and `fBrightenPipboy` scale.
- The mouse pointer: the screenshots show an amber arrow (`interface\icons\misc\
  glow_cursor.dds` is in the archives); NVForever now shows the game's own `cursor.dds`
  tinted amber everywhere in the window (`engine/nvf-play/src/cursor.rs`, 2026-10-16).
