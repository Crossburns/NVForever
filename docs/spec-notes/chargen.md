# Spec note: the character-creation menus

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Rules: `crates/fnv-game/src/chargen.rs`; drawing:
`engine/nvf-play/src/chargen.rs` and `chargen/{draw,lists,name}.rs`; the Vigor Tester's
S.P.E.C.I.A.L. menu has its own note, `vigor-tester.md`; the race menu (1036) is NVForever's own
character creator, `character-creator.md`.

## 1. Sources and when the menus open

- **Eleven screenshots from the owner**, 2026-10-11, 1280 × 720 (kept privately): the
  trait menu twice (Four Eyes chosen, then Early Bird and Four Eyes taken), the tag-skill
  menu twice (Speech and Survival tagged with Lockpick chosen; then Guns tagged too), the
  name menu, and six of the Vigor Tester. The owner's words: up to two traits or none;
  three tag skills, +15 each, all three required; the name menu sets the character's
  name for the save; the Vigor Tester starts every attribute at 5 with 5 more to spend,
  1 to 10 each, all points spent before going on.
- **The menu files** in `Fallout - Misc.bsa` (described here in plain words; nothing of
  them is shipped): `menus\chargen\char_gen_menu.xml` (menu class CharGenMenu, the tag
  skills), `menus\trait_menu.xml` (TraitMenu), `menus\dialog\texteditmenu.xml`
  (TextEditMenu, the name), `menus\chargen\love_tester_menu.xml` (LoveTesterMenu), and
  the shared pieces they include (`list_box.xml`, `list_box_template.xml`,
  `scrollbar_vert.xml`, `text_box.xml`, `top_bracket.xml`, `bottom_bracket.xml`,
  `line.xml`, `vertical_fade_line.xml`, `menus\globals.xml`). `menus\trait_select_menu.xml`
  is a "Medical Questionnaire" on paper textures (`terminals\med_history_*.dds`) that the
  opening does not open [observed: no script opens its code].
- **Which function opens which menu** [observed, layer5-captures.md §10.2–§10.4]: stage 10
  of VCG01 calls `GetPlayerName` (menu 1051, the name), 36 `ShowRaceMenu` (1036), the
  Vigor Tester's OnActivate `ShowLoveTesterMenuParams` (1074), 90 `SetTagSkills` (1048,
  after the psych test has tagged three skills), 102 `ShowTraitMenu` (1084). Each stage's
  timer goes on once the menu closes.
- **Settings the masters hold** [observed: FalloutNV.esm GMST]: `sTraitMenuTitleText`
  "CHOOSE UP TO %d TRAITS (OR NONE)", `iTraitMenuMaxNumTraits` 2, `sDefaultPlayerName`
  "Courier", `fAVDTagSkillBonus` 15. The other words the menu files name (`sReset`,
  `sDone`, `sOk`, `sEnterName`, `sPCMenuHintR`, `sPCMenuHintA`) are not in the masters:
  they are the program's own defaults, which the encrypted program does not let us read,
  so the words drawn are those on the screenshots ("Reset", "Done", "Ok", "Enter character
  name.", "R)", "A)") [observed].

## 2. The data the menus show

- **Traits**: every PERK whose DATA says Trait, Playable and not Hidden [documented: xEdit
  `wbDefinitionsFNV.pas`, PERK DATA: Trait, Min Level, Ranks, Playable, Hidden, a byte
  each]. Each of the sixteen traits stores `01 01 01 01 00` [observed]: ten in
  FalloutNV.esm (Built to Destroy, Fast Shot, Four Eyes, Good Natured, Heavy Handed,
  Kamikaze, Loose Cannon, Small Frame, Trigger Discipline, Wild Wasteland) and six in
  OldWorldBlues.esm (Claustrophobia, Early Bird, Hoarder, Hot Blooded, Logan's Loophole,
  Skilled). The menu lists them by name [observed: the screenshots' alphabetical order,
  the Old World Blues ones among the others]. A trait's requirements are the CTDA before
  its first effect (PRKE); the CTDA and DATA after it belong to the effects [documented:
  xEdit record order]. Its picture is its ICON.
