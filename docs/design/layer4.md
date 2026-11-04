# Layer 4 design: core simulation

Spec section 5 Layer 4 ("entity model (forms and references), save-agnostic game state,
collision and character controller, basic physics, activation, containers, inventory")
and milestone M4 ("player controller, collision, activation, inventory"), in the section
9 format. Status 2026-10-07: **simulation core (workstream 3) feature-complete for this
cut**: forms, references (incl. placed explosives), inventories with per-stack extra data,
equipping, encumbrance, leveled lists, activation, enable-parent chains, game settings,
game clock and globals, container respawn, delta state, scripted traces, item listing and
leveled-list sampling — all headless, on the owner's install, with `L4.sim.containers`
and `L4.sim.items` passing against the independent oracle. Behaviour rules stay
[hypothesis] until the in-game captures (simulation.md §6) exist. Collision and the
character controller are workstreams 1 and 2 (§7).

## 1. Summary and assumptions

Task: the part of the engine that makes the static world (Layer 3) interactive, kept
**headless and deterministic** (spec section 7) so every behaviour can be tested without a
renderer and compared with captures from the original game.

Assumptions and decisions (2026-10-07):
- **Developed in parallel with Layer 3** (another session works on world rendering). To
  stay out of its way, Layer 4 lives in its own crate (`fnv-sim`), touches no rendering,
  viewer or cell-streaming code, and changes shared files only by small additions
  (workspace member, `fnvtool sim` subcommand, one `pub(crate)`). After Layer 3 merged,
  the placed-reference table was rebuilt on Layer 3's `fnv_world::WorldIndex` (§2.3).
- The core does **not** use an ECS (§2.2).
- Behaviour that the data cannot show (what activation does, how leveled lists roll) is
  implemented as clearly tagged **[hypothesis]** and isolated behind small functions, so
  the in-game captures of docs/spec-notes/simulation.md §6 can correct it.

## 2. Findings and design

Facts and their tags: `docs/spec-notes/simulation.md`. Summary of what the data showed
[observed, ten masters]: 427,022 placed references; the persistent flag 0x400 matches the
persistent-children groups exactly; the player base is NPC_ 0x7 and no plugin places the
player reference 0x14; locks occur only on doors and containers; 1,376 of 1,378 teleport
doors are reciprocal pairs; 105 carried-item entries have count 0; 1,492 references use
engine-reserved bases 0x17/0x20 (occlusion planes and portals: Layer 3 culling).

### 2.1 Crates

| Crate | Role | State |
|---|---|---|
| `fnv-sim` | forms, references, inventories, leveled lists, activation, delta state, action traces | **done (first cut)** |
| `fnv-collide` | Havok collision blocks from NIFs → engine-neutral shapes in game units | next (§2.8) |
| `fnv-phys` | rigid bodies, queries, character controller over a physics engine | after `fnv-collide` |

`fnv-sim` depends on `fnv-esm` and `fnv-world` (plus serde, rand_chacha, rayon). No
Bevy, no renderer: the Bevy engine will drive it, never the reverse.

### 2.2 Entity model: plain ordered maps, not an ECS (decision)

- **Forms** (`FormStore`): the winning record of each FormID, interpreted for the record
  types Layer 4 needs (items with value/weight, containers, doors, activators, furniture,
  actors' carried items, leveled item lists, globals); every other winner kept as its
  signature so any reference can be classified.
- **Placed references** (`RefTable`): read-only authored placement (base, cell, position,
  rotation, scale, count, owner, lock, teleport, enable parent, linked reference).
- **Runtime state** (`World`): `BTreeMap<FormId, RefState>` of *materialised* references
  only (§2.4), plus created references (FormIDs from 0xFF000000).

Rejected for the core: `bevy_ecs` (ties the simulation to Bevy's release cadence, which
ADR-0001/0002 deliberately confine to the `engine/` workspace; query iteration order is not a stable
contract, which determinism needs) and `hecs` (no benefit at this stage: there are no
per-frame systems iterating thousands of entities yet). Revisit when AI and physics need
per-frame iteration over many actors (Layer 6); the Bevy app mirrors `World` into ECS
entities through an adapter rather than owning the state.

### 2.3 Loaders read through the Layer 1 schema

Fields are looked up by the names `schemas/fnv` gives them (`extract.rs`), so layout
corrections need no code change, and anything skipped is counted in an issues list
("nothing silently dropped", spec section 7). Which references exist and which cell each
is in come from Layer 3's `fnv_world::WorldIndex` (`RefTable::from_index` lets the engine
share one index); `fnv-sim` adds count, owner, lock, teleport and linked reference.
Switching to the shared index left every `sim forms` statistic and the Goodsprings trace
unchanged [observed]. Full load of the ten masters (forms, index and all 427,022
references) takes about 0.8 s [observed, release build].

### 2.4 Save-agnostic state

A reference nothing has touched has no runtime entry; the first access derives it from
the placement data (initial inventory: carried items with leveled lists resolved; lock;
enabled flag). A `Snapshot` (`fnv.sim.state/1`) holds exactly the materialised
references, the RNG position, the next created FormID and the clock's digits the f32 time
globals cannot hold (simulation.md §5), so it is the set of changes
against the plugins and independent of the original savegame format (savegame
compatibility is Layer 7). Restoring a snapshot and continuing gives the same result as
an uninterrupted run (tested).

### 2.5 Inventory

One stack per base item with a count (`Inventory`, ordered). Per-instance extra data
(condition, ownership, weapon mods, equipped state) becomes a list of extra-data entries
per stack when Layer 6 needs it. Weight/value totals report items whose layout is not yet
in the schema (NOTE, IMOD, CHIP, CCRD, CMNY) instead of guessing.

### 2.6 Leveled lists

Rules in docs/spec-notes/simulation.md §4, consistent with in-game samples for the
`leveled.basic` and `leveled.b8` cases [observed] (LVLG globals are read at their
run-time values, `leveled::resolve_in`). One seeded ChaCha8 stream
(`rand_chacha`, reproducible across platforms and versions by that crate's contract)
makes runs repeatable; matching the original game's outcomes is a *distribution*
question (spec section 8.1). The lab's `distribution` comparator (2026-10-07) compares
sampled outcome counts from the original game with the candidate's samples (chi-square
test of homogeneity) or exact probabilities (goodness of fit), per case, pooling rare
outcomes; the candidate side is `fnvtool sim sample` (docs/spec-notes/simulation.md §7).
Smoke test [observed]: 300 draws per case (seed 12345) against the scenario's 20,000
(seed 0) pass every case of `leveled_basic` (p 0.18–1.0); swapping two cases' outcomes
fails both.

### 2.7 Activation

`World::activate(actor, target)` dispatches on the target's base type (item → pick up,
container → open, door → teleport or toggle, actor → talk, activator/furniture →
activated), after checking enabled/deleted state and locks (a carried key unlocks first).
The base form's script is reported for Layer 5's OnActivate. Every rule is [hypothesis]
until captured; the outcome is a value, not a side channel, so the trace shows it.

