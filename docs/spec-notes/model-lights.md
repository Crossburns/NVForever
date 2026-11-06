# Lights animated by their model

Some LIGH records' models hold an `NiPointLight` whose colour a sequence of the model
animates: a lantern that flickers, an electric arc that flashes, a vending machine light
that blinks, the REPCONN klaxon whose light turns. This note says what the controllers
hold and how NVForever plays them. Tags as in CLAUDE.md. Found by the effects animation
audit ([effects-animation-audit](effects-animation-audit.md) §1, row
`NiLightColorController`) and the node audit ([nif-node-audit](nif-node-audit.md) row 8,
`AttachLight`).

**Sources** [observed]: `fnvtool nif decode` of the models below from
`Fallout - Meshes.bsa`; `fnvtool world refs --interiors` and `--world WastelandNV` over
`FalloutNV.esm` alone (no add-on read); `fnvtool esm decode` of the LIGH records and the
klaxon's script.

## 1. The controllers

Block layout [documented: nif.xml]: `NiLightColorController` (an NiPoint3 interpolator
controller) has a `Target Color` (`LightColor`: 0 LC_DIFFUSE, 1 LC_AMBIENT);
`NiLightDimmerController` drives the light's float `Dimmer`. Both target an `NiLight`.

What the files hold [observed]:

- Every light colour controller met has target colour 0 and flags 108 (0x6C: Active, cycle
  type clamp in bits 1-2, manager controlled, bit 5). None runs by itself: an embedded
  `NiControllerSequence` drives it, naming the light (`object0`) as the controlled
  block's node, controller type `NiLightColorController`, controller ID `Diffuse`. The
  interpolator in the sequence is an `NiPoint3Interpolator` over `NiPosData` keys,
  quadratic (interpolation 2) with forward and backward tangents.
- The keys are the light's whole diffuse colour, 0–1 per channel, as the light's stored
  `Diffuse Color` is (the first key equals the stored colour where that is not black).
  The stored dimmer stays as stored; one file (`monumentlight01.nif`) also has an
  `NiLightDimmerController` track whose keys stay at 1.0.
- The light is not a scene object of the rig: it hangs from a node (`Omni01`, `Omni05`,
  the `… NonAccum` node), sometimes under a node named `AttachLight`.

| Model | Sequence (cycle) | Colour keys | Moves the light? |
|---|---|---|---|
| `dungeons\caves\lamplight\lamplightwarm.nif` | `Idle`, loop, 3 s | 6 keys, red 0.62–0.78 (warm white), never off | No |
| `lights\fxlightelectricarc01.nif` | `Idle`, loop, 8.77 s | 44 keys, blue-white (0.47, 0.57, 1.0) flashing to black | `AttachLight01` turns 90° about its own origin (the light sits at that origin) |
| `lights\fxlightelectricarc02.nif` | `Idle`, loop, 8.03 s | 54 keys, same colours | No |
| `lights\nukacolavendlightblink.nif` | `SpecialIdle`, loop, 7.33 s | 17 keys, black to (0.56, 0.46, 0.46) blinks; stored diffuse black (−0.0015 in green and blue) | `AttachLight` turns 90° about its own origin |
| `dungeons\vault\roomu\klaxonglow.nif` | `Forward` clamp 0.067 s, `Left` loop 2.3 s, `Backward` clamp 0.3 s | Forward black → (1, 0.5, 0); Left held at (1, 0.5, 0); Backward to black. Stored diffuse black, dimmer 0.75 | Yes: `Left` spins `Dummy` once about z per 2.3 s; the light sits on `AttachLight`, 224 units out along x |
| `dlc05\dungeons\mz\accessories\dlc05klaxonglow.nif` | as the klaxon | blue (0, 0.55, 1) | as the klaxon |
| `clutter\questitems\monumentlight01.nif` | `Idle`, loop, 4.4 s | red 0.36–0.69, plus the dimmer track | No |
| `dungeons\office\lights\offrmlightblink01-04.nif` | one sequence each | grey blinks | — |

Placed in `FalloutNV.esm` as LIGH references [observed, 87]: `LamplightWarm` 14,
`LamplightWarmBrighter` 36, `LamplightWarmBrighter400` 3 (sewers, Quarry Junction
barracks, Vault 22), `KlaxonGlowRepcon` 17 (REPCONN HQ), `FXLightElectricArc01Fill` 5
(the Securitron vault), `FXLightElectricArc01FillSM` 2 and `FXLightElectricArc02FillSM` 1
(Vault 34, with enable parents), `NukaColaVendLightBlink384` 8 (BisonSteve02) and
`NukaColaVendLightBlink96` 1. The monument and office models are not placed as lights
there. All but the klaxon have a black DATA colour (world.md §8 "Black LIGH colours");
`KlaxonGlowRepcon` has (255, 174, 26), fade 1.3, and `LamplightWarm` the "Random Anim
Start" record flag.

**The klaxon's script** [observed: `KlaxonLightScriptRepcon`, 0x000E82EE, the LIGH's
SCRI]: a `state` (0 off, 1 flashing) set to 1 in game mode when the REPCONN HQ floor's
alarm (`RepconHQFreeform.1stFloorAlarm` …) is raised; while 1 it plays `playgroup left`
whenever `IsAnimPlaying` is 0, and on load. Nothing plays `Forward` or `Backward` there.

## 2. What NVForever does