- **The "Req:" line** names each requirement on GetActorValue or GetPermanentActorValue
  by the actor value's short name (AVIF ANAM, "PER") with the comparison and number
  [observed: Four Eyes' "Req: PER < 10"; Early Bird's "Req: --" for none]. Four Eyes holds
  two requirements, Perception > 1 and < 10, and only the second is drawn; the menu here
  leaves out a S.P.E.C.I.A.L. "greater than 1" [hypothesis: why the original leaves it
  out is not known]. Small Frame's line comes out "AGL < 10".
- **Skills**: the tag-skill menu lists every skill but Big Guns, by name [observed: the
  thirteen on the screenshots in alphabetical order; Big Guns' AVIF is named "Big Guns -
  OBSOLETE" in the masters]. A skill's name, description and picture are its AVIF's FULL,
  DESC and ICON. The masters' AVIF FormIDs run in actor value code order, 0x3E8–0x3EE for
  Strength to Luck and 0x4B0–0x4BD for Barter to Unarmed [observed: each record's name is
  its code's; Guns is Editor ID AVSmallGuns and Survival AVThrowing].
- **Skill values**: the player's skill with the tag bonus as the menu has it (skill
  formula, actor-values.md; + `fAVDTagSkillBonus`). With the screenshots' attributes
  (5, 3, 7, 7, 5, 6, 7) and Speech, Survival and Guns tagged, every value matches
  (Barter 20, Energy Weapons 12, Explosives 12, Guns 33, Lockpick 12, Medicine 16, Melee
  Weapons 16, Repair 16, Science 16, Sneak 18, Speech 35, Survival 35, Unarmed 20)
  [observed: `fnvtool actor chargen`, §6].

## 3. The tag-skill menu (1048)

[documented: char_gen_menu.xml unless tagged] Lengths in the interface's units, 960 to
the view's height, from the top left of a 1024 × 800 rectangle centred on the screen.
- A dark picture behind (`interface\shared\background\solid_black.dds`), 1144 × 1070, 50
  left of and 125 above the rectangle.
- The headline: a line 50 long with a line fading down at its left end, the title (font
  6) at 70, 10 above the line, then the line again from 20 after the title to the right
  side, fading down there too. The title reads "SKILLS:  2/3 Selected" [observed].
- The list: 15 left of the rectangle, 450 wide, 14 rows; its top is "set in code", 66
  down [observed: fitted to the highlight box on the screenshot]. A row: the name at 40,
  20 down, wrapping at 390; the value centred at 360; a filled square at 15, 17.5 (25
  square) for a tagged skill; the chosen row in the highlight box (an outline at
  brightness 200, a fill of alpha 40, from the scroll bar's right side to 5 short of the
  list's). Rows are 52 apart [observed: 468 pixels over 12 rows at 720 lines].
- The chosen skill's picture: 400 square at 500, 20.
- The info rectangle at 445, 420, 580 wide: lines 560 long at its top and 300 down; the
  skill's description at 20, 20 wrapping at 540; the counter 20 under the second line
  ("Tag 1 Skill" at 2/3, nothing at 3/3 [observed]; "Tag 2 Skills" and "Tag 3 Skills"
  [hypothesis]).
- Reset and Done right-aligned at 1014, 60 down, Done 52 under Reset; each a text box (its
  text and 10 on every side) with its key after it ("R)", "A)"). Done can be pressed only
  when the points counted equal the most (`_CurrPoints` = `_MaxPoints`): three tagged;
  otherwise it is drawn at 127 of 255 [observed: dimmed at 2/3, bright at 3/3].
- The bottom bracket at 800: a line the rectangle's width with a line fading up at each
  end.
- **Opening state**: the skills the player has tagged (the psych test's three)
  [observed: the psych test tags three before stage 90]. Clicking a tagged skill untags
  it; an untagged one is tagged while fewer than three are [observed: the owner's
  screenshots, 2/3 then 3/3]. Reset goes back to the opening state [hypothesis].

- **The attributes page**: the same file's attribute rows, which AddSPECIALPoints opens
  (Intense Training): levelup.md §8.

## 4. The trait menu (1084)

[documented: trait_menu.xml unless tagged]
- The same dark picture, headline (title at 70: `sTraitMenuTitleText` with the most
  traits) and bottom bracket as §3.
- The list at -15, 60, 470 × 676 (13 rows of 52); a row's name at 45, 20 down, wrapping
  at 360; a filled square for a taken trait. With sixteen traits the scroll bar shows at
  the list's left: arrows at its ends, the marker 105 long with its top 23 under the up
  arrow at the list's top [observed: pixels 139–208 on the screenshot; hypothesis: it
  keeps that length and moves evenly].
- The picture: 400 square at 520, 0.
- The info rectangle 10 right of the list (465), 385 down, to the rectangle's right
  side: a line 3 short of its width at its top and at 335, each with a line fading down
  60 from its right end; the text 20 down in a clipped box 275 high, wrapping short of
  the box's scroll bar, whose arrows stand at its right, 95 down in the box [observed:
  432 and 515 pixels on the screenshot, shown with a short text]; the counter (font 6)
  20 under the second line.
- The text [observed]: "Req: …", "Ranks: 1", an empty line, the description.
- The counter [observed]: "2 TRAITS LEFT", "0 TRAITS LEFT"; "1 TRAIT LEFT" [hypothesis].
- Reset and Done as §3, Done right under Reset (51); Done can always be pressed [observed:
  bright with nothing taken].
- Clicking a taken trait gives it back; an untaken one is taken while fewer than the most
  are [observed]. A trait whose requirements the player does not meet cannot be taken
  and is drawn at half brightness [hypothesis: no screenshot shows one; the template's
  `_TextAlpha` allows a fainter row].

### 4.1 The trait menu drawn from its file (`[UI] TileMenus=TraitMenu`)

Off by default; the hand-built menu of §4 stays whole with the switch off and whenever
this does not hold (`engine/nvf-play/src/tiles/trait_menu.rs`). [observed: the vanilla
files, by `fnvtool ui tree --menu menus\trait_menu.xml` and the shared pieces it includes,
read privately; not seen running unless tagged otherwise.]

- **What the program writes.** The file leaves these open to code: the menu's `_CurrPoints`
  and `_MaxPoints`; `string` of the title (`LUM_Headline_Title`), the description
  (`TM_DescriptionText`) and the counter (`LUM_PointCounter`); `filename` and `visible` of
  the picture (`LUM_SelectionIcon`); the rows, made from the template `LUM_PerkTemplate`
  (row `LUM_Perk`, with `ListItemText` and `LUM_Template_ItemMarker`): `string`, `_selected`,
  `_TextAlpha`, `listindex`, `_y`, `height`; of the list (`LUM_PerkList`) its `visible`, which
  its `_enabled` follows and without which rows take no pointer and the highlight stays
  hidden, `_highlight_y`, `_selected_height`, and of its `lb_scrollbar` the counts and value.
  The buttons are `LUM_ResetButton` and `LUM_ContinueButton`. The tile ids 0 to 6 and 8 mark
  what the program addresses [hypothesis].
- **`_selected` is "taken"**, the mark the row draws; the row chosen is the highlight. Both
  are shown at once, and they differ [design: the same meanings as §4].
- **The file has a scroll bar for the description**: `TM_DescriptionScrollbar` in the box
  `TM_DescriptionBox`, moving `TM_DescriptionText` by its value times its own `_ScrollDelta`.
  No control of NVForever's is added. What the program writes for it is not in the file:
  NVForever gives it as many values as it takes the text's end to come into the box
  [design]; the file's own item count stands until code writes it. It also leaves the page
  step at nothing, so NVForever pages by the box's height less a step [design]. The bar
  stands beside the box, outside its width, so that box's window does not crop it
  (`fnv_ui::input::OUTSIDE_CLIP`) [hypothesis: a window crops what has `clips`; the arrows on
  the screenshot of §4 stand there].
- **A short text**: the arrows stay [observed: §4, the screenshot's short text] and the
  marker does not show [design: none was noted there]. Whether the original writes a count,
  or leaves the file's, is not known [hypothesis]: a capture read, below.
- **The list's scroll keeps the file's `_scroll_delta` as its step** and takes as many
  values as bring the last row's end in [hypothesis: sixteen rows of 52 in a list of 676
  over the file's step of 26 make seven values, and the file's marker at seven values is 92
  long, the 69 pixels at 720 lines measured in §4; the container capture's counts,
  barter-menu.md §8, would give it 161]. The file's own step and page are used
  (`tiles::list::counts_by_step`). Where the marker stood on that screenshot, not at the
  top of its track, is not explained.
- **Backing** [hypothesis]: the file's dark picture at 0.4 alpha (§8's reading of the
  screenshot) with the world blurred behind it; the whole-screen dark sheet is the
  hand-built menu's only.
