# Combat design: weapons, projectiles, damage, melee

Status 2026-10-09: **design for review; steps C1–C8 built**: data, rules, hit volumes, the
player shooting, damage applied, melee, explosives and destructibles, limb hits (branches
`claude/combat-data`, `-rules`, `-hitvol`, `-shoot`, `-damage`, `-melee`, `-explosives`,
`-limbs`; docs/spec-notes/combat-data.md, docs/spec-notes/combat-rules.md §7–12). Owner
requests 2026-10-09:

1. On top of the player body (docs/design/player-body.md: controls, equipping, drawing and
   holstering), "the actual damage/projectile systems in place for shooting and melee
   combat".
2. Work through it one step at a time, starting with organising the weapon and
   projectile data from the records and the references we have.
3. "There's no way to get a 1 to 1 comparison to the base game so we can take some
   liberties … improvements using more modern design might even be beneficial."
4. "The actual combat never plays a part in any of the other gameplay pieces so we
   really can do what we want as long as the main quest script hooks function the same."

Where it sits: combat is Layer 6 (README; layer5.md §1 lists it with dialogue and AI
packages). This document covers the combat systems and the player's side of a fight.
NPCs that fight back need AI packages, detection and combat styles, which are separate
Layer 6 work (§8). Requests 3 and 4 make combat the first system in the project that is
**not a parity target**: only the quest script hooks have to behave as in the original. §1 sets out what that changes and what it doesn't.

## 1. Principles

- **The content comes from the game; the mechanics are ours.** The game's records are
  the content: which weapon fires which ammunition and projectile, base damage, fire
  rate, magazine size, spread, reach, projectile speed and gravity, explosions, critical
  data, body parts, armour DT and DR. We read them with documented layouts (§4) and use
  their values as the balance inputs, so every weapon and armour in the masters and DLC
  works with no hand tuning. How a shot travels, hits and feels is NVForever's own
  design, tagged **[design]** in this document. A [design] tag makes no claim about the
  original.
- **No parity rows for the mechanics.** We don't capture the original's damage formula,
  spread or timings to copy them. Tests check our own rules (§7). Captures are still
  worth doing when a record field's meaning is unclear: a field we read is a fact and
  needs a source (CLAUDE.md charter).
- **Scripts must keep working.** This is the one hard constraint (request 4). Quest and object scripts
  call combat functions and react to combat events, so these behave as documented:
  `GetAV`/`DamageAV`/`RestoreAV Health` (already in `fnv-game`, actor.rs),
  `KillActor`, `GetDead`, `GetHitLocation`, `IsWeaponOut`, `GetEquipped`,
  `GetWeaponHealthPerc`, `FireWeapon`, `StartCombat`, `GetCombatTarget` (all in the
  function catalogue, `schemas/script/functions.json`), and the OnHit, OnDeath,
  OnStartCombat and OnCombatEnd blocks (layer5.md, events table). Damage reaches
  actors through the same actor-value model the scripts see.
- **One rule set for every attacker.** The player, NPCs, creatures, turrets and traps go
  through the same attack, hit and damage code. NPC combat AI (§8) only chooses when
  and where to attack.
- **Tunable in one place.** Every number that doesn't come from a record (spread
  growth, recoil, the DT floor, reaction times…) lives in one `CombatTuning` table with
  defaults. Changing one is a tuning change, not a code change. If the owner wants a
  closer "classic" rule set later, it becomes a second table.
