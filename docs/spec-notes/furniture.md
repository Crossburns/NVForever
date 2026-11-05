# Spec notes: furniture actors use (chairs, stools, benches, beds)

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525), **[hypothesis]**
needs verification, **[design]** is our choice where the game's rule is not known.
Counts come from `fnvtool actor furniture` (fnv.actor.furniture/1, §6) over FalloutNV.esm,
models and clips read from the default archive order.

Code: `fnv_actor::furniture` (markers, marker flags, the clip an IDLE names, frames,
the phases of a use). Related: animation.md §1–2 (IDLE records and their tree),
actor-behaviour.md §1 (packages that send actors to furniture), ai-observer.md §7
(GetSitting readings in the original game).

## 1. The FURN record

- **Subrecords** [documented: xEdit wbDefinitionsFNV.pas FURN; schemas/fnv/objects.json]:
  the usual object set (EDID, OBND, FULL, MODL, SCRI, destruction data) and `MNAM`
  "Marker Flags", a byte array.
- 234 bases, 7,054 placed references; 190 bases have a display name (the 44 without
  place 695 references); 16 have a script [observed].
- **MNAM is four bytes on every base** [observed], read here as one little-endian number:
  - **Bit 30** is set on every base whose model markers have IDs 10–20 (seats, §2) and
    on 7 of the 30 bases with marker 22; **bit 31** on every base whose markers have IDs
    1–6 (beds) and on one base with marker 22; 31 bases have neither (22 with marker 22,
    2 wall-lean markers, the 7 without markers) [observed]. Reading: bit 30 "sit",
    bit 31 "sleep" [hypothesis; the names are ours].
  - **The low bits enable markers by their index in the model** [observed: `Chair01L`,
    `Chair01R` and `Chair01F` use one model whose markers are left, right and front in
    that order and carry bits 0, 1 and 2; the same for `SubChairL/R/F`; table chairs
    (`Chair01TableL/R`) have only a side enabled]. Of the 227 bases with markers, the bits
    enable every marker on 94 and only some on 133; 22 bases also set bits past the last
    marker (three-seat benches with 0x1F) [observed], which enable nothing.
  - No base with markers has all of them disabled [observed].

## 2. Markers on the model (`BSFurnitureMarker`)

- **Layout** [documented: nif.xml `BSFurnitureMarker` and `FurniturePosition` for
  Fallout 3 / New Vegas files; schemas/nif/scene.json]: an extra-data block named `FRN`
  with a count and that many positions, each an offset (three floats), an orientation
  (u16) and two position references (u8 each).
- 227 of 234 bases have a readable model with markers; one model is missing
  (`furniture\dancemarker.nif`) and six console and intercom models have none
  [observed]. Models carry 1 to 6 positions (62, 72, 73, 12, 5, 3 bases) [observed].
- **The two position references are equal on every position** [observed, 516 positions].
  The value is what `GetFurnitureMarkerID` returns [hypothesis, supported by §3: the
  IDLE tree tests exactly the values found here]. The catalogue's note says 1–10 are
  beds and 11–20 chairs [documented: GECK wiki GetFurnitureMarkerID, catalogue notes].
  Values found, with the names the IDLE tree's editor IDs suggest [observed values;
  names hypothesis]:

  | ID | Positions | Placed refs of bases having it | Reading |
  |---|---|---|---|
  | 1, 2 | 50, 46 | 615, 818 | bed, left and right side |
  | 3, 4 | 5, 4 | 502, 494 | floor bed (bedroll), left and right |
  | 5, 6 | 5, 3 | 155, 96 | sleeping on the floor, left and right |
  | 10 | 1 | 29 | the crucifix (NVCrucifixFurniture): sat on, see below |
  | 11, 12 | 93, 93 | 2,547, 2,550 | chair, entered from its left and right |
  | 13 | 0 | 0 | chair from behind (idles exist, no model uses it) |
  | 14 | 153 | 3,067 | chair or bench, from the front |
  | 15 | 21 | 692 | stool |
  | 16 | 1 | 0 | pod chair |
  | 17 | 3 | 304 | sitting on the floor |
  | 18 | 1 | 10 | restrained sitting |
  | 19 | 3 | 352 | leaning on a wall |
  | 20 | 4 | 323 | child at a bar |
  | 22 | 30 | 526 | standing work spots (workbench, stove, bar, terminal…) |

- **Offset**: in the model's own space; for seats the height is the floor under the
  model (−39 to +2 across chair models) [observed], so a position is where the actor
  stands before using the furniture [hypothesis, supported by §4].
- **Orientation**: 0, 1570, 3141 and 4712 on 469 of 516 positions, the rest other values
  up to 6283 [observed]: thousandths of a radian [hypothesis]. On chairs, ID 11 has 1570
  (88 of 93), ID 12 has 4712 (88 of 93) and ID 14 has 3141 (123 of 153) [observed]. Read
  as a heading clockwise from the model's +Y, the actor at each position faces the
  furniture [hypothesis, supported by §4: the clip then carries it onto the seat].

## 3. Which clip: the furniture part of the IDLE tree

