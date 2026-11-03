# Spec notes: which factions an actor is in (templates, leveled lists) and which way a faction reaction points

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was read from the owner's install (FalloutNV.esm only, FNV 1.4.0.525),
**[hypothesis]** needs verification, **[design]** is our choice where the game's rule is
not known. Counts come from `fnvtool actor factions --survey` (fnv.actor.factions/1, §6).
"The wiki" is the owner's local export of the GECK wiki (page names given).

Code: `fnv_game::templates` (template chain, leveled lists), `fnv_game::data`
(`ActorDef::template_factions`, `LeveledActorDef`), `fnv_game::functions`
(`factions_of`, `faction_rank`, `faction_relation`). Related: actor-behaviour.md §3 (what
factions and reactions mean), actors.md §2.1–2.2 (the same template and leveled rules for
the drawn actor), actor-values.md (Stats and AI Data from templates).

## 1. Records

- NPC_ and CREA `SNAM`: one per faction, the faction and a rank [documented: xEdit;
  schema `gameplay.json`].
- ACBS template flags, one bit per group of data the template supplies: 1 Traits,
  2 Stats, 4 Factions, 8 Actor Effect List, 16 AI Data, 32 AI Packages,
  64 Model/Animation, 128 Base Data, 256 Inventory, 512 Script [documented: wiki
  GetActorTemplateFlag, "Use Factions" = 4; xEdit names the same bits]. `TPLT` names the
  template: an NPC_ or LVLN for an NPC, a CREA or LVLC for a creature [documented:
  xEdit].