- **The clean room is unchanged.** Field meanings come from xEdit's public definitions,
  the GECK wiki (the owner's exports) or observation. No decompiled or leaked code; nv-rs
  is not opened.

## 2. What exists (main, 7e435da)

| Piece | Where | Combat use |
|---|---|---|
| Player body, controls, draw/holster, aim down the sights, block pose | `engine/nvf-play/src/control.rs`, `engine/nvforever/src/player_body.rs`, `fnv_actor::player_clips` (`WeaponHands`) | the attack input and clip layer plug in here; Use (left mouse) and Ready Item tap (reload) are bound but not built (player-body.md §1.2) |
| NPC weapons, holstered and drawn | `fnv_actor::weapon` (`AnimType`, stance prefixes) | NPC attack clips use the same prefixes |
| WEAP reader (models, animation type, base damage) | `fnv_actor::records::Weapon` | visuals only; §4 adds the combat stats |
| Record layouts: WEAP `DATA`, `CRDT`, `VATS`, sounds; AMMO `DATA`, `DAT2`, `RCIL`; IPCT, IPDS; MGEF, IMOD; ARMO DT and DR | `schemas/fnv/objects.json`, `placed.json`, `gameplay.json` | §4 |
| Actor values: base from the record or formula, permanent modifier, damage | `crates/fnv-game/src/actor_values.rs`, `functions/actor.rs` (docs/spec-notes/actor-values.md) | health and damage, as the scripts see them; skills, Strength, Luck, Agility and the derived damage values for the formulas |
| Inventory, ammo items | `crates/fnv-sim` (`ItemKind::Ammo`) | ammo use |
| Physics rays | `fnv_phys::World::ray`, `ray_all` (world.rs), `Filter` with Havok layers (`fnv_phys::layers`: BIPED 8, DEAD_BIP 29, PROJECTILE 6, …) | shots, line of sight, melee sweeps |
| Crosshair ray from the eye | `Player::eye` (player-body.md §1.4) | where the shot starts |
| Script overlay (lines, message boxes) | `engine/nvf-play/src/script_hud.rs` | hit and ammo feedback until a real HUD exists |
| Gunfight observations (start order, targets, deaths, end) | docs/spec-notes/ai-observer.md §7 | reference for the later AI work, not for the mechanics |
| Clip catalogue: `AttackRight` and other sequences; text keys `hit` 931, `fire` 62, `eject` 88, `release` 40, `attach`, `detach` | docs/spec-notes/animation.md §3.2 [observed] | the timing of attacks and reloads; what each key means is still [hypothesis] |

## 3. What's missing

The gaps as of 2026-10-09, before steps C1–C8; all seven have been built since
(docs/status.md §6, Combat, player side).

1. **Weapon and projectile data**: WEAP `DNAM` (the weapon's main data block) is read
   as raw bytes. PROJ, EXPL and BPTD have no record layout (layer1.md §2 lists EXPL
   and BPTD among the types still undecoded).
2. **Hit volumes**: NPCs and creatures have no physics bodies. The crosshair picks them
   by their rendered bounding box (`activate.rs`), so a ray would pass through them.
3. **Health and death of actors**: base health from the NPC_ and CREA records, derived
   maximum health, and a dead state in the simulation (`GetDead`).
4. **The attack cycle**: firing, the delay between shots, ammo use, reloading, attack
   and reload clips.
5. **Damage rules**, including armour, criticals, body parts and condition.
6. **Death and hit presentation**: hit reactions, death clips, ragdolls.
7. **Feedback**: hit markers, an ammo count, the target's health.

Out of this document's scope but exposed by it: NPC combat AI (§8), audio (no sound
system exists yet; dependencies.md item 7), impact effects (IPCT/IPDS decals and
particles), VATS.

## 4. Step C1: weapon and projectile data

The first step organises the data. It adds no behaviour.

### 4.1 Records and sources

| Record | What combat reads | Layout today | Source for C1 |
|---|---|---|---|
| WEAP | `DATA` (value, health, weight, base damage, clip size), `DNAM` (weapon data), `CRDT` (critical damage, chance multiplier, effect), `VATS`, `NAM0` ammo, `INAM` impact data set, `WMI1–3` mods, sound FormIDs | done (`DNAM` as the named type `WeaponDNAM`, optional from `resist_type`) | xEdit `wbDefinitionsFNV.pas` |
| AMMO | `DATA` (speed, flags, clip rounds), `DAT2` (projectiles per shot, projectile, consumed ammo), `RCIL` ammo effects | done | — |
| AMEF (ammo effect) | which value it changes (damage, DR, DT, fatigue, spread, weapon condition) and how (add, multiply, subtract) | none | xEdit AMEF; GECK wiki Ammo Effect page (owner, §4.2) |
| PROJ | type (missile, lobber, beam, flame), speed, gravity, range, impact force, tracer chance, fade duration, muzzle flash, explosion with its trigger (impact, timer, proximity, remote detonation), flags (hitscan, can be disabled or picked up, pins limbs, passes through small transparent objects), default weapon, sounds | none | xEdit PROJ; meanings from the GECK wiki Projectile page (§4.2) |
| EXPL | force, damage, radius, flags (world orientation, ignore line of sight, push the source only), radiation (level, radius, dissipation), enchantment, impact data set, knockdown (always, by formula, never), placed impact object, image-space radius and modifier, art, sounds | none | xEdit EXPL; meanings from the GECK wiki Explosion page |
| BPTD | body parts: bone, damage multiplier, severable/explosive flags, limb health share, hit-reaction data | none (has repeated part blocks; may need the schema's count- or sequence-grouped arrays) | xEdit BPTD |
| IPCT / IPDS | impact effect per collision material: decal (texture set, size, placement), effect model and duration, sounds | done | — |
| DEST (on objects), DEBR | destruction data: health and up to 8 stages (health percentage, self-damage per second, cap damage, disable, destroy, model damage stage, explosion, debris, replacement model); debris models | DEST done (`common.json`); DEBR none | xEdit; GECK wiki Destructible Object Stage, Debris |
| ENCH, MGEF, SPEL | weapon and critical effects (poison, cripple, knockdown) | MGEF done; ENCH and SPEL none | xEdit |
| IMOD | weapon mod item (what a mod changes is in the WEAP's `DNAM`/mod fields) | done | — |
| NPC_, CREA, RACE | NPC: level (or player-level multiplier with min/max), SPECIAL, skills, Base Health added to the health derived from Endurance and level; creature: Health, combat skill, attack damage and reach, fatigue, base scale; body-part data (CREA/RACE) | partial (ACBS read for templates) | xEdit; GECK wiki Stats Tab pages (§4.2) |
| ARMO | DT, DR | done (actors.md §1.1) | — |
| GMST | combat game settings: damage (`fDamageWeaponMult`, `fDamageSkillBase`/`Mult`, melee and unarmed actor-value settings, arm condition, sneak multipliers), gun shells, auto aim, VATS | values readable from the masters | GECK wiki Weapon Damage Formula, Gun Shell, Auto Aim and VATS Settings pages (§4.2) |

For the records with no layout yet, the "What combat reads" column is the editor's view
of them [documented: GECK wiki, §4.2] for WEAP `DNAM`, AMEF, PROJ, EXPL, BPTD and DEBR.
For ENCH and SPEL it is what we expect to find [hypothesis]. Which bytes hold which
field is settled only by the layouts C1 writes from xEdit.

Rules for C1 are those of `schemas/README.md`. A layout needs a cited source, unknown
fields are `unknown_N`, and every layout is checked with `fnvtool esm coverage` against
the ten masters, aiming for zero trailing bytes. Field meanings that the xEdit name
doesn't settle (units, whether a value is per shot or per projectile, seconds or
frames) go into a spec note, `docs/spec-notes/combat-data.md`, each tagged. Where it
matters, they are checked against the records themselves: for example, how fire rate
values spread across automatic and semi-automatic weapons [observed].

### 4.2 What the GECK wiki export documents

The owner exported Category:Items on 2026-10-09: 17 pages, kept privately at
`C:\fnv-private\geckwiki\Items.xml` and never committed. The ones combat uses are
Weapons, Ammo, Armor, Equip Type, Ragdoll Template and Destructible Object Data. They
give the editor's names for fields. C1 matches those names to xEdit's subrecord fields
and writes the meanings into `combat-data.md` in our own words. What they settle for
this design [documented: GECK wiki, page named]:

- **Weapons.** Damage is per hit (per shot for guns). Fire Rate (shots per second) only
  matters for automatic weapons. A semi-automatic's rate follows its attack animation
  and Attack Multiplier, and for NPCs the semi-automatic fire delays. Min Spread is
  the cone's median angle in degrees, while the Spread field is unused in New Vegas.
  Melee reach is `fCombatDistance` × the actor's scale × Reach. Crit % Mult is divided
  by Fire Rate for automatic weapons. The page also covers Ammo Use (rounds per shot),
  projectiles per shot, Limb Dmg Mult, the damage type (which resistance applies), Kill
  Impulse and Impulse Dist, and the On Hit dismember/explode choice. Min and Max Range
  are for the combat AI only. The Action Points and Damage to Weapon Multiplier
  overrides, Sight FOV, Regen Rate (ammo that recharges, no manual reload), the
  flags (Automatic, NPCs Use Ammo, Minor Crime, No Jam After Reload, …), the effects
  of weapon mods, the reload and attack animation choices, and the Animation
  Multiplier are there too. Reload Time and Jam Time are unused.
- **Ammo.** The ammo's projectile and its non-zero projectiles per shot replace the
  weapon's, and the weapon's damage is shared among the projectiles. Speed is unused.
  An impact script runs on actors that are hit. Ammo Effects can be listed. "Ammo
  Consumed" adds items such as casings to the shooter's inventory, with a percentage
  chance.
- **Armor.** AR is DR, a percentage capped by `fMaxArmorRating`. DT is a separate field.
  The Light/Medium/Heavy class sets a movement speed penalty.
- **Ragdoll Template.** `.rdt` files override the skeleton's rigid bodies and
  constraints per armour piece. This matters for the later ragdoll step.

These change the design in three places: the base damage is split across projectiles
(§5.3), the ammo's projectile wins (§4.3), and the spread cone starts from Min Spread
(§5.1).

A second export, Category:Actor Data (`C:\fnv-private\geckwiki\Actor-Data.xml`,
2026-10-09, 15 pages), adds [documented: GECK wiki, page named]:

- **ActorValue.** A value is its base (or a SetAV override) plus three pools:
  temporary (effects with Recover, such as chems and armour enchantments), permanent
  (ModAV, skill books, and some player abilities) and damage (DamageAV/RestoreAV,
  which only lowers the value). The combat system puts Health damage into the damage
  pool. `fnv-game` already models the permanent and damage parts (actor.rs). The
  temporary pool is missing, and C5 adds it when effects need it.
- **Body Part / BodyPartData.** Each part names its bone, a VATS target bone, a damage
  multiplier, a health percentage, a VATS hit chance, the actor value that drops when
  the part is reduced to zero (the limb condition), and flags for IK, head and head
  tracking. The part is crippled at that percentage of the actor's total health. A part
  can also be severable or explodable, with an explosion, debris, an impact data set,
  a replacement limb model and a gore position. This is the content behind §5.2's
  body-part hit volumes and limb damage.
- **Race.** The Child flag makes an actor of that race take no damage and impossible to
  kill, even by script.
- **Perk.** Perks change values through entry points with ranks and priorities. The
  Entry Point page itself is not in the export.
- Ragdoll is a stub page with nothing usable.

A third export, Category:Special Effect (`C:\fnv-private\geckwiki\Special-Effect.xml`,
2026-10-09, 13 pages), covers projectiles, explosions and impacts [documented: GECK
wiki, page named]:

- **Projectile.** There are five types. A missile flies straight (bullets, rockets). A
  lobber is thrown and bounces (grenades, mines). A beam hits at once whatever its
  speed. A flame behaves like a missile that actors don't stop. Continuous beam
  appears unused. Fields: speed, gravity, range, impact force, tracer chance (how
  often the projectile is drawn), fade duration (beams), muzzle flash (light, effect,
  duration), sound and its detection level, and "supersonic" (the sound comes from the
  path's closest point). With **Hitscan** set, the hit is found by an instant ray, but
  the damage still arrives after distance ÷ speed. A projectile with gravity and no
  hitscan is simulated in flight. An optional explosion fires on impact or on an
  alternative trigger: a timer (for mines, lengthened by the placer's Explosives
  skill), a proximity radius, or remote detonation (C-4). Other flags: can be disabled
  (mines, which give XP), can be picked up, pins severed limbs, passes through the
  small-transparent layer (chain-link fences). Placed mines are projectiles. A
  projectile with an explosion uses only the explosion's impact effects.
- **Explosion.** The chain is weapon → ammo (optionally a form list of ammo) →
  projectile (the ammo's, else the weapon's) → explosion. Fields: force (pushes rigid
  bodies in the radius), damage (anything within the radius may take it), radius, an
  image-space radius and modifier, flags (aligned to the world, ignore line of sight,
  push only the source), radiation (rads per second, radius, how long it lasts), an
  enchantment, impact data set, knockdown of living actors (always, by a formula, or
  never), and a placed impact object. Unless it is aligned to the world, an
  explosion's orientation follows the surface or the incoming angle.
- **ImpactDataSet / ImpactData.** The set picks an ImpactData by the hit object's
  collision material. ImpactData gives a decal (texture set, sizes, placement radius,
  angle threshold, depth, parallax), an effect model with duration and orientation,
  and impact sounds.
- **Destructible Object Data / Stage, Debris.** An object with destruction data has
  health and up to 8 stages. Each stage triggers at a health percentage and can set
  self-damage per second (to burn down), cap damage (no stage skipped in one hit),
  disable or destroy the object, switch the model's damage stage (`BSDamageStage`
  nodes), set off an explosion (at `BSBlastNode`s) and spawn debris (at a
  `BSDebrisNode`), or replace the model. At zero health an object counts as destroyed.
  Containers and furniture become unusable then. Debris is a weighted list of models.
- The ImageSpace, EffectShader, CameraShot, AddOnNode and Acoustic Space pages are for
  later work (screen effects, kill cams, audio).

A fourth export, miscellaneous pages (`Misc.xml`, 2026-10-09, about 280 pages, many of
them stubs), adds [documented: GECK wiki, page named]:

- **Weapon Damage Formula.** Guns: the base damage × `fDamageWeaponMult` × a condition
  penalty × a skill bonus, passed through the attacker's "Calculate Weapon Damage"
  perk entry points, plus the critical bonus, minus what armour absorbs. Melee adds
  the MeleeDamage actor value (Strength × `fAVDMeleeDamageStrengthMult` plus an
  offset) and a penalty for crippled arms inside the perk step. Unarmed adds the
  UnarmedDamage actor value (a base plus a multiple of the Unarmed skill). The skill
  bonus is `fDamageSkillBase` + `fDamageSkillMult` × skill / 100 (defaults 0.5 and
  0.5: half damage at skill 0, full at 100). The critical bonus is the weapon's
  Crit Dmg through "Calculate My Critical Hit Damage" perks, added before armour,
  and armour still applies to it. Sneak attacks multiply the perk-modified damage by
  `fCombatDamageBonusSneakingMult` (2) for guns and
  `fCombatDamageBonusMeleeSneakingMult` (5) for melee and unarmed. A hit whose damage
  is 0 before perks counts as a miss: no blood, no sound, no critical. The page's
  condition settings are ignored by New Vegas (it says the calculation is hard-coded
  there). Its armour term is the older armour-rating form and doesn't describe DT.
- **Entry Point.** The full list of perk entry points. The ones combat runs through are
  Calculate Weapon Damage, Calculate My Critical Hit Chance and Damage, Modify Enemy
  Critical Hit Chance, Calculate Gun Spread, Modify Aiming Move Speed, Modify Attack
  Speed (non-automatic), Reload Speed, Equip Speed, Modify Damage Threshold (defender
  and attacker), Adjust Limb Damage, Knockdown Chance, Modify Item Damage (wear),
  Modify Throwing Velocity, Calculate Mine Explode Chance, Calculate Weapon Attack AP
  Cost, Modify Chance for Ammo Item (casings), See Enemy Health, Has Meltdown, and the
  unarmed power-attack and counter-attack entries. Consumables that raise damage do
  it by adding a perk for their duration.
- **NiTextKeyExtraData.** The meaning of the clip text keys:
  - `Hit`: the moment a ranged weapon fires or a thrown one leaves the hand, and when
    a melee weapon deals damage to what it touches. It also ends looping reloads.
  - `Fire`: works like `Hit` and also starts the internal loop of a clamped sequence;
    `Loop` jumps back to it.
  - `Eject`: a shell casing from the `ShellCasingNode`.
  - `A`: which attack group plays next, and from when the clip may be interrupted for
    it.
  - `Hold` / `Release`: holding and releasing a thrown weapon.
  - `Attach` / `Detach` / `Prn`: as player-body.md already uses them.
  - `Enum`: `CameraShake` (0–1), `HitShader` (the hit blur) and sounds.
- **Stats Tab – NPC / Creatures.** An NPC's health is derived from Endurance and level,
  plus its Base Health. Its level can follow the player's level × a multiplier, between
  a minimum and a maximum. A creature's Health is direct. A creature also has a combat
  skill (used with weapons, not natural attacks), an attack damage and an attack reach
  (scaled by its base scale) for natural attacks, and fatigue (unconscious at zero in
  New Vegas). Health 0 makes an actor start as a corpse.
- **Ammo Effect** (the same text as the owner's paste) adds that fatigue effects do
  nothing in the original. **Gun Shell Settings** give the shell eject speed (160) and
  lifetime (5 s). **Auto Aim** and **VATS Settings** list their settings by name.
  **Combat Style** describes the melee, dodge and cover decisions of the combat AI
  (Layer 6 AI, not this document). **BhkRagdollConstraint** and
  **BhkLimitedHingeConstraint** are for the ragdoll step.

A fifth export, Game Mechanics and Formulas (`GameMechanics.xml`, 2026-10-09, 21
pages), fills the formula gaps [documented: GECK wiki, page named]:

- **Critical Hit Chance Formula.** The CritChance actor value is `fAVDCritLuckBase` +
  `fAVDCritLuckMult` × Luck (defaults 0 and 1), and abilities such as Finesse add to
  it. The chance is CritChance × the weapon's Crit % Mult (divided by Fire Rate for
  automatic weapons), passed through "Calculate My Critical Hit Chance" perks, plus
  `fVATSCriticalChanceBonus` (15) in VATS. Weapon condition doesn't affect it in New
  Vegas.
- **Gun Spread Formula (New Vegas).** The page says none of the Fallout 3 spread
  settings or formula apply. A shot's spread is the actor's spread plus the weapon's
  spread, times the ammo's spread modifier (and a split-beam mod's factor).
  - The **actor's spread** sums penalties for not aiming (`fUnaimedSpreadPenalty`
    0.2), walking or running (`fWalkingSpreadPenalty` 0.1 / `fRunningSpreadPenalty`
    0.2, divided by "Modify Aiming Move Speed" perks), standing rather than crouching
    (`fStandingSpreadPenalty` 0.1), crippled arms (four settings by one or two arms
    and one- or two-handed weapon) and unmet Strength or skill requirements. That sum
    is multiplied by 1 − `fWobbleToSkillConversion` (0.5) × skill / 100, kept at least
    `fMinGunSpreadValue` (0.01), then passed through "Calculate Gun Spread" perks. In
    the original this value drives the hand sway; scope sway is the same motion.
  - The **weapon's spread** is Min Spread minus Decrease Spread mods. Weapon spread
    makes no sway.
  - For NPCs the total is scaled up by `fNPCMaxGunWobbleAngle` (15).
  - The page's authors measured the angles in the game: one unit of actor spread is
    about 5.955°, and weapon spread maps through 0.0125·W² + 0.125·W before the same
    factor. These numbers are their measurements, not the editor's. They are the
    diameter of the circle the shots fall in, so a shot's cone has half that angle
    (spec-notes/combat-rules.md, Cone).
  - Requirement penalty: 0.025 per missing point of Strength. The skill requirement
    is bugged in the original (its own setting is ignored).
