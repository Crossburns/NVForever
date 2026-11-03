# Spec note: the barter and container menus

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/menu/look.rs` (the
drawing), `menu/model.rs` (lists, trade, commands), `menu/barter.rs` (prices),
`menu.rs` (opening, keys, pointer), `crates/fnv-game` `ShowBarterMenu`.

## 1. Sources

- **Four pictures from the owner**, 2026-10-11 (kept privately): two screenshots of the
  original's barter menu with the merchant Blake (1280 x 720 and 1600 x 1024, the second
  with a Gold Bar put up for sale), a small screenshot of the container menu on a
  refrigerator (384 x 216), and a cut from a web page showing a container menu with an
  item's figures (DPS, WG, VAL, STR, CND, ammunition, mods). The owner: the container
  menu looks "very similar" to the barter menu, without the caps.
- **The game's interface files** in `Fallout - Misc.bsa`: `menus\barter_menu.xml`,
  `menus\container_menu.xml` and the shared pieces they include
  (`menus\prefabs\list_box.xml`, `list_box_template.xml`, `scrollbar_vert.xml`,
  `item_stats_display.xml`, `card_info.xml`, `text_box.xml`, `bottom_bracket.xml`,
  `line.xml`, `vertical_fade_line.xml`, `box.xml`, `menus\globals.xml`). Described here
  in plain words; nothing of them is shipped.
- **The textures they name**, in `Fallout - Textures2.bsa`: the title arrows
  `textures\interface\main\main_arrow_left.dds` / `_right.dds`, the caps arrows
  `textures\interface\barter\barter_arrow_buy.dds` / `_sell.dds`, the item arrows
  `textures\interface\shared\arrow\left.dds` / `right.dds`, and from the shared atlas
  (`textures\interface\interfaceshared.tai`): `square.dds`, `square_filled.dds`,
  `arrow_up.dds`, `arrow_down.dds`, `vert_marker.dds`, `solid.dds`, `fade_to_top.dds`,
  `fade_to_bottom.dds`. Items' pictures are their ICON paths
  (`interface\icons\pipboyimages\items\...`).
- **The GECK wiki** (owner's export): "Barter System Formula", "Item Condition And
  Value", ShowBarterMenu, "Merchant Restocking", GetBarterGoldAlt, Service Tab.
- **FalloutNV.esm**: the barter settings, Chet's records.

## 2. What the files say

[documented: the two menu files and their pieces] In the interface's units (960 to the
screen's height):

- Both menus are a 1024 x 800 rectangle centred on the screen. Behind it a black
  picture (`interface\shared\background\solid_black.dds`) 40 left of it and 105 above,
  1144 x 970, with the shared background opacity (32 of 255).
- Two halves of 464 x 480: the player's 20 in and 20 down, the other 560 further right.
  Each has a heading: a short line fading downwards at each end, a line along the top,
  the title (font 6) 50 in with an arrow 17.5 x 35 before it at 30 and after it, the
  line resuming 20 after the right arrow, and a figure in font 2 right-aligned at 449,
  20 down. The barter menu's title stands 10 above the line, the container menu's 12.5.
  The player's title is the inventory's word for items; the other half's title is set
  by the code (the container's or merchant's name). The arrows are clickable and so are
  the titles (the mouse wheel on them).
- The barter menu has a figure on both halves (set by the code; the screenshots show
  the caps). The container menu has one on the player's half only (the screenshots show
  the weight carried and the most that can be).
- Each half's list is 15 left of the half and 60 down, 462 (player) or 464 wide and 365
  high, clipped. A scroll bar 32 wide sits at its left, with 16-high arrows at its ends
  and a marker 6 wide, 12 in, at least 32 high; it shows only when the list is longer
  than the window. The list scrolls by 26, half a row. Only one list is enabled at a
  time, the one with the focus; moving the pointer over a half gives it the focus.
- A row's height is its text's and 20. In a row, 32 right of the list's left (after the
  scroll bar's room): the name in font 2 at 40, 20 down, wrapped at 300 in the barter
  menu and 370 in the container menu; in the barter menu a value right-aligned at 403;
  at 15, 15 a 25 x 25 square, filled for an equipped item, open for an item chosen for
  the trade. The row under the pointer gets a box one unit shorter than the row, its
  lines at brightness 200 and its fill at 40 of 255 at half brightness. Rows play
  `UIMenuOK` when clicked and `UIMenuFocus` when the pointer comes on them.
- Each half's footer: lines fading upwards from 440 to 480 at its two ends and a line
  along 480.
- A bottom bracket under both halves 820 down the screen: a line with lines fading up
  40 at its ends.
- Barter only: between the halves, set visible by the code, the caps arrow: a picture
  120 x 360 (`barter_arrow_buy.dds` or `barter_arrow_sell.dds`, chosen by the code's
  direction) and the amount in font 6 centred over the word "Caps" in font 2. The
  arrow's rectangle is 449 right of the player's half and 260 down the screen; with
  the second direction everything moves 20 right, the picture 25 left within it, and
  the figure 7.5 right; a figure wider than 70 moves by its excess.
- Container only: between the halves, set visible by the code, an item arrow 64 x 128
  (`left.dds` or `right.dds`), 474 right of the player's half and 250 down the screen.
- Buttons at the bottom right (text boxes, right-aligned, the right end 10 left of the
  other half's right end, 670 down the screen, each its text and 15 high): Accept and
  Exit in the barter menu, Take All and Exit in the container menu, each followed by its
  PC key hint. Accept is drawn at 128 of 255 when it cannot be used.
- The item's picture (256 x 256) 22 right of the player's half and 472 down from it,
  and the item's figures 535 down from the halves: 287 into the menu (barter) or 307 into
  the player's half (container). The figures are tiles a third of 530 x 120 (176.7 x 40):
  damage, damage per second, resistance or threshold at the left; weight at the
  second third and 10; value at the third; strength needed above the value; condition
  as a bar (62 in, 15 down, 20 high, 72 short of the tile) and the ammunition at the
  second row; effects and mods across. A tile has a line along its top, a line fading
  down its right side, its title at 5 and its value right-aligned 10 before its end,
  both 20 down.
- Container only: a text centred under the menu, 800 down the screen.
- Fonts by number: font 2 is `Monofonto_Large`, font 6 `Glow_Monofonto_VL_dialogs`
  (`fonts.md` §4). Lines are 2 thick on a PC.

## 3. What the screenshots add

[observed, the 1280 x 720 barter screenshot unless said otherwise]

- Positions agree with §2 to a pixel or two: the names start 57 right of the half, the
  values end 420 right of it, the weight and value tiles stand at the second and third
  thirds 555 down, the Accept text's right end at 1024 of the menu.
- Rows are 51 apart (39 pixels at 720 lines, 54 at 1024), seven to a list.
- A row reads "Name (count)" when there is more than one; caps are not listed in the
  barter menu but shown as "Caps  N" in each heading; the container's player heading
  reads "Wg  carried/most". The container lists "Bottle Cap (18)" as an item.
- Rows are sorted by name in byte order: "$5 NCR" and ".223 Round" before digits and
  letters, "LAER+" before "Lad's Life" (capitals before small letters).
- The values are prices, not the items' values: at Barter 100 the Gold Bar (value
  10547) shows 9492, Hydra (55) 50, Jet (20) 18, the .308 Armor Piercing round (9) 10,
  the .308 Round (4) 4, the $5 NCR (2) 2. Items worth nothing show "--"; Embalming Fluid
  shows "0.9". The VAL tile shows the item's own value (10547).
- An item chosen for sale leaves the player's list and stands first in the merchant's
  list with the open square (second screenshot: Gold Bar, value 9492); the arrow points
  at the player with "9492 Caps".
- The key hints read "Accept A)", "Exit X)" (barter) and "Take All R)", "Exit E)"
  (container); the hint stands about 16 after its button's text. Accept is dim with
  nothing chosen and bright with a trade.
- The picture behind darkens the world by about a third at its left edge, with a soft
  edge about 25 wide; the world behind is blurred (second screenshot).
- The container screenshot's note reads "This container respawns and is unsafe to
  store your items."

## 4. Barter rules

- **Prices** [documented: GECK wiki "Barter System Formula"]: buying costs the value
  times `fBarterBuyBase + Barter × fBarterBuyMult / 100`, selling fetches the value times
  `fBarterSellBase + Barter × fBarterSellMult / 100`; a bought item never costs less
  than its value and a sold one never fetches more. FalloutNV.esm sets the four settings
  to the page's defaults: 1.55, -0.45, 0.45, 0.45 [observed: `fnvtool esm decode`].
  Barter is the player's skill; the merchant's does not count [documented].
- **Rounding** [observed: §3, all the screenshot's prices]: to the nearest cap, halves
  up; under one cap one decimal is kept [hypothesis: from the single "0.9" row].
- **Condition** [documented: "Item Condition And Value"]: an item's value is multiplied
  by `fItemConditionValueBase + (1 − base) × fItemConditionValueMult × (condition % /
  10) ^ fItemConditionValueExp`; FalloutNV.esm sets the base to 0 and the multiplier to
  0.0316 and has no exponent, so the page's default 1.5 is used [observed].
- **Discount** [documented: GECK wiki ShowBarterMenu]: `ShowBarterMenu 50` makes the
  merchant sell at half and buy at one and a half of the normal prices; the parameter is
  -100 to 100. It is applied after the limits [hypothesis].
- **The merchant's goods and caps** are its merchant container's (the XMRC of its
  reference) [observed: Chet's `VendorContainerChet` holds his stock and two vendor caps
  lists of 300; his own inventory holds a few items and pocket change]. The merchant's
  own inventory is not offered [hypothesis]. Without a merchant container, the actor's
  own inventory is used [design].
- **What the merchant sells** is limited to its service categories (AIDT "Buys/Sells",
  or its class's): the other items of its stock are held back; it buys anything
  [documented: GECK wiki Class; the categories: vendor-services.md §2].
- **The total** is the sum of each line's rounded price times its count [hypothesis].
  Accept is refused when the side that pays has too few caps [hypothesis; the wording is
  ours: "You don't have enough caps." / "<merchant> doesn't have enough caps."].
- **Quest items** cannot be sold [documented: GECK wiki Category:Items]; they are not
  listed [hypothesis].
- **Restocking** on two days of the week [documented: "Merchant Restocking"] is the
  simulation's (`fnv-sim` merchant restock days), not the menu's.
- **The menu's code** for `MenuMode` is 1053 [hypothesis: the GECK wiki's list as
  remembered; no shipped script tests it].

## 5. What NVForever does

- Draws both menus by §2 and §3 with the game's two fonts and pictures, read from the
  owner's archives at run time; without them the same layout with Bevy's font and plain
  lines.
- The player's half is at the left (as the files have it; the earlier plain container
  menu had it at the right).
- Category arrows switch each list between all items, weapons, apparel, aid, misc and
  ammunition [observed: titles "ITEMS" and "MISC"; the rest hypotheses]. The other
  half's title is its name for all items, else the category's name [hypothesis].
- Names wrap onto a second line as the files say; a two-line row is 35 taller.
- The pointer selects the row it moves onto and gives its half the focus, and never
  scrolls the list: a row half shown at the list's foot stays where it is when the pointer
  takes it; only the wheel and the keys scroll [design: the owner's rule, playtest list 7
  item 11; `MenuModel::pointer_pick`]. A click acts
  on the row (barter: chooses one unit for the trade, or takes it back; container: takes
  or stores one), Shift and a click the whole stack [design: the original asks how many
  with its quantity menu, not built]. The keys are listed in `menu.rs`'s notes: the
  hints' letters (A, X, R, E) as the screenshots show them, Esc and Tab to leave,
  `[` and `]` for the categories [design].
- The item's picture and its figures: the Pip-Boy ITEMS page's (pipboy.md §4–§5,
  `nvf_play::pipboy::items::info_of` and `tiles_in`, the same `item_stats_display.xml`)
  laid out in this menu's 530 × 120: DAM (weapons) or DT (armour), DPS over it, STR over
  the value, CND as a bar for weapons and armour, the ammunition or armour class beside
  it, the effects across [design: the owner's wish, playtest list 7 item 25]; WG and VAL
  stay one unit's, WG with two decimals [observed], VAL at the unit's condition before
  the trade's multiplier. Log `menu: figures of <item>: <tiles>`. The mods tile is not
  built.
- Currency (NCR notes, Legion coins) are the caravan money records (CMNY) that
  inventories hold; their worth is their DATA "Absolute Value" (`Item::money`), the Value
  of the MISC records of the same names [observed: FalloutNV.esm; playtest list 7 item
  7].
- The caps arrow and its figure are drawn at half brightness, as a disabled button is,
  while the side that pays holds fewer caps than the balance (Accept is then refused)
  [design: the owner's request].
- The container menu's note shows for a container whose record has the Respawns flag
  [hypothesis: every such container]; a refused trade's words show in the same place
  [design].
- The world behind is darkened by a third [observed] with the background picture's soft
  edges; its blur is not built (the message box note has the same gap).
- `ShowBarterMenu` (fnv-game) opens the menu for scripts (MenuMode 1053) and asks the
  engine to draw it. From a dialogue choice the menu opens over the conversation, which
  waits under it and goes on when it closes [hypothesis: the original returns to the
  conversation].

Not built: the quantity menu, the BarterExit lines merchants say when the menu closes
(GECK wiki Service Tab), the blur, the mods tile, the
"Calculate Buy Price" perk entry point, repair at merchants, companions' inventories
(OpenTeammateContainer).

## 6. Looking at one

- `NVF_PLAY_TEST_BARTER=<merchant reference>[:discount]` opens the barter menu with that
  merchant and makes one trade, logging the lists, prices and caps; with
  `NVF_PLAY_TEST_SHOT=<path.png>` it saves `<stem>-barter.png` and
  `<stem>-barter-trade.png`. With `*` it waits for a script to open one.
  `NVF_PLAY_TEST_BARTER_DELAY` gives it a delay of its own (else `NVF_PLAY_TEST_DELAY`),
  so that it can run beside the container check. Its sounds: audio.md §12.
- The dialogue path, Chet in the Goodsprings General Store:
  `NVF_PLAY_TEST_TALK="10:GSChetRef=for sale|leave" NVF_PLAY_TEST_BARTER="*"
  NVF_PLAY_TEST_SCRIPT_MENUS=0.3 nvforever --data <Data> --cell GSGeneralStore --play
  --no-voice --size 1280x720 --run-seconds 50` (expect "menu: a script opens the barter
  menu with 0x00104C79", "menu: barter with 0x00104C79: stock in 0x00104C7A", a trade,
  "menu: close", then the conversation's Goodbye).
- `NVF_PLAY_TEST_MENU=nearest` does the same for the container menu (menu.rs).

Checked this way on 2026-10-11 at 1280 x 720: Chet's menu matches the owner's barter
screenshot's layout (headings, lists, prices' column, tiles, icon, buttons, bracket)
within a few pixels; the refrigerator of GSHouseInterior03 matches the container
screenshot.

## 7. How many? (moving part of a stack)

Sources: `menus\quantity_menu.xml` and its prefabs `meter.xml`, `top_bracket.xml`,
`bottom_bracket.xml` [documented: the owner's extraction of the game's menu files, read in
plain language here]; three screenshots of the original over the Pip-Boy, a stack of six
Varmint Rifles being dropped, at 1920 × 1080 (C:\fnv-private\itemstacks) [observed].

- **When it asks** [owner, 2026-10-15]: moving a stack of more than 5 units (take, store,
  buy, sell, take back from the trade, drop from the inventory). The original's threshold
  is a game setting with no record in FalloutNV.esm; it was not read, so "more than 5"
  is the owner's word. Five or fewer move one unit per press, as before.
- **Layout** (interface units, 960 to the screen's height): a 740 × 370 box centred on the
  screen, on the shared black background picture at the menus' backing; inside it, 22 in
  and 42 down, a frame 690 wide: a top bracket and a bottom bracket 290 lower, each a line
  with 50-long ends fading towards the other. "How many?" (game string `sHowMany`, not in
  the masters; the screenshots spell it) in font 2, centred at 345, 50 down. The meter at
  149.5, 115, 391 × 24, with the left and right arrows (`main_arrow_left/right.dds`,
  17.5 × 35) at 130 and 549. The amount in font 6 centred at 345, 170 down. Ok and
  Cancel right-aligned at 665, 155 and 200 down, with their key hints "A)" and "E)".
- **The meter**: a ground of the interface colour at a quarter of its opacity, and over it
  `hud_tick_mark.dds` repeated as far as the value (0 to 1) reaches. The value shown is
  the amount over the stack; the amount is the value times the stack, rounded. A drag
  sets the value from the pointer; the arrows step it.
- **The screenshots agree** [observed]: the frame is 775 × 327 pixels (690 × 291 units at
  1080 lines), its ends fade over about 50 units, ticks are 9 pixels (8 units) apart,
  and the first screenshot shows the whole stack (6 of 6), the later ones 4 and 3.

What NVForever does (`engine/nvf-play/src/menu/model.rs` `Quantity`, `look.rs`
`spawn_quantity`): the prompt opens at the whole stack, goes from 1 to the stack, Ok does
what asked with that many, Cancel (or closing) does nothing and leaves the menu open. Keys:
Left/Right and the wheel ±1, Page Up/Down ±10, Home/End 1 and the stack, A/Enter/Space
Ok, E/Esc Cancel [documented: A) and E); the rest ours]. While it is open the menu and the
Pip-Boy take no other key. Shift (whole stack) and Shift + right click on the
Pip-Boy's ITEMS page (drop the stack) still move the
stack without asking [design]. Over the Pip-Boy the prompt is drawn centred on the window
[hypothesis: the original draws it on the Pip-Boy's screen; the screenshots show it
centred over it]. Smoke scenario `item-stacks`; `NVF_PLAY_TEST_DROP=<s>:<item>:ask`
leaves it open for a screenshot.

**Drawn from its file (T15a).** With `[UI] TileMenus=QuantityMenu` the prompt of the
container, barter and inventory menus is drawn from the game's `QuantityMenu` tiles
(`engine/nvf-play/src/tiles/menus.rs`), the code above not spawned once the tree has the
file (the frame the prompt opens draws it by hand once; a missing file keeps it hand-built);
the crafting menu's prompt stays drawn by it. The tree builds the file while the prompt
shows, whatever the switch (`Game::ui_prompt`, as the open menus build; MenuMode does not
report it): with the switch off its tiles are readable by scripts but nothing writes or
draws them. The code writes the meter's `user0`
(the stack) and `user2` (the amount) and the step sizes `user3`/`user4` (1 and 10, ours)
[documented: the file's comments on them]; the file works out the meter's value, its
fill (the tick picture tiled over the value's part of the meter: one picture, no piece
per tick) and the amount's text. The words are the game strings `sHowMany`, `sOK`,
`sCancel` and the key hints `sPCMenuHintA`/`E` (menu-xml.md §2.4; NVForever's own labels
when the load order has none). A pointer click on OK, Cancel or an arrow does what the
keys do; a press on the meter, held as it drags, sets the amount under the pointer; the
keys stay the ones above (the file's bindings act for a script's MenuTapKey and for the
controller's buttons that stand in for no key). Smoke `tiles-quantity`; the drop aid's
`ask` now holds the prompt open until it is answered.

