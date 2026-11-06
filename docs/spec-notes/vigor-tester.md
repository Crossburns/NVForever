# Spec note: the Vigor Tester (S.P.E.C.I.A.L. menu)

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own. Implementation: `engine/nvf-play/src/chargen/vigor.rs` (opening,
keys, clicks, drawing), `chargen/vigor/model.rs` (pages, controls, which part shows
what), `chargen/vigor/view.rs` (where the machine sits before the camera),
`engine/nvforever/src/vigor_view.rs` (its camera and lights, the room behind); the rules
(40 points, 1 to 10, Done only at 0 left) are `crates/fnv-game/src/chargen.rs`
`SpecialSheet`.

## 1. Sources

- **Six screenshots from the owner**, 2026-10-11 (1280 × 720, kept privately): the
  review page at 05, 02 and 00 points remaining, and the Strength, Charisma and Agility
  pages at 05 points to go. The owner: every attribute starts at 5 with 5 more to give
  out; an attribute goes no lower than 1 and no higher than 10; every point must be
  given out before going on.
- **Captures of the original**, 2026-11-06 (1280 × 720, kept privately): the Attract and
  Review pages of one save, sampled every 3 frames on opening and during a turn.
- **The menu file** `menus\chargen\love_tester_menu.xml` (`Fallout - Misc.bsa`; read,
  not shipped).
