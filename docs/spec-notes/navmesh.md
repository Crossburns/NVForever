# Spec notes: navmeshes (NAVM, NAVI)

The navmesh is the area actors can walk on: triangles carrying slope, edge type, cover,
"preferred" and water information [documented: GECK wiki, Category:NavMesh]. It replaced
Oblivion's path grids as the pathfinding data of Fallout 3 and New Vegas [documented:
same page]. Code: `crates/fnv-nav` (reader, graph, paths, following); tools: `fnvtool nav
survey | graph | near | path | walk` (§6); design and engine choices:
docs/design/navmesh.md.

Observations are from the owner's install with the six masters (FalloutNV.esm and the
DLC masters) on 2026-10-09 (`fnvtool nav survey`, `nav graph`) unless tagged otherwise.

## 1. Records

### 1.1 NAVM [documented: xEdit wbDefinitionsFNV.pas NAVM, wbDefinitionsCommon.pas flags]

One record per navmesh, stored in a cell's children group (temporary children in the
masters [observed]). Record header flags: bit 11 "Initially Disabled", bit 26 "AutoGen".

| Subrecord | Layout |
|---|---|
| EDID | Editor ID (none in the masters [observed]) |
| NVER | u32 version |
| DATA | cell FormID, then u32 counts: vertices, triangles, edge links, cover triangles, door links |
| NVVX | vertices, 3 × f32 each (game units, the space's coordinates) |
| NVTR | triangles, 16 bytes each: vertex indices u16 × 3; edges i16 × 3 (0-1, 1-2, 2-0); flags u16; cover u16 |
| NVCA | cover triangles, u16 indices |
| NVDP | door links, 8 bytes each: door reference FormID, triangle u16, 2 unused bytes |
| NVGD | the navmesh's own lookup grid: u32 divisor, f32 max X and max Y distance, bounds min/max (3 × f32 each), then per grid square a u16 count and that many u16 triangle indices |
| NVEX | edge links, 10 bytes each: u32 type, navmesh FormID, triangle u16 |

Triangle flags: bits 0–2 "edge 0-1 / 1-2 / 2-0 link", 3 Deleted, 4 No Large Creatures,
5 Overlapping, 6 Preferred, 9 Water, 10 Door, 11 Found. Edge-link types: 0 portal,
1 ledge up, 2 ledge down, 3 enable/disable portal. The cover field holds per-edge cover
values and flags; xEdit says its own flag names for it are wrong (an enum of 4 bits per
edge plus flags), and NVForever does not use it yet. Cover can only be set on edges on the
outside of a navmesh [documented: GECK wiki, Toolbar, "Edit Cover for Edge"].

**How an edge value is read** [documented: xEdit wbNVTREdgeToStr]: when the edge's link
flag (bits 0–2) is set, the value indexes NVEX (a triangle of another navmesh, or of the
same navmesh for ledges, §3); otherwise it is a triangle of the same navmesh.
−1 means no neighbour [observed: 328,656 edges hold −1 and no other negative value
appears].

### 1.2 NAVI [documented: xEdit NAVI]

One "navmesh info map" record per plugin [observed: 6 records, one in each master].
NVER u32, then repeated:

- **NVMI** (one per navmesh): u32 flags (bit 4 Initially Disabled, bit 5 Is Island),
  navmesh FormID, location FormID (cell or worldspace), grid Y and X (i16 each),
  approximate location (3 × f32); when bit 5 is set, island data (bounds min/max, u16
  vertex count, u16 triangle count, vertices 3 × f32, triangles 3 × u16); then f32
  "preferred %".
- **NVCI** (one per navmesh): navmesh FormID, then three u32-counted FormID arrays:
  standard connections, preferred connections, door references.

NVForever builds its graph from NAVM alone (§4); NAVI is read and checked only.

## 2. What the masters hold [observed]

- **6,129 navmeshes** win after load order (4,771 definitions in FalloutNV.esm, 146 in
  DeadMoney, 585 in HonestHearts, 346 in OldWorldBlues, 290 in LonesomeRoad, 1 in
  GunRunnersArsenal); none is deleted or initially disabled; all have NVER 11 and no
  header flags. 621,020 vertices, **826,351 triangles**, 110,137 edge links, 1,364 door
  links, 40,156 cover triangles.
- By space: WastelandNV 4,037 navmeshes and 589,719 triangles; the 645 interior navmeshes
  95,786; NVDLC02ZionCanyon 59,987; NVDLC03BigMT 29,734; 27 other worldspaces the rest.
- **Several navmeshes per cell are common**: 2,240 cells have one, 838 two, 306 three, and
  one cell has ten.
- DATA counts always equal the arrays' lengths; DATA's cell always equals the cell whose
  group holds the record; every NVGD has divisor² squares (divisors 1–12) that together
  list every triangle.
- **Every non-degenerate triangle winds counter-clockwise seen from above** (826,282;
  69 degenerate, 0 clockwise). Paths rely on this: leaving a triangle by edge *e*, its
  first vertex is on the right (`fnv_nav::graph::NavGraph::portal`).
- **Internal edges are consistent**: every same-navmesh neighbour points back, and shares
  the edge's two vertices.
- **Cross-navmesh links**: 109,936 are reciprocal (the other triangle links back to this
  one), 95 are not, and 106 name a triangle beyond the target navmesh's count (left as
  outline edges). Across a seam the two navmeshes' vertices need not coincide: a long edge
  in one navmesh faces several shorter ones in the other, and links to one of them.
