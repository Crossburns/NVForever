# Destruction stages and a model's damage-stage nodes

What a destructible object's data says about how its model looks as it is damaged, and
what NVForever draws from it. Combat's side (health, stage thresholds, explosions,
self-damage) is in `combat-rules.md` (step C7); this note is about the model.

## 1. The records

- **DEST** on the base form: health, stage count, flags; then per stage a **DSTD**
  (health percent, index, *damage stage*, flags 1 cap damage / 2 disable / 4 destroy,
  self-damage per second, explosion, debris, debris count), optionally a **DMDL**
  replacement model, ended by **DSTF** [documented: xEdit wbDEST; read in
  `crates/fnv-game/src/data.rs` `DestructionStage`].
- A **STAT** carries no DEST [observed: `Motorcycle01Static` below; NVForever's STAT
  decode has none]. Its references cannot be damaged, so they stay at damage stage 0
  [hypothesis].
- `Motorcycle01` (MSTT `00077B40`): health 200; stages (percent → index / damage stage):
  93 → 0/0, 89 → 1/1 (self-damage 2), 83 → 2/2 (self-damage 5, explosion `000179DA`),
  75 → 3/3 (self-damage 5), 65 → 4/4 (cap damage + destroy, explosion
  `MotorcycleExplosion` `00077B42`, DMDL `Vehicles\Motorcycle01Hulk.NIF`)
  [observed: `fnvtool esm decode --formid 0x00077B40`, FalloutNV.esm].
- `Motorcycle01Static` (STAT `001088F5`) uses the same model `vehicles\motorcycle01.nif`
  with no DEST [observed: same tool].

## 2. The model's nodes

- `BSDamageStage` derives from `BSBlastNode` and `BSRangeNode`, whose fields are `min`,
  `max` and `current` [documented: nif.xml `inherit` chain and BSRangeNode fields].
- In `motorcycle01.nif`: `Motorbike` > `DamageStage1-3` (BSDamageStage, min 1, max 3,
  current 0) > `AddOnNode2` (BSValueNode, value 2, extra data `billboardBSUp`); also
  `BlastNode2` (2-2) and `BlastNode4` (4-4) with no children [observed:
  `fnvtool nif decode`, Fallout - Meshes.bsa]. In every damage-stage node looked at the
  name repeats the range (`DamageStage<min>-<max>`) [observed].
- ADDN with node index 2 is `MPSSmokeMed01` (`MPS\MPSSmokeMed01.NIF`); index 0 is
  `MPSFireMed01` [observed: ADDN DATA, FalloutNV.esm].
- `motorcycle01hulk.nif` (the stage-4 DMDL) has two `DamageStage3-5` nodes, each with
  `AddOnNode0` (fire) holding `AddOnNode2` (smoke), and `BlastNode05` (5-5) [observed].
- **Meaning** [hypothesis]: the children of a `BSDamageStage` are the model's look while
  the reference is at a model damage stage from `min` to `max`, the number being the
  DSTD's damage stage (0–8) of the destruction stage reached; at any other stage they are
  not shown. So a `Motorcycle01` starts smoking when damage takes it to 89 % health,
  smokes through 75 %, and on destruction becomes the burning, smoking hulk; an intact
  bike, or any `Motorcycle01Static`, never smokes. This fits the owner's report from the
  original (2026-10-20): the bikes outside the Prospector Saloon do not smoke there.

## 3. The bikes at the Prospector Saloon

- Three references of `Motorcycle01Static` in front of the saloon (WastelandNV cell
  `000DAEB9`, grid -17,0): `00106341` (-67727, 3093, 8369), `00106342`
  (-68101, 3177, 8354), `0010750C` (-67955, 3115, 8352) [observed: `fnvtool world refs`].