- **The models** in `Fallout - Meshes.bsa`, `meshes\architecture\goodsprings\`:
  `nv_vitomaticvigortester_activate.nif`, `…_cabinet.nif`, `…_cabinet02.nif`; their
  textures in `Fallout - Textures.bsa` / `Textures2.bsa`
  (`textures\architecture\goodsprings\nv_vigor-tester_*`,
  `textures\terminals\english\vitomatic_page_0…4.dds`, `vitomatic_special-values1/2.dds`,
  `textures\terminals\BBNumber0…10.dds`, `BBIncrease/DecreaseOff/On.dds`,
  `BBLT/RTOff/On.dds` and the `pc\`, `xbox\`, `ps3\` variants of the last two).
- **The records**: ACTI `VCG01VigorTester` (0x00104C0A, "Vit-o-matic Vigor Tester")
  has the model `architecture\Goodsprings\NV_VitoMaticVigorTester_Cabinet02.NIF`
  [observed: `fnvtool esm decode`].

## 2. The menu file [documented: love_tester_menu.xml]

- Menu `LoveTesterMenu`, class `&LoveTesterMenu;`, no click past, alpha 0, a menu fade of
  0.25 s.
- Pad bindings: RT → `next_page`, LT → `previous_page`, right → `increase_value`, left →
  `decrease_value` (both repeat while held, `repeathorizontal`), up → `index_up`, down
  → `index_down`, X → `exit_menu` (whose own button is A).
- The named parts are invisible click targets with placeholder positions and no
  picture: `next_page`, `previous_page`, `increase_value`, `decrease_value`,
  `exit_menu`, the two 1 × 1 hot rectangles `index_up`/`index_down`, and a
  1600 × 1200 hot rectangle `clickable` over the whole screen. Everything seen is
  drawn by the menu's class from the 3D model [hypothesis: clicks land on the model's
  buttons through the full-screen `clickable`].
- No key of the PC is named. The keys NVForever gives the menu are [design] (§6).

## 3. The models [observed: the decoded files]

The three files share one frame: +z up, the machine's front towards +y, the screen's
right is −x.

- **`…_cabinet02.nif`**: the placed activator. Cabinet, glass, bulbs, and a fixed
  poster shape with `vitomatic_page_0.dds` and blank label boards.
- **`…_cabinet.nif`**: the cabinet without glass or poster, the ten bulbs
  `P1_PointVal_01…10` (`:2` the socket, `:3` the bulb with the numbers, lit by the
  per-pixel shader with an environment map) and, per bulb, two glow shapes
  `P1_PointVal_<nn>_GLOW:4` (an additive sprite, `textures\effects\fxparttinyglowy.dds`)
  and `:5` (the bulb again, unlit, white emissive × 1.125). No record names this file
  [observed for the vigor tester's ACTI; not searched further]: it is taken to be the
  menu's [hypothesis: it has what the menu needs and the placed one does not].
- **`…_activate.nif`** (8,516 blocks, 229 animated nodes): the moving parts.
  - Label boards, ten per page, at x 5…26: `S_01…S_10`, `P_`, `E_`, `C_`, `I_`, `A_`,
    `L_` (their UVs pick one of the 70 labels in `vitomatic_special-values1/2.dds`) and
    `Att_01…10` (blank, `NV_Vigor-Tester_Backdrop.dds`). Row 1 is at the bottom
    (z 69.3), row 10 at the top (z 114.7).
  - Poster segments at x −26…3: `SPECIAL-Title_<Page>`, `PB-<Page>_01…06`,
    `Desc-<Page>_01…04` for the pages Attract, Strength … Luck, Review (UVs into
    `vitomatic_page_0…4.dds`: page 0 holds "Rate Your…" and "Let's Review…", pages 1–4
    two attributes each).
  - Carriers that turn with the boards: `Value-Flippers_01…10`, `PB-Flippers_01…06`,
    `Desc-Flipper_01…04`, `SPECIAL-Title_Flipper` (the cabinet texture).
  - The review page's rows: `Index_<Stat>Decrease_Btn` (left, `BBDecreaseOff`),
    `Index_<Stat>PointVal` (the value, `BBNumber0`), `Index_<Stat>Increase_Btn` (right,
    `BBIncreaseOff`), Strength at the top (z 108.7) to Luck (z 86.4).
  - `P1_PointsRemainDigit1` (tens) and `Digit2` (ones), `BBNumber0` [observed: Digit2
    lies right of Digit1 on both pages].
  - The lever's two halves `P1_Increase_Btn` (upper, "+") and `P1_Decrease_Btn`
    (lower, "−"); the arrow buttons `P1_LT_Btn` (←), `P1_RT_Btn` (→), `LookInside_Btn`
    (→ of the first page) and `AllDone_Btn` (`BBLTOff`/`BBRTOff`).
  - The stored pose is the first page (Attract): the Attract layer and blank boards in
    front (y −10.87), the other pages' layers behind it at y −16.62 (Strength),
    −17.12, … −19.62 (Luck), −20.12 (Review), the blank boards when not in front at
    −16.12; the cabinet's backdrop quad (`Cabinet:9`, y −14.1) hides what is behind.

## 4. The sixteen sequences [observed: the end poses and keys of each]

Each `NiControllerSequence` keys all 229 nodes, clamps (cycle type 2), lasts 1.267–1.3 s
and carries only the text keys `start` and `end`. Their names are generic (locomotion
names); what each does:

| Sequence | Turns from → to | | Sequence | Turns from → to |
|---|---|---|---|---|
| Forward | Attract → Strength | | AimUp | Strength → Attract |
| Backward | Strength → Perception | | Aim | Perception → Strength |
| Left | Perception → Endurance | | Holster | Endurance → Perception |
| Right | Endurance → Charisma | | JumpLand | Charisma → Endurance |
| FastForward | Charisma → Intelligence | | JumpLoop | Intelligence → Charisma |
| FastBackward | Intelligence → Agility | | JumpStart | Agility → Intelligence |
| FastLeft | Agility → Luck | | TurnRight | Luck → Agility |
| FastRight | Luck → Review | | TurnLeft | Review → Luck |

In each, the outgoing page's boards and poster segments each turn once round
(Euler y −π → π, one after another from the top, about 0.33 s apiece) and drop back to
their layer, the incoming page's turn and come to the front. Besides:

- Forward: the lever's halves turn half round about z to face the player; `P1_RT_Btn`
  rises to z 58.72 and `LookInside_Btn` sinks to z 0 (out of sight below the base);
  `P1_LT_Btn` rises from 56.83 to 58.72; the points digits turn to face the player
  (Rx 11.0 → 14.14). AimUp undoes all of this.
- FastRight: the review rows turn to face the player (Rx −π → 0) and the points digits
  move to the review poster's circle (z 102.88 → 71.98, x −16.5/−19.9 → −2.4/−4.4,
  scale 1 → 0.52). TurnLeft undoes it.
- `AllDone_Btn` stays at z 0 in every sequence: no sequence shows it.

So the pages run Attract, Strength … Luck, Review, and the menu opens on Attract
[observed: the captures of 2026-11-06, §5].

## 5. What the screenshots show [observed: the six screenshots]

- The machine fills the screen; the room shows blurred at both sides.
- Attribute page: title, "ADJUST YOUR SCORE… IT AIN'T CHEATIN'!", picture, the circle
  "05 POINTS TO GO" (the points remaining, not the attribute: all three pages read 05
  with every attribute at 5; it is the digits node, §3), description, "THIS AFFECTS
  YOUR…"; the attribute's ten labels on the boards; the bulbs 1…value lit white with a
  glow, the others dull gold with their numbers; ← and → both black with light arrows.
- Review page: the seven values in circles; ◄ and ► at each; at 00 points remaining
  every ► is gone (6.jpg); the circle at the bottom reads the points remaining; no bulb
  lit; blank boards. The → is grey at 05 and 02 remaining and black at 00.
- The mouse pointer over a row or the → changes nothing seen.
- The captures of 2026-11-06: the machine shaded with warm light, falling off a little
  towards its edges: cream poster, dark wood, brass. Lit bulbs bright white with a soft
  halo that spills on the board, their numbers still legible; dull bulbs warm gold. The
  room nearly black at this spot, a dark corner. On opening the machine is in place in the
  first frames, with no slow fade to speak of at 3-frame samples.
- The captures' first page is Attract ("Rate Your…", "TRY IT NOW!"), with the → and no ←,
  the lever's face turned away, and bulbs 1 to 5 lit on it. NVForever lights none there
  (§6, not changed) [hypothesis: the first attribute's value, this save's not read].
- The camera: fitted to ten points of 7.jpg and 10.jpg (label rows 1 and 10, their
  edges, the poster's edges, the lever's + and −, the two arrow buttons), 7 px RMS: an
  eye at model point (0, 78.4, 98.9) looking along −y, tipped 9.15° down, vertical
  field of view 48.4° [observed: fitted].

## 6. NVForever's menu [design unless tagged]

- Drawn as the two models (`…_cabinet.nif` and `…_activate.nif`) below the player's
  camera, scaled by 0.13 about the eye, with the camera's field of view set to the fitted
  48.4° while the menu is open and given back after.
- The machine, a camera on the player's with its field of view, and that camera's lights
  share a render layer no other view has (`VIGOR_LAYER`): a view takes every light whose
  layers meet its own and there is no list per mesh, so this is what keeps the room's
  lights off the machine and the rig's off the room. The camera draws into an image the
  size of the window, shown as a full-screen image under every other UI node. As with the
  other menus' cameras (lockpicking, terminals), what asks for the one `Camera3d` (level
  of detail, grass, streaming) waits while the menu is open.
- Lit: a warm key above and to the left of the eye, a dimmer fill low to the right, a warm
  ambient light, no shadows; not scaled by `--light-scale`. Each part's colour has a tint
  (`model::shade`): boards and posters as they are, the cabinet darker, dull bulbs warm
  gold. The model says which shapes are unlit. All to be tuned against the captures (§5):
  `vigor_view.rs` `KEY`, `FILL`, `AMBIENT`, their colours and places, `LIGHT_RANGE`;
  `model.rs` `BOARD_TINT`, `CABINET_TINT`, `BULB_DULL_TINT`, `BULB_LIT_TINT`,
  `BULB_HALO_TINT`.
- The room behind: the image-space blur at radius 3 (`ROOM_BLUR`, the wait menu's) and
  black over it at 45 % (`ROOM_DARK`, what the earlier sheet had, matched by eye to the
  six screenshots' margins). The machine is drawn after that, so it stays sharp.
- The fade [documented: 0.25 s, §2; design: how]: the picture and the room's blur and
  darkening come in over 0.25 s from 60 % (`FADE_FROM`, because of §5's first frames) and
  go out to nothing over 0.25 s, the picture holding the machine's last frame. The world's
  field of view comes back at once on closing. The sequences are not touched.
- A page turn plays the sequence of §4 on the model (from its start; a turn asked during
  another starts at once).
- The bulbs 1…value are lit on an attribute's page by showing their two glow shapes (`:5`
  over white so that the numbers stay dark, `:4` the halo); none elsewhere. The view has
  no bloom, so the halo is the sprite alone.
- Numbers: each review row's value and the two digits of the points remaining show the
  `BBNumber<n>.dds` picture.
- Review rows: ◄ hidden at 1, ► hidden at 10 or with no point left (the ◄ at 1 by
  symmetry with 6.jpg [hypothesis]).
- → : `textures\terminals\pc\BBRTOff.dds` when it works, `pc\BBRTOn.dds` when not (on
  the review page until every point is given out); ← `pc\BBLTOff.dds` [hypothesis: the
  PC pictures, which show arrows as the screenshots do; the models name the console
  pictures with "RT"/"LT" on them].
- Keys: Left/Right or A/D lower/raise the attribute of the page (on the review page,
  the chosen row's); Up/Down or W/S choose the review row; Tab or Page Down the next
  page; Shift+Tab or Page Up the previous one; Enter or E Done. On the review page the
  → (and Tab) is Done too. Once Up/Down was used, the chosen row's arrows show their
  grey `On` pictures (the original shows no row with the mouse).
- Clicks: on the parts in front for the page (§3): ←, →, the first page's →, the lever's
  halves, and on the review page each row's ◄, ► and value (which chooses the row). A
  click is matched against each part's outline on the screen (its bounds projected),
  the smallest outline under the pointer winning.
- Done (only with 0 points left) writes the sheet to the player's permanent attributes
  and answers the menu (`fnv_game::chargen::SpecialSheet::apply`, `chargen::finish`).
- Log lines: `vigor tester: open, S.P.E.C.I.A.L. 5 5 5 5 5 5 5, 5 points remaining …`,
  `vigor tester: Strength 6, 4 points remaining (…)`, `vigor tester: page Strength
  (sequence Forward, …)`, `vigor tester: … refused: …`, `vigor tester: done,
  S.P.E.C.I.A.L. …`, `vigor tester: closed`, and at exit `vigor tester summary: …`.
- Test aid `NVF_PLAY_TEST_VIGOR` (module docs of `vigor.rs`); its seconds count from the
  menu's first frame, so a slow start does not run every step at once.

## 7. Check

```
NVF_PLAY_TEST_SCRIPT_MENUS=off NVF_PLAY_TEST_SCRIPT=3:menu:1074 \
NVF_PLAY_TEST_VIGOR="2:click:LookInside_Btn:0,4.5:raise,4.6:raise,4.7:done,5.5:page:review,8:select:luck,8.1:raise,8.2:raise:L,8.3:raise:endurance,8.5:click:P1_RT_Btn:0" \
engine/target/release/nvforever --data <Data> --cell GSDocMitchellHouse --play --no-voice --run-seconds 20
```

Expect exit 0, "vigor tester: open, S.P.E.C.I.A.L. 5 5 5 5 5 5 5, 5 points remaining",
"page Strength (sequence Forward, click)", "Strength 7, 3 points remaining", "Done
refused: points still to give out", the turns to Review ending "(sequence FastRight …)",
"Endurance 6, 0 points remaining", "done, S.P.E.C.I.A.L. 7 5 6 5 5 5 7 (click)" and
"vigor tester summary: opened 1 times, 5 changes, 8 page turns, 2 clicks on its
buttons, done 1 times". `NVF_PLAY_TEST_SHOTS` saves pictures of the pages to compare
with the owner's.

## 8. Open questions

- The PC keys of the original menu (none in its file).
- Does the ← work on the first page, where its button is not seen (§5)?
- Do the bulbs on the first page show the first attribute's value (§5)?
- Is the → on the review page Done, or does `AllDone_Btn` (which no sequence raises)
  replace it once every point is given out?
- What do the `On` pictures of the buttons mean: dimmed (as the → in 10.jpg), pressed,
  or the chosen row?
- Does the ◄ of a row at 1 disappear as the ► does at 10 and at 0 points left?
- Is a row chosen with the keys shown, and how?
- Sounds (list 5 item 39): the menu file names none and the two models carry no sound
  key [observed]. NVForever plays Fallout 3's S.P.E.C.I.A.L. book sounds, which the
  masters keep with two records added for New Vegas [observed: FalloutNV.esm SOUN,
  files in `Fallout - Sound.bsa` under `sound\fx\obj\book\special\`; hypothesis:
  that the Vigor Tester plays them, by their names]: a page turn `OBJBookSpecialPageTurn`
  (two takes), an attribute raised `OBJBookSpecialNumber`, lowered
  `OBJBookSpecialNumberDown`, a review row chosen `OBJBookSpecialFocus`, Done
  `OBJBookSpecialNumberFinished`, a refusal `UIMenuCancel` (`nvf_play::ui_sounds`, menu
  `vigor tester`).
- Not built: any sounds of the lever and the bulbs
  of their own, and the repeat of a held Left/Right (`repeathorizontal`); a turn asked
  during another does not wait for it.
- The original lights the machine and blurs the room [observed: §5]; what light it uses
  is not known, and NVForever's rig (§6) is a stand-in tuned by eye. So are the strength of
  the blur and of the darkening: the captures' spot was too dark to judge them.
- Does the original fade its menu at all, and from what? The file says 0.25 s, the first
  frames of the capture show none to speak of.