### 2.8 Collision, physics and the character controller (next)

1. **`fnv-collide`**: convert the Havok blocks that Layer 2 already decodes at 100%
   (`bhkCollisionObject` → `bhkRigidBody(T)` → shapes: MOPP tree over packed triangle
   strips, convex vertices, box, sphere, capsule, list, (convex) transform, NiTriStrips
   shapes) into engine-neutral shapes in game units, with body transform, collision
   layer, motion type and mass. No physics engine dependency, so it can be checked
   against an independent oracle like every other format.
2. **Havok scale**: the factor between Havok units and game units is commonly given as 7
   for this engine generation [hypothesis]. It will be *measured*: for every mesh with
   both, compare collision-shape bounds with render-geometry bounds; the ratio
   distribution must peak sharply at one value.
3. **`fnv-phys`**: `rapier3d` with `enhanced-determinism` (pure Rust, cross-platform
   determinism) — preferred to Jolt bindings (C++ build and FFI) and to `avian3d`
   (Bevy-coupled). Static world: placed references' collision plus terrain from LAND
   heights (shared with Layer 3). Character controller: a kinematic capsule with step
   height, slope limit and gravity; the original values are unknowns to capture (§5).
4. **Scenarios**: shape statistics per archive against an oracle (exact with tolerance);
   later, player position after scripted movement against in-game captures.