- Cause of the always-on smoke: placed models' add-on nodes started emitting in
  `world::spawn_emitters` / the rig path of `world::spawn_model` (branch
  `claude/gomorrah-dance-flames`, PR #530), every `BSValueNode` at once, including
  `AddOnNode2` under `DamageStage1-3` [observed: on main, where no placed model emits,
  a frame of the three bikes at noon shows no smoke].

## 4. What NVForever does

- `nvf_assets::model::ModelNode::damage_stages` holds a `BSDamageStage`'s `min`/`max`;
  `damage_stages_of` gives a node the overlap of the ranges above it (an unread range:
  1 and up [design], so it is never shown intact).
- `fnv_game::Game::model_damage_stage` is the DSTD damage stage of the stage reached (0
  undamaged, without DEST, or after ClearDestruction).
- A placed add-on node below a damage-stage node becomes a `particles::StagedAddon`: its
  emitter is switched on while the reference's model damage stage is in range and off
  outside it, checked every frame. Log lines: `damage stage: <ref> add-on node <n> waits
  for model stages <lo>-<hi> (at <s>)`, `… on at model stage <s>`, `… off at model stage
  <s>`.
- Since 2026-10-25 (`engine/nvforever/src/destroyed.rs`):
  - **Replacement model**: a reference whose destruction stage reached one with a DMDL
    is drawn with the DMDL of the highest such stage reached
    (`Game::destruction_model`) [hypothesis: it stays through later stages without one];
    its drawn entities are replaced at once, and the streamer keeps the model so the cell
    drawn again, or a loaded save, draws the hulk. ClearDestruction brings the intact
    model back. Log `destroyed: <ref> drawn with <model> (model damage stage <n>)`. Seen:
    the bike `00157C97` burns down to `Vehicles\Motorcycle01Hulk.NIF`, and its explosion
    takes the truck, the street cleaner and the car beside it to their hulks (smoke
    `bike-smoke-damaged`).
  - **Shapes below a damage-stage node** of a model drawn without a rig of its own are
    drawn only at the node's stages (`StagedShape`), as the add-on nodes are.
  - **Debris**: each stage reached that names a DEBR throws its count of pieces, each
    model picked by its DEBR percentage, from the object's middle as gore pieces are
    thrown (`dismember.rs` `throw_object_debris`) [design: speeds and spin]. In the
    masters 18 activator and 12 container stages name debris [observed: `fnvtool esm
    fields`]; not yet seen in a game run.
- Since 2026-11-02 (issue #1410, `engine/nvforever/src/wrecks.rs`):
  - **A hulk's loose parts.** A player blew up a car and its doors, hood and debris hung
    in the air. `vehicles\carhulk01.nif` (DMDL of `Car01` `00014E78`, last stage 65 %,
    Cap Damage + Destroy) holds six dynamic bodies (motion system 4, quality 3) and no
    constraint: the chassis (layer 20, 300 kg), `CarLeftDoor01`, `CarRightDoor01`,
    `CarHood01`, `CarTrunk01` (layer 19, 300 kg) and `CarPort01` (80 kg). Every node has
    the identity rotation and the parts are modelled where the blast throws them: the
    doors 113 units out to the sides, the hood raised [observed: `fnvtool nif decode`,
    owner's install]. The intact `car01.nif` has one dynamic body (layer 10, 600 kg).
    Of the 28 DMDL models of the masters, 18 have two or more dynamic bodies and no
    constraint: every vehicle hulk (cars 01-03 and 10, trucks 01-06 and 08, the
    motorcycle, the street cleaner, the bus `FFEU08FallingObject` with 29, the
    vertibird with 49), the baby carriage, grenade bouquet and mailbox bomb traps, the
    electric box, the fire extinguisher and the fusion generator; the other 10 have
    none or one [observed: the DMDL subrecords of the ten masters, each model's bodies].
    [hypothesis: the original hands these bodies to Havok when the hulk swaps in, and
    the parts fall; not seen in the original yet.]
  - **Collision.** A destroyed reference collides as its destruction model
    (`nvf_play::phys::PhysState::set_destruction_models`, `RefCollisionOptions::model`):
    built again when the stage is reached and whenever its cell loads, held where it is
    drawn (`hold_jointed`, physics.md §4.3.1). Log `phys: 0x<ref> collides as its
    destruction model <DMDL>`. ClearDestruction brings the base model's back.
  - **The parts fall.** A model with two or more dynamic bodies and no constraint is
    `PreparedModel::loose`; drawn as a reference's destruction model it gets its own
    nodes, and once its collision is in, its bodies become an object ragdoll without
    joints at their nodes' poses, the held collision stops taking part, and each moving
    body moves its node (as a swinging trap's, traps.md §4). They start still; the
    stage's explosion pushes them as it pushes any loose object. Calm as a corpse is
    (net motion under 12 units/s and 0.6 rad/s for 1.5 s, ragdoll.md §4 "Rest"), or
    still moving after 20 s, the wreck is held at rest where it lies until a blast or a
    push rouses it: the car's hubcap (`CarPort01`, a thin 15-sided disc) otherwise
    rocked on its edges for 45 s without the physics engine calling it asleep. Log
    `wrecks: <ref> <model>: <n> parts fall (<m> moving)`, `wrecks: <ref> came to rest
    after <s> s; farthest part <node> <d> units from its start (<dz> in height)`.
    [observed, smoke `wreck-car-parts-fall` and `wreck-parts-fall`, 2026-11-02: the
    lone `Car01` `0013DA2F` and the two cars its blast destroyed came to rest in 1.6 to
    2.0 s, no part farther than 322 units from where it started (the hood); at the
    Goodsprings wreck pile the motorcycle's seat was thrown 3012 units by the
    neighbouring wrecks' explosions, the other parts within 511.]
    [design: they do not hurt actors; drawn again (cell reload, a loaded save) they fall
    again from the file's poses, where they came to rest is not kept.]
- Not built: a model's own particle systems below a damage-stage node are not gated;
  shapes below one in a model drawn as a rig (an idle, a door); blast nodes
  (`nif-node-audit.md` item 3); creatures' destruction stages (76 stages with debris, the
  robots').
