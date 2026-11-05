# Script function audit (2026-10-16)

The owner asked for every script function NVForever registers, or the shipped scripts use,
to behave as close to the original as the documentation allows. The trigger: `IsWeaponOut`
read state the engine set only for the player, so every NPC read 0. This audit hunts that
kind of gap ("the engine knows something the rules never hear") and functions that return a
constant where the original returns something live.

## 1. Method

- **Every registered function** (`crates/fnv-game/src/functions/mod.rs`: 398 rows
  below, some grouping several functions; 425 names including the console's): what state it
  reads, and who writes that state, by reading the implementation and grepping its writers
  in `crates/` and `engine/` (only the player? only tests? never the engine?).
- **Against the documentation**: the catalogue (`docs/spec-notes/script-functions-catalogue.md`,
  our wording of the GECK wiki, with call and condition counts over the ten masters), the
  owner's GECK wiki export (private), and decompiled shipped scripts (`fnvtool script
  decompile`) for who calls what. Tags as CLAUDE.md asks: [documented], [observed],
  [hypothesis].
- **In the original game** where the docs left the outcome open and the function matters:
  three batched, unattended captures with the `nvf_capture` plugin (§3).
- Status: **matches** (the documented or observed original), **fixed here**, **gap left**
  (reason in the note or §5), **open** (the docs do not settle it; §4 says what a capture
  needs), **stub** (`Kind::Stub`/`NoOp`; docs/design/script-stubs.md §4 has each one's
  reason).

Totals: 283 matches, 37 stub, 35 fixed here, 25 gap left, 18 open.

## 2. What was fixed

Ordered by use (calls / condition uses over the ten masters):

| Gap | Use | Fix | Evidence |
|---|---|---|---|
| NPCs had nothing equipped (GetEquipped, GetWeaponAnimType, GetArmorRating, the weapon-health and weapon-type functions) | 76 / 479 + | NPCs put on outfit and weapon the first time their cell loads; PlaceAtMe actors at once (`fnv_sim::World::outfit`) | observed (Sunny Smiles, Ringo) |
| GetItemCount and GetEquipped ignored form lists | 737 / 1286 | sum / any member | documented |
| SetGhost had no effect | 357 | ghosts take no damage | documented |
| SetRestrained / SetUnconscious had no effect on movement | 281 / 199 | they stay where they are (fnv-ai) | documented / hypothesis |
| GetOpenState: DOOR only, never 2/4 | 178 / 4 | every object 1/3, actors 0, 2/4 from the engine's door animation | observed + documented |
| GetHasNote ignored carried notes | 167 / 277 | carried NOTE items count | observed |
| SetPos / SetAngle were overwritten | 148 / 50 | teleport / redraw / player travel | hypothesis (applied at once) |
| GetGameSetting of exe-only settings errored | 106 (135 iLockLevelMax* reads) | the game's values | observed |
| GetMapMarkerVisible gave no 2, never saw found markers | 41 / 31 | 0/1/2; found markers from the engine; ShowMap canTravel | documented + observed |
| GetLevel errored for NPCs | / 46 | the record's level | documented + observed |
| GetAnimAction stub (0 = "equipping") | 44 / 1 | −1 none, 0/1 drawing/holstering from the engine | observed |
| IsWeaponOut for NPCs not drawn | 19 / 51 | the draw rule moved into the rules crate | observed |
| GetHealthPercentage | 57 / 129 | the wiki's formula | documented |
| IsSneaking for unmeasured actors | 35 / 33 | `Game::sneaks` | documented |
| GetDayOfWeek 3 days off | 24 / 11 | days since Sunday 13 October 2281 | observed |
| AddPerk / HasPerk / RemovePerk forTeammates | 19 | the teammates' own list | documented |
| ResurrectActor, KillActor's cause, GetCauseofDeath | 515, 597 | heal and forget; the cause kept | documented / hypothesis |
| IsRunning while standing | / 4 | the player's speed mode | documented + observed |

Tests: `crates/fnv-game/tests/functions.rs` (`script_function_audit_fixes`,
`is_weapon_out_answers_for_undrawn_actors`, the calendar asserts),
`crates/fnv-game/tests/package_choice.rs` (weekday schedules), `crates/fnv-sim/tests/synthetic.rs`
(`an_npc_starts_with_its_outfit_and_weapon_equipped`), `crates/fnv-ai/tests/carry.rs`
(`a_restrained_actor_stays_in_place_until_released`); smoke `function-audit` (what the
engine tells: Ringo's weapon type, his weapon out after SetAlert, the player's run mode,
GetOpenState) and `script-stubs` (counts).

## 3. Captures run (original game, xNVSE 6.4.9, `nvf_capture`, save `nvftest`)

Jobs and outputs are private (`C:\fnv-private\capture\audit`, `...\out\fa_*.jsonl`).
All three ran unattended and ended; the game's saves were not changed.

| Job | What | Outcome |
|---|---|---|
| `fa_batch1_queries` | 60 read-only queries in the gas station: NPCs in an unloaded cell, the player, globals, settings | Sunny/Trudy GetEquipped 0 though carried (cell never loaded); GetWeaponAnimType Sunny 1, Trudy 0; GetLevel 2 / 5; GetDisposition 35 / 35; GetAnimAction −1; HasLoaded3D 0 far, 1 near; safe GetOpenState 3; Goodsprings marker GetMapMarkerVisible 2; player IsRunning 1 standing; GetDayOfWeek 5 on 25 Oct 2281 (GameMonth 9); iLockLevelMax* 0/25/50/75/100 (both quoted and bare names) |
| `fa_batch2_loaded_npcs` | MoveTo next to Sunny, then Trudy, then back | Sunny once loaded: GetEquipped 1/1, GetWeaponAnimType 5, GetArmorRating 10, GetCurrentAIPackage 6 (34 while unloaded), procedure 43, player.GetLineOfSight 1 at 41 units; all kept after leaving. Trudy never loaded at her spot (HasLoaded3D 0 within 1 unit): nothing learned |
| `fa_batch3_actions` | GetOpenState of nine objects, a holotape added, SetAlert on Ringo, KillActor on a far coyote | every object 3, Ringo 0; GetHasNote 1 / GetItemCount 0 after AddItem; Ringo GetWeaponAnimType 4, IsWeaponOut 0 → (3 frames: 0, GetAnimAction 0) → (180 frames: 1, GetAnimAction −1) → 0 after SetAlert 0; GetRestrained 1; GetIsGhost 1; KillActor on the unloaded coyote failed as a script step; its GetCauseofDeath alive read 0 |

## 4. Open: what a later capture batch needs

Round 2 (2026-10-17, docs/design/script-function-coverage.md §3) ran the unattended
group with the probe generator `tools/capture/function_probe.py`: GetDisposition, GetCauseofDeath,
HasLoaded3D, GetPCMiscStat and the leap day are settled; GetCurrentAIPackage's codes are
partly (Sandbox 36 agrees), GetSleeping on the player in the bed's menu reads 0. The
attended group below is still open.

Grouped so they can run together (the owner asked for batches). Jobs that need actions the
plugin cannot script (combat, walking an NPC through a door) are attended.

- **Unattended queries**: GetCurrentAIPackage codes (Sunny read 34 unloaded, 6 loaded; our
  numbering vs the original's for each package type); GetDisposition (35 for two NPCs with
  no modifier — base value and what moves it; the stub returns 0 and 15 conditions test it);
  GetCauseofDeath of a live loaded actor (−1 or 0?) and after KillActor with a cause on a
  loaded, non-essential actor; HasLoaded3D right after a cell load (how many frames);
  GetSleeping on the player in a bed; GetPCMiscStat "Locations Discovered" against found
  markers; GetDayOfWeek across 29 February 2284 (does the game have leap days?).
- **Attended**: GetActionRef/IsActionRef when an NPC opens a scripted door; IsInCombat of
  two hostile NPCs out of the player's detection reach; ResetAI on an actor in combat;
  GetCurrentAIProcedure at a Travel destination; IsInCriticalStage after a plasma crit kill;
  GetUnconscious and GetKnockedState of an essential knocked out by damage, and how long;
  GetIsID / IsInList on a PlaceLeveledActorAtMe actor.

## 5. Gaps left, with the reason

(Round 2 closed the terminal locks and RewardXP's levelling; the full status of every used
function, with the system each gap needs, is now docs/design/script-function-coverage.md.)

- ~~EquipItem / UnequipItem on NPCs are not drawn~~ (199 / 51 calls): fixed 2026-10-17.
  The engine now dresses and arms NPCs by what the rules have equipped
  (`fnv_sim::World::equipment`, engine/nvforever/src/outfits.rs) and redraws when that
  changes; combat's worn armour reads the same; smoke `npc-equip-drawn` [design]. NPCs
  whose cell the rules have not loaded keep the engine's own roll until they are.
- ~~**SetUnconscious in the engine**: an unconscious actor does not lie down~~: right as it
  is (pods, sleepers); it now holds a pushed-down actor down (unconscious.md §5, 2026-10-18). (ResurrectActor's corpse is stood up again since
  docs/spec-notes/resurrect.md.)
- ~~**GetKnockedState / GetUnconscious**~~: fixed by knock-outs (#403) and knockdowns
  (2026-10-18, unconscious.md §5): explosion knockdowns are told and read 2.
- **GetAnimAction for attacks and reloads, and for the player**: the engine knows them
  (combat, nvf-play) but does not tell the rules.
- **IsMoving** gives no direction (2–4); **Look**'s rotate-body flag is kept, not acted on;
  **player.GetLineOfSight** reads 0 (the player is not a detector; 4 of 5 callers in
  FalloutNV.esm use it; the original said 1 at 41 units).
- **IsSpellTarget / HasMagicEffect / RemoveSpell** do not see NPC, creature and race
  abilities (SPLO) or enchantments on worn items: abilities are not applied at all.
- **Terminals** (GetLocked, GetLockLevel, Lock, Unlock): terminal locks live apart from the
  world's lock data (hacking.md §6).
- **GetChallengeCompleted** for kill challenges (only scripted challenges progress);
  **MenuMode** 1 (the Pip-Boy, per the wiki example) and the loading codes;
  **SetMinimalUse** not read by the engine; **SetPCCanUsePowerArmor** not read.
  (GetPCMiscStat's counters and SetScale's drawn size: done 2026-10-18,
  spec-notes/misc-stats.md.)
- **IsInCombat / GetDetected** for pairs out of the player's detection reach, and the player
  as detector: measured only near the player (performance choice of the engine).

## 6. Every registered function

Columns: calls / scripts and condition uses as the catalogue counts them (blank: none);
"Reads" and "Written by" as found in the code at the audit's start (rows fixed here say
what writes it now).

| Function | Opcode | Calls / cond | Reads | Written by | Status | Note |
|---|---|---|---|---|---|---|
| Abs | 0x1221 | 67/0 | arg | — | matches |  |
| Activate | 0x100D | 1309/- | `g.activate` | rules | matches |  |
| AddAchievement | 0x114A | 152/0 | writes `achievements` | never read | matches | no achievement system; harmless |
| AddFormToFormList | 0x1233 | 651/- | writes `g.form_lists` | rules | matches | Index 0 [documented]. |
| AddItem | 0x1002 | 1720/- | `w.add_item` | rules | matches | FLST expands [documented]. |
| AddItemHealthPercent | 0x11BC | 166/- | inventory | rules | matches |  |
| AddItemToLeveledList | 0x1236 | 20/- | `w.add_to_leveled_list` | rules | matches | Health not kept [documented reason]. |
| AddNote | 0x117C | 192/0 | writes `player.notes` | Pip-Boy DATA reads | matches | corner message `sComputersAddedNote` [documented] not checked here |
| AddPerk | 0x1176 | 188/0 | writes `player.perks` | rules | fixed here | forTeammates put the perk on the player; now on the teammates' list [documented]. |
| AddReputation | 0x1239 | 504/0 | writes | — | matches |  |
| AddReputationExact | 0x123B | 10/0 | writes | — | matches | limits [hypothesis] |
| AddScriptPackage | 0x1097 | 1192/ | `extra.packages` | rules | matches |  |
| AddSPECIALPoints | 0x1219 | 21/- | `ui.special`, open menu | rules / chargen | matches |  |
| AddSpell | 0x101C | 40/- | `Game::add_spell` -> effects | rules | matches |  |
| AddTagSkills | 0x121B | 3/- | `ui.extra_tags` | rules / chargen | matches |  |
| AddToFaction | 0x117F | 542/ | `extra.factions` | rules | matches |  |
| AddTopic | 0x1058 | 11/0 | writes `topics` | dialogue.rs reads | matches |  |
| AgeRace | 0x11E4 | 2/0 | stub | — | stub |  |
| AlwaysShowActorSubtitles | 0x1277 | 16/ | `senses.always_subtitles` | rules; engine voice | matches |  |
| ApplyImageSpaceModifier | 0x1191 | 355/0 | writes | engine draws | matches |  |
| AttachAshPile | 0x1211 | 4/0 | stub | — | implemented 2026-10-21 (ash-piles.md) |  |
| AutoDisplayObjectives | 0x1225 | 2/0 | stub | — | stub | script-stubs.md §4 |
| Autosave | 0x115E | 74/- | `ui.autosaves` | rules / save.rs:326 | matches |  |
| CastImmediateOnSelf | 0x11D9 | 233/- | `apply_magic_item` | rules | matches |  |
| ClearDestruction | 0x11D8 | 2/- | writes damage/destroyed | rules | matches |  |
| ClearFactionPlayerEnemyFlag | 0x11C9 | 4/ | `player.enemy_of` | rules | matches |  |
| ClearOwnership | 0x118C | 1/- | writes owner | rules | matches |  |
| CompleteAllObjectives | 0x11AD | 105/- | writes objectives | rules | matches |  |
| CompleteQuest | 0x1071 | 124/- | writes completed | rules | matches | Does not stop the quest [hypothesis]. |
| CreateDetectionEvent | 0x1068 | 1/ | queue_detection_event | engine hears | matches |  |
| DamageActorValue | 0x1181 | 138/ | `av_damage` | rules | matches | Health at 0 or less kills, or knocks out an essential actor, with no killer [hypothesis: unconscious.md §2.2; observer-checks.md check 88]. |
| DamageObject | 0x116C | 89/- | `Game::damage_object` | rules + engine combat.rs:1783 | matches |  |
| DetonatePlacedExplosives | 0x124D | 1/ | `combat_requests` | engine | matches |  |
| Disable | 0x1022 | 3336/- | `w.enabled` | rules / RefChanged | matches |  |
| DisablePlayerControls | 0x1061 | 220/0 | writes `controls_disabled` | engine reads: script.rs ControlLocks | matches |  |
| Dispel | 0x1142 | 56/- | `g.effects` | rules | matches |  |
| DispelAllSpells | 0x1148 | 11/- | `g.effects` | rules | matches | Abilities kept [hypothesis]. |
| Drop | 0x1057 | 4/- | `w.drop_item` | rules | matches |  |
| DropMe | 0x10A6 | 58/- | `w.drop_item` | rules | matches |  |
| Enable | 0x1021 | 3324/- | `w.enabled` | rules / engine follows RefChanged | matches |  |
| EnableFastTravel | 0x1111 | 83/0 | writes `fast_travel` | engine: wait.rs:313 reads wait only | matches | fast travel not built |
| EnableLoadingMenu | 0x1270 | 2/0 | stub | — | stub |  |
| EnableNavMesh / DisableNavmesh / setAllReachable | 0x11C0 / 0x11BF / 0x1159 | 5/0, 2/0, 0/0 | stub | — | stub |  |
| EnablePlayerControls | 0x1060 | 177/0 | writes | same | matches | short-list rule [hypothesis] |
| EnterTrigger | 0x11BA | 160/- | ref event | rules | matches |  |
| EquipItem | 0x10EE | 199/- | `w.equip` | rules; the engine draws NPCs' equipped items (outfits.rs) | fixed 2026-10-17 | The rules' equipped flags change (NPCs start equipped), and the engine redresses and rearms the NPC from them (§5; smoke `npc-equip-drawn`). |
| EvaluatePackage | 0x105E | 2285/ | package choice | rules | matches | (package layer, not audited deeply) |
| Exists | 0x119F | /21 | combat_group / condition_target | rules | matches | "near" not measured (hypothesis). |
| ExitGame | 0x11B5 | 1/0 | stub | — | stub |  |
| FireWeapon | 0x11E2 | 24/ | `combat_requests` | engine takes | matches |  |
| ForceActiveQuest | 0x11DD | 82/- | `ui.active_quest` | rules / pipboy/data.rs:579 | matches |  |
| ForceActorValue | 0x110E | 31/ | `av_permanent` | rules | matches |  |
| ForceFlee | 0x10F5 | 8/ | `start_flight` | rules | matches | Cell argument ignored (noted). |
| ForceRadioStationUpdate | 0x122C | 3/- | counter | rules | matches | Engine polls every frame [design]. |
| ForceTerminalBack | 0x1223 | 80/- | `terminals.back` | rules / terminal::choose | matches |  |
| ForceWeather | 0x112D | 13/- | `weather.request` | rules / main.rs:1419 | matches |  |
| GetActionRef | 0x10CD | 328/- | `ctx.action_ref` | runner | open | Correct for script/player activations. NPCs opening doors on paths go through `Doors::set_open` only (residents.rs:125), never `Game::activate`, so a door's OnActivate never sees an NPC; whether NPC door use fires OnActivate in the original needs a capture. |
| GetActorCrimePlayerEnemy | 0x11CB | / | crime_enemies | rules | matches |  |
| GetActorFactionPlayerEnemy | 0x11CC | 3/2 | factions x enemy_of | rules | matches |  |
| GetActorValue | 0x100E | 431/2074 | `extra.actor_values/av_permanent/av_temporary/av_damage`, world SPECIAL | rules (combat hits, aid, scripts) | matches | Worn enchantments not in the temporary pool (module note). |
| GetAlarmed | 0x103D | 1/ | crime_enemy | rules | open | Meaning is hypothesis. |
| GetAngle | 0x1008 | 10/- | `w.rot` (deg) | AI rot[2], sync_sim rot[2] | matches | Player pitch (rot[0]) never written [observed]; minor. |
| GetAnimAction | 0x10DB | 42/1 | stub 0 | engine: NPCs drawing (0) and holstering (1) | fixed here | Was a stub returning 0 ("equipping"). Now −1 for none [observed at rest] and 0/1 while drawn NPCs draw or holster [observed: Ringo 0 three frames after SetAlert 1]. Attacks, reloads and the player's actions: gap left. |
| GetArmorRating | 0x1051 | 1/1 | world equipped ARMO DR | equipped armour (now set for NPCs) | fixed here | Read 0 for every NPC; Sunny reads 10 in the original once loaded [observed]. |
| GetAttacked | 0x103F | /11 | `provocations.attacked` | rules from engine hits | matches |  |
| GetBaseActorValue | 0x1115 | 6/16 | base | rules | matches |  |
| GetBroadcastState | 0x11D4 | 6/- | `x.broadcast` else TACT flag | SetBroadcastState | matches | Flag bit [hypothesis]. |
| GetButtonPressed | 0x101F | 211/0 | `message_box.pressed` | engine: script.rs:530 answer() | matches |  |
| GetCasinoWinningsLevel | 0x125C | 24/0 | `casino_levels` | written by blackjack rounds: a quarter of the casino's max winnings per level, 4 at the max (blackjack.md §4; the quarters a hypothesis) | open | roulette and slots still stubs. Open: does cashing chips at a cage count (CSNO max_winnings)? capture needed |
| GetCasinoWinningStage | 0x1262 | 0/25 | same | same | open | as above |
| GetCauseofDeath | 0x118D | 1/1 | `extra.last_hit.weapon` | KillActor's cause, else the last hit | fixed here | KillActor's cause now counts. A live loaded actor reads -1, 1 after KillActor player 0 1, -1 after ResurrectActor [observed: probes, as ours]. A live unloaded coyote read 0 (`fa_batch3_actions`). |
| GetChallengeCompleted | 0x1266 | 23/0 | `challenges[c].completed` | rules: IncrementScriptedChallenge, runner | gap left | decomp asks e.g. NVChallengeShootemUp/Fistful (CHAL type "Kill from a Form List" [observed: esm decode]); nothing counts kills into challenges → always 0 |
| GetCombatTarget | 0x10E8 | 26/ | awareness combat target | rules | matches | Player as caller (auto-aim target) [documented] returns 0, but no base-esm script calls it on the player [observed]. |
| GetContainer | 0x10CF | 11/- | `ctx.inst` holder | runner | matches |  |
| GetContainerInventoryCount | 0x127A | 3/- | inventory sum | rules | matches |  |
| GetCurrentAIPackage | 0x106E | 15/15 | dialogue, awareness state, current package | rules | matches | Codes partly hypothesis. |
| GetCurrentAIProcedure | 0x108F | 16/64 | as above + `extra.furniture`, `package_goal_met` | rules; furniture from engine | open | Travel reads 0 until goal met then 17; page says a Travel package waits (3) at its destination. Capture: GetCurrentAIProcedure on an NPC that arrived at a Travel target. |
| GetCurrentTime | 0x1012 | 89/138 | GameHour global | fnv-sim clock | matches |  |
| GetDayOfWeek | 0x10AA | 24/11 | calendar globals | calendar globals | fixed round 2 (2026-10-17) | Now whole GameDaysPassed mod 7, day 0 a Sunday [observed: setting the calendar globals to 28 February 2284 left it at 5; each day passed moved it]. No 29 February [observed: 28 Feb 2284 + 24 h = 1 March]; the sim clock had Gregorian leap days (fixed). |
| GetDead | 0x102E | 1120/725 | `extra.dead` | `Game::kill` (KillActor, `combat.rs:356` engine hits, effects), `starts_dead` at new game | matches | Engine hits route through rules `hit`. |
| GetDeadCount | 0x1054 | 27/444 | `player.dead_count[base]` | `Game::kill` | matches | Templated-actor special case [documented: notes] not modelled; minor. |
| GetDestroyed | 0x10CB | 27/6 | `x.destroyed` | SetDestroyed, `Game::damage_object` stage (combat.rs:484) via engine combat.rs:1783 | matches |  |
| GetDestructionStage | 0x11D7 | 15/9 | `x.damage` vs DEST stages | DamageObject, engine weapon hits | matches | Stage arithmetic [hypothesis]. |
| GetDetected | 0x102D | 69/22 | `detection.get(d,t).band` | engine detection.rs (drawn+near) | open | Unmeasured pair = 0; dead detector drops after 1 s (original keeps it [documented: notes]). Player as detector always 0. |
| GetDetectionLevel | 0x10B4 | / | detection band | engine | matches | Player-as-target 0 in original [documented] not copied; unused. |
| GetDialogueEmotion | 0x11B3 | 0/20 | dialogue_emotion | rules | gap left | SayTo/Say lines read the first response's emotion throughout; `Speech::response` (game.rs:615) tracks the current one (dialogue.rs:963) |
| GetDisabled | 0x1023 | 804/168 | `w.is_enabled` | Enable/Disable, destruction stage / - | matches |  |
| GetDisposition | 0x104C | 0/15 | stub 0 | — | fixed round 2 (2026-10-17) | Was a stub (0). Now the record's Disposition Base + fDispTargetCharMult x (the target's Charisma - 5), 0..100, or SetDisposition's value [observed: 35 for every Goodsprings resident at Charisma 5, 85 at 10, 0 at 1, 25 towards Sunny (Charisma 4), Karma no effect; script-function-coverage.md §3.2]. |
| GetDistance | 0x1001 | 350/140 | `w.{cell,pos}` both | AI set_actor_place (NPCs), sync_sim (player) | matches | f32::MAX across spaces is a [hypothesis] (wiki: script crashes). |
| GetEquipped | 0x10B6 | 76/479 | `inventory.is_equipped(item)` | fnv-sim `World::outfit` (first load), EquipItem/UnequipItem | fixed here | NPCs had nothing equipped. Now an NPC puts on its outfit and weapon the first time its cell loads, by the engine's wearing and arming rules [observed: Sunny Smiles 0 before her cell ever loaded, 1 after, kept after leaving; `fa_batch2_loaded_npcs`]. Form lists: any member [documented: wiki notes]. |
| GetFactionCombatReaction | 0x119B | /1 | reaction() | records/rules | matches |  |
| GetFactionRank | 0x1049 | /64 | as above | rules | matches |  |
| GetFactionRankDifference | 0x103C | /2 | ranks | rules | matches |  |
| GetFactionRelation | 0x11C2 | 13/11 | faction reactions | records/rules | matches | Combination rule is hypothesis. |
| GetForceHitReaction / GetPlantedExplosive | 0x1259 / 0x11AC | 0/2, 0/1 | stub | — | stub |  |
| GetFriendHit | 0x1120 | 1/9 | friend hits | rules | matches |  |
| GetFurnitureMarkerID | 0x10A0 | 8/79 | `x.furniture.marker_id` | as GetSitting | matches |  |
| GetGameSetting | 0x1100 | 106/0 | GMSTs in load order | load order, else `EXE_SETTINGS` | fixed here | Settings no master defines errored (0). The five iLockLevelMax* read by 27 trap-plate scripts now have the game's values [observed: 0/25/50/75/100]. Other exe-only settings: none found used. |
| GetGlobalValue | 0x104A | 9/2887 | world globals | — | matches |  |
| GetGroupMemberCount | 0x11A0 | /66 | combat_group | rules | matches |  |
| GetGroupTargetCount | 0x11A1 | /3 | foes_of | rules | matches |  |
| GetHasNote | 0x117E | 167/277 | `player.notes` | AddNote, and NOTE items the player carries | fixed here | Notes added as items did not count [observed: AddItem of a holotape → GetHasNote 1, `fa_batch3_actions`]. |
| GetHeadingAngle | 0x1063 | 11/6 | `w.pos`, `w.rot[2]`; actor check by base sig | positions | matches | As GetIsCreature: no leveled bases in the shipped placements. |
| GetHealthPercentage | 0x11AF | 57/129 | permanent Health, damage | actor values | fixed here | Now the wiki's formula: current over base, 1.0 with no base, no upper bound [documented: wiki notes]. |
| GetHitLocation | 0x1187 | 1/321 | `extra.last_hit.location` | rules `hit` from engine hits (explosions -1) | matches |  |
| GetIgnoreCrime | 0x10C0 | 3/ | as above | rules | matches |  |
| GetIgnoreFriendlyHits | 0x1152 | 1/ | as above | rules | matches |  |
| GetInCell | 0x1043 | 282/1000 | `cell_of(r)` + EDID prefix | AI set_actor_place, player sync_sim, MoveTo | matches | Prefix rule [documented cat]. |
| GetInCellParam | 0x10E6 | -/18 | GetInCell for arg actor | as GetInCell | matches |  |
| GetInCharGen | 0x127F | 3/0 | `in_chargen` | — | matches |  |
| GetInFaction | 0x1047 | 186/3582 | `extra.factions`, base SNAM | rules | matches |  |
| GetInSameCell | 0x1020 | 99/33 | `cell_of` both | as above | matches | Cell-boundary caveat matches wiki. |
| GetInWorldspace | 0x1136 | 91/345 | `world_of(cell_of(r))` | as above | matches |  |
| GetIsAlerted | 0x105B | 2/21 | `extra.alerted` or combat state | rules | matches |  |
| GetIsAlignment | 0x11DA | /3 | alignment by karma | rules | matches |  |
| GetIsClass | 0x1044 | /10 | traits def | records | matches |  |
| GetIsCreature | 0x1040 | 1/57 | base sig CREA | placed record type | matches | The gap reported for placed leveled actors cannot occur with the shipped data: none of the 6,018 placed ACHR/ACRE of FalloutNV.esm in the WastelandNV worldspace and the interior cells has a leveled list as its base (all NPC_ or CREA; other worldspaces not surveyed) [observed: `fnvtool world refs`]. |
| GetIsCreatureType | 0x11B6 | 9/79 | stats def creature_type | records | matches |  |
| GetIsCurrentPackage | 0x10A1 | 29/5 | `current_package` | rules | matches |  |
| GetIsCurrentWeather | 0x1095 | 2/- | request else `weather.shown` | main.rs:1433 | matches |  |
| GetIsGhost | 0x10ED | 4/ | `extra.ghost` | SetGhost | matches | (the flag has no effect, see SetGhost) |
| GetIsID | 0x1048 | 111/23785 | `base_of(r)` | base form | matches | As GetIsCreature: no leveled bases in the shipped placements [observed]. |
| GetIsLockBroken | 0x120A | 4/- | `locks.broken` | lockpick.rs:523 | matches |  |
| GetIsObjectType | 0x11B1 | 0 | 1 | form type by code; table missing locally (script-stubs.md §6) | matches |  |
| GetIsRace | 0x1045 | /52 | traits def | records | matches |  |
| GetIsReference | 0x1088 | 233/444 | subject == arg | - | matches |  |
| GetIsSex | 0x1046 | 6/451 | traits def / `player.female` | records, chargen | matches |  |
| GetIsUsedItem / ...Type / ...EquipType / GetUsedItemActivate | 0x10F6 / 0x10F7 / 0x11E0 / 0x1103 | 0/16, 0/6, 0/1, 0/3 | stub 0 | — | stub | no used-item AI |
| GetIsVoiceType | 0x11AB | /17517 | traits def voice | records | matches |  |
| GetItemCount | 0x102F | 737/1286 | `w.inventory(of).count(item)` | world inventories | fixed here | Form lists sum their members [documented: wiki notes]; notes count 0 [documented; observed `fa_batch3_actions`]. |
| GetKnockedState | 0x106B | 5/35 | `extra.unconscious` | rules | gap left | Never 2: the engine rolls explosion knockdowns (`nvf-play/combat.rs:2140-2155`, "not animated yet") but never tells the rules; never -1 (fatigue not built); 1 never clears after an essential knockout. |
| GetLevel | 0x1050 | 46/46 | `player.level` / `extra.level` | `Game::actor_level` | fixed here | Errored for NPCs without SetLevel; now their record's level [documented; observed: Sunny 2, Trudy 5]. |
| GetLineOfSight | 0x101B | 49/ | `detection.get(r,t).seen` | engine detection (actor pairs only) | gap left | Player as caller always 0 [documented: player may test any object via the camera]; 4 of 5 base-esm callers are `player.GetLineOfSight 0x00123CC7` (package scripts 0x00123CC8, 0x001480E4, 0x00147556, SCPT 0x001639C8) [observed]. |
| GetLinkedRef | 0x116B | 529/- | `placed.linked` | ESM / - | matches |  |
| GetLocked | 0x1005 | 58/95 | `w.lock_status`, `locks.broken` | Lock/Unlock, lockpick.rs:523 / engine lockpick | fixed round 2 (2026-10-17) | Terminals: 0 while their cell is not loaded, 1 with lock data, 2 after Lock or a lockout, 0 after Unlock [observed: an Easy terminal; documented: wiki]. Doors and containers as before. |
| GetLockLevel | 0x1041 | 5/1 | `w.lock.level` | Lock, XLOC | fixed round 2 (2026-10-17) | Terminals: their difficulty code while loaded (Easy 1), 5 after Lock 50, -1 after Unlock, 0 unloaded [observed]. Gap left: a loaded door without lock data reads 255 in the original, 0 here [observed: the gas station's door]. |
| GetMajorCrimeCount | 0x1075 | /2 | crime_counts | rules | matches |  |
| GetMapMarkerVisible | 0x11EC | 41/31 | `map_marker_shown` (ShowMap / ShowAllMapMarkers) | ShowMap / ShowAllMapMarkers, and markers the player found (engine compass → `Game::set_found_markers`) | fixed here | Gave 0/1 only and never heard of found markers. Now 0/1/2 [documented]; a found marker 2 [observed: Goodsprings]. Markers authored Visible: not known to the rules. |
| GetMinorCrimeCount | 0x1074 | / | crime_counts | rules | matches |  |
| GetObjectiveCompleted | 0x11A4 | 1032/1284 | `objectives[i].completed` | Set*, CompleteAllObjectives / - | matches |  |
| GetObjectiveDisplayed | 0x11A5 | 1063/820 | `objectives[i].displayed` | Set* / - | matches |  |
| GetOpenState | 0x109D | 178/4 | `w.open`, base sig DOOR | engine door animation (`Senses::open_states`), else the open flag | fixed here | Only DOOR bases answered, never 2/4. Now every non-actor object 1/3, actors 0 [observed: nine objects and Ringo, `fa_batch3_actions`], 2/4 while the engine animates [documented]. |
| GetOwnerLastTarget | 0x124E | 5/0 | stub 0 | combat now built (awareness, combat hits) | gap left | decomp: weapon scripts (cattle prod, boxing gloves/tape, 0x0013B2CA) set a target var from it; still "assigned to J" in stubs.rs though combat targets exist |
| GetParentRef | 0x1113 | 2/- | `placed.enable_parent` | ESM / - | matches |  |
| GetPCEnemyOfFaction | 0x10C5 | 7/2 | `player.enemy_of` | rules | matches |  |
| GetPCIsSex | 0x1083 | 8/190 | `player.female` | chargen | matches |  |
| GetPCMiscStat | 0x1138 | 4/0 | `misc_stats` | rules and engine counters (`misc_stats.rs`) | fixed 2026-10-18 | All 43 names have codes and compile; 26 statistics are counted where the rules or the engine see the event, the rest only by ModPCMiscStat (spec-notes/misc-stats.md). Original read People Killed 1 after a kill [observed]. |
| GetPermanentActorValue | 0x11EF | 7/134 | base+permanent | rules | matches |  |
| GetPlayerControlsDisabled | 0x1062 | 5/0 | `controls_disabled` | rules only (scripts) | matches | wiki examples match `controls()` |
| GetPlayerName / ShowRaceMenu / SetTagSkills / ShowTraitMenu | 0x11E1 / 0x10DA / 0x121A / 0x1268 | 3/3/3/5 | open menus | engine chargen | matches |  |
| GetPlayerTeammate | 0x11C6 | 26/2 | `extra.teammate` | rules | matches |  |
| GetPlayerTeammateCount | 0x11C7 | /2 | teammates alive | rules | matches |  |
| GetPos | 0x1006 | 77/4 | `w.pos` | AI, sync_sim, physics settle, SetPos | matches | Live for NPCs and player [observed]. |
| GetQuestCompleted | 0x1222 | 263/574 | `completed \|\| failed` | stage flags, CompleteQuest / - | matches | Optional quest arg handled. |
| GetQuestRunning | 0x1038 | 353/285 | `quests[q].running` | Start/StopQuest, start-game flags / - | matches |  |
| GetQuestVariable | 0x104F | -/11384 | quest instance var | scripts / - | matches |  |
| GetRadiationLevel | 0x11F7 | 0/7 | stub 0 | — | stub | no environment radiation |
| GetRandomPercent | 0x104D | 459/578 | rng | — | matches |  |
| GetReputation | 0x123D | 220/14 | `player.reputation` | rules: reputation.rs, crime_infamy | matches |  |
| GetReputationPct | 0x123E | 0/2 | rep / scale | — | matches |  |
| GetReputationThreshold | 0x123F | 143/1643 | levels → tiers grid | — | matches | grid [observed] |
| GetRestrained | 0x10F4 | 5/2 | `extra.restrained` | SetRestrained | matches |  |
| GetScale | 0x1018 | 1/1 | `RefState::scale` else XSCL | SetScale | matches |  |
| GetScriptVariable | 0x1035 | -/1756 | ref instance var | scripts / - | matches | Errors (reads 0) for a ref without a script. |
| GetSecondsPassed | 0x100C | 821/0 | instance run times | rules tick | matches | [observed] §10.7 |
| GetSelf | 0x10CE | 367/- | `ctx.self_ref` | runner | matches |  |
| GetShouldAttack | 0x1042 | / | `would_attack` | rules | open | No wiki page; hypothesis from 6 uses. |
| GetSitting | 0x109F | 25/73 | `x.furniture` non-bed state | engine furniture.rs tell_scripts for NPCs (SitOn) and player | matches | NPCs covered via residents' SitOn; actors not drawn (headless sandbox) read 0 [OPEN whether original differs]. State 1 never produced. |
| GetSleeping | 0x1031 | 7/18 | `x.furniture` bed state | as GetSitting | open | Answers 3 for the player in a bed; wiki: "will NOT work on the player" (value unclear). |
| GetSpellUsageNum | 0x122B | -/- | `spell_usage` | aid | matches |  |
| GetStage | 0x103A | 650/1335 | `quests[q].stage()` (max done, override) | SetStage / - | matches | Highest done [documented wiki; observed L5.quest.stage_order]. |
| GetStageDone | 0x103B | 317/510 | `quests[q].stages_done` | SetStage / - | matches |  |
| GetTalkedToPC | 0x1032 | 22/141 | `extra.talked_to_pc` | rules: dialogue.rs:463/468 | matches |  |
| GetTrespassWarningLevel | 0x1090 | /29 | `crimes.warnings` | rules | matches |  |
| GetUnconscious | 0x10F2 | 43/99 | `extra.unconscious` | SetUnconscious and essential knockout (`combat.rs:312,353`) | gap left | Reads 1 after an essential's health knockout [documented: notes say it does not detect that]; and the flag is never cleared, so an essential stays "unconscious" for good (also excludes it from `would_attack`). |
| GetVATSValue | 0x1198 | 0/250 | stub 0 | — | stub | outside VATS [hypothesis, script-stubs.md] |
| GetWeaponAnimType | 0x106C | 14/44 | equipped weapon else `extra.held_weapon` | equipped weapon (now set for NPCs), else the engine's held weapon | fixed here | NPCs not drawn read 1 (unarmed). Equipping on first load answers for them [observed: Sunny 5 once loaded, Ringo 4]. |
| GetWeaponHealthPerc | 0x11F4 | 2/- | equipped weapon's condition | equipped weapon (now set for NPCs) | fixed here | NPCs had no equipped weapon (read 100). |
| GetXPForNextLevel | 0x1215 | 3/0 | level, xp | — | matches | remaining XP [hypothesis] |
| HasLoaded3D | 0x122E | 4/- | present && cell in `loaded_cells` | engine set_loaded via script.rs | matches | The original needed 1 game loop after a cell load or MoveTo [observed: probes]; ours answers at once. |
| HasMagicEffect | 0x10D6 | 41/5 | `g.effects` by MGEF | as IsSpellTarget | gap left | Same missing sources. |
| HasPerk | 0x11C1 | 126/493 | `player.perks` (player only) | rules | fixed here | forTeammates asks the teammates' list [documented]. |
| IgnoreCrime | 0x10AB | 196/ | `extra.ignore_crime` | rules; read by crime witnesses | matches |  |
| IncrementScriptedChallenge | 0x1275 | 134/- | `player.challenges` | rules | matches | XP/message not given (noted in code). |
| IsActionRef | 0x1069 | 578/- | `ctx.action_ref` | runner | open | Same note as GetActionRef. |
| IsActor | 0x1161 | 1/- | base sig NPC_/CREA | placed record type | matches | As GetIsCreature. |
| IsActorAVictim | 0x113A | / | crimes.victim | rules | matches |  |
| IsActorRefEssential | 0x1272 | 2/ | essential() | rules | matches | Same as IsEssential (base flag included); page is a stub. |
| IsActorsAIOff | console | - | `ai_off()` | ToggleActorsAI, SetActorsAI | matches |  |
| IsActorTalkingThroughActivator | 0x11AE | 0/92 | stub 0 | `extra.talking_actor` written by SetTalkingActivatorActor, never read | stub | no conversation through activators; 0 right while none [script-stubs.md] |
| IsAlwaysHardcore | 0x126B | 4/0 | hardcore + `hardcore_was_off` | — | matches | "all game" [hypothesis] |
| IsAnimPlaying | 0x1128 | 108/ | `anims.playing` | engine actor_groups/object_anims | open | Counts only PlayGroup groups; whether an actor's own locomotion counts is unknown. 61 self callers (activators) [observed]. |
| IsCellOwner | 0x1118 | / | cell ownership | records/rules | matches |  |
| IsChild | 0x116D | /203 | race child flag | records | matches |  |
| IsCombatTarget | 0x1203 | 6/1 | awareness targets | rules | matches |  |
| IsCurrentFurnitureObj | 0x10A3 | 1/56 | base of furniture ref | as GetSitting | matches |  |
| IsCurrentFurnitureRef | 0x10A2 | 7/2 | `x.furniture.reference` | as GetSitting | matches |  |
| IsDLCInstalled | 0x126F | 0(8 decoded)/0 | load order | — | matches | name match [hypothesis] |
| IsEssential | 0x1162 | 1/3 | essential() | rules | matches |  |
| IsGoreDisabled | 0x1226 | 6/0 | const 0 | — | matches | gore on [decision] |
| IsHardcore | 0x124A | 11/278 | `player.hardcore` ∨ config | rules: SetHardcore; engine: test aid only | matches | no in-game settings toggle (original has one) — not a function gap |
| IsIdlePlaying | 0x1070 | 11/2 | `extra.idle` | engine `idles.rs:627` | open | Undrawn actors play nothing; IDLE-tree idles (PickIdle) not built. |
| IsImageSpaceActive | 0x1232 | 43/0 | `image_modifiers` | rules ages them | matches | only script-applied IMADs; engine-applied ones (magic effects) not in the list [hypothesis: no shipped call asks for those] |
| IsInCombat | 0x1121 | 312/219 | `awareness.state(r)`, player `awareness.hunted` | `observe_threat`/`settle_awareness` from engine detection (drawn+near) | open | Correct for actors near the player. NPC vs NPC beyond the player's detection reach is never measured, so far fights never start; capture: two hostile NPCs fighting out of the player's reach. |
| IsInCriticalStage | 0x1213 | 16/- | `x.critical_stage` | SetCriticalStage only | open | No engine crit-death (goo/disintegrate) sets it; if the original's crit death sets the stage itself (not via the effect script) this stays 0. Capture: IsInCriticalStage on a plasma-crit kill. |
| IsInInterior | 0x112C | 35/56 | `cell_of`, `world_of` | as above | matches |  |
| IsInList | 0x1174 | 10/49 | ref or base in list | AddFormToFormList | matches | Leveled-actor base caveat as GetIsID. |
| IsInMyOwnedCell | 0x1092 | / | cell ownership | records/rules | matches |  |
| IsKiller | 0x1199 | 45/ | `extra.killer` | `Game::kill` | matches | "Lost when disabled" [documented] not modelled; minor. |
| IsLeftUp / IsFacingUp | 0x111D / 0x106A | 0/22, 0/41 | stub 0 | — | fixed round 2 (2026-10-17) | Were stubs (0); 1 on a standing actor [observed: Ringo]. Knockdowns not built. |
| IsLimbGone | 0x118E | 12(51 decoded)/0 | stub 0 | — | implemented | dismemberment built 2026-10-20 (dismemberment.md §5) |
| IsMoving | 0x1019 | /49 | `motion.moving` | engine detection.rs (drawn+near) | gap left | Only 0/1: backward/left/right (2/3/4) never [documented]; the player's `MoveIntent.forward/right` already holds the direction. |
| IsOwner | 0x1116 | -/18 | `w.extra.owner` (placed XOWN at start, world.rs:337) | rules | matches |  |
| IsPC1stPerson | 0x1188 | 2/9 | `senses.first_person` | engine `control.rs:370` every frame | matches |  |
| IsPCAMurderer | 0x10B0 | / | player.murderer | rules | matches |  |
| IsPCSleeping | 0x10AF | 2/- | `rest.passing.sleep` | engine wait.rs:280/512/602 begin, :281/686 end | matches |  |
| IsPlayerActionActive | 0x1170 | 2/0 | stub 0 | engine knows aiming / firing | gap left | decomp: both OWB calls ask action 9 (iron sights) |
| IsPlayerGrabbedRef | 0x11D0 | 3/1 | stub | — | stub | no grab |
| IsPlayerInRegion | 0x1260 | 57/- | player `cell`,`pos`, region areas | sync_sim | matches |  |
| IsPlayerMovingIntoNewSpace | 0x1166 | 6/8 | `g.loading` | engine `nvf-play/script.rs:1317` | matches |  |
| IsPlayerTagSkill | 0x11CE | 23/0 | `player.tag_skills` | rules: chargen.rs:259 (tag menu), SetPlayerTagSkill | matches |  |
| IsRaining | 0x103E | 6/3 | `weather.raining` | engine `set_shown_weather` (main.rs:1433) | matches | Live from the sky. |
| IsRunning | 0x111F | /4 | `motion.running` | engine: the player's Walk toggle (`Motion::run_mode`), NPCs measured | fixed here | Standing still read 0; the wiki says the speed mode counts "even if they are standing still" [documented; observed: player 1, Sunny 0]. |
| IsSneaking | 0x111E | 35/33 | `senses.motion[r].sneaking` | engine measure, else `Game::sneaks` | fixed here | NPCs the engine does not measure read 0; the rule answers for them. |
| IsSpellTarget | 0x10DF | 33/37 | `g.effects` by source | aid, AddSpell, Cast, hits (combat.rs:279) | gap left | Actor effects from NPC_/CREA/race SPLO and worn enchantments are never in `g.effects`, so 0 for them on every actor. |
| IsSwimming | 0x10B9 | 0/7 | stub 0 | engine knows player swimming (player.rs) | stub | player always 0 [documented: wiki note]; NPC swimming not built |
| IsTalking | 0x108D | 19/33 | `speech`, `dialogue.line` | rules (say_line, conversation) | open | decomp calls `player.IsTalking()` (4); the player never speaks here → always 0. Capture: player.IsTalking during a conversation |
| IsTimePassing | 0x1109 | -/- | `rest.passing` | wait.rs | matches | Fast travel not built, so travelling never counts. |
| IsTrespassing | 0x1091 | /1 | trespass_cell (player only) | rules | open | NPC callers read 0. |
| IsTurning | 0x101A | /1 | `motion.turning` | engine detection.rs | matches | Threshold hypothesis. |
| IsWaterObject | 0x1130 | 0/1 | stub | — | stub |  |
| IsWeaponInList | 0x118F | 9/14 | `equipped_weapon` | equipped weapon (now set for NPCs) | fixed here | NPCs had no equipped weapon. |
| IsWeaponOut | 0x1065 | 19/51 | `extra.weapon_out` | engine report for drawn actors (each frame); `Game::wants_weapon_out` for the others | fixed here | NPCs not drawn read 0, and a drawn NPC that unloaded kept its last value. The draw rule moved into the rules crate [observed: Ringo 0 → 1 within 180 frames of SetAlert 1, 0 after SetAlert 0; `fa_batch3_actions`]. Dead actors 0 [hypothesis]. |
| IsWeaponSkillType | 0x106D | -/52 | subject's WEAP base, else equipped weapon | equipped weapon (now set for NPCs) | fixed here | NPCs had no equipped weapon. |
| IsWin32 | 0x120C | 1/2 | const 1 | — | matches |  |
| IsXBox / IsPS3 | 0x1135 / 0x120B | 2/2, 15/2 | const 0 | — | matches | PC [decision] |
| KillActor | 0x108B | 597/ | `Game::kill` | rules (`Game::kill`) | fixed here | The cause parameter was dropped; it now reaches GetCauseofDeath [documented]. The limb is not dismembered (no dismemberment). |
| Lock | 0x1072 | 303/- | `w.lock` | rules / lockpick reads | fixed round 2 (2026-10-17) | On a terminal: the terminal's own saved lock (level at most 5; 0 keeps the level) [documented: wiki Lock; observed: GetLockLevel 5 after Lock 50]; the opening reads it. |
| Look | 0x105C | 159/30 | `senses.looks` | engine `script_look.rs` (head only) | gap left | `rotate_body` stored but never read by the engine (grep: only ai_state.rs/senses.rs). |
| MarkForDelete | 0x11BB | 602/- | writes deleted | rules | matches | Immediate removal [hypothesis]. |
| MenuMode | 0x1024 | 13/11 | `menu` / `dialogue` | engine: menu.rs (1002/1008/1053), lockpick, terminal, wait, crafting, levelup, script boxes | gap left | (a) `MenuMode 1` = Pip-Boy open [documented: wiki example] compared as a literal code: 2 `Begin MenuMode 1` blocks + LonesomeRoad `if menumode(1)` never fire though the Pip-Boy is 1002; (b) 1007 / 1013 never set: engine `PlayState::Loading` and the pause menu never reach rules; 8 `Begin MenuMode 1007` blocks (DM, HH, OWB) + 4 `menumode(1007) ∥ (1013)` never run. Code meanings 1007 loading / 1013 start menu [hypothesis; wiki: both true at the main menu] |
| ModActorValue | 0x1010 | 62/ | `av_permanent` | rules | matches |  |
| ModPCMiscStat | 0x1137 | 6/0 | writes | — | matches |  |
| ModWeaponHealthPerc | 0x11F6 | 8/- | equipped weapon | equipped weapon (now set for NPCs) | fixed here | As GetWeaponHealthPerc. |
| MoveTo | 0x109E | 1463/- | writes `w.{cell,pos,rot}`, `teleported` | rules → `Game::teleported`, `Game::turned` for objects, Moved event | fixed here | A drawn non-actor object moved by MoveTo was not redrawn; it now is, as after SetPos. |
| MoveToMarkerWithFade | 0x124F | 272/- | deferred move | fade | fixed here | Fade out, move after FADE_OUT_HOLD, fade in (2026-10-19); the player moves at once. |
| OpenTeammateContainer | 0x11C8 | 26/- | `teammate_containers` | rules / menu.rs:98 | matches |  |
| PickIdle | 0x1064 | 2/0 | stub | — | stub |  |
| PipBoyRadio | 0x11A7 | 6/- | `radio.pipboy` | rules / radio.rs | matches |  |
| PipBoyRadioOff | 0x1224 | 11/- | `radio.pipboy` | rules / pipboy, radio.rs:596 | matches |  |
| PlaceAtMe | 0x1025 | 468/- | `w.place` | rules / dropped & actor spawn | matches | Position exact [hypothesis]. |
| PlaceAtReticle | 0x1263 | 3/0 | stub | engine has the crosshair ray | stub | implemented 2026-10-18 |
| PlaceLeveledActorAtMe | 0x11D1 | 5/- | as PlaceAtMe | rules | matches | Level mod/zone ignored [noted in code]. |
| PlayBink | 0x1114 | 4/0 | stub | — | stub | intro decoder exists elsewhere (licence note) — could play now |
| PlayerInRegion | 0x1264 | -/74 | as IsPlayerInRegion | sync_sim | matches |  |
| PlayGroup | 0x1013 | 864/ | queues `anims.requests` | engine `actor_groups.rs` / `object_anims.rs` | matches | Not-drawn actor: nothing plays and IsAnimPlaying goes 0 (design). |
| PlayIdle | 0x1190 | 318/ | `idle_requests` | engine `idles.rs:517` | matches | Plays on the player always; original only in fly-cam / activation frame [documented: notes] - deliberate for the opening. |
| PlayMagicShaderVisuals / StopMagicShaderVisuals | 0x1123 / 0x1125 | 205 / 79 | `script_looks` | engine effect_shaders.rs | matches |  |
| PlayMusic | 0x11FF | 38/- | `music_requests` | rules / music.rs | matches |  |
| PlaySound | 0x1026 | 502/- | `sound_requests` | rules / world_sounds.rs:648 | matches |  |
| PlaySound3D | 0x10B2 | 120/- | `sound_requests` | rules / world_sounds.rs | matches |  |
| PushActorAway | 0x118A | 35/0 | `Game::push_actor_away` | `RefExtra::knocked_out` → engine ragdoll | fixed 2026-10-18 | knockdown, unconscious.md §5 |
| ReleaseWeatherOverride | 0x1158 | 11/- | `weather.release` | rules / main.rs:1430 | matches |  |
| RemoveAllItems | 0x10AD | 59/- | `w.transfer_all/remove_all_items` | rules | matches |  |
| RemoveAllTypedItems | 0x1249 | 295/- | inventory | rules | matches | Type codes partly [hypothesis]. |
| RemoveFromAllFactions | 0x1214 | 19/ | factions | rules | matches |  |
| RemoveFromFaction | 0x1180 | 683/ | `extra.factions` | rules | matches |  |
| RemoveImageSpaceModifier | 0x1192 | 74/0 | writes | — | matches |  |
| RemoveItem | 0x1052 | 1458/- | `w.remove_item` | rules | matches |  |
| RemoveMe | 0x10A5 | 107/- | `ctx.inst` item | rules | matches |  |
| RemoveNote | 0x117D | 125/0 | writes `player.notes` | rules | fixed here | Also takes the carried note. |
| RemovePerk | 0x11A8 | 374/0 | writes `player.perks` | rules | fixed here | forTeammates takes from the teammates' list. |
| RemoveRecurringFromChallenge | 0x126C | 5/0 | writes | — | matches |  |
| RemoveReputation | 0x123A | 11/0 | writes | — | matches |  |
| RemoveScriptPackage | 0x1098 | 185/ | `extra.packages` | rules | matches |  |
| RemoveSpell | 0x101D | 146/- | `x.spells`, dispel | rules | gap left | Cannot remove an actor's authored SPLO abilities (never applied). |
| Reset3DState | 0x1146 | 12/0 | anims.reset | engine object_anims | matches |  |
| ResetAI | 0x11FA | 711/ | packages, looks, weapon use | rules | open | Combat not reset (engineering choice, page says combat re-evaluated); capture: ResetAI on an actor in combat. |
| ResetHealth | 0x1150 | 54/ | `av_damage` | rules | matches |  |
| ResetInventory | 0x11ED | 17/- | `w.reset_inventory` | rules | matches |  |
| ResetPipboyManager | 0x1201 | 4/0 | stub | — | stub |  |
| ResetQuest | 0x11DB | 6/- | replaces QuestState | rules | matches | Running kept [hypothesis]. |
| ResetXP | 0x126D | 2/0 | writes | — | matches |  |
| RestoreActorValue | 0x1182 | 284/ | `av_damage` | rules | matches |  |
| ResurrectActor | 0x108C | 515/46 | sets `extra.dead=false` | rules | fixed here | Only cleared `dead`. Now also heals Health, clears unconscious, killer and cause [documented: "from the dead or unconscious state"; hypothesis for the rest]. The engine stands the corpse up again since docs/spec-notes/resurrect.md (a dead actor generated again from its record). |
| RewardKarma | 0x1204 | 163/ | `change_karma` | rules | matches | Clamp iKarmaMin/Max and corner note present. clamped iKarmaMin/Max |
| RewardXP | 0x1177 | 1303/0 | writes `player.xp` | rules: combat.rs, lockpick.rs; engine: compass.rs:131 | matches | Levelling by experience is built now (experience.rs; status.md); the gap is closed. |
| Rotate | 0x1004 | 4/- | `g.rotations` -> `w.rot`, `turned` | rules / moved.rs:62, script_draw.rs:131 | matches |  |
| Say | 0x1033 | 623/0 | Game::say | — | matches |  |
| SayTo | 0x1034 | 994/0 | Game::say | engine facings, voice | matches | forceSubtitle not read |
| ScriptEffectElapsedSeconds | 0x10BA | 20/- | `g.effect_elapsed` | game.rs:2163 | matches |  |
| SendAssaultAlarm | 0x105F | 93/ | crime | rules | matches |  |
| SendTrespassAlarm | 0x115C | 6/ | crime | rules | matches |  |
| SetActorAlpha | 0x1129 | 127/0 | `script_looks.alpha` | engine | matches |  |
| SetActorFullName | 0x111C | 26/ | `forms.names` | rules | matches |  |
| SetActorRefEssential | 0x1273 | 22/ | `extra.essential` | rules | matches |  |
| SetActorsAI | 0x118B | 3/ | `extra.ai_off` | rules; fnv-ai, engine detection | matches | Animation/collision not stopped (noted). |
| SetActorValue | 0x100F | 730/ | base override | rules | matches |  |
| SetAlert | 0x105A | 39/ | `extra.alerted` | rules; engine alert.rs/creature_stance.rs read | matches |  |
| SetAllVisible | 0x115A | 5/0 | stub | — | stub | perf switch |
| SetAlly | 0x1079 | 127/ | as above | rules | matches |  |
| SetAngle | 0x1009 | 50/- | writes `w.rot[i]` | as SetPos | fixed here | As SetPos. |
| SetBroadcastState | 0x11D5 | 65/- | writes `x.broadcast` | rules / radio.rs reads | matches |  |
| SetCameraFOV | 0xF02B | console | `ui.camera.fov` | free_cam.rs | matches |  |
| SetCellImageSpace | 0x1196 | 2/0 | stub | — | stub | image modifiers exist; cell image space would be implementable |
| SetCellOwnership | 0x1119 | 18/- | `forms.cell_owners` | rules | matches |  |
| SetCellPublicFlag | 0x10BB | 12/- | `forms.public_cells` | rules | matches |  |
| SetCombatStyle | 0x10B1 | 92/ | `extra.combat_style` | rules | matches |  |
| SetCriticalStage | 0x1212 | 7/- | writes `x.critical_stage` | rules | matches | GooEnd / DisintegrateEnd cull (engine hides, no hit volume or ragdoll), 2026-10-19; Start stages' ragdoll changes not built. |
| SetDestroyed | 0x10CC | 517/- | writes `x.destroyed` | rules / engine reads (radio.rs:760, activators.rs:83) | matches |  |
| SetDisposition | 0x1184 | 2/0 | stub | — | fixed round 2 (2026-10-17) | Was a stub; GetDisposition reads the value [observed: 20 after SetDisposition player 20]. |
| SetEnemy | 0x1078 | 478/ | `forms.faction_reactions` | rules | matches |  |
| SetEssential | 0x1003 | 37/ | `forms.essential` | rules | matches |  |
| SetFactionRank | 0x1089 | 34/ | as above | rules | matches | Negative rank stored as membership -1, fine. |
| SetForceSneak | 0x10D3 | 3/ | `extra.force_sneak` | rules; engine sneaking.rs via `sneaks` | matches |  |
| SetGhost | 0x10EC | 357/91 | `extra.ghost` | `RefExtra::ghost` → `Game::is_immune` | fixed here | The flag was never read: ghosts took damage. Now no damage [documented: wiki notes]; GetIsGhost 1 [observed]. |
| SetGlobalTimeMultiplier | 0x1186 | 7/0 | writes | engine script.rs time_multiplier | matches |  |
| SetHardcore | 0x124B | 2/0 | writes | — | matches |  |
| SetIgnoreFriendlyHits | 0x1151 | 132/ | `extra.ignore_friendly_hits` | rules; read by provocation.rs:301 | matches |  |
| SetInCharGen | 0x10E7 | 16/0 | writes | — | matches |  |
| SetItemValue | 0x1155 | 2/- | writes `forms.item_values` | rules | matches |  |
| SetLevel | 0x1133 | 6/ | `extra.level` / `player.level` | rules | gap left | NPC inventory not re-rolled [documented: notes]; minor (6 calls). |
| SetMinimalUse | 0x1228 | 1/- | writes `x.minimal_use` | rules / engine never reads | matches | Kept for the engine, which ignores it; 1 call. |
| SetNoAvoidance | 0x115B | 3/0 | stub | — | stub | perf switch |
| SetNPCRadio | 0x11C4 | 6/- | `radio.npc_radios` | rules / radio.rs:773 | matches |  |
| SetObjectiveCompleted | 0x11A2 | 2412/- | writes objectives | rules | matches |  |
| SetObjectiveDisplayed | 0x11A3 | 3242/- | writes objectives | rules | matches |  |
| SetOpenState | 0x10DD | 241/- | writes `w.open` | rules / doors.rs:165 sync_sim follows | matches |  |
| SetOwnership | 0x1117 | 144/- | writes `w.extra.owner` | rules | matches |  |
| SetPackDuration | 0x1122 | 12/ | `package_ends` | rules | matches |  |
| SetPCCanUsePowerArmor | 0x1229 | 2/0 | writes `power_armor` | never read (rules or engine) | gap left | effect missing: equipping power armour is not gated by training |
| SetPCEnemyOfFaction | 0x10C6 | 24/ | `player.enemy_of` | rules | matches |  |
| SetPCSleepHours | 0x10BD | 3/- | `rest.requests` | rules / wait.rs | matches |  |
| SetPlayerTagSkill | 0x11CD | 23/0 | writes | — | matches |  |
| SetPlayerTeammate | 0x11C5 | 189/ | `extra.teammate` | rules; cleared on death `combat.rs:249` | matches |  |
| SetPos | 0x1007 | 148/- | writes `w.pos[i]` only | rules → `Game::teleported` / `turned` / Moved event | fixed here | Changed only the stored value, which the AI and engine overwrote. Now actors are teleported, objects redrawn, the player travels [hypothesis: applied at once]. |
| SetQuestDelay | 0x11DC | 167/- | writes `quest.delay` | rules | matches | 0 = default [documented]. |
| SetQuestObject | 0x110D | 7/- | `w.set_quest_object` | rules | matches |  |
| SetReputation | 0x1238 | 255/0 | writes | — | matches | [observed captures §9.3] |
| SetRestrained | 0x10F3 | 281/117 | `extra.restrained` | `RefExtra::restrained` → fnv-ai | fixed here | Restrained actors kept walking their packages. Now they stay in place [documented: catalogue summary]; GetRestrained 1 [observed]. |
| SetScale | 0x113C | 9/- | writes `RefState::scale` | drawn (moved.rs, script_draw.rs, actor_scale.rs) and collided (phys.rs) | fixed 2026-10-18 | spec-notes/misc-stats.md §5. |
| SetSceneIsComplex / SetRumble / PurgeCellBuffers / PreloadMagicEffect | 0x115D / 0x11FB / 0x1189 / 0x1163 | 2/17/32/1 | NoOp | — | stub | script-stubs.md §5 |
| SetSecuritronExpression | 0x1248 | 37/0 | texture swap | engine | matches |  |
| SetSPECIALPoints | 0x1218 | 2/- | `ui.special`, open menu | rules / chargen | matches |  |
| SetStage | 0x1039 | 2449/- | writes `g.quests` | rules | matches | Pure rules state. |
| SetTalkingActivatorActor | 0x1171 | 31/- | writes `x.talking_actor` | rules | matches |  |
| SetUFOCamSpeedMult | 0xF033 | console | `ui.camera.fly_speed_mult` | free_cam.rs | matches |  |
| SetUnconscious | 0x10F1 | 199/72 | `extra.unconscious` | `RefExtra::unconscious` → fnv-ai | fixed here | Unconscious actors no longer walk [hypothesis]. Lying down is not built (gap left, §5). |
| SetVatsTarget / GetVATSTargetHeight / GetVATSMode | 0x11EB / 0x10EB / 0x10E0 | 10/0, 0/7, 2/2 | stub | — | stub | no VATS |
| SetWeaponHealthPerc | 0x11F5 | 1/- | equipped weapon | equipped weapon (now set for NPCs) | fixed here | As GetWeaponHealthPerc. |
| SetWeather | 0x1104 | 14/- | `weather.request` | rules / main.rs:1419 | matches |  |
| SexChange | 0x11E7 | 12/9 | `player.female` | rules | gap left | Errors on NPCs (`actor.rs:497`); resetAppearance param ignored. All base-esm callers are the player [observed], so low impact. |
| ShowAllMapMarkers | 0x11F8 | 2/0 | writes `all_markers` | Pip-Boy map reads | matches |  |
| ShowBarberMenu / ShowPlasticSurgeonMenu / ShowTutorialMenu | 0x11F1 / 0x11F2 / 0x11E3 | 4/2/2 | stub | — | stub |  |
| ShowBarterMenu | 0x1173 | 168/0 | opens 1053 + request | engine menu.rs | matches |  |
| ShowBlackJackMenuParams | 0x1254 | 6 | `blackjack_requests` → nvf-play `blackjack` | the casino, bets and max winnings of the call; refused without chips or when banned | implemented 2026-10-17 | docs/spec-notes/blackjack.md |
| ShowRoulette / SlotMachine MenuParams | 0x1256 / 0x1252 | 6/5 | stub | — | stub |  |
| ShowCaravanMenu / AddCardToPlayer | 0x125A / 0x125B | 29/0, 4/0 | stub | — | stub |  |
| ShowLoveTesterMenuParams | 0x1245 | 1/0 | opens 1074 | engine | matches |  |
| ShowMap | 0x1055 | 244/0 | writes `map_markers` | rules | fixed here | canTravel was ignored; now kept (GetMapMarkerVisible 2) [documented]. |
| ShowMessage | 0x1059 | 1717/0 | writes `message_box` (one slot) | engine: script.rs answer | matches | a second button box shown before the first is answered replaces `message_box`, so the first script never gets its button [observed in code; frequency unknown] |
| ShowRecipeMenu | 0x1250 | 15/0 | opens 1077 | engine crafting.rs | matches |  |
| ShowRepairMenu | 0x117A | 15/0 | stub | — | stub | repair not built |
| ShowSleepWaitMenu | 0x1279 | 2/- | `rest.requests` | rules / wait.rs:269 | matches |  |
| ShowWarning | 0x11B9 | 71/0 | log only | — | matches |  |
| Sqrt | 0x121F | 2/0 | arg | — | matches |  |
| StartCannibal / GetCannibal / HasBeenEaten | 0x107E / 0x10E3 / 0x107F | 4/0, 0/1, 6/4 | stub | — | stub | perk action not built |
| StartCombat | 0x1016 | 938/ | `Game::start_combat` | rules | matches |  |
| StartConversation | 0x1056 | 135/0 | force_greet | — | matches | no walk-up, location args unread [stand-in] |
| StartMisterSandman / GetSandman | 0x107C / 0x10E2 | 0/0, 0/2 | stub | — | stub |  |
| StartQuest | 0x1036 | 423/- | writes `quests[q].running` | rules | matches |  |
| StartRadioConversation | 0x11D6 | 16/- | `radio.conversations` | rules / radio.rs:706 | matches |  |
| StopCombat | 0x1017 | 282/ | `Game::stop_combat` | rules | matches |  |
| StopCombatAlarmOnActor | 0x1101 | 97/ | awareness | rules | matches |  |
| StopLook | 0x105D | 242/16 | `senses.looks` | rules | matches |  |
| StopQuest | 0x1037 | 466/- | writes running | rules | matches |  |
| SwapTexture | 0x1269 | 0(383 decoded)/0 | writes | engine texture_swap.rs | matches |  |
| SystemSave / ForceSave | 0x127B / 0x1217 | 5/0, 2/0 | stub | saves exist (Autosave implemented) | stub | implemented 2026-10-18 |
| ToggleActorsAI | console | - | `toggles.actors_ai_off` | rules / AI | matches | Global [hypothesis]. |
| ToggleCollision | console | - | `toggles.asked` | console.rs:428 | matches |  |
| ToggleDetection | console | - | `toggles.detection_off` | rules | matches | Player-only detection [hypothesis vs cat "each other"]. |
| ToggleFlyCam | 0xF04B | console | `ui.camera.fly_toggles` | free_cam.rs:117 | matches |  |
| ToggleGodMode | console | - | asked | console.rs | matches |  |
| ToggleGrass | console | - | asked | console.rs | matches |  |
| ToggleLODLand | console | - | asked | console.rs | matches |  |
| ToggleMenus | 0xF055 | console | `ui.camera.menus_hidden` | engine HUD | matches |  |
| ToggleNavmesh | console | - | asked | console.rs | matches |  |
| ToggleScripts | console | - | `toggles.scripts_off` | rules | matches |  |
| ToggleSky | console | - | asked | console.rs | matches |  |
| ToggleTrees | console | - | asked | console.rs | matches |  |
| ToggleWaterSystem | console | - | asked | console.rs | matches |  |
| ToggleWireframe (console) | — | — | stub | — | stub | console only |
| trapUpdate | 0x110C | 0/0 | stub | — | stub |  |
| TriggerScreenSplatter / SetScreenSplatterFade / ClearScreenSplatter | 0x1240 / 0x1243 / 0x1241 | 0-1/0 | stub | — | stub |  |
| UnequipItem | 0x10EF | 51/- | `w.unequip` | as EquipItem | fixed 2026-10-17 | As EquipItem: the item comes off the drawn NPC (§5). |
| Unlock | 0x1073 | 450/- | `w.lock` | rules | fixed round 2 (2026-10-17) | On a terminal: it opens at its menu [documented: wiki Unlock; observed: GetLocked 0, GetLockLevel -1]. |
| UnlockChallenge | 0x125E | 44/0 | writes | — | matches |  |
| UseWeapon | 0x119C | 13/ | `weapon_uses`, `world.equip` | engine use_weapon.rs | matches | The only place an NPC weapon gets the equipped flag. |
| WakeUpPC | 0x10AE | 3/- | `rest.requests` | rules / wait.rs | matches |  |