- **Keys** stay the hand-built menu's (`crate::chargen`), so a press acts once, and R and A
  through the file's `_PCButton_*` act only for a script's MenuTapKey or a controller not
  standing in for them. With tiles, Left and Right (the D-pad's too) scroll the description
  [design: the original's way is not known], and Page Up/Down step by the rows the list
  shows. The wheel over the description or its bar scrolls the text, elsewhere the list;
  scrolling never changes the chosen row, nor does the pointer resting on a row that moved
  under it.
- **Not known** (reads for a capture of the original, none taken yet): at 1280 x 720 with
  sixteen rows, the list's `_scroll_delta`, its bar's `_number_of_items`,
  `_number_of_visible_items`, `_current_value` and the marker's `y` at the top and bottom;
  the description bar's `_number_of_items`, `_current_value`, `_ScrollDelta`, `_jump_size`,
  `_show_always`, `visible` with a short text and a long one, and the text's `y` at the
  end; `LUM_Background`'s `alpha`; `_CurrPoints` and `_MaxPoints` as traits are taken;
  `_TextAlpha` on a trait that cannot be taken, and whether its row's `_enabled` goes off;
  `LUM_ContinueButton`'s and `LUM_ResetButton`'s `target`; `stats_icon_badge`; what Reset
  restores with traits already taken; the world's blur radius behind the menu.
- **Evidence**: the unit tests (`tiles::trait_menu`) use a small layout written for them,
  which says nothing of the file's geometry; the smoke scenarios `tiles-trait-*` read the
  vanilla file.

## 5. The name menu (1051)

[documented: texteditmenu.xml unless tagged]
- A box 720 × 180 centred on the screen; a dark picture 25 out on every side; the top
  bracket (a line with lines fading down 50 at its ends) and the bottom bracket at 180.
- The question centred, 32 down: "Enter character name." [observed]; the name centred
  in the middle of the box's height, wrapping at 250; Ok right-aligned 16 in from the
  right side, its box's bottom 16 above the box's.
- The question and the name glow [observed: the screenshot's letters]; they are drawn in
  font 1 (`glow_monofonto_large`) [hypothesis: the default of a text the file gives no
  font]; Ok in font 2, a text box's own [documented: text_box.xml].
- It offers the player's name, or `sDefaultPlayerName` ("Courier") the first time
  [observed]. Typing adds to the name, Backspace rubs out, Enter or Ok takes it
  [hypothesis: the keys; no caret is drawn, none is on the screenshot]. A name of spaces
  only is not taken; at most 24 characters [hypothesis].

## 6. What Done writes, and the survey

- Name: `PlayerState::name`. Tag skills: the tagged skills replace the player's tags.
  Traits: the player has exactly the taken ones among those on offer (perk rank 1); what
  a trait does is its perk's effects, which nothing runs yet. S.P.E.C.I.A.L.: each
  attribute's permanent value becomes the sheet's (vigor-tester.md).
- `fnvtool actor chargen --schema schemas/fnv <Data> <plugins…> [--special 5,3,7,7,5,6,7]
  [--tag Speech --tag Survival --tag Guns]` (`fnv.actor.chargen/1`) lists the name
  offered, the S.P.E.C.I.A.L. sheet, the tag-skill list with values and the traits with
  their requirement lines, after applying the given sheet as Done would.

## 7. Checks

- Unit tests: the sheets' rules (`fnv_game::chargen`), the list model, the texts, the
  name box, the test aid (`nvf-play` `chargen`).
- Open a menu without the opening: `NVF_PLAY_TEST_SCRIPT=<s>:menu:<code>`; play it with
  `NVF_PLAY_TEST_CHARGEN=<s>:pick:<row>,<s>:type:<text>,<s>:clear,<s>:reset,<s>:done`;
  set `NVF_PLAY_TEST_SCRIPT_MENUS=off` so the unattended runs' auto-close leaves it open.
  The smoke test in CLAUDE.md runs the tag-skill and trait menus and the name menu.
- Screenshots of ours beside the owner's (kept privately): the rows, picture, text,
  lines, buttons, scroll bar and counter of the trait and tag-skill menus and the name
  box stand within a few pixels of the original's.

## 8. Not built, or guessed

- The world behind is not blurred (the original blurs it); the dark picture is 0.4 alpha
  [observed: 33 against 54 brightness inside and just outside it].
- Sounds (list 5 items 41, 42; `nvf_play::ui_sounds`, menus `tag skills` and `traits`):
  the chosen row moving and the pointer onto Reset or Done `UIMenuFocus`, a row taken or
  given back `UIMenuPrevNext`, a row refused `UIMenuCancel`, Reset `UIMenuCancel`, Done
  `UIMenuOK`. The files silence the rows (CGM_SelectItem, LUM_SkillItem `&nosound;`)
  [observed]; the owner wants them, so the rows take the template's pointer sound and
  the trait file's perk-row click (LUM_Perk `UIMenuPrevNext`) [design]; Done's click
  and the text boxes' pointer sound are the files' [observed]; Reset's is [design].
- The keys besides the hints, Reset's meaning, the counters' other wordings, a trait the
  player cannot take, the name's length limit: [hypothesis], listed in
  docs/owner-review.md.

## 9. The character revision (VCG04)

- **The original** [observed: FalloutNV.esm quest VCG04 "Player Character Revision",
  script VCG04ActivatorScript 001164A4, `fnvtool script decompile`]: activating one of
  the quest's activators at Goodsprings' edges stores the player's XP and shows message
  VCG04Message. "Revise" calls `player.SetLevel 1`, `ResetXP`, `RemovePerk` on the ten
  traits of FalloutNV.esm and eighteen perks of the early levels (Intense Training,
  Swift Learner, Educated, …) and `ShowRaceMenu`; the next frames set each
  S.P.E.C.I.A.L. to 5 with `SetSPECIALPoints 40` (the Vigor Tester), each skill to 0
  with `SetActorValue` and open `SetTagSkills 3`, then `ShowTraitMenu`, then the
  message again. "Done" gives the stored XP back with `RewardXP`, so the player levels
  up again and spends the points and perks anew.
- **Here** the revision keeps the player's progress [design: the owner's wish, playtest
  list 7 item 1, "retain our currently selected perks and our skill levels … minus our 3
  tag skills that are reapplied"] (`fnv_game::revision`): from that script only,
  SetLevel, ResetXP and the closing RewardXP on the player do nothing, RemovePerk takes
  traits only (the trait menu offers them again), and SetActorValue of a skill on the
  player does nothing. A player's skill is the skill formula over the current
  S.P.E.C.I.A.L. + the tag bonus of the current tags + the points spent at level-ups
  (`fnv_game::actor_values`), so the spent points stay and the new S.P.E.C.I.A.L. and
  tags re-derive the rest. SetActorValue of an attribute on the player does nothing
  too, and the script's `SetSPECIALPoints 40` opens the character menu's attributes
  page (menu 1048, the one Intense Training's AddSPECIALPoints opens, levelup.md §8)
  with the points the player's attributes add up to (40 or more), so the player moves
  points around on the S.P.E.C.I.A.L. they have [design: owner, 2026-10-26, the rebuild
  still showed 5 in each; owner's playtest, #1031: the attributes page, not the Vigor
  Tester]. The Vigor Tester stays for Doc Mitchell's first visit
  (`ShowLoveTesterMenuParams`). The tag-skill menu starts with the old three tagged (the
  SetTagSkills rule of §1). Each kept call is logged `revision: keeps …`.
- The script is known by its record (SCPT 001164A4) whichever plugin edited it last:
  OldWorldBlues.esm overrides it with the same calls [observed: `fnvtool script
  decompile` of the base game and add-ons; the owner's game ran
  `OldWorldBlues.esm|SCPT 0x001164A4`, 2026-10-26], and a check on the
  FalloutNV.esm copy alone kept nothing with the add-ons loaded.
- Before this the revision zeroed every skill: SetActorValue overrides a value's base
  (`functions::actor`), and a player's skill was then 0 whatever its formula.