- **Explosion Knockdown Formula.** The chance is (damage × `fKnockdownDamageMult` 0.3
  + base) ÷ (Agility × 10 × `fKnockdownAgilMult` + base), capped at
  `fKnockdownChance` (0.25). There is no knockdown below 50 % of current health
  (`fKnockdownCurrentHealthThreshold`), and always one above 75 % of maximum health
  (`fKnockdownBaseHealthThreshold`).
- **Melee Knockback Formula.** The force is (Agility × 10 × `fKnockbackAgilMult`
  (−0.008) + `fKnockbackAgilBase` (1)) × (damage × `fKnockbackDamageMult` (10) +
  `fKnockbackDamageBase` (50)), capped at `fKnockbackForceMax` (512 units), applied
  for `fKnockbackTime` (1 s).
- **Derived Skill Settings.** A skill is its base setting + the linked SPECIAL × 2 +
  ⌈Luck × 0.5⌉, and tagging adds 15. NPC skills for the damage and spread formulas come
  from this when not set.
- **Experience Settings.** Kill XP by the target's difficulty, NPC and creature
  separately. The player gets none unless they dealt a share of the target's health
  (`iXPDeathRewardHealthThreshold`, 40 % by default, when others also hit it).
- The derived-statistic settings (health from Endurance and level) are documented
  nowhere we have. The owner confirmed on 2026-10-09 that the wiki's derived-settings
  page covers only skills. §8 C5 has the plan.

