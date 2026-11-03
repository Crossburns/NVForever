# The tutorial menu (ShowTutorialMenu)

Plain-language notes for `nvf_play::tutorial_menu` and `fnv_game::Game::show_tutorial`
(2026-10-18, claude/menu-hooks). Sources: the GECK wiki export (owner's, C:\fnv-private\geckwiki),
the game's `menus\tutorial_menu.xml` (owner's extract, C:\fnv-private\menus-extract) and
the masters' scripts decompiled with `fnvtool script decompile`. No game data is kept here.

## 1. What scripts do with it

- `ShowTutorialMenu <message>` "shows the specified Message Box form, similar to
  ShowMessage, but with slightly different visuals. These are the 'VDSG Manual' styled
  message boxes" [documented: GECK wiki ShowTutorialMenu].
- The masters call it twice [observed]: CGTutorial (QUST 0x00059C85) stage 212, entries 0
  and 1, show `HelpHealingLimbs` (MESG 0x00072358) and `HelpHealingLimbsXBox`
  (0x00072359); the entries' conditions pick the platform. Stage 212 is set from the
  quest script's `Begin MenuMode 1` block when a limb of the player is crippled, so the
  menu opens while the Pip-Boy is open.
- The messages are message boxes (DNAM Message Box) with a title (FULL "Healing Limbs")
  and a text (DESC) marked up with `<div>` at the start and `<p>` between paragraphs
  [observed].

## 2. The menu file [documented: menus\tutorial_menu.xml]

Lengths in the interface's units, from the background's top left.

- A black background (`solid_black.dds`) 0.8 of the screen wide and high, centred, at
  the menus' fill alpha (`_background_fill_alpha`, 32 of 255).
- Top: at x 48, y 96 a line fading down (vertical_fade_line.xml, 60 long) and a line 50
  long to the right; the title (font 6, "replaced in code") 10 after that line, 5 lower
  than centred on it; a line on to the page text "??/??" (font 6, right-justified, 10
  before the right stub; "replaced in code"); when the page text is hidden a line fills
  its place; a stub 50 long ending 48 before the right side, and a line fading down there.
