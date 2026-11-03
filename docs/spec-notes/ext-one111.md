# The 1111 NVSE plugin's functions (2026-11-01, #1012)

One111's 51 script functions as NVForever builds them, all on the game side
(`crates/fnv-game/src/nvse_one111.rs`, reached from `nvse::call`). Sources: the GECK
wiki export [documented: the page named], and the probe set `one111` of
`tools/capture/jip_probes.py` run in the game with OneOneOneOne.dll installed
(2026-11-01) [observed: one111, the probe id named]. Every change lasts for the session
(no page says one is saved). Tests: `crates/fnv-game/tests/one111.rs`.

**Probe results** (`L5.ext.one111`, 118 rows, golden private): 117 match. The one that
differs, `one.armor.doc.DT` (the game 2, NVForever 0), is not One111's: the headless
candidate (`fnvtool ext queries`) does not equip an NPC's inventory (Doc Mitchell's
GetEquippedObject reads none in every slot there), so the sum has no worn piece to add.
The first run changed NVForever in six places, each now `[observed]` below: the quest
objective by place, an ability perk entry's pages (−1), GetOnDamageCalcAdditionalNumb
and GetOnDamageCalcFlag outside an event (0 and −1), GetWeapAdditionalFlag (−1),
GetWeapAdditionalVectorArray of an empty vector (no array), and SetConditionForm…Form's
run-on reference on a condition not run on a reference (accepted). The probes confirm
[observed]: the counts, every Int/Float/Form value ID on a recipe, an idle and a message
button (opcode + 0x1000, the parameter types, INT_MIN and none for what a list or
condition lacks), EvalConditionList…, Add (GetVampire == 1, 4136) at the end and before
a condition, a bad position refused, Delete and Swap, SetConditionFormOneInt's
comparison and run-on type (9 refused), SetConditionFormOneFloat, the damage getters
outside an event, the stat vector (the page's (0.9; 1), (0.5; 0.2) example, a negative
value refused), ApplyAmmoEffects for the six types, SetAmmoEffectTraitNumericAlt,
SetWeaponItemModEffectAlt and SetWeaponItemModValue2, the scope shader getter and a
non-shader refused, GetProjectileRefAmmo on a non-projectile (none). Every writing probe
puts back what it changed in the same step; no real scope shader is set (no vanilla
weapon has one to put back).

## 1. Condition lists

A record's condition list, by the kind of record [documented: GetConditionFormOneCount,
GetConditionFormTwoCount, GetConditionFormThreeCount]:

| Family | Records | Arguments after the record |
|---|---|---|
| FormOne | recipe, idle animation, package, topic info | — |
| FormTwo | actor effect and ingestible (an effect, by index), message (a button), terminal (a menu item) | `first` |
| FormThree | quest (`first` −1: its own list; else the objective at place `first` (not its number), its target `second` [observed: one.count.three.quest.0.0]), perk (`first` −1: its requirements; else the entry `first`, its condition page `second`, 0–2; an ability or quest entry has none, −1 [observed: one.count.three.nerdperk.0.0]) | `first second` |

