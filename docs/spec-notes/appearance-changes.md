# Spec notes: race, sex and level changes by script; the power armour gate

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was read from the owner's install (FNV 1.4.0.525, the masters' records and
compiled scripts, read privately with `fnvtool esm decode` and `fnvtool script decompile`;
nothing quoted beyond names and FormIDs), **[hypothesis]** needs verification, **[design]**
is NVForever's choice. Sources: the function catalogue (`schemas/script/functions.json`,
the GECK wiki's AgeRace, SexChange, SetLevel and SetPCCanUsePowerArmor pages as
summarised there), xEdit's record definitions (`schemas/fnv`).

Code: `fnv_game::appearance` (AgeRace, SexChange, MatchFaceGeometry), `fnv_game::functions::actor::set_level`,
`fnv_sim::World::set_power_armor_training` and `World::equip`, `fnv_game::functions::
player::refuse_power_armor`; the engine's `appearance.rs` (drawing anew), `player_body.rs`,
`nvf_play::creator::preset_look`, `fnv_actor::Resolver::resolve_changed`. Tests:
`crates/fnv-game/tests/appearance.rs`. Smoke: `agerace-sexchange-npc`,
`sexchange-player-reset`, `setlevel-npc-reroll`, `power-armor-training`. Capture jobs:
`tools/capture/probes/actor_changes.toml` batches ac1–ac3, run 2026-10-18 on nvftest: every
answer of ac2 (AgeRace, SexChange) and ac3 (power armour) matched NVForever's; ac1 showed
that SetLevel keeps the inventory (§3).

## 1. Who calls them [observed]

Every call in the ten masters is on the player (`player` = 0x14):

- **AgeRace**: VCG00 (the opening, stage 5) calls `player.AgeRace -1` then
  `player.AgeRace 1`.
- **SexChange**: VCG01 (the opening at Doc Mitchell's) and the FO3 leftovers CG00 ask the
  player's sex in a message box and call `player.SexChange male 1` / `female 1` (reset);
  six dialogue lines' end scripts call `player.SexChange male` / `female` without reset.
- **SetLevel**: the press-demo quest's script and Doc Mitchell's "revise your character"
  activator (VCG04ActivatorScript: `player.SetLevel 1`, then ResetXP and RemovePerk).
- **SetPCCanUsePowerArmor 1**: only the Generic quest's stage 80.

So the NPC cases below are built for mods and the console; the quests need the player's.

## 2. AgeRace and SexChange

- **RACE links** [documented: xEdit RACE ONAM "Older", YNAM "Younger"]. In FalloutNV.esm
  the four playable races link adult ↔ child (YNAM/ONAM) and adult ↔ old (ONAM/YNAM),
  both ways; the four `…OldAged` races link back to Caucasian only; Ghoul and the raider
  races have none [observed].
- **AgeRace n** takes |n| steps along the links (older for n > 0) and stops at the end of
  the chain [documented: catalogue notes "overshooting clamps at the oldest or youngest"].
  A race without links stays [documented]. So VCG00's -1 then 1 leaves the player's race
  unchanged [observed: the links are symmetric].
- **SexChange [sex] [reset]**: no sex flips it; no calling reference is the player
  [documented]. Reset applies only on the player and only when the sex really changes
  [documented: catalogue notes]: the face becomes "the default preset for that race and
  gender" [documented], which NVForever takes to be the character creator's first
  starting face for that race and sex, with its hair, eyes and head parts
  [hypothesis].
- **What is drawn**: the new race's head, body and its FaceGen with the actor's own
  values [documented: AgeRace notes "the actor takes the default face textures of the new
  race"]; an NPC's hair that the new race does not list (RACE HNAM) or that is not for the
  new sex (HAIR Not Male / Not Female) gives way to the race's default hair for the sex
  [documented: "invalid hair falls back to default"; hypothesis: what makes it invalid].
  The player's hair is kept [hypothesis; the wiki: "restored on return for the player
  only"].
- **Kept**: on an NPC's reference (saves keep it); on the player in the creator's look,
  or before the creator in the player's own race and sex fields [design]. GetIsRace,
  GetIsSex, GetPCIsSex, IsChild and the race's abilities read the changed values.

## 3. SetLevel

- On the player it changes the level number only, not the stats [documented: catalogue
  notes]; nothing else changed here.
- On another actor it sets the level, with the level-to-player flag as an offset from the
  player's within min and max [documented]: player level 1, `SetLevel 2 1 5 20` gave 5
  [observed: capture ac1, Joe Cobb]. **The inventory stays** [observed: ac1, 2026-10-18:
  three Stimpaks given to Easy Pete were still carried after `SetLevel 7 0`, and his item
  count was 8 after it and after a second `SetLevel 7 0`]. The catalogue's note that it
  "regenerates the inventory, re-evaluating leveled lists, even if the level did not
  change" did not show within a second of the call; whether it happens later (when the
  actor's 3D or cell reloads) is not captured.

## 4. Power armour

- **The flag**: SetPCCanUsePowerArmor sets "the flag that lets the player use power
  armor"; the Power Armor Training perk is a display dummy [documented: catalogue notes].
  The perk's only effect is Quest + Stage, Generic stage 80, whose script calls
  SetPCCanUsePowerArmor 1 [observed]. Off at the start [hypothesis: nothing else sets it].
- **What it gates**: armour with BMDT general flag "Power Armor" [documented: xEdit]. The
  NCR salvaged power armour lacks the flag and carries a dummy enchantment named "No Power
  Armor Training Required" [observed], so it is worn freely. Only the player is held back
  [hypothesis: NPCs in power armour are placed wearing it].
- **The refusal**: the masters' message `PowerArmorMessage` (MESG 00058FEF, a message box:
  "You lack the specialized training needed to move in Power Armor.") [observed]; shown
  when the Pip-Boy or a script's EquipItem tries [hypothesis], as a box that waits for the
  Pip-Boy to close (`Game::show_box`); the menu's log line says why too.

## 5. MatchFaceGeometry (Fallout 3's)

- **Who calls it** [observed: Tale of Two Wastelands 3.4's masters, decompiled privately]:
  Fallout3.esm's Vault101ExitScript (SCPT 0x0605CF93) and CG04 stage 65 (QUST 0x0001F388)
  on Dad, after `MatchRace player`, with a global as the percentage
  (`MatchFaceGeometry player CGMatchFace`). The New Vegas masters never call it.
- **What it does** [documented: GECK wiki MatchFaceGeometry]: the calling actor's face
  geometry moves `percent` (0–100) of the way from its current face to the source's;
  100 is the same face; the face texture is not changed.
- **How** [hypothesis]: the geometry is the FaceGen shape coefficients, symmetric (FGGS,
  50) and asymmetric (FGGA, 30) [documented: xEdit wbFaceGen]; each coefficient moves in
  a straight line, a coefficient one face lacks counting as 0
  (`fnv_game::appearance::blend_coefficients`). Calls on one actor apply in order.
- **Kept** [design]: on an NPC each call is kept on the reference (`RefExtra::
  face_matches`, saved) with the source's coefficients when the game holds them (the
  player's, from the character creator), so the face matched is the one the source had
  then; another source's are read from its record when the NPC is drawn. The engine
  keeps the morphs for the resolver (`ActorWorld::faces`) and draws the NPC anew, as for
  a race change (§2). On the player the creator's look itself changes, which needs a
  source face the game holds; otherwise the call is refused.