## 3. Code, schema and tests

- `crates/fnv-sim`: `form`, `extract`, `forms`, `refs`, `inventory`, `leveled`, `world`,
  `actions`. Tests: unit tests (FormID text, inventory incl. a conservation property
  test), `tests/synthetic.rs` (a plugin built in memory and decoded with the repository
  schemas: loaders, locks and keys, pick-up/drop, teleport/toggle doors, errors,
  leveled lists, uniformity of the RNG helper, action trace, snapshot resume).
- `fnvtool sim forms | containers | run` (`crates/fnvtool/src/sim_cmd.rs`); contract in
  docs/spec-notes/simulation.md §7.
- Independent oracle: `tools/capture/oracle_sim.py` (`containers`, `items`), written from
  the spec notes only.
- Parity lab: `parity-lab/scenarios/layer4_sim.json` — `L4.sim.containers` and
  `L4.sim.items` (keyed vs the oracle) and `L4.sim.trace.goodsprings_doc_mitchell` (observations vs an in-game
  capture; `needs-local-capture`); action scripts in `parity-lab/inputs/layer4/`.

## 4. Validation plan and exit criterion (M4)

1. Loaders: zero unexplained issues on the ten masters; every anomaly documented in the
   spec notes — done for this cut (remaining issues are schema gaps, listed in §5).
2. Static contents: `L4.sim.containers` and `L4.sim.items` against the independent oracle.
3. Behaviour: action traces against in-game captures (Goodsprings first, then a locked
   container with its key, a lock without key, a teleport pair, dropping and re-taking).
   Locks and drops: `L4.sim.trace.goodsprings_locked_safe_key`, `_locked_safe_no_key`
   and `goodsprings_drop_and_retake` (captured 2026-10-08; docs/spec-notes/layer4-captures.md §1).
   Close-out 2026-10-09: `L4.sim.trace.goodsprings_cell_respawn` (72 h cell respawn of an
   ordinary Respawns container, 16/16, predicted before the capture) and
   `L4.sim.trace.nipton_placed_mines` (scripted activation of a placed mine does nothing,
   7/7 after the fix; 5/7 before).
4. Leveled lists: distribution scenarios — `L4.sim.leveled.basic` passes against in-game
   samples (7 cases), and `L4.sim.leveled.b8` (10 cases: LVLG globals, all levels between
   entry levels, nesting 3–5 deep; 2026-10-08).
