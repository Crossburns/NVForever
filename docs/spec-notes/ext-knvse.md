# kNVSE's animation functions, second slice (2026-11-01, #1018)

The 16 kNVSE functions left after the first slice (docs/design/knvse-anim.md,
`crates/fnv-game/src/nvse_anim.rs`), built in `crates/fnv-game/src/nvse_knvse.rs`.
Sources: the GECK wiki export [documented: the page named], and the original game with
kNVSE installed [observed: probe set `knvse2` of `tools/capture/jip_probes.py`, lab
scenario `L5.ext.knvse2`; the first slice's `knvse-anim`]. No kNVSE source is read.

As in the first slice, the game keeps what scripts asked (`Game::knvse`) and answers
from it and from the clip the engine says each actor's body plays (`AnimState::clip`).
Paths compare without regard to case [decision: as the first slice, knvse-anim.md §2].

## 1. The clock and a sequence's time

| Function | NVForever |
|---|---|
| GetGlobalAnimTime `[first]` | what SetGlobalAnimTime gave plus the game seconds since; else the game seconds since the game first asked about the actor, that first frame counted [hypothesis: the clock starts with the actor's 3D; observed: knvse-anim and knvse2, `Ceil` of it reads 1 a few frames after placing an actor] |
| SetGlobalAnimTime `time [first]` | sets it, per view [documented]; the page's freeze example (`Set… (Get… − GetSecondsPassed)` each frame) holds the value |
| GetAnimCurrentTime `path` | the sequence's offset plus the actor's clock [documented: "the sum of the offset field … and … GetGlobalAnimTime"]; a sequence starting to play (PlayAnimationPath, ActivateAnim, the blends) gets the offset that makes its time 0 [observed: knvse2, under 0.1 the frame after PlayAnimationPath] |
| SetAnimCurrentTime `path time` | from the next frame, the offset that makes the current time `time` [documented; observed: knvse2, the same frame still reads the time from before, the next 1.5 and a frame] |

## 2. Weights, additive poses, text keys, attributes

| Function | NVForever |
|---|---|
| SetAnimWeight `path weight` | kept by actor and path, read back as `seqWeight` [documented] |
| SetAnimAdditive `path [first] [refPose] [refTime]` | 1 when the files exist, kept for the engine; else 0 [documented: "Returns 1 if the additive animation is successfully applied"; hypothesis: success is the files existing] |
| AddAnimTextKey `path time key` | one key more for every actor; 1, or 0 for a file that does not exist [documented: the page gives a Float return; hypothesis: its value]. The knvse2 probe cannot take its key off again (kNVSE 37 has no function for it); the change is in the probe game's memory only and the game quits after the set |
| SetAnimTextKeys `path times values` | the actor's own keys (no reference: every actor's); nothing while the actor plays the sequence [documented]; nothing when the lists differ in length [hypothesis] |
| GetAnimationAttributes `path` (GetAnimAttrs) | a string map of the 13 keys with no reference; on an actor, 22 keys (9 more, `lastTime` … `multiplier`) when it plays the sequence, else ar_Null [documented; observed: knvse2, 13 keys with no reference, 22 for a playing sequence, ar_Size −1 for one the actor never played] |
| GetPlayingAnimBySequenceType `type` (GetAnimByType) | the attributes of the sequence of that type the actor plays, else ar_Null [documented]. The body's clip is Idle (0) for the Idle group and Movement (1) for any other locomotion group [hypothesis] |

The attribute values the engine has not told read as defaults: the file's stem as
`sequenceName`, `frequency` 1, `beginKeyTime`/`endKeyTime` 0, `cycleType` 0 (loop),
`accumRootName` "Bip01", `animGroupId` −1, `keyType` 1 (0 for a clamped sequence)
[hypothesis]. The engine reads every sequence file it loads and can tell the real ones
through `Game::set_sequence_info` (follow-up: not wired yet). On a playing sequence
`state` is 1 (Animating), `offset` SetAnimCurrentTime's, the times the current time,
`multiplier` 1 [documented: the State list; hypothesis: the rest].

## 3. Blends and the rest

| Function | NVForever |
|---|---|
| CrossFadeAnims `source dest …` | 1 when the source plays and the destination does not (and exists): the source stops and the destination plays at `fWeight` (default 1); else 0 [documented: "a convenience function that calls DeactivateAnim on the source … and ActivateAnim on the destination"] |
| BlendToAnim `source dest [first] [duration] [destFrame] [sourceWeight] [destWeight] …` | 1 when the destination exists, also with a source that does not play [observed: knvse2; the page says the source "must be an actively playing animation" and that 0 means the blend could not start]; the destination plays at `destWeight` (default 1) from `destFrame`; a playing source keeps `sourceWeight`, stopping at 0 [hypothesis: the default source weight 0] |
| BlendToAnimFromPose `path [first] [duration] [destFrame] …` | the sequence plays from `destFrame`; 1 when the file exists [hypothesis] |
| CopyAnimationsToForm `from to` | SetActorAnimationPath's and SetWeaponAnimationPath's overrides of `from` added to `to`, both views [documented; hypothesis: added to the target's own] |
| AllowAttack | the actor's anim action set to 0 [documented: "Sets the current Anim Action … to 0"]; the call waits for the engine (`Game::take_allow_attack`). Not in the installed kNVSE 37 (a runtime compile of it fails) [observed: knvse2's first run] |
| ForceFireWeapon | the equipped weapon fires as FireWeapon does (`CombatRequest::Fire`) [documented] |

## 4. Probe results and follow-ups

Probe set `knvse2` (31 rows, in the game with kNVSE 37 on a placed copy of Doc's NPC;
golden `jip/knvse2.json`, private), lab scenario `L5.ext.knvse2`: 29 of 31 rows agree
headless (`fnvtool ext queries`). The two that differ need the engine, which the headless
run does not have:

- `k2.attrs.cycle`: the smile idle's KF says clamp (2); headless the file is not read,
  the default 0 stands (the engine's `Game::set_sequence_info` would tell it).
- `k2.bytype.idle`: the actor's body plays its idle (22 keys); headless no body clip is
  told (`AnimState::clip`), so no sequence of type Idle plays (ar_Null). The first
  slice's body-clip rows (`kn.bygroup.idle` …) differ headless for the same reason.

A first run's `Floor (time × 10)` the frame after SetAnimCurrentTime 1.5 read 16 (the
game's frames were slow); the row keeps whole seconds.

Follow-ups (engine side, slice 2 of knvse-anim.md): the engine's animation player does
not yet read the weights, offsets, global time, additive poses or text keys kept here,
take AllowAttack, or tell the sequence headers (`Game::set_sequence_info`).