| Functions | NVForever |
|---|---|
| GetConditionForm…Count | the list's length; −1 for a record or index without one [documented] |
| GetConditionForm…Int `cond id` | 0 the function's opcode, 1/2 the parameters' types (−1 none, 0 int, 2 reference, 3 base form), 3 the flags (bit 0 OR, bit 2 Use Global), 4 the comparison (0 == … 5 <=), 5 the run-on type (0 Subject … 4 Linked Reference); INT_MIN when wrong [documented: GetConditionFormOneInt]. The split of the record types into references (`ptReference`, `ptActor`, `ptObjectReference`, `ptMapMarker`, `ptContainer`, `ptFurniture`) and base forms is ours [hypothesis]; no condition function of the catalogue takes a float, so 1 never comes back |
| GetConditionForm…Float `cond id` | 0 the value compared with, 1/2 the parameters that are numbers (enumeration codes too) [documented: GetConditionFormOneFloat]; with Use Global on the value has no number: INT_MIN [hypothesis] |
| GetConditionForm…Form `cond id` | 0 the global, 1/2 the parameters that are forms, 3 the run-on reference [documented: GetConditionFormOneForm]; 0 when wrong |
| SetConditionForm…Int `cond id value` | the opcode (an opcode with no condition function refused, other than GetVampire [hypothesis]; the parameters of the new function keep a value of the same kind, the rest are cleared [hypothesis]), the flags, the comparison, the run-on type (2 keeps the reference it had [hypothesis]); the parameter types cannot be set [documented: SetConditionFormOneInt]. Turning Use Global on or off clears the compared value [documented: the page warns to zero it first "or the game might crash"; hypothesis: done here instead] |
| SetConditionForm…Float `cond id value` | the value only with Use Global off, except 0, which clears the global [documented: SetConditionFormOneFloat; hypothesis: what 0 does]; a parameter that is a number, or 0 into any parameter [documented] |
| SetConditionForm…Form `cond id form` | the global (Use Global on, a GLOB [hypothesis: anything else refused]); a parameter that is a form, or any parameter of GetVampire (4136); the run-on reference: accepted whatever the run-on type [observed: one.set.one.recipe], kept by NVForever only with run-on type 2 [hypothesis] [documented: SetConditionFormOneForm] |
| AddConditionForm… `[cond]` | GetVampire == 1 run on the subject, at the end (omitted or −1) or before the condition at `cond`, which must exist [documented: AddConditionFormOne] |
| DeleteConditionForm…, SwapConditionForm… | [documented]; 0 for a position without a condition |
| EvalConditionListForm… `action target` | 1 when the list passes with `action` as the subject and `target` as the target, the game's own condition evaluation (`Game::conditions_pass`) [documented]; an empty list passes [hypothesis: as the game's own lists] |

A changed list is kept whole and read instead of the record's by these functions and by
the message box's buttons (`Game::hidden_buttons`). **Follow-up**: recipes, idles,
packages, topic infos, effects, terminal items, quest targets and perks still read their
records' lists in the game's own systems.

