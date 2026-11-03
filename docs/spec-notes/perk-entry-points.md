# Perk entry points in combat

ESM gap audit row 2 ([esm-gap-audit-2026-10-21](../handoff/esm-gap-audit-2026-10-21.md)),
built in `claude/e2-perk-combat-entry-points`. The general rules of entry points (ranks,
order, the Set/Add/Multiply functions) were first written for aid items
([aid-items](aid-items.md) §10), experience ([experience](experience.md) §2) and skill
points ([levelup](levelup.md) §9); this note adds the argument tabs, the other functions
and the combat entry points. Code: `fnv_game::perks` (evaluation),
`fnv_combat::damage::PerkHooks` / `EntryOps` (the hit), `nvf-play` `combat.rs`
(`hit_perks`, `player_weapon_perk`) and `vats_mode.rs` (AP cost).
Every perk of the masters and whether NVForever acts on it: [perk-audit-2026-10-23](../handoff/perk-audit-2026-10-23.md).

## 1. Entries, functions and ranks

- A PERK effect of type 2 (entry point) has DATA `point, function, parameter type`,
  conditions in tabs (PRKC before the CTDA), EPFT (the kind of EPFD) and EPFD
  [documented: xEdit wbDefinitionsFNV.pas PERK; observed: the layouts, aid-items.md §10].
- The entry point codes 0–73 and their names [documented: GECK wiki
  GetNthPerkEntryFunction, "Entry Function Codes"]; what each modifies [documented: GECK
  wiki Entry Point, its table].
- Functions [observed: every PERK of FalloutNV.esm and the five add-ons]: only 1, 2, 3
  are used on numbers, always with EPFT 1 and one float (Set 28 times, Add 14, Multiply
  96 in FalloutNV.esm); 8 with EPFT 3 (a leveled list) and 9 with EPFT 4 (a script, the
  Activate choices). The names Set Value, Add Value, Multiply Value [hypothesis: from the
  values: 0/1 flags with 1, bonuses with 2, factors with 3].
- The GECK's other number functions, numbered by its Function list's order, which puts
  Add Leveled List eighth and Add Activate Choice ninth as the data shows [hypothesis for
  every code and parameter layout below: no record of the masters uses them]:
  4 Add Range To Value (EPFD two floats, low and high: a number rolled between them is
  added), 5 Add Actor Value Mult (EPFD the actor value code as a 4-byte integer, then a
  float factor: the owner's actor value × the factor is added), 6 Absolute Value, 7
  Negative Absolute Value, 12 Set to Actor Value Mult, 13 Multiply Actor Value Mult, 14
  Multiply 1 + Actor Value Mult. `fnv_game::perks::resolve` turns each into an `Op`
  (Set, Add, Mul, Abs, NegAbs) when it is used; Add Range rolls on the game's RNG.
- Of a perk with several ranks only the highest owned rank's entries apply; entries apply
  by ascending priority, then perk FormID [hypothesis, as aid-items.md §10].
- Only the player has perks (AddPerk on another actor is refused), so only the player's
  hits, the hits the player takes and the player's weapon figures change [design: as
  before; the GECK wiki GetPerkModifier says companions answer by the player's companion
  perks, not built].

## 2. The combat entry points

Tabs [documented: GECK wiki GetPerkModifier, the list of each entry function's
additional arguments; observed: the masters' entries put their conditions on these
tabs, below]. Tab 0 is always the perk owner. "Perks" are FalloutNV.esm's entries
[observed].