5. Collision: shape conversion vs oracle; Havok scale measured; then movement traces.
   Status 2026-10-08: the controller rests on contact shells like the arrival rule, the
   corner jitter is fixed, gravity / jump / run / walk are measured in game (physics.md
   §6.3). Captured the same day: B11 15/20 after fixes, 14/20 since Portal Box markers
   are solid again (open: Havok trees; follow-up 2026-10-08 [observed]: rock b and the
   maize push are the game's own spread over repeated identical drops, rock a lands on a
   step edge where the game's player passes; on the owner's decision the golden is now
   the median of five repeats per drop: 16/20, physics.md §4.2), traces fence_slide 6/6, fence_stop 6/6,
   porch_steps 6/6, running_jump 0/6 (air control open), hill_run 6/6 and portal_wall
   5/6 (goldens rebuilt with the corrected onset on the owner's decision, no longer
   held-out evidence; the steep downhill row is open; physics.md §4.2, §6.3, §6.4,
   layer4-captures.md §1).
   Status 2026-10-08 (late) [observed]: air control (gain 0.30 = `fJumpMoveMult`, full
   target when a key is down at take-off) and the landing speed profiles are modelled from
   the `mv_air` / `mv_air2` calibrations: running_jump 6/6. The body is now a radius-22
   cylinder on a cone bottom fitted to fence-edge drops, and a falling body no longer
   slides off steep contacts: B11 17/20 against the median golden (rock a now passes);
   the Havok tree rows are still 14 and 10 low, against a contradiction between the tree and the fence edge that
   is open (physics.md §6.4.1). The sinking on edges depends on the fall speed, which is
   not modelled.
   Swimming (physics.md §6.6):
   swim_dam_forward 10/10, swim_bay_back 8/8; placed water modelled (Water-layer
   phantoms), held-out swim_guardian 6/6 and swim_h2o_fresh 8/9 (replacing swim_h2o, retired
   2026-10-09; shallow_a is the stair edge of §6.4.1); swim_cove_dive and
   swim_lake_shore retired 2026-10-09 and replaced by swim_cove_stroke 8/8 and
   swim_lake_exit 10/10 (held out, below); banks of 12–58° (`sw_banks*`) agree while swimming and on leaving
   the water [observed 2026-10-08].
   **Accepted residuals (owner decision 2026-10-09):** the three physics rows still missing
   — `L4.phys.drops.b11_goodsprings` 17/20 (the Havok trees against the fence edge),
   `L4.phys.walk.portal_wall` 5/6 (the steep downhill row) and
   `L4.phys.drops.swim_h2o_fresh` 8/9 (a stair edge) — were worked on repeatedly without
   gain and are accepted as known differences. They stay failing as the honest record
   (scenarios, goldens and tolerances unchanged); no further tuning is planned.
6. M4 done when the player can walk the Mojave (Layer 3 world) with collision, open
   doors and containers, and take items, with the scenarios above passing.
   Status 2026-10-08 (`engine/nvf-play`, docs/design/layer4-play.md §6): walking with
   collision, doors, containers and taking items pass unattended in Goodsprings, Doc
   Mitchell's house and the gas station; interior starts and door destinations over
   nothing move to a door arrival, and falls out of an interior are caught (physics.md
   §6.7); the 106 doors into another worldspace (Mojave ↔ Freeside, Freeside ↔ Strip,
   DLC worldspaces) switch physics and rendering to the destination worldspace; items
   dropped from the inventory are drawn, collide, fall and settle, survive cell unloads
   and worldspace changes and are taken back with E (whether the game's dropped items
   fall is open, simulation.md §5). Scripted doors (the Strip gate) open through their own
   scripts since Layer 5 (layer5-play.md §6). Dialogue and barter are Layer 6.
   **Played by hand (2026-10-09, coordinator)** [observed]: `nvforever --play --cell
   Goodsprings` on `main` 9be35d7, driven with real OS keyboard and mouse input (Windows
   `SendInput`, the same path as a person's keys) for about six minutes: mouse look, WASD
   (2.5 s of W covered about 845 units, the run speed), Space jump (+51 units at 0.25 s),
   the start-up message boxes dismissed with their number key, Tab inventory (arrow keys,
   Enter, mouse click to select and equip, the Close button), the Prospector Saloon door's
   prompt and E into the interior, a container opened with E and emptied with R (carry
   weight 89.5 → 95.5), Esc/Tab/E closing menus. Found, and fixed in #97 (Esc now opens a
   Resume / Quit pause menu): the help line listed the free-fly keys in play mode; Esc in the world quit
   at once (the game opens a pause menu); message-box buttons ignored mouse clicks;
   equipping ammunition reported a raw FormID; lock prompts showed "level 0" instead of
   the game's level names.

## 5. Open questions and verification steps for the owner