The **Ammo Effect** page (owner's paste, 2026-10-09; also in `Misc.xml`) says the effects
are new in New Vegas and are listed on the ammo. Each changes one value (damage, DR, DT,
fatigue, spread or weapon condition) by adding,
multiplying or subtracting. The page notes that New Vegas uses DT, not DR, and that
the DR effect is unverified. This fits the armours storing DR 0 (actors.md §1.1
[observed]). The owner's reading is that the effects are what make ammo types such as
armour piercing or hollow point differ. Our damage step (§5.3) applies them in that
role: damage and DT effects on the hit, spread on the shot, condition on the weapon.
DR stays supported for any content that sets it.

### 4.3 The headless model

New crate `crates/fnv-combat` (depends on `fnv-esm`, `fnv-sim`; no engine). Its first
module, `data`, holds:

- `CombatForms`: an index over the loaded masters, built once like
  `fnv_actor::records::ActorForms`.
- `WeaponStats`: `DATA` + `DNAM` + `CRDT` + ammo. The loaded ammo's projectile (AMMO
  `DAT2`) replaces the weapon's own projectile; ammo with none uses the weapon's. A
  non-zero projectiles-per-shot on the ammo likewise replaces the weapon's count
  [documented: GECK wiki Ammo].
- `AmmoStats`, `ProjectileStats`, `ExplosionStats`, `BodyParts`, `ImpactSet`; armour
  DT and DR; the actor's base health inputs.