| Code | Entry point | Tabs 1, 2 | FalloutNV.esm perks | NVForever |
|---|---|---|---|---|
| 0 | Calculate Weapon Damage | Weapon, Target | 40 entries: Bloody Mess × 1.05, Cowboy × 1.25 (IsInList on tab 1), Demolition Expert × 1.2/1.4/1.6 (IsWeaponSkillType Explosives on tab 1), Lady Killer / Confirmed Bachelor / Black Widow / Cherchez La Femme × 1.1 (GetIsSex on tab 2), Animal Control and the other challenge perks (GetIsCreatureType on tab 2), Center of Mass × 1.15 (GetVATSValue Targeted Part torso on tab 0), Purifier, Pyromaniac, Laser Commander, Heavy Handed, Unstoppable Force, Spray and Pray × 0.25 (GetInFaction on tab 2) … | the damage before critical damage, every hit by the player (`damage::hit`, `weapon_damage`) |
| 1 | Calculate My Critical Hit Chance | Weapon, Target | Laser Commander + 10, Set Lasers for Fun + 2/+ 4, Ninja × 1.15 | the chance after the formula, before VATS's bonus |
| 2 | Calculate My Critical Hit Damage | Weapon, Target | Better Criticals × 1.5, Hunter × 1.75, Ninja × 1.25 and Stealth Girl × 1.1 and The Professional × 1.2 (GetDetected on tab 0: unseen), Heavy Handed × 0.4 | the weapon's critical damage |
| 34 | Calculate Gun Spread | Weapon | Fast Shot × 1.2, Trigger Discipline × 0.8, Steady Chem Set 0, Gun Strength Fail + 1 | the player's actor spread, not the weapon's minimum spread [documented: GECK wiki Entry Point, "Has no effect on a Weapon's Min Spread in New Vegas"]; the crosshair's cone too |
| 36 | Modify Enemy Critical Hit Chance | Weapon, Target | Dream Crusher × 0.5 | the attacker's chance when the player is hit; tab 2 taken as the attacker [hypothesis: the page says "Target"; no entry has conditions] |
| 40 | Action Point Cost | Weapon | Fast Shot × 0.8, Loose Cannon × 0.7, Math Wrath × 0.9, Plasma Spaz × 0.8, Trigger Discipline × 1.2, Beautiful Beatdown × 0.9 | VATS mode's AP a mark, set on entering it (logged `vats mode: perks change the AP cost`) |
| 56 | Modify Damage Threshold (defender) | Attacker, Attacker's weapon | Toughness + 3/+ 6, Sub-Dermal Armor + 4, Kamikaze − 2, Hit the Deck + 25 (Explosives on tab 2), Stonewall + 5 (melee/unarmed on tab 2), Bonds of Steel + 4 | the player's own DT when hit |
| 58 | Modify Damage Threshold (attacker) | Weapon, Target | Shotgun Surgeon Set 10, Piercing Strike Set 15, Companion Suite Set 10 | the DT the player's hit ignores: from 0, taken off the target's DT, never below 0 [documented: GECK wiki Entry Point, "read as 'How much DT to subtract from the targeted actor'"] |
| 57 | Modify Chance for Ammo Item | Weapon | Hand Loader × 2 (IsWeaponSkillType Guns on tab 1), Vigilant Recycler × 2 (Energy Weapons) | the chance of a shot's spent item, the ammo's DAT2 percentage (claude/e13-spent-casings, [spent-ammo-items](spent-ammo-items.md)) |
| 8 | Calculate To Hit Chance | Weapon, Target | Commando × 1.25, Gunslinger × 1.25, Sniper × 1.25 (GetVATSValue Targeted Part head on tab 0), Hobbler × 1.25 (legs), Wired Reflexes × 1.1, Fast Shot × 0.8, Trigger Discipline × 1.2 (IsWeaponSkillType on tab 1), Sneering Imperialist × 1.25 (GetInFaction on tab 2), Camarader-E, Lonesome Road | VATS mode's chance of each mark, read when the mark is made with GetVATSValue answering for it (since 2026-10-23, vats-mode.md §17.2) |
| 33 | Has Concentrated Fire | — | Concentrated Fire Set 1 | + `iVATSConcentratedFireBonus` for each earlier mark on the same part (vats-mode.md §17.2) |
| 28, 41 | Mysterious Stranger, Miss Fortune | — | Mysterious Stranger Set 1, Miss Fortune Set 1 | a helper may come for a VATS mode mark (vats-mode.md §18.2, since 2026-10-24) |
| 67 | VATS HelperChance | — | Friendly Help × 2 | the helpers' odds |
| 35 | Player Kill AP Reward | — | Grim Reaper's Sprint + 20 | AP back after a VATS mode sequence that killed (§18.3) |
| 39 | Action Point Regen | — | Nerves of Steel × 1.2, Atomic! (by rads), Ain't Like That Now × 1.25 | the AP's regeneration outside VATS mode (§18.4) |
| 68 | Modify Item Damage | — | Regular Maintenance × 0.5, Full Maintenance × 0.25, Built to Destroy × 1.15 | the wear of the player's weapon by its own shots and blows and by hits on it [hypothesis: armour wear too? not applied] |

Order on a hit [hypothesis where the formula pages are silent]: weapon damage after the
ammo's Damage Mod; critical chance after the formula, then VATS's bonus, then the
target's Modify Enemy Critical Hit Chance; DT: the target's DT, the ammo's DT Mod, the
target's defender entries, less the attacker's entry 58.

Not wired yet, with why:

| Code | Entry point | Perks | Why not |
|---|---|---|---|
| 37 | Reload Speed | Rapid Reload and the others with this entry point | built: the player's reload rate, Agility's 0.5 + 0.1 × Agility through this entry with the weapon's tab; the cycle and the body's reload clips both run at it (combat-rules.md §2, Reload speed) |
| 38, 43 | Equip Speed, Modify Attack Speed | Quick Draw, Fast Shot, Loose Cannon, Slayer, Melee Hacker, Old Vaquero, Causeless Rebel, Trigger Discipline | the player body plays its clips at their own speed; changing only the cycle's timing would fire before the clip does. The reload's layer rate (`WeaponAnim::reload_rate`) is the way to do it for the draw and attack clips too |
| 53 | Modify Weapon Strength Req | Weapon Handling Set 2 | built 2026-10-25 (§12) |
| 54 | Modify Aiming Move Speed | Run 'n Gun × 2 | built 2026-10-25 (§12) |

## 3. Conditions by tab

Each tab's conditions run on that tab's form, and all tabs' lists must pass (within a
list the OR flags combine as anywhere else) [hypothesis: the GECK keeps one list per
tab]:

- tab 0 on the perk owner, with the other actor of the hit (the target, or the attacker
  for the defender's entries) as the conditions' Target;
- tabs 1 and 2 on the form the entry point gives there (a weapon's base form, an actor's
  reference), with the owner as their Target [hypothesis].

An entry with conditions on a tab the caller has no form for (a shot on an object has no
target; the aid, experience and skill point entry points give no tabs) is not applied
[hypothesis]. A condition on a weapon tab runs on the weapon's base form: IsInList and
IsWeaponSkillType take a WEAP as its own base (`fnv_game::functions::base_of`)
[hypothesis].

## 4. GetVATSValue's query codes

Center of Mass and Sniper test GetVATSValue on tab 0, query 5 with the condition actor
values 26 (torso) and 25 / 31 (head, brain) [observed], so query 5 is Targeted Part, not
the page's sixth entry. The query codes are therefore the GECK wiki page's order with
Weapon Type Is moved last [hypothesis; observed: across FalloutNV.esm's 248 GetVATSValue
conditions codes 0, 2 and 3 take a form, 4, 7, 12–14, 16 and 17 none, 5 a condition
actor value, 6 the numbers 15 or 18, 15 a small number 1–13 like an animation type]: 0
WeaponIs, 1 Weapon In List, 2 Target Is, 3 Target In List, 4 Target Distance, 5 Targeted
Part, 6 VATS Action, 7 Is Success, 8 Is Critical, 9 Crit Effect Is, 10 Crit Effect In
List, 11 Is Fatal, 12 Explode Part, 13 Dismember Part, 14 Cripple Part, 15 Weapon Type
Is. (`fnv_game::vats`; vats-mode.md §12.) GetVATSValue answers only while VATS mode's
shot is leaving, so Center of Mass applies to VATS mode's shots that land while it fires
[hypothesis].

## 5. Checks

- `fnv-combat` `tests/rules.rs` `perk_entry_points_change_the_hit`: each damage entry
  point with the masters' numbers.
- `fnv-game` `tests/perk_combat.rs`: tabs 1 and 2 by weapon and target, the owner's tab,
  ranks, the defender's tabs, every function.
- Smoke `perk-combat-entry-points`: Bloody Mess and Confirmed Bachelor change the
  player's 10mm shot on Easy Pete (12.65 → 14.61, × 1.05 × 1.1), Math Wrath the VATS AP
  cost (17 → 15.3) [observed: run 2026-10-21].

## 6. Teammate perks (2026-10-23, playtest list 7 item 44)

- [documented: GECK wiki AddPerk, HasPerk] `player.AddPerk perk 1` (forTeammates)
  gives the perk to the player's teammates instead of the player.
- [observed: the masters' scripts and PERK records] The shipped calls are
  VCG01RemoveDocCarebearModeTriggerSCRIPT's `player.AddPerk CompanionSuite 1` on leaving
  Doc Mitchell's, and the companions' quest rewards in dialogue result scripts (Old
  Vaquero, Hand of Vengeance, Calm Heart, Bonds of Steel, Causeless Rebel, three in the
  add-ons). Each has one rank; their entry points test `GetIsID` of the companion they are
  for on the owner's tab ("Cass does 15% more damage with all Guns"), Calm Heart and
  Companion Suite also carry an Ability. P04CompanionFireQuestSCRIPT removes Companion
  Suite from each companion it fires (`CraigBooneREF.RemovePerk CompanionSuite`).
- Built: `Game::held_perks` gives a teammate (SetPlayerTeammate 1, alive) the teammate
  perks at rank 1, so their entry points apply to its hits, its DT and the rest as the
  player's own do (`Game::entry_point_entries`; combat asks for a hit's perks when the
  attacker or the target is the player or a teammate), and their abilities join its spell
  list while it is a teammate and leave when it is not (`Game::sync_perk_abilities_of`,
  log `perks: <actor>: <perk>'s ability <spell> on` and `perks: <actor>'s ability
  <spell> off`) [hypothesis: held only while a teammate, one rank; a companion's
  RemovePerk needs nothing, it is no longer a teammate when it is called].
- Not built: ~~entry points the engine asks only of the player (attack speed, rate of
  fire: Old Vaquero's and Causeless Rebel's), so those two do nothing yet~~ (built
  2026-10-25, §12). An ability of a
  teammate perk the player loses (`player.RemovePerk perk 1`) stays on the teammates [design:
  no shipped script does that].
- Checks: `fnv-game` `tests/perk_combat.rs` `teammate_perks_apply_to_teammates`; smoke
  `teammate-perk`.
- NPC perks (2026-10-27): `<actor>.AddPerk perk` on an NPC or creature gives it the perk
  (`RefExtra::perks`, saved with the reference; rank counted up to the PERK's ranks),
  `RemovePerk` takes it back, `HasPerk` reads `Game::held_perks` (own perks, and the
  teammate perks while a teammate), so its entry points and abilities apply as a
  teammate perk's do [documented: GECK wiki AddPerk; observed: FalloutNV.esm calls
  `<companion>.AddPerk CompanionSuite` in 25 hire lines, `RemovePerk CompanionSuite` when
  a companion is fired (P04CompanionFireQuestSCRIPT and ~60 lines), `ActionRef.HasPerk
  LightStep` in mine/trap scripts and `HasPerk Stonewall` of NPCs in knock-down scripts;
  `QJDeathclawBabyScript` gives the Quarry's mother three perks; hypothesis: no Quest +
  Stage effects for an NPC's rank, forTeammates ignored on an NPC]. Before this, each call
  on an NPC failed (`call_failed`) or answered 0. NVSE GetPerkRank / HasPerk-for-teammates
  readers (nvse_actor.rs) still read the player only. Check: `tests/functions.rs`
  `npc_perks_are_kept_per_actor`.

## 7. Locks, terminals, sneaking, weight and travel (2026-10-24)

Owner 2026-10-24: "Build the next batch: Infiltrator, Computer Whiz, Silent Running, Pack
Rat, Heavyweight, Hoarder's carry weight and Long Haul" (perk-audit-2026-10-23 §3.3,
§3.4). What each entry point does [documented: GECK wiki Entry Point]; the perks'
entries [observed: FalloutNV.esm, DeadMoney.esm, OldWorldBlues.esm PERK]. None of them has
conditions, and all are the player's (`perks::player_has_entry`, `Game::sync_perk_weights`).

| Code | Entry point | Perk | NVForever |
|---|---|---|---|
| 32 | Ignore Broken Lock: "run when the player attempts to open a broken lock; if non-zero, another chance will be granted" | Infiltrator, Set 1 ("can make one more attempt to pick a broken lock") | a broken lock opens the lockpicking menu once more (`lockpick::opening`); breaking it again spends the try (`Locks::retried`, saved), and it stays broken for good |
| 20 | Ignore Locked Terminal: "run when the player attempts to activate a locked terminal; if non-zero, another chance will be granted" | Computer Whiz, Set 1 ("one extra attempt to hack a locked-down terminal") | a locked-out terminal can be hacked once more (`terminal::opening`); locked out again, the try is spent (`Terminals::retried`, saved as `terminal_retries`) |
| 31 | Ignore Running During Detection: "if non-zero, running will be ignored" | Silent Running, Set 1 | the player's running does not raise the noise it makes to a detector (`fSneakRunningMult` not applied, `detection::Pair::ignore_running`) |
| 55 | Modify Light Items: "if set to a non-zero value, modifies the weight of 'Light' items in the reference's inventory according to fPackRatModifier" | Pack Rat, Set 1 ("items with a weight of 2 or less weigh half as much") | each unit of at most `fPackRatThreshold` weighs × `fPackRatModifier` in the player's load; FalloutNV.esm sets neither setting [observed], so the defaults 2 and 0.5 come from the perk's text [observed] |
| 73 | Adjust Heavy Weapon Weight: "modifies the weight of a Weapon which weighs 10.0 lbs or more (after the DecreaseWeight Weapon Mod effect is applied)" | Heavyweight, Multiply 0.5 | each weapon unit of 10 or more (its mods applied) weighs × the value |
| 22 | Get Max Carry Weight: "run when the actor's max carry weight is calculated" | Hoarder, Add 25 | the player's capacity (`functions::actor::sync_carry`; its −1 SPECIAL ability below 160 lbs was already an ability) |
| 51 | Has Fast Travel Always: "access Fast Travel even when overencumbered" | Long Haul, Set 1 | the Pip-Boy's fast travel takes an over-encumbered player (`pipboy::data::travel`) |

- The weights [design where the wiki is silent]: the rules (`fnv_sim::inventory::
  WeightRules`) are applied to each unit's weight with its mods, in the load the world
  counts (encumbrance, the menus' weight total); the inventory rows still show each item's
  own weight [hypothesis: the original's rows are not checked]. Both rules at once: a
  weapon of 10 or more takes Heavyweight's, a lighter unit Pack Rat's. Kept in step every
  frame with the abilities (`Game::sync_perk_weights`), logged `perks: the player's item
  weights: …` when they change.
- The extra tries [design]: "another chance" is taken as one more menu after the lock
  broke or the terminal locked out, until it breaks or locks out again; leaving the
  menu without failing keeps the try.
- Checks: `fnv-sim` `perk_weight_rules_halve_light_items_and_heavy_weapons`; `fnv-game`
  `perk_combat` `weight_perks_change_the_players_load`,
  `silent_running_ignores_the_players_running`, `a_second_failure_spends_the_extra_try`;
  smoke `perk-infiltrator`, `perk-computer-whiz`, `perk-long-haul`, `perk-weights`.

## 8. Mines, explosions, throws and movement (2026-10-24)

Owner 2026-10-24: "Build the next batch: Splash Damage, Heave Ho!, Light Step, and the
run-speed perks (Travel Light, Tunnel Runner, Implant M-5)" (perk-audit-2026-10-23 §3.2,
§3.5). What each entry point does [documented: GECK wiki Entry Point]; the entries
[observed: the PERK records].

| Code | Entry point | Perks | NVForever |
|---|---|---|---|
| 4 | Calculate Mine Explode Chance: "run when the actor comes within proximity of an armed mine; modifies the chance of an armed mine detonating" | Light Step Set 0; In My Footsteps (Dead Money) Set 0; ED-E's Light Step (Lonesome Road, NVDLC04EDENoMines) Set 0 under GetIsID ED-E | an actor whose chance is 0 does not start a mine's fuse (`nvf-play` `mines::tick`), logged once a mine `mines: <actor> does not set off mine <ref> … (… Light Step)`; the player's own, and a teammate's with the teammate perks. Chances between 0 and 1 are not rolled [design: the masters set only 0] |
| 72 | Adjust Explosion Radius | Splash Damage × 1.25 | the radius of every explosion the player (or a teammate with the perk) causes, before its hits, the pushed bodies and the destructibles are found (`combat::owner_weapon_perk` with the cause's weapon on tab 1); the explosion's look is not scaled [design] |
| 59 | Modify Throwing Velocity: "run when ANY projectile is created; modifies the velocity of ANY projectile spawned by the perk owner (filtered by conditions); use GetWeaponAnimType == 9 to restrain the effect to throwing-type weapons" | Heave, Ho! × 1.5 (no conditions); Loose Cannon × 0.75 (its condition) | every projectile the player fires leaves that much faster, as the page says (Heave Ho!'s entry has no condition, so the player's bullets too); VATS mode's throws aim their arc at that speed |
| 42 | Modify Run Speed: "changes the actor's movement speed" | Travel Light × 1.1 (GetEquipped NVTravelLightBadArmorList, the armours it does not allow), Tunnel Runner × 1.25 (sneaking, the same list), Implant M-5 × 1.2 (sneaking), Atomic! × 1.25, Stealth Suit Mk II × 1.2 | the player's movement, walking, running and sneaking alike (`player::PerkSpeed`, worked out every frame by `combat::perk_speed`), logged `movement: the Modify Run Speed perks make the player move at <p>% (<perks>)`. NPCs' speeds (the Lobotomites' × 0.75) are not changed [not built] |

Checks: `fnv-game` `perk_combat` `mine_blast_throw_and_speed_perks`; smoke
`perk-light-step`, `perk-heave-ho-splash`, `perk-run-speed`.

## 9. Kills, the HUD, repair and drinking (2026-10-24)

Owner 2026-10-24: "Build Meltdown, Living Anatomy's enemy health display, Jury Rigging
and Implant Y-3 next" (perk-audit-2026-10-23 §3.7). What each entry point does
[documented: GECK wiki Entry Point]; the entries, one each with no conditions [observed:
the PERK records].

| Code | Entry point | Perk | NVForever |
|---|---|---|---|
| 46 | Has Meltdown: "Enemies killed with Energy Weapons explode" | Meltdown, Set 1 ("foes killed by your Energy Weapons … give off a corona of harmful energy. Note: this can cause a chain reaction") | a foe the player (or a teammate with the perk) kills with a weapon of the Energy Weapons skill sets off the DOBJ's Meltdown Explosion (place 24 [documented: xEdit wbDefinitionsFNV.pas DOBJ, "Meltdown Explosion"]; FalloutNV.esm names `MinePlasmaExplosion` 0007078C [observed]) at its body, as the killer's blast with that weapon, so its own kills chain (`nvf-play` `combat::meltdown`, from the hit's `ActorHit`, which now carries the weapon); a body gives one off once |
| 47 | See Enemy Health: "Enemy Health and DT values are displayed with their health bar" | Living Anatomy, Set 1 (its damage entry was already built) | a line under the enemy bar's name, centred in its red and font, "HP:<health> DT:<dt>", the health whole and the DT to two places (`hud_target::TargetHealth::anatomy_line`; DT as combat counts it, armour and the actor's own) [observed: the owner's screenshot of the original 2026-10-24, a Deathclaw: "HP:250 DT:15.00" under "Deathclaw", about 36 interface units lower] |
| 48 | Has Jury Rigging: "access alternate and predefined - but far more inclusive - repair lists" | Jury Rigging, Set 1 | the Pip-Boy's repair takes, besides the item's repair list, any carried item of the same family (`fnv_game::repair::jury_rigs`): a weapon of the same WEAP skill and animation type, armour of the same class worn on the same part of the body [design: the original's lists are not in the records; the perk's DESC gives the examples it must allow: "a Trail Carbine with a Hunting Rifle, a Plasma Defender with a Laser Pistol, or even Power Armor with Metal Armor", which this rule does] |
| 26 | Adjust Drinking Radiation: "run when the player drinks from a placed water source; modifies the radiation from placed water sources" | Implant Y-3 × 0.01 | the rads of the water's actor effect when drinking from a sink, a toilet or the like (`Game::drink_from`), logged `perks: drinking from <ref>: rads x <m> (<perks>)` |

Drinking by hand (E on a sink): the player's hand path (`nvf-play` `script.rs`, `by_hand`)
activated the water source in the world and never reached `Game::drink_from`, so no
water's actor effect applied by hand; it now drinks as `Game::default_activation` does
(found by smoke `perk-implant-y3`, 2026-10-24).

Jury Rigging's flag is read without its conditions (`Game::holds_entry_point`), where the
Pip-Boy holds only a shared borrow of the game [design: the masters' entry has none].

Checks: `fnv-game` `repair` `jury_rigging_pairs_like_weapons_and_armour`, `data`
`default_objects_by_their_place`; smoke `perk-jury-rigging`, `perk-meltdown-anatomy`,
`perk-implant-y3`.

## 10. Attack, reload and equip speed (2026-10-24)

Owner 2026-10-24: "Build the clip-speed perks next: Rapid Reload, Quick Draw, and the
attack-speed perks (Fast Shot, Slayer, Loose Cannon …)" (perk-audit-2026-10-23 §3.2). §2
left these until the player body's clips could play faster; they now can: an animation
layer has a rate, and the reload already used it.

| Code | Entry point | Perks [observed: the entries] | NVForever |
|---|---|---|---|
| 37 | Reload Speed | Rapid Reload × 1.25 | already built with Companion Nerve (feb023fa): the reload rate is 0.5 + 0.1 × Agility × the Reload Speed entries, for the cycle and the reload clips (`fnv_combat::attack::reload_rate`, `WeaponAnim::reload_rate`) |
| 43 | Modify Attack Speed | Fast Shot × 1.2 and Trigger Discipline × 0.8 (IsWeaponSkillType Guns or Energy Weapons on tab 1), Loose Cannon × 1.3 (its list on tab 1), Slayer × 1.3 (Melee or Unarmed), Melee Hacker × 1.05 / 1.1 (Melee), Elijah's Last Words, Atomic!, Ain't Like That Now; Old Vaquero and Causeless Rebel (teammate perks) | the player's attack: a gun's shot cycle (`Timing::with_attack_rate`: the time between shots, the wind-up to the shot and the `Attach` key ÷ the rate; an automatic weapon's fire rate × it [design]) and a melee swing's timing (`SwingTiming`), and the body's attack clip plays at the weapon's Animation × Animation Attack multipliers × the perks (`WeaponAnim::attack_rate`), logged `combat: attacks at x<r> (<weapon>; perks x<p>: <perks>)` and `combat: swings at x<r> …` |
| 38 | Equip Speed | Quick Draw × 1.5 | the draw and holster of the weapon in hand: their timings (the hands' `Transition`s, the `Attach`/`Detach` switch with them) ÷ the rate and their clips at the rate (`WeaponAnim::equip_rate`), logged `combat: draws and holsters at x<r> (<perks>)` |

- The attack clip now plays at the weapon's own multipliers too [documented: GECK wiki
  Weapons, "Animation Multiplier", "Animation Attack Multiplier" scale the animation];
  before, the cycle was timed by them while the clip played at its own speed. For a
  weapon with both at 1 nothing changes.
- The teammates' attack speed (Old Vaquero, Causeless Rebel): §12.

Checks: `fnv-combat` `rules` `attack_rate_shortens_the_cycle_not_the_reload`; smoke
`perk-fast-shot-quick-draw`, `perk-slayer`.

## 11. Knockdowns and the unarmed special moves (2026-10-24)

Owner 2026-10-24: "Build melee knockdowns (Super Slam, And Stay Back) and the unarmed
special moves next" (perk-audit-2026-10-23 §3.6).

### 11.1 Knockdown Chance (52)

[observed: the entries] Super Slam Set 0.15 and 0.30 (conditions on tab 0: IsWeaponInList
and GetWeaponAnimType; the fandom wiki: "15% for Unarmed or one-handed melee, 30% for
two-handed melee"; the conditions want the weapon in WeaponListMeleeALL or
WeaponsAutomaticMeleeLIST, which the Fists are not in [observed: the FLSTs], so bare fists
get no roll whatever the fandom page says), And Stay Back Set 0.1 (IsWeaponInList on tab 0, the shotguns; the
fandom wiki: "per pellet"), Elijah's Last Words Set 0.25 (Melee on tab 1).

NVForever [design where the wiki is silent]: from 0, the attacker's entries with its
weapon on tab 1 (`combat::perk_knockdown`), rolled once a hit that does not kill: each
melee blow, each projectile (so each pellet); the player's and a teammate's with the
perk. A success knocks the target down away from the attacker as a blast's knockdown
does (`Game::knock_down`: No Knockdowns and the rest refuse it). Every roll is logged
`combat: Knockdown Chance <c> (<perks>): rolled <r>, <ref> stays up`, a success
`combat: Knockdown Chance … knocks <ref> down`.

### 11.2 The unarmed special moves (61–66)

[observed: the entries, each Set 1 with no conditions] Legion Assault 61 (forward), Ranger
Takedown 62 (back), Scribe Counter 64 (counter), Khan Trick 65 and 66 (left, right); no
perk uses 63 (crouched). The DOBJ names each direction's effect [documented: xEdit DOBJ,
places 25–30; observed: FalloutNV.esm]: none for forward and counter,
`VictoryRifleKnockdownSpell` for back (its script pushes the target away: a knockdown,
unless it has Stonewall), `HeadCrippleEffect` (Perception −4) for left, right and
crouched. The perks' DESC [observed]: "perform a Power Attack while running forward /
moving backwards / moving left or right".

NVForever:
- An unarmed power attack (the Fists or an Unarmed-skill weapon) in a direction whose move
  the player has is that move (`combat::special_move`, logged `combat: <move> (<dir> power
  attack, unarmed)`): Ranger Takedown and Khan Trick put the DOBJ's effect on the target
  with the blow (as a weapon's object effect, `apply_hit`'s items); Legion Assault knocks
  the target back [the fandom wiki, "Knock back opponents"; design: as a knockdown away
  from the player].
- Scribe Counter [the fandom wiki: "Quick counter-attack after Block"; design]: an unarmed
  blow begun within `COUNTER_WINDOW` (1 s) of the player's block stopping one is a
  standing power attack (its damage bonus, breaking a block), without the power attack's
  hold; logged `combat: Scribe Counter: the blow after the block is a power attack`.
- Not built: Khan Trick's need for dirt or sand underfoot (the fandom wiki), the crouched
  move (no perk), the moves in VATS mode. Khan Trick's forced hit reaction and Legion
  Assault's lunge: §11.4.

### 11.3 The moves' clips (2026-10-24)

Owner 2026-10-24: "the animations should exist so lets add them". They do, in both rigs'
folders (`meshes\characters\_male`, `_1stperson`) of Fallout - Meshes.bsa [observed]:

| Move | Clip (prefix `h2h`) | Why this one |
|---|---|---|
| Legion Assault (forward) | `attackcustom1power` (AttackCustom1Power, 1.6 s) | [observed: its Bip01 goes 332 units straight ahead over the clip, a running lunge] |
| Ranger Takedown (back) | `attackcustom2power` (AttackCustom2Power, 1.43 s) | [hypothesis: by elimination; its motion is in compressed splines not read here] |
| Khan Trick (left, right) | `attackcustom4power`, `attackcustom5power` (1.83 s) | [documented: GECK wiki GetForceHitReaction, "Khan Trick (AttackCustom4Power and AttackCustom5Power)"; hypothesis: 4 left and 5 right, the entry points' order] |
| crouched (no perk) | `sneakh2hattackcustom3power` | not used |
| Scribe Counter | `counter` (`h2hcounter.kf`, its `a:L` key chains a swing) | [hypothesis: by the name] |

The move is chosen as its swing starts (`combat::melee`, `MeleeEvent::Started`): the
swing's stem becomes the move's, so the body plays the move's clip and the strike comes at
that clip's `Hit` key; Scribe Counter's swing becomes a standing power attack then. Logged
`combat: <move> (<dir> power attack, unarmed): clip <stem>`.

Khan Trick in the original also sets the target's forced hit reaction (GetForceHitReaction
1, which plays its crippled-limb hit idles) [documented: the same page]; built in §11.4.

### 11.4 Khan Trick's stun, Legion Assault's lunge (2026-10-24)

Owner 2026-10-24: "Add Khan Trick's forced hit reaction (the stun) and move the player
forward during Legion Assault's lunge."

- Khan Trick: GetForceHitReaction is 1 on the target of AttackCustom4Power and
  AttackCustom5Power [documented: GECK wiki GetForceHitReaction]. FalloutNV.esm's IDLE
  `3rdPersonHitReactionsForce` (0014CC29, condition GetForceHitReaction == 1) and
  `3rdPCrouchedHitForce` (0014CC2A) hold one child per hit location
  (`3rdPHitTorsoForce` 0014CC2B for location 0, `…HeadForce` 1–2, `…ArmLeftForce` 3–4,
  `…ArmRightForce` 5–6, `…LegLeftForce` 7–9, `…LegRightForce` 10–12, by GetHitLocation),
  whose model is the IdleAnims folder alone: they play the actor's hit reaction for that
  location [observed: the records; hypothesis: the same clips as the stagger's]. So a Khan
  Trick blow that leaves its target alive and conscious sends `combat::ForcedHitReaction`
  (logged `combat: Khan Trick: <target> is forced into its hit reaction (location <n>)`),
  and the body plays the reaction for the location at once, without the stagger rule's
  crippling or wait (nvforever `deaths`; logged `hit reaction: <ref> staggers for <s> s
  (location <n>, forced: Khan Trick)`). The condition function itself stays a stub: no
  idle picker here asks it.
- Legion Assault: its clip `h2hattackcustom1power.kf` moves `Bip01` 332 units straight
  ahead over 1.6 s [observed: 18 keys]. The body holds `Bip01` still, and the player is
  moved by the clip's own travel, key by key, as every melee clip whose root travels
  (actors.md §8.13, #1069; before, a fixed `combat::LUNGE` of 332 units over 1.6 s at an
  even speed): `player::Lunge` takes the third-person body's attack layer's root travel
  each frame, at the swing's speed (the attack-speed perks), on top of walking; walls and
  actors stop it as they stop walking [design]. Logged `combat: Legion Assault: the player
  lunges with its clip`, `player: the body's attack clip carries the player` and, at its
  end, `player: lunge done: <d> units from where it started`.

Checks: smoke `perk-unarmed-moves` (with frames of each move; the stun and the lunge), `perk-scribe-counter`, `perk-super-slam`, `perk-and-stay-back`, `perk-legion-lunge`.

## 12. The spread perks, the teammates' attack speed (2026-10-25)

Owner 2026-10-24: "can you continue on with the rest of the perks".

- 53 Modify Weapon Strength Req: "Add Value of 1 means a reduction of 1 point from the
  requirement" [documented: GECK wiki Perk entry points]; the requirement counts in the
  gun spread's Strength penalty [documented: GECK wiki Gun Spread Formula (New Vegas),
  "StrReqPenalty"]. Weapon Handling sets it to 2 [observed: DATA 35 01 02, EPFD 2.0]. The
  player's entries of 53, from 0 with the weapon at tab 1, are points taken off the
  requirement (`Stance::strength_req_cut`, no shortfall below none).
- 54 Modify Aiming Move Speed: the walking or running penalty is divided by it, "it
  cannot divide below 1" [documented: the same page]. Run 'n Gun multiplies it by 2
  [observed: EPFD 2.0, with a weapon-type condition on tab 0, value 4]. From 1, into
  `Stance::aiming_move_divisor`. Both go into every spread of the player's
  (`combat::spread_perks`, the crosshair, the shot, VATS mode's hit chance), logged
  `combat: spread perks for <weapon>: Strength requirement -<n>, moving penalty / <d>
  (<perks>)` when they change. Not built: the Strength requirement in the throwing distance
  (the same page says it shortens throws); the Pip-Boy still shows the record's
  requirement.
- 43 Modify Attack Speed on teammates: Old Vaquero (× 1.33, GetIsID RaulTejada on tab 0,
  IsInList OldVaqueroWeapons on tab 1) and Causeless Rebel (× 1.3, GetIsID Veronica)
  [observed: FalloutNV.esm]. A teammate holds the teammate perks (§6), so an NPC fighter
  that is a teammate asks the entry point for its weapon (`npc_combat::teammate_attack_rate`):
  its gun's shot cycle (`Timing::with_attack_rate`, also when its reload is re-timed) and
  its blows' strike time take the rate; logged `npc combat: <name> <ref> fires its
  <weapon> at x<r> (the teammate perks)` or `strikes at x<r>`. The NPC's clips still play at
  their own speed [not built].
- 3 Calculate Weapon Attack AP Cost (Ain't Like That Now × 0.8): "run when the player
  attacks outside of VATS" [documented: GECK wiki Perk entry points]; attacks outside VATS
  cost no AP in the game, so from 0 the entry gives 0 [hypothesis: no base cost]. Nothing to
  build.
- NVDLC03LobotimiteReduceSpeedPERK (× 0.75 run speed): a player perk, added by the effect
  script NVDLC03LobotomiteReduceSpeedSCRIPT (`Player.AddPerk` at its start, `RemovePerk` at
  its finish) [observed: OldWorldBlues.esm], so §8's run speed already applies it.

Checks: `fnv-combat` `rules` `spread_perks_cut_the_strength_and_moving_penalties`; smoke
`perk-spread-perks`, `perk-teammate-attack-speed`.

## 13. The companion perks: Spotter, Search and Mark, Enhanced Sensors (2026-10-25)

The three entry points [documented: GECK wiki Perk entry points]: 69 Has Improved
Detection, "Max compass range and VATS targeting of cloaked enemies"; 70 Has Improved
Spotting, "Enemy highlighting"; 71 Has Improved Item Detection, "Item highlighting". Each
perk sets its point to 1 with no conditions [observed: `EnhancedSensors` DATA 45 01 01,
`Spotting` 46 01 01, `SearchAndMark` 47 01 01, EPFD 1.0], held by the player while the
companion follows (the companions' scripts add and remove them). The highlights are the
DOBJ's effect shaders [observed: FalloutNV.esm DOBJ slot 31 Spotter Effect
`fxSpotterShader` 0015FF60, a red edge (200, 55, 77) and no fill, flags 8 (no particles);
slot 32 Item Detected Effect `fxSearchAndMark` 0015FF61, a green fill (0, 255, 0) with
`effects\fxparttinyglowy.dds` pulsing and a green edge (85, 195, 85)].

- When: "whenever the player is actively aiming" (the fandom wiki, for both; the GECK wiki
  gives no time) [hypothesis]: while the player aims with a weapon out
  (`nvf_play::perk_marks`, ten times a second), the marks play as effect shaders
  (nvforever `effect_shaders`, a play of its own per reference and shader, until the
  player stops aiming or the mark leaves the list; logged `effect shader: <shader> on
  <ref> (…, while the player aims (a companion perk))`). The counts are logged `perk
  marks: <perk> marks <n> <what> while the player aims` when they change.
- Spotter: the actors the compass would show red (fighting the player or ready to attack
  it, `compass.rs`, aware of the player or not), alive and present, as far as actors are
  drawn (`perk_marks.rs` `ActorDrawDistance`, #1171): every actor of the loaded cells
  (uGridsToLoad's square round the player's cell, `--radius`, outdoors; the whole cell
  indoors), or, with the settings page's Actors draw-distance slider below 100%, those
  within the slider's part of the loaded cells' reach (`draw_distance.rs`). In the
  original [observed 2026-10-31, owner, observer check 19]: only while aiming, every
  hostile actor or creature whether or not in combat with the player, and seemingly every
  one in render distance [hypothesis: its render distance for actors is the loaded cells
  cut by its actor fade setting; ours has no such fade but the slider]. Before #1171 the
  reach was the detection reach (fSneakMaxDistance x the exterior multiplier: 2,500 /
  5,000 units, #1094).
- Search and Mark: items lying in the loaded cells, placed or dropped at run time
  (`World::created`), within `SEARCH_RANGE` 1500 units [design: "within a short
  distance", the fandom wiki; no setting names it]: ALCH of equip type Chems or Stimpak,
  WEAP of skill Guns or Energy Weapons, AMMO [hypothesis: the fandom wiki's "chems,
  firearms & ammunition"].
- Enhanced Sensors, compass: the compass's actor range is Perception 10's, the most a
  SPECIAL reaches, whatever the player's Perception (`compass::actor_range`) [hypothesis:
  "max compass range" read as the range at the top of the scale].
- Enhanced Sensors, VATS: an actor whose Invisibility or Chameleon is at least 1 (a
  Stealth Boy, a Nightkin's cloak) is not marked in VATS mode without the perk (logged
  `vats mode: <ref> is cloaked: not marked …`), and is with it (`… marked through
  Enhanced Sensors`) [documented: the entry point's description; hypothesis: those actor
  values are what "cloaked" reads]. This rule did not exist before: VATS mode marked
  cloaked actors like any other.
- Not built: Spotter and Search and Mark in VATS mode; the shaders' z test "Greater
  Than" (seen through walls) is not built for any shader (effect-shaders.md §4).

Checks: smoke `perk-spotter` and `perk-search-and-mark` (frames before and while aiming:
the dogs glow red, the dropped pistol green), `perk-vats-cloaked`, `perk-enhanced-sensors`.

## 14. Better Healing: Modify Recovered Health (12) (2026-10-25)

The sources disagree: the GECK wiki's entry point is "Run when the player's natural
healing rate is calculated; Modifies a bonus to the natural healing rate" [documented],
the fandom wiki's Better Healing "Regain +20% more health from all consumable sources".
The records: Better Healing multiplies point 12 by 1.2 with no conditions [observed:
DATA 0c 03 01, EPFD 1.2]; Cardiac Arrest (Old World Blues) by 1.25. So the original game
was asked (unattended captures with nvf_capture, the `nvftest` save, Health 200,
2026-10-25; private: `C:\fnv-private\capture\out\pk_better_healing*.jsonl`) [observed]:

- No natural healing: hurt to 120 or 30, the player stays there for 10 s, with the perk or
  without.
- A Stimpak (30 at once, Medicine potency): +40 without, +48 with the perk at Medicine 17;
  +90 and +108 at Medicine 100 (117, capped).
- A Brahmin Wellington (5 a second for 10 s, Survival potency): from 10 health, without
  → with the perk: Survival 0: 50 → 60; 32: 80 → 90; 50: 100 → 120; 70: 120 → 140; 100:
  150 → 180.

Every figure is the effect's magnitude × the skill potency (aid-items.md §4) × 1.2 with the
perk, the per-second amount of an effect over time cut to a whole number (32 Survival:
5 × 1.64 = 8.2 → 8 a second, with the perk 9.84 → 9) and an effect at once's total too
(40.2 → 40, 48.24 → 48). So the entry point applies to the health ingestibles restore,
the fandom wiki's reading, and the GECK wiki's "natural healing" has nothing to act on.

As built: `Game::ingest_on` (and the Pip-Boy's EFFECTS figure, `Game::aid_preview`) asks
point 12 for the user from 1, and `aid::Mods::health` multiplies a beneficial change of
Health by it (`Mods::on_change`); logged among the use's perks (`aid: the player used …;
perks: BetterHealing (entry point 12)`). Not built: the whole-number cut, which applies to
all healing, not to the perk (NVForever's healing keeps the fraction); Cardiac Arrest's
condition, if any, is the record's (not looked at beyond its entry).

Checks: `fnv-game` `aid::tests::better_healing_multiplies_restored_health_only` (the
captured figures); smoke `perk-better-healing`.