1. **In-game capture** for `L4.sim.trace.goodsprings_doc_mitchell` (procedure in
   docs/spec-notes/simulation.md §6). Can you run console commands on a pinned save in
   Doc Mitchell's house, or do you prefer that we build the xNVSE logging plugin first?
   → Owner 2026-10-07: build the plugin (`nvf_capture`, tools/capture/README.md §10).
2. ~~What happens in-game when activating a locked container *with* its key in the
   inventory: does it open directly (our hypothesis) or prompt?~~ → Owner 2026-10-08: it
   opens directly, no prompt (simulation.md §5).
3. ~~Count-0 carried items on NPCs (e.g. the Vault 21 gamblers): do those NPCs carry
   anything for those entries?~~ → Owner 2026-10-08: nothing; the entry is empty
   (simulation.md §3).
4. ~~Schema gaps that block weight/value~~ — closed 2026-10-07: IMOD, CHIP, CCRD, CMNY
   layouts added from xEdit and decoding every instance; NOTE and CHIP have no value or
   weight field and CMNY only an "Absolute Value" (docs/spec-notes/simulation.md §3). Do
   notes, casino chips and caravan money weigh anything in-game (inventory screen)?
   → Owner 2026-10-08: notes, casino chips, caravan money and caravan cards weigh nothing,
   like caps; the carried weight counts them as 0.
5. ~~Placed grenades (PGRE, 297 in OldWorldBlues.esm): live explosives or decoration?~~
   → Captured 2026-10-09 (`cl_d_mines_live`, unattended): **live**. The 297 PGRE are
   spread over five masters, not only Old World Blues (simulation.md §1); placed frag
   mines in Nipton detonate when the player comes within 70 units (not at 237), within
   1.5 s, damage 80–95, and end disabled. A scripted `Activate` does nothing (scenario
   `L4.sim.trace.nipton_placed_mines`); the Activate control tapped with a mine in the
   crosshair did nothing (`cl_d2_mines_disarm`), so disarming by the player is still
   hypothesis (whether `TapControl 5` activates at all is untested).
6. ~~Cell respawn delay `iHoursToRespawnCell`~~ → **72** in the running game (three
   saves; no master defines it); `fnv-sim` uses it (`DEFAULT_RESPAWN_HOURS`). The
   scenario `L4.sim.trace.goodsprings_cell_respawn` puts the boundary between 71.0 and
   73.1 hours away from the cell, counted from leaving it (simulation.md §3).
7. **In-game captures**: log, results and the backlog of captures still to make
   (X-13, a merchant box without the Respawns flag, cell respawn on an ordinary
   container, calendar, encumbrance edges, more leveled lists, PGRE) are in
   docs/spec-notes/layer4-captures.md. Status 2026-10-07: `goodsprings_weight_and_time`
   9/9, `leveled_basic` 7/7, `goodsprings_merchant_restock` 4/4,
   `goodsprings_doc_mitchell` 5/6 (z waits for the controller); `x13_enable_parents`
   still needs a save inside X-13. Update 2026-10-08: `x13_enable_parents` 13/13;
   merchant restocks follow fixed days (GameDaysPassed 0 or 3 modulo 7, spec note §3),
   checked by `goodsprings_merchant_midnight_restock` 11/11.
   Close-out 2026-10-09 (unattended, layer4-captures.md §2 `cl_*`): the Pip-Boy Glove and
   Pip-Boy 3000 are worn in played saves and survive `RemoveAllItems` (the simulation's
   player now starts wearing them); over-encumbered = whole part of the weight above the
   capacity (ran at 200.5, walked at 201 with capacity 200); armour on the ground is the
   world model MOD2 (a fedora, folded outfits; female MOD4 open); Chet's merchant box does
   not follow Ringo's restock days (open). `goodsprings_doc_mitchell`
   6/6 (2026-10-08): `sim run` settles teleported actors on the destination's contact
   shell (physics.md §6.2), not through the walking controller.
