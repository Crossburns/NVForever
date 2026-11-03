# Spec notes: doors that open in place

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525), **[hypothesis]**
needs verification. Counts come from `fnvtool sim door-models` (fnv.sim.door_models/1)
over FalloutNV.esm and the five DLC masters (DeadMoney, HonestHearts, OldWorldBlues,
LonesomeRoad, GunRunnersArsenal), models read from the default archive order.

Design: docs/design/navmesh.md §6.1 (doors and actors following paths). Code:
`fnv_collide::anim` (sequences, node poses, `DoorMotion`), `fnv_phys::PhysWorld::pose_ref_object`,
`nvf_play::doors`, nvforever `doors.rs`, `fnv_nav` (doors on paths).

## 1. What the data holds

- **DOOR record** [documented: xEdit wbDefinitionsFNV.pas DOOR, schemas/fnv/objects.json]:
  MODL model, SNAM open sound, ANAM close sound, BNAM looping sound (all SOUN), FNAM
  flags (bit 1 Automatic Door, 2 Hidden, 3 Minimal Use, 4 Sliding Door). Read by
  `fnv_world::placed::door_record`.
- **FNAM** over the 474 DOOR bases: 0x00 on 364, Sliding Door alone on 93, Minimal Use on
  13, Hidden on 2, Minimal Use + Sliding on 1, Hidden + Minimal Use + Sliding on 1
  [observed]. `UtlDoor01` (0x0003940E), a hinged-looking utility door, has Sliding Door
  set [observed]; what the flag changes in the game is not known.
- **REFR**: XTEL makes a door a teleport door (it leads elsewhere); without it the door
  opens in place. ONAM "Open by Default"; XLOC lock data [documented: xEdit REFR].
  `fnv_world::Placement::teleport` and `open_by_default` read them.
- **Models**: of the 474 bases (2 name a model missing from the archives:
  `PrisonMarker`, `CaveDoor`) [observed]:

  | Sequences in the model | Bases | Placed refs | Of them without XTEL |
  |---|---|---|---|
  | Open, Close | 263 | 2,347 | 1,844 |
  | none | 175 | 1,015 | 202 |
  | Backward, Forward, Left, Right (force-field effects: `NVMonorailDoor`, `VFSVegasGate`, …) | 29 | 64 | 2 |
  | Backward, Close, Forward, Open (`NVDLC03ForceFieldDoor*`) | 3 | 28 | 28 |
  | Backward, Forward (`ElevatorControl01`, `NVDLC01Gala02ElevatorControl`) | 2 | 3 | 3 |

  The models without sequences that stand in cells are frames and static doors
  (`OffDoorWoodFrameStatic`, `HotelDoor01`, `BlackDoor`, ladders) [observed]; they
  cannot be animated and are drawn as before.
- **Sequences** are NiControllerSequences in the model (no separate `.kf`). Each has
  NiTextKeyExtraData keys `start` and `end` and usually one or more sound keys
  [observed]. In all 658 door sequences with an `end` key it is at the stop time
  [observed]. Sound keys read `sound: <SOUN editor ID>` or `Sound: <editor ID> <node>`
  (586 keys; e.g. `sound: DRSMetalUtilitySmallOpen`, `Sound: DRSVaultGearOpenSteam01
  SoundNodeSteam`) [observed]; the trailing name looks like the node the sound comes
  from [hypothesis]. NVForever plays them as the door passes them (audio.md §10).
- **Lengths**: "Open" is most often 1.00 s (102 models) or 1.67 s (80), "Close" 0.97 s
  (101) or 1.67 s (74); the range is 0.03–25 s (vault doors are long) [observed].
  `UtlDoor01`: both 1.67 s, 47 tracks [observed].
- **Cycle type** clamp (2) on 265 "Open" and 257 "Close", loop (0) on 1 and 9
  [observed]. NVForever plays door sequences once and holds the end whatever the cycle
  type [engine choice].
