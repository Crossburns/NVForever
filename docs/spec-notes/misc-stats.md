# Miscellaneous statistics and SetScale

The player's miscellaneous statistics (GetPCMiscStat, ModPCMiscStat, the Pip-Boy's
General list) and a scale set by script (SetScale, GetScale). Code: `crates/fnv-game/src/misc_stats.rs`,
`functions/player.rs`, `functions/objects.rs`; the engine's counters in `engine/nvf-play`
(wait, blackjack, roulette, crafting, Pip-Boy repair, barter); the drawn scale in
`engine/nvf-play/src/moved.rs`, `script_draw.rs`, `phys.rs` and
`engine/nvforever/src/actor_scale.rs`. outstanding.md §1 row 16.

## 1. The codes

- **Names**: the GECK wiki's GetPCMiscStat page lists 43 names for New Vegas, and the
  script source names a statistic by its name in quotes [documented: GECK wiki
  GetPCMiscStat]. The compiled bytes hold a 16-bit code (scripts.md §2.4) [observed].
- **Five codes are in the shipped bytecode**: Locations Discovered 1, Times Addicted 10,
  Sandman Kills 20, Robots Disabled 22, Corpses Eaten 24 [observed: encodings.json, from
  FalloutNV.esm's compiled scripts].
- **The order of all 43** (`misc_stats::NAMES`): Quests Completed 0, Locations Discovered 1,
  People Killed 2, Creatures Killed 3, Locks Picked 4, Computers Hacked 5, Stimpaks Taken 6,
  Rad-X Taken 7, RadAway Taken 8, Chems Taken 9, Times Addicted 10, Mines Disarmed 11,
  Speech Successes 12, Pockets Picked 13, Pants Exploded 14, Books Read 15, Health From
  Stimpaks 16, Weapons Created 17, Health From Food 18, Water Consumed 19, Sandman Kills 20,
  Paralyzing Punches 21, Robots Disabled 22, Times Slept 23, Corpses Eaten 24, Mysterious
  Stranger Visits 25, Doctor Bags Used 26, Challenges Completed 27, Miss Fortunate
  Occurrences 28, Disintegrations 29, Have Limbs Crippled 30, Speech Failures 31, Items
  Crafted 32, Weapon Modifications 33, Items Repaired 34, Total Things Killed 35,
  Dismembered Limbs 36, Caravan Games Won 37, Caravan Games Lost 38, Barter Amount Traded 39,
  Roulette Games Played 40, Blackjack Games Played 41, Slots Games Played 42 [hypothesis,
  supported by everything checked]:
  - the five shipped codes fall at their places [observed];
  - the original's General list shows, from the top, Quests Completed, Locations
    Discovered, People Killed, Creatures Killed, Locks Picked, Computers Hacked, Stimpaks
    Taken, Rad-X Taken, RadAway Taken, Chems Taken [observed: the owner's screenshot,
    2026-10-14, private references "pipboy stat tracking"; values 1, 1, 0, 5, 0 …];
  - the original's saves hold a table of 43 counters (save-files.md §5.1, global type 0)
    [observed]; in the owner's 31 saves the non-zero ones fit these names: the first two 1
    at the start (a quest done and Goodsprings found), slot 3 = 5 with slot 35 = 5 (five
    creatures killed, five things killed), and in "Save 22" slot 2 = 6, slot 3 = 11,
    slot 35 = 17 = 6 + 11 (Total Things Killed is the sum), slot 36 = 10 (Dismembered
    Limbs, after a fight), slot 12 = 1 in two Goodsprings saves, slot 27 = 1 in
    another character's save (a challenge) [observed, read-only, 2026-10-18];
  - the slots of the four Fallout 3 names New Vegas dropped (Bobbleheads Found, People
    Mezzed, Captives Rescued, Contracts Completed) carry four of the new names (16, 18, 19,
    23), and the rest follow from 26 on [hypothesis: the order of the list as the original
    shows it; only 0–10, 12, 27, 35 and 36 are backed by values].
- The console and the script compiler take every name (`encodings.json` `misc_stat`,
  case-insensitive in the console).

## 2. What GetPCMiscStat reads

One counter per code (`PlayerState::misc_stats`, saved with the player). The rules and the
engine add to it as things happen, and ModPCMiscStat adds to the same counter
[hypothesis: one counter per statistic; the scripts' ModPCMiscStat calls are how the
original counts Sandman Kills, Robots Disabled and Corpses Eaten, which no engine event
names]. Locations Discovered also counts the map markers found (`found_markers`)
[hypothesis, one data point: nvftest with only Goodsprings found reads 1].

## 3. What is counted, and where

| Statistic | Counted when | Where |
|---|---|---|
| Quests Completed | a quest is completed (CompleteQuest, a Complete Quest stage), shown in the Pip-Boy or not [hypothesis: the original's "Save 3", in Doc Mitchell's house, already reads 1] | `Game::quest_completed` |
| Locations Discovered | the found map markers (§2) | `Game::misc_stat` |
| People Killed / Creatures Killed / Total Things Killed | the player kills an NPC / a creature; both count in the total [observed: the sum, §1]. Kills by companions do not count [hypothesis] | `Game::kill` |
| Locks Picked | a lock opened by picking (not by force) [hypothesis] | `lockpick::opened` |
| Computers Hacked | a hack succeeded | `terminal::hacked` |
| Stimpaks Taken, Health From Stimpaks | the player uses a Stimpak-type item (ETYP 11); the health its effects restore over their whole time | `misc_stats::ingest_counts` |
| Rad-X / RadAway / Chems Taken | a Chems-type item (ETYP 10): Rad-X when it raises Rad Resistance, RadAway when it lowers rads, a chem otherwise [hypothesis: Rad-X and RadAway are not also chems] | same |
| Doctor Bags Used | an item that is neither chem nor stimpak whose effects only restore limb conditions [hypothesis] | same |
| Health From Food, Water Consumed | a food item (ENIT Food): the health it restores; one water when it lowers dehydration [hypothesis] | same |
| Times Addicted | an addiction roll that addicts | `Game::ingest` |
| Mines Disarmed | the player disarms a placed mine | `Game::default_activation` |
| Speech Successes | a Fallout 3 style speech challenge line is said (the roll is not modelled, dialogue.md §3.3), or the player picks the pass line of a New Vegas Speech check (KNAM, its condition `GetActorValue Speech >= N` met) [hypothesis: by the name; observer-checks.md item 70] | `Game::choose` |
| Speech Failures | the player picks the fail line of a New Vegas Speech check (`< N`) [hypothesis, same]. Checks of other skills and perks count neither [hypothesis, same] | `Game::choose` |
| Challenges Completed | a challenge completes (each time for a recurring one) [hypothesis] | `functions::ui` |
| Books Read | the player reads a skill book (one per book; books that teach nothing do not count) [hypothesis] ([skill-books](skill-books.md) §5) | `Game::read_book` |
| Have Limbs Crippled | a hit cripples one of the player's limbs | `Game::hit` |
| Disintegrations | AttachAshPile turns an actor the player killed to ash or goo [hypothesis: by the name; ash-piles.md §3] | `Game::attach_ash_pile` |
| Times Slept | a sleep starts in the wait menu (not a wait; a sleep already passing that is lengthened does not count again) [hypothesis] | `Game::begin_rest` |
| Items Crafted | a recipe is made (one per make, whatever it yields) [hypothesis] | `nvf-play` crafting |
| Items Repaired | the player repairs an item from the Pip-Boy (a vendor's repair does not count) [hypothesis] | `nvf-play` Pip-Boy repair |
| Barter Amount Traded | an accepted trade: the caps that changed hands [hypothesis: not the goods' value] | `nvf-play` menu |
| Roulette / Blackjack Games Played | a spin is paid; a blackjack hand is dealt | `nvf-play` roulette, blackjack |
| Slots Games Played | a slot machine spin is paid ([slots](slots.md) §4) | `nvf-play` slots |
| Caravan Games Won / Lost | a Caravan game ends (a forfeit is a loss) | `nvf-play` caravan ([caravan](caravan.md) §7) |
| Pockets Picked | an unseen take from a pocket [hypothesis: one per take] | `fnv_game::pickpocket` ([pickpocket](pickpocket.md) §4) |
| Pants Exploded | a live explosive planted in a pocket goes off | `fnv_game::pickpocket` ([pickpocket](pickpocket.md) §5) |

**Not counted** (only ModPCMiscStat changes them): Weapons Created, Sandman Kills,
Paralyzing Punches, Robots Disabled, Corpses Eaten, Mysterious Stranger Visits, Miss
Fortunate Occurrences, Dismembered Limbs (no dismemberment), Weapon Modifications. "Days passed" is not a statistic of the list; it is
the GameDaysPassed global.

## 4. The Pip-Boy

The General list shows the 43 names in code order with their counters
(`engine/nvf-play/src/pipboy/stats`; pipboy.md).

## 5. SetScale

- "Sets the scale of the reference"; GetScale reads it [documented: GECK wiki SetScale,
  GetScale; catalogue summary]. Shipped calls: `player.SetScale 1` (VCG00 stages 0 and
  100), a growing object (`SCPT 0x00032D23`, by 0.002 a frame up to a target), a dug-up
  object shrunk to 0.01 (`SCPT 0x0011E46D`), an artillery piece set to 0 on load
  (`SCPT 0x001043CC`), one set back to 1 (`SCPT 0x001694DB`) [observed: `fnvtool script
  decompile`].
- **Kept on the reference's state** (`fnv_sim::world::RefState::scale`, saved with the
  world); GetScale reads it, else the placement's XSCL, else 1. A save from before kept
  it in `ExtraState::scale`, still read [design].
- **Drawn**: the reference joins `Game::turned` and a `Scaled` event follows; placed
  objects are drawn at the new scale about their origin (`moved.rs`; `script_draw.rs` for
  references a script enabled), also when their cell streams in later. Actors: the root's
  own factor (race height, creature base scale) times the script's scale in place of the
  placement's (`actor_scale.rs`) [hypothesis: the scale multiplies as XSCL does]. The
  player is not rescaled (only `SetScale 1` is shipped).
- **Collision**: rebuilt at the new scale (`phys.rs` `sync_ref`; cells streamed in later
  build it at that scale too). Below 0.001 the reference has no collision and is drawn at
  0.001 (`SetScale 0`) [design]. An actor's collision is its walker's capsule, unchanged.
- Does the original's SetScale also scale an actor's speed or reach? Not known; not done.

## 6. Checks

- `cargo test -p fnv-game --release`: `misc_stats` unit tests (codes, ingestible kinds) and
  `functions.rs` `misc_stats_count_and_set_scale_is_kept_on_the_reference`;
  `nvforever` `actor_scale` unit test.
- Smoke: `misc-stats-scale` (`tools/smoke/scenarios.toml`).