8. Answered by the owner 2026-10-08 (physics.md §4, §6): disabled references are not
   drawn, have no collision and cannot be interacted with; `GetPos Z` is the feet (player
   7361.05 standing on a rug at 7360 in Doc Mitchell's house). Still open: the collision
   layer matrix — a scripted drop test is proposed as backlog item B11
   (layer4-captures.md); the Havok scale needs no in-game test (collision.md §6).

9. **Movement and collision (workstream 2, 2026-10-08)**. Update 2026-10-09
   (`claude/l4-play-closeout`): capsule height, push and dropped items are captured
   unattended (`pc_head`, `pc_push`, `pc_drop_fall`, physics.md §4.3 and §6): the player
   is about 130 units tall to the ceiling's surface (the character is now 128.9 with a
   cone top), pushing agrees with push mass 80, dropped items fall; the crouched height and
   whether running or walking feels right with a real keyboard remain for someone at the
   game. Unattended jobs are ready and
   need only the game to be free (layer4-captures.md §5): `mv_b11_drops`, `mv_traces`,
   `mv_b11_clutter`, `mv_values`. Questions that need someone at the game:
   - Capsule height: standing, then crouched, under a beam or doorway lintel whose height
     `fnvtool phys cell --ray` gives (e.g. the porch roof of Doc Mitchell's house): can
     the player pass, and with jump does the head stop the rise? Report `player.GetPos Z`
     at the highest point of a jump under it.
   - Push mass: walk at a dynamic clutter item (e.g. a tin can on flat ground), forward
     held for one second, and report its position before and after (`<ref>.GetPos x/y`
     with the reference picked by clicking it in the console).
   - Is anything about movement different with the mouse/keyboard than with held keys
     (e.g. does running feel like 344 units/s, does walking with Shift match 86.8)? The
     unattended runs used `HoldKey` with the bound scan codes.
   - ~~Ladders~~ → Owner 2026-10-08: New Vegas has no climbable ladders; ladder-like
     uses are door-style transitions that teleport the player up or down, handled by the
     door/teleport path (physics.md §6.5). Swimming: captures and the swim mode in
     physics.md §6.6.

## 6. Updated spec notes

`docs/spec-notes/simulation.md` (new): forms and references, the player, items and
carried items, leveled lists, locks/doors/activation, capture procedure, CLI contract.

## 7. Workstreams (parallel sessions)

Owner decision 2026-10-07: the rest of Layer 4 is split between sessions, each owning
distinct files and working in its own git worktree and branch, merging `main` before its
PR.

| # | Workstream | Branch | Owns | Delivers | Depends on |
|---|---|---|---|---|---|
| 1 | Collision geometry | `claude/layer4-collide` | `crates/fnv-collide`, `crates/fnvtool/src/collide_cmd.rs`, `tools/capture/oracle_collide.py` (+ test), `parity-lab/scenarios/layer4_collide.json`, `docs/spec-notes/collision.md` | Havok collision blocks of every mesh → engine-neutral shapes in game units; Havok scale measured; independent oracle and scenarios | `fnv-nif` (read only) |
| 2 | Physics and character controller | `claude/layer4-phys` | `crates/fnv-phys`, `crates/fnvtool/src/phys_cmd.rs`, `docs/spec-notes/physics.md`, the physics row of `dependencies.md` | rapier3d world from placed references + LAND terrain; ray and shape queries; kinematic capsule controller; dynamic clutter; deterministic stepping | 1's shape API (stub first), `fnv-world` |
| 3 | Simulation core | `claude/layer4-sim-*` | `crates/fnv-sim`, `crates/fnvtool/src/sim_cmd.rs`, `parity-lab/scenarios/layer4_sim.json`, `parity-lab/inputs/layer4/`, `docs/spec-notes/simulation.md`, this file (except §2.8 detail owned by 1/2 in their spec notes), README Layer 4 row | per-stack extra data, equip, encumbrance, game clock, respawn, enable-parent propagation, PGRE, schema gaps (IMOD, CHIP, CCRD, CMNY, NOTE), distribution comparator for leveled lists | — |
| 4 | Engine integration | `claude/layer4-play` | new Bevy crate `engine/nvf-play`; one registration line in `engine/nvforever` | player controller in the game, crosshair activation, container/inventory screen | 2 and 3; coordinate with the renderer owner of `engine/` |
| 5 | Static actors (added 2026-10-08) | `claude/static-actors-*` | `crates/fnv-actor`, `fnvtool actor`, `tools/capture/oracle_actor.py`, `parity-lab/scenarios/layer3_actors.json`, `docs/spec-notes/actors.md`, `engine/nvforever/src/actors.rs`, an additive part-assembly API in `engine/nvf-assets`, schema layouts HAIR/EYES/HDPT/RACE `INDX` | placed NPCs drawn as one entity each (skeleton + body, head, hair, head parts, worn armour), idle pose, no AI; FaceGen and creatures as later parts. Plan: docs/design/static-actors.md | 3 (leveled lists, inventories); coordinate with the renderer owner of `engine/` |

Retired scenario (owner decision 2026-10-07): `L4.sim.trace.goodsprings_respawn` — its
24 h case passed in game only because the save's merchant restock time was due (hidden
save state), and it exercised a merchant box rather than the 72 h cell respawn it was
meant for; replaced by `L4.sim.trace.goodsprings_merchant_restock`. A cell-respawn
scenario on a non-merchant Respawns container is still to be designed.