- Link types: 105,176 portals, 2,471 ledge-up, 2,472 ledge-down, 18 enable/disable
  portals. 2,560 links (all of them ledges) lead into the same navmesh.
- Triangle flags in use: edge links (bits 0–2), No Large Creatures 253, Preferred 3,712,
  Water 34,374, Door 1,346, Found on every triangle (826,351); Deleted and Overlapping
  never.
- **Door links**: 1,363 of 1,364 NVDP triangles have the Door flag (one names a triangle
  out of range); 752 doors are placed in the navmesh's own cell, 612 in another cell (the
  worldspace's persistent cell). Every Door-flagged triangle is named by an NVDP entry.
- NAVI: 6,139 NVMI entries covering all 6,129 navmeshes, 2,481 of them islands, 6,139
  NVCI entries; every NVMI location is the navmesh's cell or its worldspace.
- **Slopes** (5° bins of the triangle's normal, 826,282 triangles): 37% under 5°, 2.6% at
  45° or more, a thin tail to 85° (stairs and small triangles). There is no sharp cut-off
  that would give the generator's slope limit. The physics character's 45° limit
  [hypothesis, physics.md §6] is stricter than navmesh the game calls walkable: a route
  across the Mojave ran onto 46–49° terrain triangles at (−48,846, 38,662).

## 3. Ledge links [observed; meaning partly hypothesis]

A ledge link joins an edge on top of a drop to a triangle below it (ledge down), and back
up (ledge up). From the edge's middle to the target triangle's centre: ledge down drops
304 units (median; 5th–95th percentile 92–755), ledge up rises the same, horizontally
168 units (median). Portals by comparison: 0 drop (median) and 103 units across. The GECK
wiki calls them drop downs, edges "safe places for NPCs/Creatures to jump down"
[documented: GECK wiki, Navmesh Creation Workflow]. Paths use them only when asked
(`PathParams::ledges`, a package's Allow Falls), and a ledge up only within a jump's reach;
the actor jumps or drops over them (actor-jumps.md §3).

## 4. The nav graph (fnv-nav) [engine design on the facts above]

- The winning definition of each navmesh is placed in the **space** of its cell: a
  worldspace (exterior cells and the persistent cell) or one interior cell.
- All triangles of the load order go into one array (global indices); each edge resolves
  to the triangle beyond it: internal, portal (NVEX type 0 or 3), ledge (types 1 and 2) or
  outline. Building the whole load order's graph takes 0.33 s; 106 links stay unresolved
  (the out-of-range ones in §2).
- **Doors join spaces**: each NVDP triangle connects to where the door's teleport (REFR
  XTEL [documented: xEdit]) arrives: the destination door's own NVDP triangle when it is in
  the destination's space, else the triangle under the arrival point. 1,357 doors join;
  3 do not (no teleport, or no triangle at the arrival).
- **Walkable areas** are the triangles joined by internal and portal edges within one
  space. WastelandNV's largest area holds 97.5% of its triangles (575,116 of 589,719, in
  193 areas); Zion 95.7%, Big MT 97.5%.