Fields keep the record's units. Conversions happen once, in `data`, with the source
in a doc comment. Weapon mods come later: C1 records which mod fields exist and
resolves nothing.

`fnv_actor::records::Weapon` stays the visual reader. The two readers share no code
until a change gives a reason to.

### 4.4 Tool and checks

- `fnvtool combat weapon --form <f> [--ammo <a>]`: a weapon's stats and its resolved
  shot chain. `fnvtool combat form --form <f>`: any combat record as read. These two
  replace the per-type commands planned here.
- `fnvtool combat survey`: every WEAP in the load order resolved, counts, references
  that point nowhere, and the combat game settings. Its numbers are in the spec note
  [observed]. Contract: combat-data.md §4.
- Unit tests on synthetic records (as in `fnv-sim/tests/synthetic.rs`).

C1 is done when the survey resolves every playable weapon and the spec note documents
each field we use.

## 5. Mechanics [design]

All of §5 is our own design under request 3. It names the record fields it uses by
role; their exact names come from C1.

### 5.1 Shots

- **Aim comes from the camera.** A shot leaves along the view ray from the eye, so the
  crosshair is always true. The original moves the gun around the crosshair and fires
  along the gun's axis [documented: GECK wiki Category:Combat, owner export
  2026-10-09]; we deliberately don't do that. In third person, a second ray from the
  muzzle to the aim point stops shots that would pass through a wall right in front of
  the character. The muzzle only matters for visuals (tracer, flash).
- **Third person: the crosshair is the camera's** (playtest list 4 item 12, 2026-10-13).
  The over-the-shoulder camera sits right of the eye, looking the same way, so the eye's
  view ray passes about 40 units left of what is under the crosshair. In third person
  the aim point is what the camera's line (from where it is level with the eye) meets
  first within the range, and the shot's view ray goes from the eye to that point
  (`control::Crosshair`); everything below then applies to that ray. While the camera
  circles the player (Toggle POV held), the eye's own view is used. The activate prompt
  follows the same line (`activate::find_target`, item 11).
- **What is seen leaves the muzzle** (built 2026-10-10 after the owner's report that
  shots looked as if they came from the camera; `fnv_combat::shot::leaving`, `Seen`).
  - The muzzle is the `ProjectileNode` of the weapon model in the rig in view
    [hypothesis: the node's meaning; docs/spec-notes/animation.md §9.9]. The player
    body gives its place to combat each frame (`WeaponAnim::muzzle`).
  - The **aim point** is what the view ray from the eye meets first, within the
    projectile's range (100,000 units when it has none); the ray's end when it meets
    nothing.
  - The hit is still decided on the view ray. A projectile is **drawn** beside its
    path: at the muzzle when it starts, and closer to its path in proportion to the
    distance flown, until it is on it at the aim point's distance. So a tracer runs
    from the barrel to the crosshair's point, and a missile or grenade is seen to
    leave the barrel. Hitscan tracers are one line from the muzzle to the hit, or to
    the end of the range when nothing is hit.
  - A muzzle that is past the aim point (a barrel pushed through a wall) is not used:
    the shot is drawn from the eye.
  - **Cover, third person only:** the second ray goes from the muzzle towards the aim
    point, 150 units at most ("right in front of the character"). If it stops on
    something other than the actor aimed at, more than 4 units short of the aim
    point, the projectiles leave the muzzle towards the aim point instead of the eye
    along the view, so they strike what is in front of the barrel, with their spread
    around that line. In first person the second ray is not cast: the gun is drawn
    below the eye and would be stopped by every ledge the player looks over.
    - Why the ray is short [observed 2026-10-10]: with the ray going all the way, a
      gatling laser burst along level ground at Goodsprings had one shot of 16 stopped
      by the ground itself, short of an aim point 1,328 units away, because the muzzle
      is lower than the eye. Far things are for the view to decide.
  - Without a muzzle (no player body, the weapon model has no such node) everything
    is as before: drawn from the eye.
  - Not built: the same for NPCs' shots (NPCs did not shoot yet; they do since CA1,
    combat-ai.md, which this note does not follow); melee is unchanged.
  - Open (docs/owner-review.md, "Shots leave the barrel"): in third person the view
    ray starts at the character's head while the camera sits over the shoulder
    (40 units to the right by default), so the ray runs parallel to the crosshair's
    line and beside it. That was so before this change and is left alone.
- **Projectiles by their PROJ type.** The PROJ type picks one of three paths.
  - Missiles are stepped on the physics tick by swept rays (`ray` from the previous
    position to the next), with the PROJ speed and gravity, up to the PROJ range.
  - Hitscan missiles and beams are traced at once. The hit is applied after
    distance ÷ speed, as the record intends, which costs nothing.
  - Flames keep going through actors, hitting each once.

  Lobbers (grenades, mines) are rapier dynamic bodies that bounce until their trigger
  fires (§5.8). Shotguns fire the ammo's projectiles per shot, each with its own
  spread. Small-transparent objects stop a projectile unless its PROJ flag lets it
  pass.
- **Spread** is a cone around the crosshair. Its size comes from the documented New
  Vegas spread (§4.2): the actor's spread (aiming, moving, crouching, crippled arms,
  requirements, skill, perks) plus the weapon's (Min Spread, mods), times the ammo's
  modifier, turned into degrees with the measured conversion. Using it keeps every
  weapon, ammo, mod and perk meaningful. What changes [design]:
  - The actor's spread widens the cone instead of swaying the weapon, so the
    crosshair stays true. A light idle sway remains in the scope, for feel.
  - The crosshair shows the current cone.
  - **Bloom**: each shot widens the cone, scaled by the weapon's spread. Continuous fire
    keeps widening it; it closes again once the trigger has rested a moment.
  - **Recoil**: the view climbs up in a random pattern that leans side to side,
    stronger for heavier weapons. It stays there: the player pulls it back down.
  - Jumping adds a penalty of its own.

  Bloom, recoil and the jump penalty are tuning values.
- **Skills** act through the documented spread and damage formulas: with low skill a
  gun is wider, never useless.
