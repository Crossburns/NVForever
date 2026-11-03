# kNVSE animation paths in NVForever (N5)

kNVSE lets scripts swap the animation files actors and weapons play, and play sequence
files directly. NVForever's mod lists call SetActorAnimationPath about 2,150 times
(docs/handoff/nvse-next-batch.md). This note says how it fits the engine. Facts are from
the kNVSE pages in the wiki export `[documented]` and the peer's knvse-anim set in the
original game (kNVSE 37) `[observed]`. No kNVSE source is read.

## 1. What the original does [observed: knvse-anim]

- A sequence is named by its path under `meshes`, in the spelling it was first loaded
  under, and lookups compare it exactly: `IsAnimSequencePlaying` with
  `characters\_male\locomotion\mtidle.kf` reads 0 while the game plays
  `Characters\_Male\Locomotion\mtidle.kf`. A sequence a script loads keeps the
  script's spelling.
- `GetPlayingAnimSequencePaths` gives 8 slots, the body's main clip first.
- `SetActorAnimationPath first enable path` returns 1 when the file exists, 0 when
  it does not; disabling returns 1. The replacement applies to the group the file's
  sequence belongs to (`h2hidle.kf` is H2HIdle, so Idle is unchanged).
- `PlayAnimationPath` plays at once: `IsAnimSequencePlaying` reads 1 in the same frame.
  `ActivateAnim` adds a sequence to the active ones.

## 2. The split

- **Game side (`fnv_game::nvse_anim`, slice 1, built):** what scripts asked (the
  replacement paths by actor or weapon and by view; the sequences scripts played) and
  the getters, answered from what the engine tells: the clip each actor's body plays
  (`AnimState::clip`, a path under meshes) and its movement group.
- **Engine side, slice 1 (built):** `nvforever` anim_state.rs tells each NPC's clip
  from its locomotion set.
- **Engine side, slice 2 (built for locomotion, `nvforever` anim_paths.rs):** an actor
  with replacement paths plays from its own copy of the standing set, a file whose stem
  is `mt` + a move's group taking that move; taking the paths back restores the shared
  set. Still to build: the idles, script-played sequences and first person. The original
  plan: the locomotion and idle code asks the game for
  an actor's replacement of a group before taking the set's clip: a per-actor clip
  set made from the shared one with the replaced moves (`Locomotion::set` is an
  `Arc<ClipSet>` shared per skeleton and body, so a replacement clones it for that
  actor), and a script-played sequence goes through the idle player as a special idle.
  The player's first-person clips the same way. The group of a replacement file is its
  sequence's name (`h2hidle.kf` → H2HIdle), read when the file is loaded.
- **Spelling:** NVForever's paths do not keep the original's spelling, so the game side
  compares paths without regard to case [decision; the original compares exactly].

## 3. The second slice (2026-11-01)

GetAnimationAttributes, Get/SetGlobalAnimTime, the blend functions, SetAnimWeight,
SetAnimAdditive, the text-key functions, CopyAnimationsToForm and the rest are built on
the game side (`nvse_knvse.rs`, docs/spec-notes/ext-knvse.md). The engine's per-actor
player does not read their state yet.