- **Door locks**: a door reference with XLOC lock data [documented: xEdit REFR XLOC] starts
  locked [hypothesis, as `fnv_sim::world` assumes]. Paths go through a locked door only
  for an actor holding its key (or allowed to open locks). Example: the Freeside gate to
  the Strip (0x00116425) has lock level 255 and key 0x00119875 [observed].
- "Preferred" triangles are ones actors favour [documented: GECK wiki, Toolbar, "Toggle
  Preferred Pathing"]; by how much is not documented.
- The GECK can cut navmesh round a selected object, with uneven results [documented:
  GECK wiki, Toolbar, "Cut Selected Object"], and navmesh data can also be generated at
  run time [documented: Category:NavMesh]. Consistent with that, the navmesh sometimes
  covers obstacles: a buried canyon rock (NVNRockCanyon12, 0x00106364) at
  (−70,418, 2,885) in Goodsprings lies under walkable triangles that our physics capsule
  cannot climb [observed: `fnvtool nav walk`, `nav near`].
- Non-teleport doors (e.g. `UtlDoor01` 0x0012C97C in OVCentralSewers01) sit across the
  navmesh with no Door flag [observed]: actors are expected to open them on their way
  [hypothesis].
- **Doors that open in place** (DOOR references without XTEL) are found by their base's
  bounds (OBND, placed by the reference's position, rotation and scale) over the
  navmesh's triangles: **1,043** of them stand on navmesh in FalloutNV.esm [observed:
  `fnvtool nav graph`, `stats.in_place_doors`]. Others stand where no navmesh runs (a
  closet door at the navmesh's edge, e.g. 0x0010618E in GSProspectorSaloonInterior)
  [observed: `nav near`]. A locked one closes its triangles to actors without its key, as
  a locked teleport door closes its link [engine choice on the lock rule above]. Their
  models and animation: docs/spec-notes/doors.md.
- A door's bounds can be wider than its opening: `UtlDoor01`'s span 249 units across
  (−69 to 180), more than the doorway the navmesh runs through at 0x0012C97C [observed];
  the walkable stretch of navmesh along the door's line is taken as the opening instead
  (docs/design/navmesh.md §4).

## 4b. Levels stacked over each other

Navmesh runs over itself wherever a catwalk, a stairway or an upper floor stands over a
lower floor (REPCONN's rocket hall, Hoover Dam's towers and power plants, HELIOS One,
vaults, sewers, two-storey houses). Seen from above, a point there lies on several
triangles at different heights, so nothing in the graph may treat "inside a triangle"
as a 2D test alone.

- **RocketLabMid** (0x00081D1F): the floor at z −128 and the catwalks at −6, 66, 194,
  250, 320, 450 and 506 are joined by flights of 24-unit steps (`UtlCatStairs*`), and
  the navmesh runs up every flight with its triangles linked: A* from the floor at
  (1000, 3400, −120) to Jason Bright's intercom 0x000CD684 finds a 62-triangle corridor
  up the stairs [observed: `fnvtool nav path`]. The navmesh data and the graph build are
  sound there.
- **What went wrong was ours** [observed, fixed 2026-10-23]: the shortcut pass
  (docs/design/navmesh.md §3, step 6) asked `walkable_line` whether the straight line
  from the floor to a catwalk corner could be walked. The line runs across floor
  triangles and was accepted as soon as it came to a triangle containing the corner
  *seen from above*: the floor triangle under the catwalk. The path then went
  (1077, 3758, −128) → (1246, 3888, 448), 576 units up in one segment, and walkers
  stuck on the floor under the catwalk. A straight walk stays on one surface, so where
  it reaches the end point's spot, the end point must be on that surface: within
  `LEVEL_GAP` (40 units) of its height, else the line is not walkable [engine choice].
  Where the line crosses a seam, the triangle beyond is now found at the surface's
  height there rather than the line's (which runs through the air between levels).
- **How far apart levels are** [observed: every usable triangle of FalloutNV.esm, the
  nearest other surface above or below its centre]: 14,788 triangle centres have
  another surface more than 120 units away, 23 between 80 and 120, 4 between 60 and 80,
  4 between 40 and 60, and 31 within 40. Those 31 are overlaps where neighbouring
  navmeshes meet on a slope (WastelandNV seams, e.g. near (74496, 65400)), one surface in
  effect. 40 units is under every gap between distinct levels seen.
- **How common it was** [observed: 40 random paths in each of the 88 interiors with
  stacked levels, before and after the fix]: 148 of 3,297 paths in 47 interiors had a
  segment rising or falling more than 1.5 times its length (more than 40 units)
  before; none after. Among them HiddenValley01 (11), HELIOSOneTower (12),
  NorthVegasHouseTools (9), 1ESteelMill (9), HooverDamIntTower01Interior (8).
- **Walking it** [observed: `fnvtool nav walk`, NPC capsule]: from the floor to
  0x000CD684 the walker now arrives at 86.8 and 340 units/s. At 340 it falls off a
  catwalk turn three times and climbs back (4 replans) until the character step-up of
  physics.md §6.9.1 is in; with it, the walk has no detour or replan.

## 4a. Checked against NPCs recorded in the game [observed]

`fnvtool nav observed` on the AI observer captures of 2026-10-08 (Goodsprings town by day,
the saloon in the evening, meeting Sunny, walking to a settler, the gunfight with the
Powder Gangers; ai-observer.md), 28 actors:

- **Where NPCs stand is on the navmesh**: of 22,625 distinct standing positions (sitting,
  sleeping, dead and disabled samples left out), 96.2% are within 8 units of a navmesh
  triangle of the actor's space, 99.7% within 32 and all within 128; the median is
  0.003 units. The farthest (81 units) are in house interiors.
- **Where NPCs walk is near the planned path**: 85 recorded walks (runs of moving samples
  in one cell whose ends are at least 512 units apart). The planner finds a path for all
  85. Recorded positions are a median 17.6 units from the planned path (61.5% within 32
  units, 78.5% within 128), and recorded walks are a median 1.003 times the planned length.
  The large deviations are wandering animals and sandboxing settlers (Victor, the
  brahmin, the bighorners: 1.2–1.8 times the planned length), not travel from A to B.
  Samples come every 60 game loops, so a recorded path is a coarse polyline.

## 5. Animation used by travelling actors [observed]

The human walk cycles are `characters\_male\locomotion\male\mtforward.kf` and
`...\female\mtforward.kf` (`Fallout - Meshes.bsa`; the top of `locomotion\` holds only
weapon-type forward clips, `mtidle.kf` and the turns). `male\mtforward.kf` is one 1.2 s
sequence, "Forward"; its accumulation root travels **85.3 units/s** at the clip's own
rate, within 2% of the player's measured walking speed (86.8 units/s, physics.md §6.3).
That NPCs walk at that speed is [hypothesis].

## 6. CLI contract

All commands take `<data_dir> <plugins>... --schema <dir>` and print one JSON document.

- `fnvtool nav survey` → `fnv.nav.survey/1`: definitions per plugin, versions, totals,
  navmeshes per space and per cell, triangle flag bits, slopes, winding, link types and
  link geometry percentiles, NVGD divisors, NAVI counts, and `checks`: a tally (count and
  up to three examples) of every consistency check of §2.
- `fnvtool nav graph [--space S]` → `fnv.nav.graph/1`: build timings, `stats` (navmeshes,
  triangles, edges by kind, unresolved links, doors), and per space its triangles, number
  of walkable areas, the largest ones and the largest one's share.
- `fnvtool nav near --at <ref|x,y,z> [--space S] [--radius R]` → `fnv.nav.near/1`: the
  triangles within R (navmesh, index, corners, flags, each edge's kind, target and whether
  its ends are outline vertices, doors, `in_place_doors`: the doors that open in place
  standing on it).
- `fnvtool nav path --from <ref|x,y,z> --to <ref|x,y,z> [--space S] [--clearance R]
  [--ledges] [--no-doors] [--large] [--open-locked] [--keys K,...]` → `fnv.nav.path/1`:
  the resolved ends, params, `door_route` (the doors to go through, with the space each
  leads into and its lock, or an error) and its time, and `path` (length, triangles
  expanded, snap distances, legs: space, polyline points, corridor size, navmeshes
  crossed, the door ending the leg, `door_stops`: each door that opens in place on the
  leg and the point where the actor stops to open it) or `error`.
- `fnvtool nav walk` (the path options, plus `--speed`, `--max-seconds`,
  `--goal-radius`, `--every`, `--max-slope`, `--any-slope`, `--with-actors`,
  `--nif-schema`, `--no-meshes`) → `fnv.nav.walk/1`: an NPC-sized physics character
  (fnv-phys defaults) walks the path in the collision world of the cells along each leg
  (built like `fnvtool phys walk`), steered by `fnv_nav::Follower` with
  `fnv_nav::follow::Unstuck`. `summary`: outcome (`arrived`, `stuck`, `timeout`),
  seconds, steps, end, distance to the goal, walked, replans, detours, doors used,
  `doors_opened` (doors that open in place: door, stop, feet, seconds waited for the Open
  sequence), largest deviation from the path, navmesh checks (and how many found no
  navmesh within 48 units), physics builds and their time, colliders touched. `events`
  (world built, `animated_doors` in the world, `door_opening`, `door_passed` for a door
  with nothing to wait for, detour, replanned, gave up) and an optional `trace`.

- `fnvtool nav observed --samples <jsonl>...` (`--min-walk`) → `fnv.nav.observed/1`:
  per §4a, recorded positions against the navmesh (counts within 8, 32, 128 and 512
  units, median, 99th percentile, the farthest) and recorded walks against planned paths
  (per walk: actor, space, ends, recorded and planned length, deviation mean / 90th
  percentile / max; overall counts, median and 90th percentile deviation, median length
  ratio). FormIDs in captures are in the game's load order and are mapped by plugin name.

## 4c. Obstacles cut the navmesh

The record header's bit 25 "Obstacle" on STAT, MSTT, ACTI, SCOL, CONT and LIGH
[documented: xEdit wbDefinitionsFNV.pas flag lists; schemas/fnv `header_flags`]: "the
object will dynamically cut the navmesh in game preventing actors from walking into it"
[documented: GECK wiki Activator, Flags; the same page: it "only works with NIFs that have
the correct collision layer set"]. FalloutNV.esm places 59,925 references of obstacle
bases: rocks (`NVNRockCanyon12` alone 10,747), cliffs whose bounds run to 2,400 units,
yuccas, Joshua trees with empty bounds; 149 of them can come and go (Initially Disabled
or with an enable parent): sandbags (55), guard rails, toxic barrels, fences, the Hoover
Dam force fields, box cars and car hulks [observed 2026-10-24, private survey script, not
committed].

As built (`fnv_nav::obstacles`): the navmesh, made in the editor, already has holes for
what is always there, so only references that can be enabled or disabled cut it, and
only while enabled [hypothesis]; of those, only the ones placed Initially Disabled, as
one placed enabled stood there when the navmesh was made too [hypothesis; observed
2026-10-28: the sandbag 0x00168F64, placed enabled under the enable parent
VMQ03aGrahamRadioREF, has the Hoover Dam deck triangles between Colonel Moore's patrol
points 0x001377C5 and 0x001377C6 under its bounds; cut, they split the deck and she
never reached Kimball's podium (smoke `scene-kimball-speech`)]. A reference cuts the triangles under its bounds (OBND,
placed, rotated, scaled) seen from above whose heights meet the bounds from 64 under the
base to 32 under the top [engine choice: a floor near the top is on the object]; a cut
triangle takes no part in paths (`NavGraph::usable`). The graph learns the obstacles
when it is built, enabled as placed; the engine brings them in line with the game's
enabled state every frame (`fnv_ai::Ai::apply_obstacles`) and actors whose way crosses a
newly cut triangle plan again, as for a navmesh switched off. Log lines: `nav: N
obstacles that can be enabled or disabled stand on the navmesh …` at the start, `nav:
obstacles switched […]` on a change. Not done: the collision-layer condition (fnv-nav
reads no models), cutting by the shape rather than the bounds.

**The poll is skipped while nothing it reads changed** (issue #1430) [design:
`fnv_ai::Ai::obstacles_in_line`, crates/fnv-ai/src/carry.rs]. `Game::present` reads the
world's reference states alone (crates/fnv-game/src/game.rs): whether a reference and the
members of its enable-parent chain have a state, their `enabled` flags, the reference's
`deleted` flag, and the placement table, which never changes. After a full poll the AI
keeps the pair (`World::presence_serial`, `NavGraph::obstacles_stamp`); while both are
the same, a poll would switch nothing, so none runs. Debug builds poll anyway and assert
that the graph agrees. Every writer of those inputs moves one of the two numbers
[design; checked by search over `crates/` and `engine/` for writes of `enabled`/`deleted`,
`RefState` literals and changes to the `refs` map]:

| writer | where | presence serial |
|---|---|---|
| a state made on first use (`World::get`) | crates/fnv-sim/src/world.rs | moves unless the state starts as placed (enabled unless Initially Disabled, not deleted), which is what presence read before; the player's state (not placed) moves it |
| items dropped (`World::drop_groups`), references created (`World::place`: PlaceAtMe, disarmed mines) | world.rs | moves |
| respawning containers reset on a cell attach (`World::attach_cell`) | world.rs | moves per state removed |
| `World::set_enabled` (Enable/Disable, script runner, actions, mines, fnvtool) | world.rs | moves when the flag changes |
| `World::set_deleted`: activation pick-ups and mine disarms (`World::activate_as`), ash piles removed (crates/fnv-game/src/ash_piles.rs), console `zap` (functions/console.rs), MarkForDelete (functions/refs.rs), JIP MoveToContainer (nvse_jip3.rs, with `set_enabled`), a projectile gone (nvse_proj.rs) | world.rs, crates/fnv-game/src/ | moves when the flag changes |
| a new or restored world (`World::new`, `World::restore`) | world.rs | starts from a number no other world of the process used (as `lock_serial`), so a swapped, loaded or new world reads as changed |

On the graph side `NavGraph::obstacles` is written only by `set_obstacles` and
`sync_obstacles` (crates/fnv-nav/src/obstacles.rs); each takes a stamp no graph of the
process had (`sync_obstacles` only when it switched something). `enabled` and `deleted`
are private to `fnv_sim::world` (issue #1464): a direct write of either field outside that
module, in `fnv-sim` or any other crate, no longer compiles, so writers go through
`set_enabled`/`set_deleted`. That does not enforce every change of the flags:
`World::get` still hands out `&mut RefState`, so `*world.get(id)? = another_state` compiles
and would replace a whole state, flags included, without moving the serial, and the cache
would leave an obstacle cut or open in release builds. A search found none [design];
whether to close that is a separate question, left open.

## 7. Open questions

- How the original chooses paths (costs, the weight of Preferred and Water, door costs)
  and how actors open doors (from how far; whether they wait for the animation; whether
  doors close behind them: docs/spec-notes/doors.md §3), avoid each other and get unstuck. The GECK's "Test
  Pathfinding" tool shows the path an actor would take between two triangles
  [documented: GECK wiki, Toolbar]: captures of it would give reference paths.
- The actor walking speed and the steepest slope actors climb (§2, §5).
- What the Found flag (every triangle) and the cover enum mean.
- Whether NAVI islands or NVCI connections are used at run time, e.g. for long-range
  routing of actors outside the loaded area.