- **No auto-aim** with the mouse (the original's auto-aim settings, §4.2, are not used)
  [design]. Controllers get an aim assist instead: the stick slows on a nearby hostile
  and aiming down the sights eases onto one (docs/design/controller.md §5).
- **Determinism**: each shot draws from an RNG seeded by (attacker, shot counter), so a
  headless replay gives the same hits.

### 5.2 Hit volumes

- First version (C3): one capsule per actor, sized from the actor's bounds, on the
  BIPED layer, kinematic, moved to the actor each frame. Dead actors move to DEAD_BIP.
  Shots hit them, the character controller ignores them. What keeps the player from
  walking through a living actor is separate (physics.md §6.8).
- Second version: one capsule per body part (head, torso, arms, legs, as BPTD names the
  parts and their bones), posed from the animated skeleton each frame. The hit part
  gives the damage multiplier (headshots) and the hit location (`GetHitLocation`).
  Damage to the part also counts toward its limb condition. The limb is crippled below
  the part's health percentage (§4.2).

### 5.3 Damage

For each projectile that hits:

The documented formula (§4.2) is the base. It brings the game's settings, actor values
and perks with it, so the balance of the shipped content carries over. Our own changes
are marked [design].

1. **Base**: the weapon's base damage × `fDamageWeaponMult` × the skill bonus
   (`fDamageSkillBase` + `fDamageSkillMult` × skill / 100), split evenly across the
   projectiles of a shot (the record's damage is per shot [documented: GECK wiki
   Ammo]). Melee and unarmed add the MeleeDamage / UnarmedDamage actor values and the
   crippled-arm penalty, as documented.
2. **Multipliers**: weapon condition [design: the original's condition rule is
   hard-coded and undocumented; ours is linear from 100 % at full condition to 50 % at
   zero, a tuning value], the ammo's damage effects, then the "Calculate Weapon
   Damage" perk entry points. Sneak attacks multiply by the documented sneak settings.
   The body part's multiplier comes last [design: the original applies it its own way,
   which is not documented here]. A hit worth 0 before perks is a miss, as documented.
3. **Criticals**: the chance is the documented formula (§4.2): the CritChance actor
   value from Luck × the weapon's Crit % Mult (÷ fire rate for automatic weapons),
   through the critical-chance perks and the target's "Modify Enemy Critical Hit
   Chance" perks. On top of that, the player's sneak attacks (sneaking, and not
   detected by the target) are always criticals [owner statement 2026-10-25;
   docs/spec-notes/combat-rules.md §4], and a headshot on an unaware target is always a
   critical [design]. The critical adds the weapon's Crit Dmg through
   the critical-damage perks before armour, and applies its critical effect. For
   automatic weapons the chance multiplier is divided by the fire rate (§4.2).
4. **Armour**: the target's DT is subtracted, with a floor so weak weapons still do a
   fraction of their damage (default 20 %). The ammo's DT effect changes the DT. DR, if
   the target has any, reduces what's left.
5. **Apply** through the actor-value damage part of Health (as `DamageAV`), fire OnHit
   on the target's scripts, and at zero health mark it dead and fire OnDeath. Limb
   health and crippling come later.

Two content rules hold whatever the tuning: actors of a race with the Child flag take
no damage and cannot be killed (scripted kills included) [documented: GECK wiki Race].
Perk entry points that change combat values, such as damage and critical chance, are
applied on top of these steps once perks are read (a later step). Our multipliers are
the starting value that the entry points modify.

The weapon's condition goes down per shot (WEAP `DATA` health is its maximum).
`GetWeaponHealthPerc` reads it. Armour wears with the hits it takes, a weapon that is
hit wears, and a weapon at zero is broken: docs/spec-notes/combat-rules.md §17.

### 5.4 Melee and unarmed