**Catalogue fixes**: the wiki pages GetConditionFormOneFloat, SetConditionFormOneFloat and
SetConditionFormOneForm carry the template name of the Int function (copied from a
sibling page), so the catalogue (`schemas/script/functions.json`) had them under the Int
names. The generator (`tools/capture/script_functions.py`, `NAME_FIXES`) now names them
by their page titles [documented: each title and its example, "GetConditionFormOneFloat
HitReactionIdles 0 1"]. AddConditionFormTwo's template lacks the `First` argument its
example passes ("StealthBoy 0" adds to Base Effect 0's list) and every other FormTwo
function takes; `PARAM_FIXES` gives it `entity first [cond]` [documented: the example and
the sibling pages; hypothesis: that `First` is required, as in the siblings]. The four
committed rows were edited to what the generator now emits (a full regeneration also
rewrites rows that were hand-edited since, so it was not committed); tested in
`tests/one111.rs` `renamed_catalogue_rows_compile_and_answer`.

## 2. The damage calculation (`GetOnDamageCalc…`, `SetOnDamageCalcNumb`)

The values of the hit whose One111 damage event runs [documented: GetOnDamageCalcNumb
IDs 0–9, GetOnDamageCalcAdditionalNumb 0–5, GetOnDamageCalcFlag the HitData bits,
GetOnDamageCalcRef 0 source, 1 target, 2 weapon, 3 projectile]. Outside an event the
numbers read INT_MIN, the additional numbers 0, the flags −1, the references none and
SetOnDamageCalcNumb 0 [observed: one.damage.*]; in an event a number not calculated
reads INT_MIN and SetOnDamageCalcNumb 1 [hypothesis].

The game keeps the hit (`One111State::damage_calc`) between
`Game::one111_damage_calc_begin` and `one111_damage_calc_end`. **Follow-up (engine)**: the
engine's damage calculation (`nvf_play::combat`) does not dispatch One111's `One:On…`
events nor call these yet, so in play the getters read the outside values.

## 3. The weapons' additional stats

A vector of pairs (health 0–1, value) per weapon base and type, sorted by health: 0
damage, 1 value, 2 min spread (replaces the weapon's), 3 after-armour damage, 4 DT, 5 DR,
6 projectile speed [documented: GetWeapAdditionalVectorValue].

| Function | NVForever |
|---|---|
| SetWeapAdditionalVectorValue `weapon type health value` | the pair, replacing one at the same health; a negative value refused [documented: "can only be a positive number or zero"]; a health outside 0–1, a type outside 0–6 or a form that is not a weapon refused [hypothesis] |
| GetWeapAdditionalVectorValue | the value at exactly that health, −1 for none [documented] |
| GetWeapAdditionalVectorValueScale | beyond the pairs the nearest one's value, between two the linear proportion, −1 for an empty vector [documented, with the page's (0.9; 1), (0.5; 0.2) example in the test] |
| GetWeapAdditionalVectorArray | the healths, ascending [documented]; no array for an empty vector [observed: one.weap.vector.array.none] |
| DeleteWeapAdditionalVectorValue | [documented] |
| GetWeapAdditionalFlag | −1 [observed: one.weap.flag]; "There is not flags for now" [documented]. SetWeapAdditionalFlag has a See-also link but no page or catalogue entry |
| GetCalculatedSpreadAlt `[type]` | the actor's spread with its equipped weapon: 1 the wobble, 2 the weapon's, 0 (default) both [documented]. The wobble by the Gun Spread Formula (New Vegas) terms `fnv_combat::spread::actor_spread` uses (unaimed, moving, standing, Strength and skill shortfalls, times 1 − fWobbleToSkillConversion × skill/100, at least fMinGunSpreadValue), from the game's settings, the motion the engine tells and the player's aiming; crippled arms and perks left out, and the full spread taken as the sum [hypothesis]. The weapon's part is its Min Spread, or vector 2 at its condition |

**Follow-up (engine)**: `Game::one111_vector_at` gives a vector's value at a health; the
engine's damage, value, spread, DT/DR and projectile speed do not read it yet.

## 4. Armour, ammunition and weapon fields

| Function | NVForever |
|---|---|
| GetActorTotalArmorDR, GetActorTotalArmorDT | the sum over the worn armour of each piece's DNAM DR or DT times its condition share (1 − (0.5 − condition) × 1.0 below half, as SetArmorConditionPenalty documents) [documented: "calculated DR of all armor"; hypothesis: the condition is part of it]. DR in the unit GetArmorArmorRating reads |
| ApplyAmmoEffects `ammo type value` | the value through each of the ammo's effects (RCIL, record order) of that type: add, multiply, subtract [documented: the page's two examples, 2 → 3.5 with ×1.75, 30 → 20 with −10] |
| SetAmmoEffectTraitNumericAlt `effect trait value` | 0 the type (0–9, One111's four extra types 6–9 allowed), 1 the operation (0–2), 2 the value; 1 when set [documented]. GetAmmoEffectTraitNumeric reads the change |
| GetProjectileRefAmmo | the ammunition the shot was fired with: the one the shooter's weapon uses when it is the weapon fired (`nvse_proj::Shot::ammo`) [hypothesis: the engine does not say which round it fired] |
| SetWeaponItemModEffectAlt `weapon slot effect` | the slot's (1–3) mod effect, 0–26 (One111's list) [documented]; read by GetWeaponItemModEffect |
| SetWeaponItemModValue2 `weapon slot value` | the slot's second value [documented]; read by GetWeaponItemModValue2 |
| GetWeaponScopeEffectShader, SetWeaponScopeEffectShader | WEAP `EFSD` "Scope Effect" [documented: the Layer 1 schema]; the setter takes an EFSH [hypothesis: anything else refused] |

**Follow-up (engine)**: the engine's combat reads the records' ammo effects and weapon mod
effects (`fnv_combat::data`), not these session changes, and One111's new effect types
(Mod DT, Ignore DR/DT …) are not applied.