- One root IDLE, `FurnitureIdles`, has the conditions `GetSitting > 0 OR GetSleeping > 0`;
  394 IDLEs lie under it [observed]. Its two children are `Sitting` (`GetSitting > 0`)
  and `Sleeping` (no conditions).
- Below them the tree first separates furniture by `GetFurnitureMarkerID` (chairs
  `>= 11` and `<= 14`, stool `== 15`, pod 16, floor 17, restrained 18, wall 19, child bar
  20, work spots 22, beds `== 1 OR == 2`, floor beds 3/4) or by `IsCurrentFurnitureObj`
  for special furniture, then by the value of `GetSitting` / `GetSleeping` [observed]:

  | Value | What the IDLEs under it name |
  |---|---|
  | 1 | the loop played on the furniture (`DynamicIdle_ChairSit.kf`; `1stP_ChairDynamicIdle.kf` under `GetIsID Player` and `IsPC1stPerson`) |
  | 2 | the clip that gets the actor on (`Chair_LeftEnter.kf` under `GetFurnitureMarkerID == 11`, `Chair_RightEnter.kf` 12, `Chair_BackEnter.kf` 13, `Chair_ForwardEnter.kf` 14) |
  | 3 | things to do while on it: eating, talking, fidgets, each behind more conditions |
  | 4 | the clip that gets the actor off (`Chair_LeftExit.kf` …), with a "fast" set under `GetIsAlerted == 1 OR GetAttacked == 1` |

  The observer saw `GetSitting` 3 on seated actors and 4 on actors getting up
  [observed: ai-observer.md §7]. That 0 is "not using furniture" follows from the root's
  conditions. What 1 means while it lasts is not known; the tree only shows that the loop
  is looked up under it [hypothesis: the state in which the game loads the loop].