- The attack clip's `Hit` text key is the moment a melee weapon deals damage [documented:
  GECK wiki NiTextKeyExtraData]. From it, a short window [design: 0.1 s] sweeps a
  capsule along the swing within the reach (`fCombatDistance` × scale × the weapon's
  Reach; a creature's natural attack uses its attack reach). Each target is hit once
  per swing.
- Power attack: holding Use starts a power attack whose clip follows the direction
  moved (standing, forward, back, left, right) and whose damage is that direction's
  `fDamagePowerAttack…Bonus`. Block (right mouse, already bound) reduces incoming melee
  damage and staggers an attacker who hits it.
- Combos: a press during a swing starts the next at the clip's `a:` key, playing the
  attack group the key names (right, left, right…); built 2026-10-11 in
  `fnv_combat::melee` (combat-rules.md §10).
- Unarmed uses the same path, with fists and the UnarmedDamage actor value; a
  creature's natural attack uses its attack damage. The `A` key's interrupt time is
  when the next swing of a combo may start.
- Damage follows §5.3 (the melee and unarmed forms).

### 5.5 The attack cycle

A headless state machine in `fnv-combat`, driven by the input and the clip timing in the
engine:

`Holstered → Drawing → Ready ⇄ Firing → (Cooldown by fire rate) → Ready`,
`Ready → Reloading → Ready`, `Ready (empty) → dry fire or auto-reload`.

- Semi-automatic weapons fire once per press. Automatic weapons fire at their rate while
  Use is held. Which is which comes from the WEAP flags.
- A weapon that spins up (a minigun: its attack clip has `Fire` and `Loop` keys) fires
  its first shot at the clip's `Fire` key, then at its rate while Use stays held;
  letting go before that fires nothing, and every new press spins up again [owner
  decision 2026-10-10; combat-rules.md §4].
- Ammo comes from the inventory (`fnv-sim`); a magazine holds the clip size. Reloading
  takes the reload clip's length; a reload cancelled partway keeps the old magazine.
  A weapon whose reload animation has an opening clip (the .357, the lever-actions,
  the hunting shotgun) opens, loads one round per pass of the reload clip up to `Hit`
  and closes; a trigger pull stops it after the round in hand, closes and fires [owner
  decisions 2026-10-11; combat-rules.md §4].
- The loaded ammo is the first of the weapon's acceptable ammo list the actor carries
  (combat-data.md §2). Ammo Swap (hotkey 2) steps down the list to the next carried
  type.
- **Jamming** (the owner's playtest list 5 item 9) [design]: below
  `CombatTuning::jam_condition` (75 %) a shot of a weapon that uses ammo (not thrown)
  jams it with `fnv_combat::condition::jam_chance` (0 at 75 %, `jam_chance_max` at 0 %,
  along `jam_curve`). The shot leaves; the cycle is then `Jammed` for the jam clip's length
  (`{prefix}jam{letter}.kf` beside `{prefix}reload{letter}.kf` [observed: file lists];
  `jam_fallback` without one): no shot, no reload. The player only for now.
- **God mode** (tgm, item 49) [design]: the magazine stays full, shots take nothing from
  the inventory and Reload does nothing.
- `fnv_actor::player_clips` gains attack, reload and hit-reaction clips. The text keys
  time them as documented (§4.2): `Hit` fires the shot, or ends a looping reload;
  `Fire`/`Loop` run an automatic weapon's loop; `Eject` throws a shell (speed and
  lifetime from the gun-shell settings); `Hold`/`Release` throw; `Enum CameraShake`
  shakes the view.
- Semi-automatic fire rate follows the attack clip and the weapon's attack multiplier,
  as in the original; "Modify Attack Speed" and "Reload Speed" perks scale the clips.
- **The clip starts at the pull** (the owner's playtest list 7 item 22, 2026-10-23): a
  pull whose shot waits for the attack clip's `Hit` key (a thrown weapon's release, a
  gun with a wind-up) starts the clip then, and the shot leaves when the clip reaches the
  key, without starting it again. The clip used to start at the shot, so a stick of
  dynamite waited for its wind-up with nothing playing, left the hand, and only then was
  lit and thrown in the clip. Log: `combat: <weapon> winds up, the throw leaves at the
  clip's Hit key in <s> s`, `player <view> rig: <clip> starts at the pull`, `combat:
  <weapon> leaves the hand`. Smoke `thrown-dynamite`. NPCs throw nothing yet.

### 5.6 Feedback

- A hit marker on the crosshair (a different one for kills), a short screen kick when
  the player is hit, and the aimed target's name and health bar.
- An ammo counter (magazine / carried) and the player's health, drawn as a small HUD
  like `script_hud.rs` until the game's HUD is built.
- What a shot shows (built 2026-10-10 at the owner's request, "actual bullet
  projectiles that aren't invisible along with the muzzle flash effects";
  docs/spec-notes/combat-rules.md §15): the PROJ's muzzle flash model and light at the
  muzzle for the PROJ's duration, and the PROJ's model on every projectile in flight.
  Choices of ours [design], listed for the owner in docs/owner-review.md:
  - every projectile is drawn, where the game draws its tracer chance of them
    (`DRAW_EVERY_PROJECTILE`);
  - a hitscan bullet, whose hit is found at once, is drawn flying from the muzzle to
    where it lands at the PROJ's speed, and a line marker along the shot stays 0.08 s,
    because at those speeds the bullet's model is on screen for a frame or two;
  - a beam is drawn from the muzzle to its end for the PROJ's fade duration.
- Hit reactions: a flinch clip layered over the current animation, when a hit cripples
  a body part and, after a cooldown, on later hits to a crippled part (owner's rule,
  combat-rules.md §12), and an impulse to the hit body part once ragdolls exist.

### 5.7 Death

- C5: the actor is marked dead in the simulation (`GetDead` 1). It plays a death clip if
  its skeleton folder has one; otherwise it is laid down (pose frozen, aligned to the
  ground). The body stays and can be activated like a container for its inventory (the
  activation part already opens containers).
- Ragdolls (built 2026-10-10, docs/spec-notes/ragdoll.md): the skeleton's Havok bodies
  and constraints as rapier bodies and joints, with the impulse from the killing hit.

### 5.8 Explosions

- EXPL damage over its radius, falling off with distance [design: the record gives one
  damage value; a linear falloff to 25 % at the edge is our tuning]. A line-of-sight
  ray blocks it unless the EXPL ignores line of sight. Force goes to dynamic bodies in
  the radius, and living actors are knocked down per the EXPL's knockdown setting:
  always, never, or by the documented knockdown formula (§4.2) with its health
  thresholds, plus "Knockdown Chance" perks.
- Radiation lingers at the point for the dissipation time. The enchantment applies to
  actors caught in the blast, and a placed impact object is spawned.
- Triggers come from the PROJ: impact, timer (mines arm after it), proximity, or
  detonation by the owner (C-4).
- **Destructible objects** (cars, barrels, …) take damage from hits and explosions and
  run their DEST stages (§4.2): model swaps, burning down, the final explosion. This
  is how the Mojave's cars blow up, and it uses the same damage path.
- Thrown weapons arc under gravity as lobbers.
- **Who a blast reaches**: every actor's hit volume within the radius, and the player,
  whose capsule stands in for a body (it has no hit volume). Until 2026-10-23 the player
  was never among them, so mines, grenades, missiles and an NPC's Incinerator (whose
  damage is its explosion's, `NVIncineratorFireBurst` 20 within 212 [observed]) did the
  player no harm (the owner's playtest list 7 items 19 and 29). The player's own blasts
  reach the player too [design]. Swings and proximity fuses skip their own attacker.

### 5.9 Limb hits (step C8)

Hits land on body parts, damage limbs and cripple them. This is planned as its own
step after C7, for three reasons. First, it needs bones posed every frame, which the
single hit capsule avoids. Second, limb damage touches movement, aiming and the HUD
beyond combat. Third, C5 already passes a hit location everywhere (`GetHitLocation`,
`HitInfo::location`), so nothing built so far has to change shape.

- **Which parts** [documented: xEdit; observed: record names]:
  - A creature names its BPTD (CREA `PNAM`). NPCs have no such field; the masters
    have `DefaultBodyPartData` (FormID 0x1D) and `PlayerBodyPartData` (0x1C)
    [hypothesis: humans and the player use these].
  - Each part names its bone, damage multiplier, health percentage, limb-condition
    actor value and part type (`fnv_combat::data::BodyPart`, read since C1).
- **Hit volumes:** one capsule per bone segment of each part, posed from the animated
  skeleton each frame, replacing the single C3 capsule for actors that have parts
  [design].
  - Lengths come from the part's bone chain, radii from a share of the actor's size by
    kind of part [design] (combat-rules.md §12). The skeleton's own collision was not
    used.
  - The human skeleton has no bone above `Bip01 Head` (the base of the skull), so a
    head chain that ends on such a bone is carried on to the crown (`nvforever`
    `limbs::skull_tip`, 2026-10-24) [observed: `skeleton.nif`, `headhuman.nif`; design:
    the reach]. Before, the head's capsule was a ball at the neck, and a VATS mark on the
    head needed the crosshair on the neck (the owner's playtest).
  - `Owner::Actor` gains the part index.
- **Damage:**
  - The part's damage multiplier in §5.3 (head ×2 for most humans [observed: C1
    survey]).
  - The location index for `GetHitLocation`.
  - Damage to the limb's condition actor value (codes 25–31, already in `fnv-game`,
    starting at 100) scaled by the WEAP Limb Dmg Mult and the "Adjust Limb Damage"
    perk hook.
  - The limb is crippled when its condition reaches 0 [hypothesis: condition counts
    down from 100 by limb damage as a share of the part's health percentage of total
    health; GECK wiki Body Part: crippled at that share of total health].