- Text: a clipping column from the title's x, under the top fade lines (y 156), to the
  lower fade lines, ending 64 before the Previous button; the text 5 in and 21 down (the
  file's "font adjustment"), wrapping 5 short of the column, moved up by the scroll bar's
  value. A vertical scroll bar beside the column at the left corner line; a step of 30.
- Buttons (text_box.xml, right-justified, right side 81 before the background's right):
  Next (`sNext`) at the column's top, Previous (`sPrevious`) under it, Close
  (`sCloseButton`) standing on the lower right fade line's top. PC key hints
  `sPCMenuHintA`, `_S`, `_E` after them; the PC keys `_PCButton_A` Next, `_S` Previous,
  `_E` Close.
- Bottom: the corners' lines fading up from 96 above the bottom, a stub 50 long, the
  VDSG symbol (`Interface\Loading\glow_loading_VDSGSymbol.dds`, 64 high, its width less
  36 of dead space), the words `sVDSGManual`, a line on to the right corner.

The strings are game settings that are not in the masters (the executable's defaults).

## 3. What NVForever does

- `Game::show_tutorial`: the message is a page of the tutorial menu, which opens over the
  menu that is open and remembers it (`Game::menu_under`); closing the tutorial makes that
  menu the open one again; a message already shown is not added twice; further messages
  while it is open are more pages [hypothesis]. A message box shown meanwhile waits.
  If the engine closes the menu underneath (the Pip-Boy put away), it is forgotten.
- MenuMode 1059 while it is open, and not 1001: it is no message box [observed: capture mh1_tutorial, save nvfstrip, 2026-10-18].
  `MenuMode 1` stays true over the Pip-Boy [hypothesis]. A second ShowTutorialMenu while it
  is open leaves one 1059 menu, and one CloseAllMenus closes it together with the menu that
  was open under it (MenuMode 0 after it) [observed: the same capture]; NVForever's
  `Game::close_all_menus` does the same.
- Drawn as §2 says, with these choices: the words (`sNext`, `sPrevious`, `sCloseButton`,
  `sPCMenuHintA/S/E`, `sVDSGManual`) are read by name from NVForever's game strings table,
  the load order's string settings over its own labels (menu-xml.md §2.4; the
  maintainer's decision 2026-10-30) [design]; the background as dark as the other menus
  (0.75) instead of the file's alpha 32 [design: readable over the Pip-Boy]; the page
  count only with more than one page; a thin scroll mark instead of the scroll-bar
  prefab; the text in font 1 [hypothesis: a text tile without a font]; `<p>` and `<br>`
  start paragraphs and other tags go.
- Keys: A or Right next, S or Left previous, E, Esc or Enter close, Up, Down and the wheel
  scroll; clicks on the buttons. The Pip-Boy's own keys are held while the menu is over
  it.

## 4. Open

- Settled by capture mh1_tutorial (`tools/capture/probes/menu_hooks.toml`): the code 1059.
  Still open: the words, and whether a second message is a second page or replaces the
  first (the capture shows only that it stays one menu, 1059); which menu was open under
  it in the capture (MenuMode was 1 before the first call, its code not asked), so whether
  the menu under the tutorial answers MenuMode is not settled.

## 5. Drawn from the menu file (T15b part 2)

Behind `[UI] TileMenus=TutorialMenu` (`engine/nvf-play/src/tiles/tutorial_menu.rs`) the open
menu (1059) is drawn from `menus\tutorial_menu.xml` and the hand-built drawing gives way to a
sheet taking the pointer at the same stacking level (109); its pages, keys, wheel and close
stay the hand-built menu's (§3) [design]. Smoke: `tiles-tutorial`, `tutorial-default-drawing`.

The original's values, read at three window sizes [observed: capture b2_tutorial, private;
values in interface units; the message 0x0007235B]:

| tile | 1280 x 720 | 1280 x 1024 |
|---|---|---|
| `TM_Background` | 170.67, 96, 1365.33 x 768, alpha 193.8 | 120, 96, 960 x 768 |
| `TM_title` | 108, 83.33, 145 x 38, font 6 (y 82.89 at 1920 x 1080) | y 82.94 |
| `TM_page` | x 1257.33, 75 x 38, font 6, the file's `??/??`, not on the shot | x 852 |
| `TM_text_container` | 108, 172, 985.33 x 440, `_text_height` 625, `_text_y` 21 | width 580, `_text_height` 811 |
| `TM_text` | 5, 21, 965 x 625, no font trait (font 1), wrap 980.33 | 558 x 811, wrap 575 |
| `TM_scrollbar` | 33.33, 172, 32 x 424, `_number_of_items` 655 | items 841 |

- **The file has no `io()` traits** [documented: tutorial_menu.xml; the review's plan expected
  some]. It gives ids to `TM_text` 0, Next 1, Previous 2, Close 3, `TM_title` 4, `TM_page` 5,
  `TM_scrollbar` 6 and the words after the symbol 7, and says "replaced in code" of the title
  and the count; the body has no string at all. So the model writes the tiles' own traits:
  `string`, and `visible` and `target` on the count and the buttons [design; what the
  original's code writes is not known].
- **Sizes**: the body's height is the measured one, font 1's 36 + (lines - 1) x 31 [observed:
  625 = 36 + 19 x 31, 811 = 36 + 25 x 31]. The file carries it on: `_text_height`, and the
  scrollbar's items are that plus its step of 30 [documented; observed: 655, 841]; its visible
  items the column's height, its jump the height less a step [documented]. Every text is
  measured by the renderer's fonts, worked out and measured again until the body stands at its
  final wrap width: that width follows Previous's place, 985.33 = 1284.33 - 127 - 108 - 64
  [observed], and that place its word's width.
- **The scroll's range** is the file's, items less visible items and at least 0: 215 here, and
  the last step is what is left of the range, not a whole step [documented: scrollbar_vert.xml,
  `_current_value`]. The hand-built `max_scroll` adds the text's 21 and rounds up to steps; it
  is not used [design].
- **The scroll's value** is the hand-built menu's. The prefab's `_current_value` adds the
  arrows' and page areas' clicks to its old value [documented]; in NVForever's tree an
  expression starts from 0 (wait-menu.md §12), so the model writes the value and pokes
  `_SetInCode` on every frame the menu is tiled, as that prefab's comment asks [documented], and
  clicks on the pieces are the hand-built menu's commands: arrows 30, page areas 410 at this
  size, the marker the value under the pointer with the prefab's `_step_height` [documented;
  design: the marker's own drag traits are not built]. The wheel and the keys stay hand-built;
  the file's `wheelmoved` stays 0.
- **Clipping and scrolling are apart.** The file moves the text: `TM_text`'s `y` is
  `_text_y`, 21 less the value [documented]. `clipwindow` on `TM_text_container` crops it
  (menu-xml.md §3.1.1) [hypothesis: a rectangular crop, nothing captured says more]; the model
  turns the reading on for this menu alone, every frame after the scripts and before the
  drawing, so a window injected or enabled later crops at once. `_NVFClipWindow` and
  `_NVFOwnAlpha` are engine-written marks, ordinary traits to the tree.
- **Alpha**: all of the menu's content is inside its background picture, whose alpha is 193.8
  in the original, and its title, body, lines and symbol read full strength, (255, 182, 66)
  [observed: shot 091 of hud_menus, 1280 x 720]. NVForever's layout multiplies alpha down from
  a picture, so the model marks `TM_depth_rect` to take none from above
  (`tiles::layout::OWN_ALPHA`); whether pictures pass alpha on elsewhere is open (observer
  check 81). The backing keeps the hand-built menu's 0.75 (191.25 of 255) [design], 1 % from
  the original's.
- **The symbol**: `width` is `filewidth` less 36 and the words after it follow, so the file
  alone reads -36 until the code writes `filewidth` [documented]. The picture is 128 x 64
  [documented: the texture's header, Fallout - Textures2.bsa]; it is read from the name the file gives, once.
- **The buttons**: with one page neither Next nor Previous shows [observed: the same shot], and
  the count is hidden with `TM_fill_line` in its place by the file's rule [documented; observed:
  not on the shot]. Previous still keeps its width, so the column keeps its place [observed:
  985.33]. With more pages a button with nowhere to go stops being a target: the file's
  `_line_alpha` dims it and it takes no highlight [documented: text_box.xml; design: the
  hand-built menu's rule; multi-page is not captured, observer check 82]. `n/N` is NVForever's
  (§3). Close is read (206, 147, 53) at its brightest on that shot, against the body's
  (255, 182, 66) [observed]; font 2's texels or an alpha, not settled.
- **One press acts once.** A click a key makes through the file's `_PCButton_*` or `x…` refs
  acts unless the hand-built menu took that key this frame (`tiles::menus::key_click_acts`).
  The triggers needed care: RT and LT stand for this menu's A and S
  (`crate::pad::menu_buttons`), not for the shared Page Up and Page Down, so the tile input asks
  `pad::menu_key_at` for them, or RT would turn the page by hand and again by the file's
  `xbuttonrt`. RB and LB, which the hand-built menu does not read, page the text through the
  file. A script's MenuTapKey and ClickMenuButton act through the file.
- **The order in a frame**: the tile input's commands reach the hand-built `keys` the same
  frame; then the page, its sizes, its range and the scroll are written together, before the
  scripts, so a key or a click never draws a new page at an old scroll. A window resize
  settles in its own frame too (the tree is worked out before the text is measured). The
  writer runs before `run_scripts`, so a page a script adds there reaches the tree the next
  frame: the count and the buttons are one frame behind the game's pages; the page shown
  and its scroll do not change with it [design].
- Off, or in the fallback, a script's queued click on the file's tiles does nothing: the
  commands take the drawing's own decision. A marker drag ends with its press or its menu.
- A file a mod restructured (a model path, a scrollbar piece or a button's words missing)
  keeps the whole hand-built drawing, with
  one warning. The frame a menu opens on its tree is not built yet, so the hand-built drawing
  shows for that one frame [design: as the other T15 menus].