- **Order among siblings**: following the previous links (animation.md §2), special cases
  come before general ones throughout: children, the fast exits, women (`GetIsSex Female`
  → the `ChairSkirt_*` clips), a quest-specific exit, then the plain clips [observed].
  The picker therefore takes the first sibling whose conditions hold, descends, and goes
  on to the next sibling when a passing IDLE leads to no clip (the fast-exit group has no
  clip for marker 13) [hypothesis; the owner's rule for stored lists is "first eligible
  from the top"].
- The two child groups carry `GetRandomPercent < 0`, which never holds [observed].
- Conditions use these functions: GetSitting, GetSleeping, GetFurnitureMarkerID,
  IsCurrentFurnitureObj, GetIsID, IsPC1stPerson, GetIsSex, IsChild, GetIsAlerted,
  GetAttacked, GetRandomPercent, and (for what to do while seated) GetCurrentAIProcedure,
  IsTalking, GetIsUsedItem, GetIsUsedItemType, GetEquipped, MenuMode, GetStage and others
  [observed]. The picker knows the first eleven and treats an IDLE with any other
  function as not chosen [design], which leaves out only the value-3 extras; those are
  chosen by the game's rules instead (§8).
- `IsCurrentFurnitureObj` (and `IsCurrentFurnitureRef`) may name a form list: the
  dance group `NVDance` (00117BAD, under `Sitting` before `ChildBar`) has one condition,
  `IsCurrentFurnitureObj` of FLST `DanceMarkerObjects` (00117E55: DanceMarker03,
  DanceMarker01, DanceMarker02, DanceMarker). Dance markers use the child bar's model
  (`markers\childbarmarker.nif`, marker ID 20), so when the list is not looked into
  `ChildBar` (`GetFurnitureMarkerID == 20`) is chosen and the Strip's courtesans outside
  Gomorrah (refs 0012299D, 0012299F, packages VStreetHooker01Dance/02Dance travelling to
  DanceMarker03 refs 0012299C, 0012299E) lean on a counter [observed: `fnvtool esm
  decode`, `actor furniture`, 2026-10-20; the owner saw them dance in the original].
  Any member matching counts, as `GetIsVoiceType` takes a list [hypothesis: the GECK
  wiki types the parameter as a furniture only; the shipped data relies on a list].
- ID 22 has no enter, loop or exit clip in the tree [observed]: work spots play their
  special idles while standing. ID 10 has none by its ID either, but the one model that
  carries it, the crucifix, has its own group under `Sitting`: `NVCrucifixIdles`
  (IsCurrentFurnitureObj of FLST NVCrucifixMarkerObjects, which lists
  NVCrucifixFurniture) with NVCrucifixEnter (GetSitting 2), NVCrucifixDynamicIdle
  (GetSitting 1), NVCrucifixExit (GetSitting 4) and the fidgets at GetSitting 3
  [observed: the IDLE records, 2026-10-23]. So marker 10 is *sat* on, though the
  catalogue counts 1–10 as beds; the base sets MNAM bit 30 (sit), not 31. NVForever
  counts beds as 1–9 [hypothesis: the crucifix's tree is the only use of 10; owner's
  playtest list 7 item 34, the crucified did not get onto their crosses while 10 was a
  bed].

## 4. How the clips move the body

Measured on the first sequence of each clip: the translation of the `Bip01` track and the
turn about Z of the `Bip01 NonAccum` track, at the sequence's start and stop [observed].
Every one of these sequences names `Bip01` as its accumulation root (88 clips over the
marker IDs in use); the getting-on and getting-off clips move it and never turn it, the
loops have no track for it [observed]. An accumulation root's motion is the actor's own
movement (actors.md §7.2), so these clips move the actor.

| Clip | `Bip01` start → stop (x, y) | `Bip01 NonAccum` z, turn start → stop |
|---|---|---|
| `chair_leftenter` (1.13 s) | (0, 0) → (0.1, 50.7) | 66.7, 1.44 → 36.9, −3.13 |
| `chair_rightenter` (1.13 s) | (0, 0) → (0.1, 50.7) | turn 1.44 → 0.02 (height not read) |
| `chair_forwardenter` (1.73 s) | (0, −1.1) → (0, 54.3) | 67.4, 1.44 → 36.8, −1.55 |
| `dynamicidle_chairsit` (13.3 s loop) | no track | 36.8, 1.59 throughout |
| `chair_leftexit` (3.33 s) | (0.1, 50.9) → (−0.5, −0.8) | 37.5, −3.12 → 66.9, −1.70 |
| `chair_forwardexit` (1.53 s) | (0, 54.3) → (0, −1.1) | 36.8, −1.55 → 67.4, −1.70 |
| `chair_forwardexitfast` (0.47 s) | (0, 54.3) → (−2.5, 28.1) | 36.8, −1.55 → 66.5, −1.70 |
| `stoolenter` (4.63 s) | (20.5, −41.0) → (1.5, 6.2) | 62.5, 1.43 → 54.5, 1.58 |
| `dynamicidle_sitstool` (9 s loop) | no track | 54.5, 1.58 |
| `stoolexit` (4.33 s) | (1.5, 6.2) → (25.9, −40.9) | 54.5, 1.58 → 65.8, −1.72 |
| `bedleft_enter` (3.0 s) | (0, 0) → (−10.5, 68.4) | 66.7, 1.44 → 49.6, 1.27 |
| `dynamicidle_sleep` (4 s loop) | no track | 49.6, −0.30 |

What this shows:

- **The enter clip carries the body from the entry point onto the furniture.**
  `chair01.nif` has its left marker 51.9 units to the left of the model's origin facing
  it, and the left enter clip moves `Bip01` 50.7 units forward; its front marker is 54.6
  units in front, and the front enter clip moves 55.4 forward [observed]. The stool's
  marker offset is (20.5, −41.0) and `stoolenter` starts `Bip01` at exactly (20.5, −41.0):
  that clip is authored around the stool's origin, the chair clips around the entry point
  [observed].
- **The enter clip ends turned the way the loop needs.** The loop holds the standing turn
  (1.59; a standing actor has 1.44–1.59). The left enter stops 4.72 (= −1.57) from it,
  the right +1.57, the front 3.14: a quarter turn left, a quarter right, and about-face,
  each ending with the body facing the model's +Y [observed].
- **The exit clip starts where the enter clip stops** (same `Bip01` position and turn)
  and ends at the entry point turned 3.14 from standing: the actor ends with its back to
  the furniture [observed].
- Sound keys name the sounds (`Sound: NPCHumanSitChairDown`, `…SitChairUp`,
  `…SitChairUpFast`, `NPCHumanBedEnter`), and `Blend:15` / `Blend: 9` appear on most
  [observed; the blend unit is not known, animation.md §9.3].

**Model built on it** [hypothesis for the game, design here; `UseFrames`]:

1. The actor stands at the entry point, facing along the marker's heading (§2).
2. Getting on plays in a frame placed so that the clip's first `Bip01` position is the
   entry point (for chair clips, the entry point itself); the actor's root follows the
   `Bip01` track from there, keeping the entry heading.
3. When it ends the root is where the clip left `Bip01`, and it turns by the difference
   between the clip's last body turn and the loop's; the loop plays there.
4. Getting off plays in the frame of step 2 again, the root following the clip's `Bip01`
   track; when it ends the root turns by the difference from the standing turn.

Checked on `chair01` from the left and the front, unturned and turned: the seat frame
lands within 2.5 units of the model's origin facing its +Y, and the actor ends within 1.5
units of the entry point with its back to the chair (`fnv_actor::furniture` tests). Seen
in the engine: §7.

## 5. Open questions

1. **Which marker an actor takes** when several are enabled, and whether it must be
   reachable on the navmesh (level-design.md §6 says the approach side needs navmesh).
   Here: the nearest enabled one [design].
2. **Whether the player walks to the marker** or is put there. Here: put there [design].
3. **Occupied furniture**: one user per marker, per seat or per reference? Benches have
   one marker per seat; chairs three markers for one seat. Here: one user per reference
   unless every marker has ID 14 or 15 (benches, tables of stools), where it is one per
   marker [design].
4. **GetSitting 1**, and whether the loop is chosen once or looked up again.
5. **The blend keys** and how the game crosses from clip to loop.
6. **Camera**: what the first-person view does while getting on. Here the view is the
   third-person body's eye, the body not drawn [design]. On a bed the view lies down
   with the body (§7) [engineering choice].
7. **MNAM bits 30 and 31** against the editor's own labels.
8. **Marker ID 10** (one model) and the six furniture bases without markers. Whether
   creatures use furniture at all: §9.
9. **How sitting ends for the player** and what the prompt says (the crosshair shows
   "Activate" here). Here: a movement control, activating the furniture again, or
   Activate with nothing under the crosshair [design].
10. **Beds and the player.** In the original the player sleeps through a menu and does
    not lie down [hypothesis from how the game is played]. Here the player lies down on
    a bed as a sleeping actor does [owner request 2026-10-10; not the original's]: the
    same markers, clips and frames as an actor on that bed, by the rules of a seat (§7).
    Once lying, the Sleep menu opens (wait-menu.md §8).

A capture can answer 1, 3 and 4: an NPC sent to `Chair01F`, `Chair01L` and a bench from
different sides with the observer logging GetSitting and GetFurnitureMarkerID per frame.

## 6. CLI contract

`fnvtool actor furniture --schema <schemas/fnv> [--nif-schema <dir>] [--summary-only]
<Data> <plugins…>` prints `fnv.actor.furniture/1`:

- `summary`: counts of bases, placed references, models without markers, bases per marker
  count, positions per reference pair, per orientation, per ID and height, bases per
  marker-flag value, how the low flag bits compare with the markers, positions per type
  bits and ID, placed references per marker ID, the size of the furniture IDLE subtree.
- `clips`: per marker ID in use, the clip the IDLE tree names for a grown man who is not
  the player (`loop`, `get_on`, `get_off`, `get_off_alerted`), for a woman (`get_on_female`)
  and for the player in first person (`loop_first_person`): path, IDLE editor ID, times,
  cycle type, text keys and `body` (the `Bip01` translation and `Bip01 NonAccum` turn at
  start and stop); null where the tree names none.
- `furniture` (unless `--summary-only`): per base its FormID, editor ID, name, model,
  marker flags, sit and sleep bits, enabled marker indices, script flag, placed count,
  marker IDs and markers.

## 7. In the engine

Code: `engine/nvforever/src/furniture.rs` (actors and the player), `nvf_play::furniture`
(the activation message, the hold on the player, telling the script runtime),
`locomotion::drive` (an actor's furniture clip replaces its other clips),
`player_body.rs` (`pose_furniture`).

- **Actors.** A `SitOn { furniture, stay }` component on an actor's root starts a use:
  the nearest enabled, free marker is the entry point; the actor takes the last steps to
  it as a person does: it turns on the spot to face the way, walks there forwards at
  walking speed, and turns on the spot to the entry heading (under 16 units it only
  shuffles there without turning round; no path; refused beyond 400 units) [design];
  then the clips and frames of §3–4. Before, it slid there in one straight move while
  turning, and the walk clip played forwards whichever way it went: Easy Pete, walked
  to his chair's origin by `--npc-sit`, went backwards out of the chair to the front
  marker before sitting (owner's playtest 2026-10-16: "moonwalks"). `GetUp`, or `stay` seconds on the furniture, starts getting off. When
  it is off, the components are removed and a traveller's position and heading are
  updated. One user per reference, except where every marker has ID 14 or 15 (one per
  marker) [design, §5 item 3].
- **Leaving the cell and coming back** [design; owner's playtest 2026-10-11]. An actor
  whose visual is taken away while it is on furniture (or walking up to it) gives the
  furniture up at once (log: `is off … unseen (no longer drawn)`), so the furniture is
  free again for anyone, itself included; an actor's own earlier use never counts as
  "in use" for it. Drawn again with the same furniture still its package's, a resident is
  put straight back on it in the loop, without the walk and the getting-on clip
  (`SitOn::at_once`, log: `is back on`). Before this, the stool stayed "in use" by its own
  sitter, who then stood in it (`cannot use …: in use`), and Easy Pete coming back out of
  the saloon to the porch chair he had sat on before stood beside it [observed in
  NVForever: `--cell GSProspectorSaloonInterior --play --hour 12` with the door
  0x0010618E activated at 14 s and ProspectorSaloonDoor at 24 s; `--cell Goodsprings
  --play --hour 11.9` with Pete on 0010634A, the saloon entered, two hours waited inside
  and the saloon left].
- **`--npc-sit ACTOR=FURNITURE[:SECONDS]`** sends a travelling NPC (docs/design/navmesh.md
  §5) to a furniture reference (hex FormID) and gives it a `SitOn` on arrival. The trip
  ends at the enabled entry point nearest the actor's placed position (trip target
  `x,y,z@FURNITURE`), not at the furniture's origin, so it no longer walks into the seat
  first [design].
- **The player** uses furniture by activating it. The third-person body plays the clips
  at the frames of §4; movement, jumping and the character's collision are off
  (`PlayerSeat`), the weapon is put away [design], and a movement control or activating
  the furniture again gets up [design, §5 item 9]. Afterwards the character stands at the
  entry point. In first person the view is the body's eye and the body is not drawn
  (with `--first-person-body` it is, and the view is held to a neck's reach:
  docs/design/player-body.md §1.5).
- **The player on a bed** [owner request 2026-10-10; engineering choices, the original's
  player never lies down]. Activating a bed is a use like any other: the entry point is
  the nearest enabled marker (IDs 1–6), the clips are the ones the IDLE tree names under
  `GetSleeping` (on `NVbedtwin01` from marker ID 1: `SpecialIdle_BedLeft_Enter`, 3.0 s,
  then the loop `DynamicIdle_Sleep`, then a getting-off clip of 3.1 s [observed in
  NVForever]), and the rules are a seat's: movement, jumping and collision off, weapon
  away, up again on a movement control or Activate, standing at the entry point
  afterwards. Scripts see `GetSleeping` 2, 3 and 4 for the player and `GetSitting` 0.
  - **The loop lies on its side** [observed in NVForever on that bed: the pelvis faces
    along the mattress, one side down, knees drawn up, a hand by the face].
  - **First-person view** (`player_body::lying_view`), the same with the body drawn or
    not: the view keeps its place in the body's own frame (its pelvis's turn since the
    bind pose) while the body lies down and gets up, so the picture leans over with the
    body and, lying, shows the room as a head on a pillow sees it, on its side [owner
    request 2026-10-10: "sideways the camera when laying"]. The mouse turns and tilts the
    head in that frame, at most 80° to either side and 75° towards the feet (the seated
    limits of player-body.md §1.5), and, the further the body has lain back, the less
    below level, down to 10°: lower is the pillow and the mattress. With
    `--first-person-body` the eye also goes round the neck with the view, about the
    body's length. The player's heading and pitch are the level view nearest to the
    leaning one (what the crosshair and scripts use).
  - **Third-person view**: the camera's pivot is lifted up to 40 units above the head as
    the body lies back; left by the head, the camera's way round the body ran into the
    bed and the picture was the inside of the mattress [observed in NVForever].
  - The crosshair lying down is rarely on the bed, so Activate with nothing under it
    gets up (§5 item 9).
- **Scripts.** While a use lasts, `GetSitting` / `GetSleeping` give 2, 3 and 4 for the
  phases and `GetFurnitureMarkerID`, `IsCurrentFurnitureRef` and `IsCurrentFurnitureObj`
  answer for the user. State 1 is never reported (§5 item 4).
- **Clips change without a cross-fade**: the clips of one use play in different frames
  (§4), so blending two of them would swing the body [design].
- **Not done**: work spots (marker 22: no getting-on clip), the fast exits (nothing
  sets alerted or attacked), children, checking the entry point against the navmesh, a bed's owner for lying
  down (only sleeping in it is refused, wait-menu.md §9) or a sleeper already on
  it (the bed is then "in use"), and actors choosing furniture by themselves (AI
  packages).

Seen on the owner's install, 2026-10-10 [observed in NVForever, not in the original
game]:

- `--cell Goodsprings --play --npc-sit EasyPeteRef=0010634A:14`: Easy Pete walks to his
  chair on the saloon porch (`HotelChair01F`, front marker, ID 14), turns and sits facing
  away from the wall, and after 14 s stands up in front of it with his back to it. The
  player, started next to the neighbouring chair (0x00106349), sits on activating it and
  stands on pressing Forward.
- `--cell GSProspectorSaloonInterior --play --npc-sit SunnyRef=00106194:30 --npc-sit
  00104F08=00107C9E:30`: Sunny Smiles sits on a bar stool (ID 15, `SpecialIdle_StoolEnter`),
  a settler slides into a booth from its left (`DinerBooth02TableL`, ID 11,
  `SpecialIdle_Chair_LeftEnter`), and the player sits on the next stool.
- `--cell GSHouseInterior03 --play` started next to Easy Pete's bed (0x00176ACE,
  `NVbedtwin01`), unattended, in third person and in first person with and without
  `--first-person-body`: the player sits on the edge, lies down on its side, and gets up
  to stand beside the bed on Activate; lying, the first-person picture is on its side
  with the pillow along one edge, shows the ceiling, the walls and the foot of the bed
  when turned there, the body's own shoulder and arm at the limit towards the feet, and
  never the inside of the body or the mattress. Before the view was adapted the picture
  lying down was the mattress or the inside of the body, in both modes.

## 8. Fidgets on furniture (value 3 of §3)

The owner asked (2026-10-16), after the drawn-weapon fidgets (idles.md §11), whether
actors on furniture fidget too. The tree has these under `GetSitting == 3` and
`GetSleeping == 3`; they are now played.

### 8.1 What the data holds [observed: `fnvtool actor anims`, FalloutNV.esm]

`FurnitureIdles` is the fifth of the people's roots in chain order, after
`KnockedDownIdles`, `HitReactionIdles` (wants `GetSitting == 0` and `GetSleeping == 0`),
`GreetIdles` and `MenuModeIdles`, and before `DialogueIdles`, `EatIdles` and
`GeneralIdles`: a walk from the roots reaches it first for an actor on furniture.
Below it, the nodes for being on the furniture (siblings of the getting-on, loop and
getting-off nodes of §3), in chain order; `GetRandomPercent` in percent, loops as
minimum–maximum, replay delays 0 unless given:

| Furniture (node) | Gate | What follows |
|---|---|---|
| crucifix (`NVCrucifixIdles`, one base) | each leaf `GetSitting == 3` | `NVCrucifixScream` (≤ 1), `NVCrucifixGiveUp` (≤ 3), `NVCrucifixIdleFidget` (≤ 5); dialogue idles under procedure 4 |
| chairs, IDs 11–14 (`SittingChairIdles`) | `GetSitting == 3`, `GetIsUsedItemType 0x15 != 1` | the player (`PlayerSittingChairIdles`: `IsPC1stPerson != 1`, `GetIsID` player): a woman `ChairSkirtSitA` / `ChairFemaleSittingA` (Loop Forever), a man at a table chair `SitChairLeanBackA` (3–8), else `PlayerSitChairRelaxIdleA` (< 45), `B` (< 65), `C`. NPCs (`GetSitting != 4`, not the player): eating under procedure 10, dialogue under procedure 4, a terminal chair, then `SitChairRelax` (not a child, not procedure 10, not in a menu): a woman `ChairFemaleSkirtSittingA` / `ChairFemaleSittingA` (5–10, no random test), a table chair `SitChairLeanBackA` (10), else `SitChairRelaxIdleA` (< 45), `B` (< 65), `C` |
| slot-machine stools (two bases) | `GetSitting == 3` | dialogue, `NVSittingStoolNoLeanthinnker` (≤ 5), slot machine (≤ 4, ≤ 5) |
| stools, ID 15 (`SittingIdlesStool`) | `GetSitting == 3` | dialogue (procedure 4), a blackjack stool (≤ 5), eating and drinking (procedure 10); **nothing else**: a stool has no idle fidget |
| dance marker (`NVDance`, one base) | each leaf `GetSitting == 3` | five men's and six women's dance fidgets, each < 2 (< 4 one) |
| floor, ID 17 (`FloorSitIdles`) | `GetSitting == 3` | eating and drinking only (procedure 10); dialogue (procedure 4) has no gate |
| wall lean, ID 19 (`WallLeanIdles`) | `GetSitting == 3`, random < 30, package type != 6 | `WallLeanSmoking` (weapon away, a faction, < 50, replay delay 80), `WallLeanLegDownA` (< 50), `WallLeanLegUpA` (< 80); dialogue idles under procedure 4 with the gate |
| beds, IDs 1–2 (`BedSpecialIdles`) | `GetSleeping == 3`, a man, not a child | `NVBedSleepOnBack` (one bed base, < 45), `BedStirInSleepA` (< 30, 3–5), `B` (< 60, 3–5), `C` (3–6); women only on one bed base (`NVBedSleepOnBackSexy`) |
| floor beds, IDs 3–4 (`FloorBedSpecialIdles`) | `GetSleeping == 3`, random < 20 | `FloorBedStirInSleepA` (< 50), `B` (men), replay delay 30, 4–6 loops |
| work spots, ID 22 (`NonSitFurniture`) | none | barkeeping, consoles, workbenches, stoves… by `IsCurrentFurnitureObj`: work spots are not usable here (§3), so these are not reached |

- The sleeping branch has no player nodes: the player in bed gets the men's stirring
  [observed: the tree; whether the original plays it is moot, its player does not lie
  down, §5 item 10].
- Clips checked: `SitChairRelaxA` (16.1 s), `ChairFemaleSittingA` and
  `BedStirInSleepA` (13.4 s) have 58 tracks, all bound to the people's skeleton, and
  no track for `Bip01` (the loops have 59) [observed: `fnvtool actor anims`]: laid over
  the loop they move the limbs, not the body's place.

### 8.2 Rules as built (`Game::fidget` with `FidgetKind::OnFurniture`)

- **The walk** is the drawn-weapon fidgets' (idles.md §11.2): roots of the actor's
  folder in chain order, the first passing node gone into, the first passing leaf with
  a file the pick, a node whose children all fail left for its next sibling
  [hypothesis]. The pick is kept only when a node on its way, the leaf included,
  requires `GetSitting == 3` or `GetSleeping == 3` (not joined by OR) [design: the
  rest of the tree is not built]. Dialogue and eating leaves under such a node are
  kept: the conditions choose them (a seated speaker under procedure 4, an eater under
  procedure 10) [hypothesis: they are the tree's state-3 idles like the others].
  Since 2026-10-16 an actor in a conversation gets no furniture fidget: its talking
  gestures (idles.md §12) walk the same branch for it.
- **The player** walks the same way from the roots (the tree has the player's own
  chair nodes), only in third person [design: the tree's chair nodes want it, and on
  a bed a stir in first person would roll the lying view, §7].
- **Conditions the rules cannot answer** [observed: the function catalogue]:
  `IsLastIdlePlayed` (index 451, 86 uses on IDLEs) has no opcode in the script
  catalogue (animation.md §1 item 6); since 2026-10-16 it is answered from the last
  idle the actor was reported playing (idles.md §12.2), so the food-talk nodes
  (`ChairFoodTalk`, `StoolFoodTalk`) and `WallLeanLegUpTalkA` / `…ListenA` follow the
  idle played before (headless, with nothing played, it still answers 0). `GetIsUsedItem`,
  `GetIsUsedItemType`, `GetUsedItemActivate` and `GetSandman` are stubs answering 0:
  eating takes the generic leaf (`SitChairEatB`, `SitStoolEatGeneric`), `!= 1` tests
  pass, the terminal-activation and Sandman nodes are never entered.
  `GetFurnitureMarkerID`, `IsCurrentFurnitureObj` and `GetSitting` / `GetSleeping` are
  answered from the actor's use (§7).
- Checked headless (`fnvtool actor fidgets --furniture REF:MARKER_ID`, 1,000 checks, a
  game where nothing has happened) [observed]: Easy Pete on his chair (0010634A, ID 14)
  `SitChairRelaxIdleA` 432, `B` 390, `C` 178 (every check gives one); Sunny Smiles there
  `ChairFemaleSittingA` 1,000; the player there the `PlayerSitChairRelaxIdle` three
  (464, 360, 176), in first person none; Easy Pete and the player on Pete's bed
  (00176ACE, ID 1) `BedStirInSleepA` / `B` / `C` about 30/40/30%, Sunny none; Easy Pete
  on a saloon stool (00106194, ID 15) none. Smoke test `furniture-fidgets-tree`.

### 8.3 In the engine (`engine/nvforever/src/idles.rs`)

- An NPC on furniture in state 3, not in an idle collection and not playing another
  idle, is checked every 2 s, as a standing one with its weapon out (idles.md §11.2);
  the player on furniture in third person too. The pick is asked as
  `IdleSource::FurnitureFidget`. With the chair's unconditional last leaf a seated man
  fidgets back to back [hypothesis: the original's seated actors keep moving; how often
  it looks at its tree is not known].
- The fidget plays over the furniture's loop on the idle layers (an NPC:
  `locomotion::drive`; the player: `player_body::pose_furniture`, which now lays the
  third-person idles over the furniture clip). Getting on and off is untouched: a
  fidget is only asked in state 3, and when the actor starts getting off it fades out
  over the start of the getting-off clip, which starts where the body sits (§4)
  [design].
- Only in the game being played (`--play`): the furniture's state is told to the
  script runtime's game (`nvf_play::furniture::set_use`), not to the residents' own.
- Log: `idle: … plays SitChairRelaxIdleA (… ) (FurnitureFidget)`, and at exit
  `furniture fidget summary: N fidgets on furniture started on actors, M on the
  player`. Smoke test `furniture-fidgets`.

### 8.4 Open

- Whether the original walks the tree for seated actors continuously (here: every 2 s
  when nothing else plays), and whether a fidget plays out before getting off (here:
  it fades out at once).
- Work spots (ID 22) need a way onto them before their idles can play.

## 9. Who may use which furniture (issue #1522) [design]

Every sandboxing actor chose any seat or bed in its area, whatever its skeleton: a Nipton
mongrel climbed a Legion cross, with a human clip bound to its skeleton [observed: the
game's log, issue #1522]. What follows is NVForever's policy, an interim one. Nothing here
says how the original decides, only that a watch of it found no creature on furniture:

- [observed, 2026-11-04 and 05, the original, minutes of watching and not days; item 85 of
  docs/handoff/observer-checks.md] no creature read as on furniture: four super mutants
  among `SuperMutantBed` and `FloorSitMarker` for 534 s at hours 12 and 23 (one of them never
  under a sandbox package while sampled); Colmillo and
  GibsonDog01 under `Sandbox256` for 358 s, 98 to 168 units from `OldLadyGibsonChair`, which
  was probably free [hypothesis]. Weaker: three rats by an outhouse, partly in combat, and
  Neil, never under a sandbox package. People in the loaded area did read as on furniture,
  so the reads fire. The Nipton mongrels, 596 s by a free cross: not on it either. That cross's
  record carries XIBS [documented: `fnvtool` over the record], so that no may be the flag.
- [observed: headless simulation only, 2026-11-05, at the last code commit of this rule]
  `fnvtool ai run` over 129 placed creatures
  of WastelandNV for 24 game hours: on main 24 creatures with 232 furniture events; with this
  rule none, 31 creatures with pieces excluded, none unknown. One worldspace, one day, 129
  references, an authored game with no quest started, and nothing drawn: it shows what the
  rule does here, not what the original does.

Two rules, applied before a choice or a reservation (`fnv_ai::furniture`). A search looks
past what does not fit, so a near piece does not hide a farther one and a Find does not count
it. Named furniture that does not fit is nothing to use: the actor goes to the package's place
and stands, or a Find or a Use Item At ends with nothing used, and its goal is met.

| Rule | Applies to | Not to |
|---|---|---|
| **Compatibility**: not a creature with no clip for it | a sandbox's beds, meal seats and seats; a Sleep or Eat package's search; what a package names or links to; a patrol point; what a Find or a Use Item At looks for or names | idle markers, PlayIdle, the player |
| **No cross by choice**: no enabled marker of ID 10 (§2) | a sandbox's beds, meal seats and seats; a Sleep or Eat package's search | what a package names or links to, so the crucified stay on theirs; a patrol point; what a Find or a Use Item At looks for or names, which is what its package asks for |

Marker IDs come from the model, by the flags that enable them by index (§1): the record's
flags alone do not tell a cross from a chair. No species list, FormID or editor ID comes
into it.

**Compatibility** (`fnv_actor::furniture_facts`) has three answers. What the actor is comes
from its own record, a leveled base's pick if its base is a list: `NPC_` is a person and `CREA`
a creature, whatever template its model comes from (`fnv_game::nvse_nx::is_creature`). Its
skeleton is the one that template gives.

| | Skeleton found, tree gives a clip from its folder | Skeleton found, no such clip | Skeleton not found, or the actor or the model not read |
|---|---|---|---|
| Creature | *Supported* | *Unsupported* | *Unknown* |
| Person | *Supported* (any clip of the tree under the folder) | *Unknown*, never Unsupported | *Unknown* |

- A clip the tree can give is one it reaches for a marker the base enables, in the phase of
  getting on, asking only the conditions about the piece and the phase
  (`UseIdles::clips_for_getting_on`). This over-approximates: it counts every sibling where
  playback takes the first that passes, and every marker the base enables where playback
  takes the nearest. So a creature could come out Supported and still be given a human clip;
  the guard below then refuses a clip whose body tracks have no node on the skeleton (matching
  names pass), and a refusal does not repair the AI's state: a safe failure, not correct
  behaviour. No shipped creature is in
  this case today, as no creature's clip is in the tree.
- A skeleton counts as found when its file is in the VFS: it is not read, so a corrupt skeleton
  would read as Unsupported for a creature. Not a case seen.
- All of the tree's furniture clips are under `characters\_male\idleanims` [observed: the
  base game and its DLCs, 2026-11-04], so today every creature whose skeleton is found comes
  out Unsupported and every person on `characters\_male` Supported.
- A person is never ruled out by a folder: their clips may be shared, and people off every
  chair would be the worst mistake this rule could make. What the tree does with a
  particular piece stays the engine's, as it was.
- Unknown stays eligible, and without facts (the tests, a run whose assets do not open)
  everything behaves as before this rule.

**File names prove nothing.** A creature's own files do not make it Supported, because
nothing plays a clip the tree does not reach. So these come out Unsupported, and that is the
interim rule, not a finding about them: the aliens (`dlc05\creatures\alien`, three placed
references, with `specialidle_enterchair.kf`) and the super mutants
(`creatures\smspinebreaker\idleanims`, with `specialidle_sitcycle.kf` and
`specialidle_sleepcycle.kf`), though the game has a `SuperMutantBed` [observed: the
catalogue of `fnvtool actor anims`, whose IDLE records name none of these files under
furniture conditions]. If a creature is seen on furniture in the original, the rule changes,
starting with how it reaches its clip.

**The engine's guards** (`drive_actors`, `fnv_actor::furniture_facts::missing_body_nodes`):
a clip from another actor's folder does not fit when one of its body tracks (`Bip01`,
`Bip01 …`) has no node in the actor's skeleton. A folder alone is no proof, and no share of
matching names is used. A human chair clip drives `Bip01 L Finger1` and its kin, which a
dog's skeleton does not have [observed: both files decoded].