- **Tracks** (`nvf_assets::anim_play::LightTrack`): a sequence's light colour track with
  controller ID `Diffuse` and its dimmer track are kept in the clip (`Clip::lights`) and in
  what plays it (`Playing::lights`, sampled at the sequence time like its other tracks).
  Ambient colour tracks are not kept (no file has one, and the engine draws no ambient
  colour of a light).
- **The rig** (`nvf_assets::model`): a sequence that drives the file's light counts as one
  that animates the model, so a lantern whose `Idle` only changes its light colour is
  drawn as a rig playing it (with a random start phase where the base has "Random Anim
  Start"). `PreparedModel::light` names the light, the rig node it hangs from and its
  transform there.
- **The light** (nvforever `lights.rs`): a LIGH reference's light whose model drives it
  gets `ModelDrivenLight`; when the model's rig is drawn (`world.rs`, `LightRig`), a point
  light is moved under the node the model hangs it from, so it moves with that node (the
  klaxon's turning light). Each frame its colour is the playing clip's diffuse key times
  the dimmer (its key, else the stored dimmer), clamped and read as display values like
  the DATA colour and added to the display-value lighting sum as it is (world.md §8
  "Light on surfaces in display values") [hypothesis, as the model colour of a black DATA
  colour, world.md §8];
  its brightness stays the record's fade.
- A light whose two colours are black but whose model drives it is now drawn (black until
  its keys light it): the Nuka-Cola blink light was not drawn before.
- Log: `model lights: <n> following their models, <m> coloured by a playing clip`.
  Smoke scenarios `model-lights-lantern` (the North sewers' lanterns) and
  `model-lights-klaxon` (REPCONN HQ with the first floor's alarm raised by the console:
  the five klaxons play `Left` and their orange light sweeps the walls in the frames)
  [observed 2026-10-23].
- A placed light's source carries its reference's `RefVisual` (`lights.rs` `spawn_light`)
  [design]: `RefVisual` sync hides it while the reference is disabled, by a script or
  through its enable parent, and when its cell loads again, and shows it once enabled;
  hidden, it lights nothing and `detection.rs` does not count it. One whose parent is the
  cell moves with its reference as the model does (`nvf-play` `moved.rs`). It is left out
  when a reference's model is swapped or its root sought (`NotLightSource`). Smoke
  scenario `lights-disable`.

## 3. Open questions

- **The klaxon at rest** [hypothesis]: while no group plays (alarm not raised), the light
  keeps its DATA colour, lit and still, at the node's rest place (224 units out). The
  keys suggest it should be dark (Forward starts from black, Backward ends black, the
  stored diffuse is black); a capture of REPCONN HQ before the alarm decides it.
- **Key colour versus DATA colour** [hypothesis]: a key replaces the DATA colour while a
  clip drives the light (the klaxon's keys are (1, 0.5, 0) × 0.75, its DATA colour
  (255, 174, 26)).
- **Not built**: lights spawned disabled (`spawn_dormant`, the Vault 34 arcs while their
  enable parent is off) keep a still colour, since their model is drawn later by
  `script_draw.rs`; spot lights a model drives keep their place (none in the base game).

## 4. Lights on an `AttachLight` node without a sequence

Placed LIGH references whose base has a model [observed 2026-10-27, `fnvtool world refs`
over `FalloutNV.esm`, interiors and WastelandNV]: six models. Five are the sequence-driven
ones of §1 (their lights move under their node once the rig is drawn, so the rule below
only sets where they start); the sixth is `clutter\barrel02firelight.nif` (226
references, every `Barrel02Fire*`, `Barrel03Fire1024`, `FireBarrelSwitching` and
`OVSewerBarrel02Fire*` light).

The fire barrel's model holds no light block, only an `NiNode` named `AttachLight` at
(−0.4, 0.8, 102.1), at the height of its flames, beside `AddOnNode14` (the
`MPSFireSmall01` flames, 72.5 up) [observed: `fnvtool nif decode`]. The record radius is
small (`Barrel02Fire128`: 128 units, colour (255, 159, 64), flicker).

**Rule** [hypothesis: the node's name and height; the GECK wiki export has no page on it]:
the reference's light sits on the model's `AttachLight` node (name `AttachLight` or
`AttachLight<n>`, any case), placed through the reference's own transform (scale
included), else at the reference's origin (`nvf_assets::model::attach_light`, nvforever
`lights.rs` `spawn_light`). Before the rule the barrel's light sat at the barrel's foot, so
a 128-unit light lit only the floor around it (owner report 2026-10-27: the Abandoned
Shack's barrel, `GSAbandonedShack`, gives off very little light).

**Reach** [hypothesis, fitted to the owner's frames of the original, 2026-10-27, five
frames of the shack]: in the original the barrel's warm light covers the wall behind it
and the floor round it out to about 190 units from the flames, steady across the frames,
while the record radius is 128. Flickering lights (Flicker, Flicker Slow) therefore reach
1.5 × their radius (`lights.rs` `FLAME_REACH`) and dim to 0.8 of their brightness at
most (`FLICKER_LOW`, 0.55 before). The room's own lighting stays as it is: the original's
interiors are brighter overall (lit floors), and the owner prefers ours (2026-10-27).
Smoke scenario `fire-barrel-light`. The capture plugin's screenshots failed three times
that day (`StretchBlt failed`, even with the game in front); the owner took the frames
by hand.