- **Collision**: door models carry 652 keyframed rigid bodies (644 on layer 2
  AnimStatic, 8 on layer 28) and 288 static ones (frames, mostly layer 1) [observed].
  For the 610 keyframed bodies on Open/Close models, posing the model with "Open" at its
  end moves every one of them, and "Close" at its end equals the stored (bind) pose for
  606; the 4 bodies of `Lucky38ExtDoorDUPLICATE000` differ [observed]. So the stored
  pose is the closed door, and keyframed collision is meant to follow the animated nodes
  [hypothesis from the motion type, consistent with these poses].
- **Locks**: 244 placed doors without XTEL carry XLOC (241 on Open/Close models); 203
  carry ONAM [observed].

## 2. Behaviour implemented, and its status

- A door without XTEL whose model has Open and Close that move its nodes is drawn with
  its node hierarchy and posed from its state; teleport doors stay static.
- Activating it (E) toggles it when it is not locked or the actor carries the key; a
  locked door says so [observed for containers, simulation.md; doors per the GECK wiki,
  as fnv-sim already did]. Lock data means locked at the start [hypothesis, fnv-sim].
  The door's clock follows the simulation's open flag, so a script's `SetOpenState` or
  `Activate` moves it too (the Novac motel bathroom door 0x000CD3F1 has an OnActivate
  script, SCPT 0x001777EC, whose `Activate` toggles it [observed in NVForever's runtime]).
- **A door flagged Open by Default starts open** in the simulation and is drawn and
  collides open [hypothesis from the flag's name; a gate so flagged let the player drop
  through it, physics.md §4]. The shared collision rule `fnv_phys::ref_collision` still
  leaves such a door's moving parts out (no animation there); the game poses them instead
  (`RefCollisionOptions::posed_doors`).
- A door told to close while opening (or the reverse) turns back from the same fraction
  of the other sequence [engine choice; not observed].
- While a door swings (opening or closing), its moving parts are not solid: the leaf
  passes through actors instead of hitting or pushing them, and rays miss it; at rest,
  open or closed, it is solid again [engine choice, owner request 2026-10-09; whether
  the original game's swinging doors push actors is not known]. The frame and other
  static parts stay solid throughout.
- A sequence counts as finished at its `end` text key [engine choice from §1].
- Sounds: NVForever has no sound playback; the sound keys are logged when passed.
- Actors on paths: they stop in front of the door, open it and wait until the Open
  sequence has ended, then walk through square to the door (docs/design/navmesh.md §4,
  §6.1) [owner decision for the waiting; the rest engine choices].
- `GetOpenState` still reports only 1 (open) and 3 (closed); the GECK wiki's opening
  and closing codes are not produced yet.

## 3. Open questions (not answered by the data)

1. Do doors close by themselves (behind an actor, after a time)?
2. How close must an NPC be to open a door: does it have to touch it, or does it open it
   from a distance as it approaches?
3. What "Sliding Door" changes, given that hinged-looking doors carry it.
4. Whether NPCs wait for the open animation to finish in the original game (NVForever's
   followers do, by owner decision 2026-10-09).

## 4. Later: solid again only once the space is clear (not built; to test)

A swinging door's moving parts are not solid (§2), and they become solid again the
moment the sequence ends. An actor standing where the leaf comes to rest (in the doorway
when it closes, in the leaf's open position when it opens) is then inside a solid body:
the character controller may stick or be pushed out. NPCs following a path wait clear of
the door (docs/design/navmesh.md §6.1), so this concerns the player and NPCs that stand
in a doorway for other reasons.

Proposed logic (owner request 2026-10-09, for later testing): when a sequence ends, keep
the moving parts non-solid while any character overlaps them at their rest pose (an
overlap query per keyframed collider against character capsules, each frame); make them
solid once none does. The door looks closed (or open) meanwhile. Tests to write: a
character standing in the doorway while the door closes is not pushed or stuck, and the
door blocks once the character has stepped out; the same for a character standing where
the open leaf lands; a door with nobody near is solid on the first frame at rest. Open
question for captures: what the original game does when a door closes on an actor (stops,
reopens, pushes, passes through).

## 5. NPCs at scripted doors (built 2026-10-25, `fnv_game::npc_doors`)

### 5.1 What the doors' scripts do for an NPC [observed: survey of the ten masters]

The survey decompiled the script of every DOOR base that has one (`fnvtool script
decompile` over the ten masters; base forms and placements read from the plugins) and
followed each OnActivate block with the action reference an NPC: tests that it is the
player taken as false, every other condition as either.

| Doors (of 214 with a script) | What an NPC gets |
|---|---|
| 15 | no OnActivate block (HVLevel1To2Door, vCCHQDoor among them) |
| 12 | `Begin OnActivate player`: the block is not for it (IndDoorSmAnim01, REPCONN's basement doors and ladder) |
| 111 | the block activates the door for it (`if IsActionRef player … else activate`, the GECK wiki's own pattern) |
| 34 | depends on the game's state (a quest stage, a variable, the actor's faction) |
| 42 | never activated for it: 11 in FalloutNV.esm (VStripGate, GomorraElevatorDoor, V22ElevatorDoor, V22BathroomScareDoor, SVComputerRoomDoor02, NVMonorailDoor, NVCCBarge, NVCCInvisibleFortGate, NVCCRadTruckDoor, VHDExteriorLadder and its fake), 31 in the add-ons (most of them the doors back to the Mojave) |

Some blocks look after NPCs themselves: VHDExteriorLadder moves an NPC to the ladder's
linked marker (`MoveToMarkerWithFade`), GomorraElevatorDoor moves the player's teammates
to the player.

### 5.2 The rule

- The original fires the activation event for NPCs walking through doors, in unloaded
  cells too [observed: xNVSE OnActivate events, script-function-coverage.md §3.3].
- A block runs instead of the default activation; the door opens only when the block
  calls `Activate` [documented: GECK wiki Activate, IsActionRef]. Applied to an NPC's walk
  through the door as to the player's hand [hypothesis: no capture saw an NPC held].
- NVForever: as an actor the AI walks comes within its step and 96 units of the next door
  of its way, the door's OnActivate block runs with the actor as the action reference
  (GetActionRef, IsActionRef). A block that calls `Activate` for it lets it through: the
  AI moves it and opens the door as for a door without a script, and the door's activate
  children go off. A block that does not, on a load door (XTEL), lets it through all the
  same: about 420 actor/package pairs of FalloutNV.esm walk through the Strip gates and 15
  through Gomorrah's elevator doors, whose blocks activate for the player only [observed:
  `fnvtool ai reach` `routes_through_door`; hypothesis: the original moves them]. On a door
  that opens in place, a block that does not closes the door to that actor's paths for 120
  game seconds (an hour at TimeScale 30) and its package is taken up afresh: another way,
  or the package set aside as out of reach as for a locked door. A door asked and passed
  is not asked again by that actor for 10 s. A `Begin OnActivate player` block does not
  run for an NPC and does not hold it [hypothesis].
- The Strip's packages "through" the gates (observer check 21, 2026-11-01, o19_strip on
  nvfstrip) [observed]: the gate doors pair across walls of the one Strip worldspace at
  x about 0 (0016A14D/14E at y 5284/4656, 0016A14F/150 at y -836/-1615). With the player
  moved to the Embassy's patrol start (south of both gates) for 1.5 game hours, the NCR
  MP B (VStripNCRMPBREF, placed north at y 8547) kept package 00151E3E, did not move and
  was never in a loaded cell; he never took VStripMPBPatrolEmb nor came south. With the
  player moved north he took VStripMPABPatrol38 and patrolled the north section, no gate
  crossed. VStreetBot03REF stood north already (no crossing to see). The quest variable
  that picks those patrols (VStreetFluffNPC iPlayerInArea) could not be read: the
  observer's runtime compiler does not take a quest's Editor ID. So the original moved no
  MP through a gate in these runs; whether ours would route him through depends on the
  same variable (open in observer-checks.md item 21).
- Not saved: after a load the doors are asked again.
- Open, for a capture: whether the original's NPCs wait at a refusing door, take another
  way or never plan through it; whether a refusal is remembered.