- **Crippled limbs:**
  - arms: the documented spread penalties (§4.2, already in `spread::actor_spread`)
    and the melee arm-condition term (already in `damage::hit`);
  - legs: slower movement [design];
  - head: a screen effect (the Enum `HitShader` blur) [design; not built in C8].
  - The HUD shows the limb states.
- **Later (gore):** severing and exploding limbs by the part's flags, chance and
  explosion, with debris and limb replacement models.
- **Tests:** a hit volume per part on a test skeleton, damage multipliers, crippling
  thresholds, and spread with crippled arms.

## 6. Where the code goes

| Item | Where |
|---|---|
| Record layouts | `schemas/fnv/objects.json` (WEAP `DNAM`, PROJ, EXPL, AMEF), `gameplay.json` (BPTD, ENCH, SPEL) |
| Data model, rules, attack cycle, projectile stepping (headless) | new crate `crates/fnv-combat`, modules `data`, `damage`, `attack`, `projectile`, `melee`. Projectile and melee queries go through a small trait that `fnv-phys` implements, so the rules are tested without a physics world |
| Hit volumes | `fnv-phys`: actor capsules (C3), then body-part capsules |
| Health, death, script events | `fnv-game` (actor values, `KillActor`/`GetDead`, OnHit/OnDeath dispatch), `fnv-sim` (dead state) |
| Attack, reload, hit and death clips | `fnv_actor::player_clips`, `fnv_actor::weapon` |
| Input, projectiles in play, HUD | `engine/nvf-play/src/combat.rs` (new plugin) |
| Posing hit volumes from bones; the muzzle's place (`player_body.rs`, for `WeaponAnim::muzzle`) | `engine/nvforever` |
| Where a shot leaves and is drawn from (`shot::leaving`); tracers and the flash marker | `crates/fnv-combat`, `engine/nvf-play/src/combat.rs` |
| Tools | `crates/fnvtool/src/combat_cmd.rs` (`fnvtool combat …`) |
| Facts | `docs/spec-notes/combat-data.md` |

## 7. Testing

The mechanics have no original to match, so tests check what we specify:

- Unit tests for each rule in `fnv-combat`: damage with and without DT, the floor,
  criticals, spread growth and recovery, the cycle's states, projectile drop over
  distance, melee windows. Each runs on synthetic records.
- Headless scenarios, run by a `fnvtool combat sim`: an attacker, a target, a weapon
  and a script of inputs, with a fixed seed, printing hits and health. Their expected
  output is our own specification, so they live with the crate's tests, not as
  parity-lab goldens (lab goldens come from the original).
- Script compatibility: `fnv-game` tests for `GetDead`, `KillActor`, `GetHitLocation`
  and OnHit/OnDeath order.
- In the engine: test aids in `test_aid.rs` (`NVF_PLAY_TEST_COMBAT=fire:<n>`,
  `reload`, `target:<ref>`) and an unattended run at Goodsprings: shoot a placed target,
  check its health and death in the log. Plus the CLAUDE.md smoke tests.

## 8. Steps

Each step is a branch and a PR, reviewed before the next starts.

| Step | Content | Needs |
|---|---|---|
| **C1** | Data: layouts, `fnv-combat::data`, `fnvtool combat`, spec note (§4) | — |
| C2 | Rules, headless: damage, criticals, spread and recoil, attack cycle, projectile stepping; `CombatTuning` | C1 |
| C3 | Hit volumes: actor capsules in `fnv-phys`, synced in the engine | — (can run alongside C2) |
| C4 | The player shoots: input, attack and reload clips, ammo, projectiles in play, tracers | C2, C3 |
| C5 | Damage applied: health, death, script events, death clip, looting the body, kill XP (documented settings, §4.2), HUD. NPC maximum health: C1 lists the masters' health-related game settings [observed]. A small capture job then reads `GetBaseAV Health` for a few dozen NPCs of known level, Endurance and Base Health, and the rule is fitted and checked against it (the AI observer already records Health, ai-observer.md §1) | C4 |
| C6 | Melee and unarmed, block, power attacks, knockback (documented formula, §4.2) | C5 |
| C7 | Explosions, thrown weapons, mines, destructible objects (DEST stages) | C5 |
| C8 | Limb hits (§5.9): a hit volume per body part, part multipliers, limb condition and crippling, their effects on spread, movement and the HUD | C5; skeleton bones posed per frame |
| later | perk entry points (the combat ones listed in §4.2; built in claude/e2-perk-combat-entry-points, spec-notes/perk-entry-points.md); fatigue and knockdown (fatigue ammo effects included, which do nothing in the original); weapon mods; ammo swap and jamming; impact effects (IPDS → IPCT by collision material: decals, effect models); severed and exploded limbs; ragdolls; audio; VATS or a replacement | |
| Layer 6 AI | NPCs fight back: combat AI using the same attack cycle, detection, factions and hostility, combat styles (CSTY). Detection states: docs/design/alert.md; CA1 (actors in Combat shoot and strike, the player can die): docs/design/combat-ai.md, 2026-10-11 | C5; actor-behaviour.md |

## 9. Owner questions

1. **Facts for C1**: the owner's Items, Actor Data and Special Effect exports and the
   Ammo Effect page (§4.2) cover weapons, ammo and its effects, armour, body parts,
   actor values, projectiles, explosions, impacts and destruction; the Misc export adds
   the damage formula, perk entry points, clip text keys and actor stats; the Game
   Mechanics export adds critical chance, New Vegas spread, knockdown, knockback,
   derived skills and kill XP. The one rule no page documents, an NPC's health from
   Endurance and level, is settled by observation in C5 (§8).
2. **Feel**: hit markers yes? Floating damage numbers (default off)? Recoil and bloom
   strength like a modern shooter, or subtler?
3. **VATS**: rebuild it later, replace it (e.g. a slow-motion aim assist), or leave it
   out?
4. **Death**: death clip now and ragdolls later (as in §5.7)?
5. **Audio**: a separate workstream after C4 (combat stays silent until then)?
6. **Consequences** before AI exists: shooting a Goodsprings resident kills them, and
   nobody reacts (no hostility, no crime) until the Layer 6 AI work. Is that acceptable
   for testing?