Retired scenarios (owner decision 2026-10-09): `L4.phys.walk.swim_cove_dive` and
`L4.phys.walk.swim_lake_shore` — their inputs swim at a constant 348 units/s, which the
game never does (the speed follows a stroke cycle averaging 339), and the shore's bank rows
were captured with the cell not loaded (travel by a long SetPos); replaced by
`L4.phys.walk.swim_cove_stroke` and `.swim_lake_exit` at new places, captured with the cell
loaded by `cow` (physics.md §6.6).
Also retired (owner decision 2026-10-09): `L4.phys.drops.swim_h2o` — its capture placed
the player while it still swam from the previous drop, which fresh probes cannot reproduce;
replaced by `L4.phys.drops.swim_h2o_fresh` (a fresh `coc` before each drop, four new points
around the swimming depth).

Workstream 3 runs as two sessions since 2026-10-07 (owner's split by agreement between
them): **3-core** (`claude/layer4-sim-core`: `crates/fnv-sim`, `crates/fnvtool/src/sim_cmd.rs`)
and **3-lab** (`claude/layer4-sim-lab`: Layer 1 schema layouts for the simulation,
`tools/capture/oracle_sim.py`, the `distribution` comparator in `parity-lab/fnvlab`,
`parity-lab/scenarios/layer4_sim.json`, `parity-lab/inputs/layer4/`, contracts and capture
procedures in docs/spec-notes/simulation.md, this file). Each merges the other's branch.

Rules: shared files get additions only — one workspace member, one `mod` + one dispatch
line in `crates/fnvtool/src/main.rs`, one pointer paragraph in `docs/cli-contract.md`
(the contract itself lives in the workstream's spec note). In-game captures are the
owner's (docs/spec-notes/simulation.md §6).

**Shape API between 1 and 2** (`fnv_collide::shape`, stub committed first on
`claude/layer4-collide`; field meanings documented in `docs/spec-notes/collision.md`):
`Shape` = box (half extents), sphere, capsule (two points + radius), convex hull
(points), triangle mesh (vertices, triangles, per-triangle material), compound
(children with transforms); `CollisionObject` = the NIF node it is attached to, motion
type, collision layer, mass, friction, restitution, the body transform and the `Shape`.
All lengths in game units, Z up, after the Havok scale is applied. Workstream 2 consumes
only this API, never NIF blocks.