- The getting-on clip and the loop are checked before the furniture is taken or anything is
  bound. Log: `furniture: <actor> cannot use <furniture>: clip … does not fit skeleton …`.
  What happens to the AI's state after that is not decided here (issue #1506). `--npc-sit`
  goes through this.
- A getting-off clip is checked when it is chosen. One that does not fit is not played: on the
  next update the actor is off, at the entry point and the furniture free, the way of a seat
  with no exit clip. Log: `furniture: <actor> gets off <furniture> without a clip: …`. It
  may look abrupt; it is not refused, as nothing else would release the piece.
- The player's clips are not checked.

**A risk this rule takes.** A script waiting for `GetSitting` or `GetSleeping` on a creature
could stall if that creature's furniture is refused [hypothesis]. No shipped case was found:
of the 27 compiled scripts of the base game and its DLCs that call either function, none is
on a creature's record, and those that name a reference name the player or a placed NPC
[observed: `fnvtool script decompile` and the CREA records, 2026-11-05]. Two are not settled
that way: `Vault112ResidentScript`, which asks of itself and is on no NPC or creature record, and `NPCVendorFollowPlayerTriggerScript`,
which asks of a linked actor held in a variable. Conditions on dialogue, packages and quest
stages were not searched.

**Reporting.** The AI's event `furniture_eligibility` is written once per actor and piece
when a piece was excluded, and once per actor, with reason `no-evidence`, when an answer was
Unknown. The game logs `ai furniture eligibility: actor=<ref> furniture=<ref>
decision=excluded|unknown reason=casual-marker-10|no-clips-for-skeleton|no-evidence`.
`fnvtool ai run` gives the same events, and for each actor `furniture`: its `starts` per
piece and its `excluded_pieces` per reason over the whole run (not capped as `events` is),
and `unknown`, null when the run has no facts. The facts are read once for a run, the models
in parallel; the game logs `furniture: eligibility facts: …` with the time this took.