- LVLN and LVLC: `LVLD` chance none, `LVLF` flags (bit 0 "Calculate from all levels <=
  player's level"), `LVLO` entries (level, an actor base or another list, count)
  [documented: xEdit; schema `gameplay.json`]. 708 such lists [observed].
- FACT `XNAM`: another faction, a disposition modifier and a group combat reaction
  (0 Neutral, 1 Enemy, 2 Ally, 3 Friend) [documented: xEdit; wiki
  GetFactionCombatReaction for the numbers].

## 2. The "Factions" template flag

- With the flag set, the actor's factions and ranks are its template's, not its own
  `SNAM` list [documented: xEdit hides the faction list when the flag is set; wiki
  GetActorTemplateFlag]. The template can have the flag too, so the chain is followed
  until an actor without the flag (or without a usable template) [hypothesis, the same
  reading as for the other groups, actors.md §2.2].
- How common [observed]: 2,044 of 3,816 NPC_ and 811 of 1,578 CREA have the flag. Their
  template is an NPC_ for 1,604, a CREA for 418, an LVLN for 440, an LVLC for 391; 2 have
  the flag and no template (they keep their own list). The chain is 1 step for 2,190
  actors, 2 for 548, 3 to 9 for 115.
- What the flagged actor's own list holds, for the 2,661 actors whose factions do not
  depend on a leveled pick [observed]: the same factions and ranks as the source for
  1,815, nothing for 669, something else for 177 (for example `Raider1MeleeAM`,
  `WastelanderAF`: leftovers the editor kept). So reading the own list, as the runtime
  did before, is wrong for 846 of them.
- Of the 6,385 placed actors of FalloutNV.esm, 3,970 take their factions from a template.
  Against the own list alone: 3,037 had none and now have some, 104 have different ones,
  3,090 are unchanged, 154 have none either way [observed].
- Example [observed]: `1EFiendViolet` (0x000F56FD) has the flag and template
  `1EFiendRangedAAM`, which has `FiendsFactionNV` and no flag. Violet's own list happens
  to be the same.

## 3. Templates that are leveled lists

- A `TPLT` can name a leveled list; the placed actor then stands for one actor the list
  produces, and takes the flagged groups from it. Example [observed]: creature
  0x00156766 `VSpawnTier1CoyoteMed` (placed as 0x00157B36 and 0x00157B37 near
  Goodsprings) has no `SNAM`, flags Traits, Stats, Factions, Actor Effect List, AI Data,
  Model/Animation, Base Data, Inventory, and template LVLC 0x001567B1
  `VEncTier1CoyoteMed`. The list has one entry, `VCrTier1CoyoteMed`, which takes its
  factions from `CrCoyote`: VCrCoyoteFaction, CreatureFaction, AnimalFriendFaction,
  CreatureHitDog.
- What the game picks: by the player's level against the encounter zone, at the time the
  actor is made [documented: wiki Category:Leveled Lists]. The details (zone level,
  the reference's level modifier, when the pick is redone) are not in the export.
- "When using a leveled list as template, make sure you have a creature/NPC of level 1 in
  the list. Otherwise the game will crash" [documented: wiki Category:Leveled Lists].
  Every list used as a template by a flagged actor has an entry at level 1 [observed:
  0 without].
- Does the pick matter for the factions? Of the 860 flagged actors whose chain passes a
  list, every actor the lists can produce has the same factions for 668; for 192 the pick
  decides (for example `LvlRaiderRifle`, `LvlWastelandDog`, `LvlSupermutantAllWithCentaur`)
  [observed].
- No placed actor of FalloutNV.esm has a list as its base; lists are only reached through
  `TPLT` [observed: 0 of 6,385].
- **Rule built [design]:** a list stands for one entry, chosen as for the drawn actor
  (actors.md §2.1): the entries of the highest level at or under 1, all entries at or
  under 1 with "Calculate from all levels"; uniform among them from a random stream
  seeded by the reference's FormID; each list picked once per actor, nested lists
  followed; chance none not applied. This is not the game's roll: it reads the list for
  a level-1 player and ignores encounter zones. It is repeatable, and the actor that
  behaves is the actor that is drawn: for the first list on the way the entry is the one
  `fnv_actor::resolve` draws for 3,003 of 3,003 placed actors [observed]. A deeper list
  could differ when the drawn actor's inventory rolls come before it [hypothesis: not
  seen in the data, not checked beyond the first list].
- A list with nothing to pick, or a template that is no actor and no list: the actor
  keeps its own data [design].
- The same walk now supplies Stats and AI Data (actor values) through lists; before, an
  actor with a list as template showed its own placeholder values. AI Packages
  (`GameData::base_package_list`) still stop at a list: not built.

## 4. Which way a reaction points

- Each faction has its own list of reactions towards other factions, and the two lists
  of a pair are separate [documented: wiki SetEnemy, "makes the BrotherhoodSteelFaction
  Neutral to EnclaveFaction but EnclaveFaction will treat BrotherhoodSteelFaction as an
  Enemy"; "Will add the second argument faction to the first argument faction's reaction
  list if it is absent"]. An entry in faction A's list for B says how A's members treat
  B's members.
- Pairs not listed are Neutral [documented: wiki Category:Factions].
- Who attacks whom [documented: wiki Category:Factions and GetFactionCombatReaction]:
  Enemies "are attacked on sight by Aggressive and Very Aggressive actors"; Neutrals
  "will be attacked by Very Aggressive actors"; Friends and Allies cannot attack each
  other. Aggression codes: 0 Unaggressive, 1 Aggressive, 2 Very Aggressive, 3 Frenzied
  [documented: xEdit].
- The two lists of a pair often differ [observed]: of the 975 entries naming another
  faction, Enemy is answered by Enemy 178 times, by Neutral 5 times and by no entry 81
  times; Ally by Friend 21 times. One-way enemies include `CentaurFaction → PlayerFaction`,
  `MQ01SupermutantsGNR → PlayerFaction`, `BrotherhoodSteelFaction → PlayerFaction`
  (Neutral back) and `PlayerFaction → VRRCInitiationFaction`.
- Runtime: `faction_relation(a, b)` reads the reactions of a's factions towards b's, so
  "is this actor hostile to the player" is `faction_relation(actor, player)`. This agrees
  with the documentation above. For the player it never matters in FalloutNV.esm: no
  placed actor is named an enemy by the player's factions without naming the player's
  factions an enemy itself [observed: 0].
- `GetFactionRelation` for actors in several factions (enemy first, then friend, then
  ally) stays a [hypothesis] (actor-behaviour.md §3).

## 5. The player, and why a Fiend reads as neutral

- The player's base (0x00000007) has 22 factions of its own and no template flags:
  `PlayerFaction` (0x0001B2A4), `DukovShareFaction`, `RRShareBed` and nineteen
  `MS08NoteOwnerFaction…` [observed]. They were loaded before this change as well.
- `FiendVioletRef` (0x000F570B): `FiendsFactionNV` lists six factions and
  `PlayerFaction` is not one of them; `PlayerFaction` lists 45 and `FiendsFactionNV` is
  not one of them. So the relation is Neutral in both directions, and the value 0 that
  the sleep work logged is what the records say [observed]. Violet's aggression is 2,
  Very Aggressive, and Very Aggressive actors attack Neutrals [documented, §4]. Whoever
  asks "would this actor attack the player" has to use the aggression together with the
  relation: Enemy and aggression ≥ 1, or Neutral and aggression ≥ 2. Frenzied (3) is not
  described in the export [hypothesis: attacks anyone]. `fnv_game::rest::hostile_to_player`
  (the wait and sleep refusal) reads it this way, so Violet counts as hostile.
- This is how most hostile actors are set up. Placed actors by aggression (AI Data
  source) and their factions' reaction towards the player's [observed]:

  | | Enemy | Neutral | Friend | Ally |
  |---|---|---|---|---|
  | NPCs, Unaggressive | 2 | 806 | 12 | 0 |
  | NPCs, Aggressive | 79 | 2,131 | 6 | 0 |
  | NPCs, Very Aggressive | 36 | 314 | 0 | 0 |
  | creatures, Unaggressive | 0 | 749 | 2 | 7 |
  | creatures, Aggressive | 180 | 618 | 0 | 16 |
  | creatures, Very Aggressive | 70 | 1,357 | 0 | 0 |

  The Very Aggressive, Neutral creatures are the dangerous wildlife: cazadores 191, fire
  geckos 127, small radscorpions 88, bark scorpions 84, lakelurks 61, golden geckos 50,
  deathclaws 44 (counted by factions source).
- **Open:** 618 placed creatures are Aggressive and Neutral to the player, among them
  bighorners 80, giant rats 75, coyotes 51 (the Goodsprings example of §3 is one), worker
  ants 33, bloatflies 32, bark scorpions 31, mole rats 16. By the documented rule they
  leave the player alone until attacked. Whether the game does that for each of them is
  not known from a capture [hypothesis: the rule holds; the AIDT aggro radius fields,
  which the export does not describe, may add to it]. A capture of `GetFactionRelation
  player`, `GetAV Aggression` and `IsInCombat` on a coyote and a giant rat while the
  player walks up would settle it. **2026-10-13:** most of them have Aggro Radius
  Behavior with a radius (giant rats 700, coyotes 1,024, bighorners 500–700, ants 512,
  bloatflies 1,000, bark scorpions 400, mole rats 700) [observed]: they now warn and then
  attack a player who comes inside it (combat-ai.md §13.1).
  **2026-11-01 (o16_air, nvfairport):** two spawned CrCoyote (GetAV Aggression 1) 700
  units from a still player were in combat with it at once and about 1 s in, with no
  10 s of warnings first, and bit it; two at 250 units the same [observed]. Two
  CrDogWild (Aggression 1) at 250 units never detected the player and never attacked
  [observed]. Whether the coyotes' first second held a warning is not readable from the
  fight fields; #1341.

## 6. CLI contract

`fnvtool actor factions --schema <dir> <Data> <plugins…> [--actor <name>]… [--survey]`
prints `fnv.actor.factions/1`.

- `--actor` (`player`, a placed actor, or an actor base: its first placed reference):
  `chain` (each actor on the way with its own factions and flag, each list with its
  entries and the actor picked), `factions_source`, `factions`, `factions_now` (with
  run-time changes; none in the authored game), `aggression`, `aggro_radius` (when its
  AI data turns the behaviour on), `towards_player` and `player_towards_it`.
- `--survey`: `records` (§2, §3), `reactions` (§4), `placed` (§2, §3, §5; with
  `aggro_radius_not_hostile_on_sight`, combat-ai.md §13.1).

Check after changes to templates, leveled actor lists or faction functions:
`fnvtool actor factions --schema schemas/fnv <Data> FalloutNV.esm --survey --actor player --actor FiendVioletRef --actor 00157B36`
and compare with the figures above; explain any change.

## 7. Not built

- The game's own leveled pick (player level, encounter zone, level modifier) and when it
  is redone.
- Disposition modifiers of `XNAM`. (Assistance: combat-ai.md §10.3; the aggro radius,
  cowards and every template group through leveled lists: combat-ai.md §13, 2026-10-13.)
