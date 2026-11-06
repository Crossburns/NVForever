# Challenges (CHAL)

Code: `crates/fnv-game/src/challenges.rs` (counting, completion, rewards),
`engine/nvf-play/src/challenges.rs` (corner messages, the Pip-Boy list),
`engine/nvf-play/src/pipboy/data.rs` (the Notes tab's Challenges view). Built in
claude/e3-challenges (ESM gap audit row 3).

## 1. The record

[documented: xEdit wbDefinitionsFNV.pas CHAL, schema `schemas/fnv/gameplay.json`; GECK wiki
GetChallengeType, GetChallengeFlags, GetChallengeInterval, GetChallengeForm1/2,
GetChallengeValue1–3 (JIP pages; their "Challenge Filters" page is not in the owner's
export)]

- EDID, FULL (the name), SCRI (the reward script), DESC (the description), DATA, SNAM
  ("form 1"), XNAM ("form 2"). No ICON in any master [observed].
- DATA: type (u32), threshold (u32), flags (u32: 1 Start Disabled, 2 Recurring, 4 Show
  Zero Progress), interval (u32: "at which intervals of progress a progress notification
  will show up"), value1 (2 bytes), value2 (2 bytes), value3 (4 bytes).
- Types: 0 Kill from a form list, 1 Kill a specific form, 2 Kill any in a category, 3 Hit
  an enemy, 4 Discover a map marker, 5 Use an item, 6 Acquire an item, 7 Use a skill,
  8 Do damage, 9 Use an item from a list, 10 Acquire an item from a list, 11 Miscellaneous
  stat, 12 Craft using an item, 13 Scripted.

## 2. The masters' challenges and what the values mean

Survey of the six masters (`fnvtool esm inventory` + `esm decode`, 2026-10-21) [observed]:
184 challenges — FalloutNV.esm 105, OldWorldBlues 21, GunRunnersArsenal 21, DeadMoney 19,
LonesomeRoad 11, HonestHearts 7. By type: Scripted 50, Miscellaneous Stat 41, Kill from a
Form List 25, Do Damage 20, Hit an Enemy 18, Kill any in a Category 10, Discover a Map
Marker 8, Kill a Specific Form 4, Use an Item 3, Use an Item from a List 2, Acquire an Item
from a List 2, Craft Using an Item 1. Types 6 (acquire one item) and 7 (use a skill) are
in no master. 178 have a reward script; the six read (`fnvtool script decompile`:
ChallengeMediumXP, LowXP, Animal, CaravanMaster, Challenge, Fistful) are effect scripts
with only a ScriptEffectStart block: `player.RewardXP n`, with `AddPerk` (the challenge perks:
Animal Control, Bug Stomper, Lord Death …) with a ShowMessage, `UnlockChallenge` of the
next tier, `RemoveRecurringFromChallenge` once a perk's last rank is given, or
`AddAchievement`.

The value meanings are read off the records against their descriptions; no document
gives them [observed unless marked]:

- **value3 = weapon animation type + 1** (0 any): 1 "Unarmed", 2 "one-handed Melee", 3
  "two-handed Melee", 4 "one-handed Guns/pistols", 5 "one-handed laser/Energy", 6
  "rifle-grip Guns", 7 "automatic rifle-grip Guns", 9 "large two-handled weapon", 10
  "shoulder mounted", 14 "thrown weapons" — the WEAP DNAM animation types 0 Hand to Hand …
  13 Thrown, plus one, in every case.
- **value2 for kills (types 0, 1) and damage (8) = the weapon's skill** as an actor-value
  code: 34 Energy Weapons, 35 Explosives, 38 Melee Weapons, 41 Guns, 45 Unarmed.
- **value2 for "kill any in a category" (2) = creature type + 1**: 1 "Animals", 2 "Mutated
  Animals", 3 "Mutated Insects", 4 "Abominations", 5 "Super Mutants", 6 "Feral Ghouls",
  7 "Robots" (CREA DATA types 0 Animal … 6 Robot).
- **value2 for "hit an enemy" (3) = what the hit did**: 0 damage ("inflict damage to enemy
  heads"), 1 a critical hit, 2 a knockdown ("Knock down enemies"), 3 a crippled limb.
- **value1 for hits (3) and damage (8) = the body part**: 1 head, 2 torso, 4 right arm
  ("Cripple enemies' right arms") [hypothesis: 3 left arm, 5 left leg, 6 right leg — the
  order of the limb-condition actor values 25–30, which fits the three used].
- **value1 for "miscellaneous stat" (11) = the statistic's code** (misc-stats.md §1): 27
  "Complete challenges", 35 "Kill anything", 37 "Win at Caravan", 4 "Pick locks", 5 "Hack
  … terminals", 32 "Craft items", 1 "Discover locations" … all 41 fit.
- **SNAM**: the victims' list (type 0), the victim's base NPC (1: Caesar 00121FEF,
  Kimball 0013002D, Elijah), the marker reference (4), the item (5: RadAway, Rad-X,
  Turbo), the item list (9, 10), the ingredient (12: the Sierra Madre chip).
- **XNAM**: a weapon list (types 0, 1, 2, 3, 8), or a list of markers (type 4: "Visited
  every location in the Big MT").

## 3. What counts (built)

Every count is the player's; others' deeds count nothing [design].

| type | moves by | on | filters |
|---|---|---|---|
| 0, 1, 2 | 1 | `Game::kill` with the player the killer | victim (SNAM list, SNAM form against the reference or its base, creature type), weapon |
| 3 | 1 | `Game::hit` by the player (damage > 0, critical, crippled), a knockdown by the player | body part, weapon (value3, XNAM; value2 is the hit kind) |
| 4 | 1 | the compass's discovery of a marker | SNAM, XNAM |
| 5, 9 | 1 | `Game::ingest` by the player | SNAM, SNAM list |
| 6, 10 | the count | every item added to the player (`ItemAdded`: AddItem, pick-ups, the loot and barter menus) | SNAM, SNAM list |
| 8 | the damage dealt | `Game::hit` by the player | body part, weapon |
| 11 | the rise | `Game::count_stat` (also ModPCMiscStat); Locations Discovered from each marker found | value1 |
| 12 | the recipes made | the recipe menu | SNAM among the ingredients |
| 13 | 1 | IncrementScriptedChallenge | — |

- The weapon of a kill is the one the player last hit the victim with, else the one the
  player holds; none is fists (Unarmed, Hand to Hand) [hypothesis].
- A kill of anyone counts, friend or foe [hypothesis: the descriptions say "enemies"], but
  the player's own death is none [design: `Game::kill`].
- Damage is counted as dealt (after DT and the floor at 0 health); fractions are kept
  until they make a whole step [hypothesis].
- Acquiring counts every item that reaches the player, also from the barter menu and
  containers [hypothesis].
- A challenge with threshold 0 (Find Vault 34) completes at 1 [hypothesis].

## 4. Completion, rewards and messages

- Start Disabled: no progress until UnlockChallenge [hypothesis, as before]. A completed
  challenge stops unless Recurring (and not cleared by RemoveRecurringFromChallenge); a
  recurring one starts again from 0, the others stay at the threshold [documented: GECK
  wiki GetChallengeProgress notes]. Progress past the threshold in one step is dropped
  [hypothesis].
- Each completion counts in Challenges Completed (misc stat 27, which itself moves "Up to
  the Challenge") and queues the reward script: its ScriptEffectStart block runs once on
  the player at the start of the next rules frame, in its own variables (InstKey
  Challenge), never inside the hit or kill [design; hypothesis that every reward script
  is such an effect script, §2].
- Corner messages [hypothesis: wording and icon; the masters hold no setting for them]:
  "Challenge completed: <name>" with `interface\icons\pipboyimages\perks\perk_challenge.dds`
  (the only interface texture named for challenges); "<name> <progress>/<threshold>" each
  time progress passes a multiple of the interval.
- Log: `challenge: <name> <p>/<t>` for each step (damage challenges only at an interval),
  `challenge: <name> <t>/<t>, completed`, `challenge notice: <text>`.
- Saved: `PlayerState::challenges` (progress, the part step, completed, times, unlocked,
  not recurring). The queued rewards are not saved.

## 5. The Pip-Boy list

The DATA page's Notes tab has the button "Challenges" with "R)" (pipboy.md §5); the
original's `map_menu.xml` has a Challenges list box and a "current Challenge Type"
headline set in code [documented: menus-extract map_menu.xml]. Built: R or the button turns
the Notes list into the challenges and back (the button then reads "Notes" [hypothesis]).
Listed: unlocked challenges with a name that have progress or are completed, and those
flagged Show Zero Progress also at 0 [hypothesis: the flag's name]; running first, then
completed at half strength, each by name [hypothesis]. The right side shows the
description and "<progress>/<threshold>" or "Completed" [hypothesis]. Not built: the
challenge-type headline and the original's grouping by type.

## 6. Not built

- Type 7 (use a skill): in no master.
- The original's sound for a completion (none found by name) [observed: no file or form
  named for challenges].
- Achievements (AddAchievement) stay a stub.
