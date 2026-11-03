# The weapon-instance plugin's functions: ppNVSE (2026-11-01, #1014)

ppNVSE's 42 catalogued script functions (catalogue origin `PP`) as NVForever builds them:
on the game side (`crates/fnv-game/src/nvse_pp.rs`, reached from `nvse::call`), the
file functions in `crates/fnv-game/src/mod_files.rs`, and two functions' output
variables in the VM (`out_params`, `crates/fnv-script/src/vm.rs`). Sources: the GECK
wiki export [documented: the page named] and the probe set `pp` of
`tools/capture/jip_probes.py`, run in the game with ppNVSE installed as
`VanillaPluginsPlus.dll` [observed: pp probes, the row named]; what neither settles is
marked `[hypothesis]`. The DLL registers 36 of the 42 names; EvaluateString,
GetAllAttachedWeaponMods, GetAllModsInSlot, GetWeaponBase, IsWeaponInstance and
NewWeaponInstance are not among them, so those six stay `[documented]` only. Tests:
`crates/fnv-game/tests/pp.rs`; lab scenario `L5.ext.pp` (§8).

Eleven catalogue names have no page under their own name. Their pages are there under
other titles, which the catalogue records (`page` in `schemas/script/functions.json`):
AX_Dump … AX_Size on "AX Dump" … "AX Size", GetAllAttachedWeaponMods on "GetAttachedMod"
(whose template carries the wrong name) and GetAllModsInSlot on "GetAllSlotMods". So
these are `[documented]` too, from those pages; what they leave open is marked.

Three catalogue rows were short of what their pages' examples pass, so scripts calling
them as the pages show did not compile; they are fixed in the generator's `PARAM_FIXES`
(`tools/capture/script_functions.py`) and the same rows edited in `functions.json`:
GetAllAttachedWeaponMods takes the slot after the weapon [documented: GetAttachedMod's
examples; optional, hypothesis: the table lists the weapon only], and OnAttachItemMod /
OnDetachItemMod take up to two filters after the flag, each its number and its form, the
flag and filters optional for the removal form `udf 0` [documented: OnAttachItemMod's
examples]. [decision: the rows were hand-edited and not regenerated, because a full
regeneration from today's wiki export also rewrites unrelated rows.]

## 1. AX variables

Values kept by a string key, optionally on a form, each key a list of entries; they last
for the session and are not saved [documented: every AX page, "all AX values remain valid
for the session"]. Keys are matched with case: "NVFPP" does not find "nvfpp" [observed:
ax.exists]; a key on a form and the same key without one are different variables
[documented: AX Exists, "If form is omitted, checks if the global key exists"]. In the
game AX_Exists with no form crashed it [observed: first run], so the global form is
`[documented]` only.

| Function | NVForever |
|---|---|
| AX_Set `value key [index] [form]` | the value (number, form, string or array) at the index, 0 when omitted [documented: AX Get, "Index 0 is assumed"]; an index below 0 or from 65,536 sets nothing and gives 0 [decision: PR #1291 review, a key's list is bounded]; the list grows with null entries to reach it [hypothesis]; an array is held as it is, not copied or flattened [documented: AX Set; observed: ax.get2 reads the array's entry back]. Returns the value [hypothesis: the page's return type "variable"] |
| AX_Get `key [index] form` | the value [observed: ax.get0, ax.get1, ax.get2]; 0 for none or a null entry [hypothesis] |
| AX_GetType `key [index] form` | −2 no such aux (or no such index), −1 null, 0 number, 1 form, 2 string, 3 array [documented; observed: ax.none, ax.types, ax.null] |
| AX_Size `key form` | the number of entries, nulls included; 0 for no key [observed: ax.none, ax.set, ax.null] |
| AX_Exists `key [form]` | [documented; observed: ax.exists, ax.drop] |
| AX_Find `value key deep [form]` | the index of the first equal entry, strings with case, −1 for none; the deep flag does not look inside arrays held [observed: ax.find: "hello" against "Hello", and 5 held in an array with the flag, both −1; hypothesis: equal entries are found, which no probe could show: the call without the flag did not compile in the game] |
| AX_Null `key [index] [form]` | the entry made null, the others keep their indexes [observed: ax.null]; index omitted or −1 deletes the key [documented]. Returns 0 [observed: ax.null] |
| AX_Erase `key [index] form` | the entry removed, the later ones shift down [observed: ax.erase]; −1 deletes the key [observed: ax.drop]. Returns 0 [observed: ax.erase, ax.drop] |
| AX_Dump `key [form]` | the entries to the console [documented]; 1 when the key exists [hypothesis] |

The catalogue's optional index comes before the form, so the trailing arguments are read
by type: the first number is the index, the first form the form.

## 2. Form instances

An instance is a new form standing for a base form, made by script and "tied to the users
save game" [documented: CreateFormInstance]. NVForever makes it a clone of the base form
(the same id space and field reading as TempCloneForm, `nvse_data`) [observed: inst,
inst.mod: IsFormInstance 1, GetStaticForm the base, GetWeaponItemMod the base's], with
the editor ID "Inst" and a count: the page says the base form's editor ID comes first,
but the 10mm pistol's instance was "Inst1" while GetEditorID gave the pistol's own
[observed: inst, edid.pistol]; the count rises with every instance the game makes and
is restored from the saved editor IDs, so one dropped on a load does not hand its
number to the next [hypothesis: the page's "InstanceCount"; decision: PR #1291 review,
no two instances share an editor ID]. Instances take clone IDs (`CLONE_BASE` and up);
when those reach the compiled scripts' range, CreateFormInstance gives 0
[decision: same review]. GetEditorID and
EditorIDToFormID find it. Instances are saved with the game
(`GameSnapshot::form_instances`) and come back as clones on a load.

| Function | NVForever |
|---|---|
| CreateFormInstance `form [key]`, NewWeaponInstance `weapon [plugin]` | the new instance [observed: inst]; 0 for an instance of an instance [documented: "You can not create instances of instances"; the call without a key did not compile in the game, so not probed]. NewWeaponInstance's plugin is not used and it returns the instance too [hypothesis: the page's return type says none] |
| GetStaticForm, GetWeaponBase | the base form an instance (or a reference's base) was made from; any other form itself [documented; observed: static.bat, inst] |
| IsFormInstance, IsWeaponInstance | [documented; observed: isinst.bat, inst]; a TempCloneForm clone is not an instance [hypothesis] |
| SetOnInstanceDeconstruct, SetOnInstanceReconstruct `priority udf [key]` | registered for the session; on a load the old game's instances are deconstructed, then each loaded one reconstructed, and a reconstruct handler that sets its function value true drops that instance [documented: both pages]; higher priority first [hypothesis, as event handlers]. They are carried across the load with the session (`Game::take_session`), function scripts only, as the event handlers. DeleteFormInstance has no catalogue entry, so instances are deconstructed on a load only |

## 3. Item mods

The vanilla slots "1"–"3" are the weapon's WMI1–3, on for an inventory item through its
own mod flags (as SetWeaponRefModFlags). On a form instance AttachMod turned no vanilla
slot on and gave 0 [observed: attach.vanilla]; a base form is taken the same
[hypothesis]. Every other slot name is one of the Extended Item Mods System's, whose
slots Devkit defines (§6): a name no Devkit file defines takes nothing and gives 0
[observed: attach, "nvfslot" on an instance]. NVForever reads no Devkit, so its set of
defined slots (`PpState::slots`) is empty and no extended slot can be filled until a
loader exists; the attach/detach machinery behind it is built and tested with slots
put in by the test. Attachments last for the session [hypothesis].

| Function | NVForever |
|---|---|
| AttachMod `object slot [itemMod]` | on an inventory item a vanilla slot is turned on [documented: the first example]; an item mod given must be that slot's WMI [hypothesis]; elsewhere nothing, 0 [observed: attach.vanilla]. A defined named slot gets the mod, or with none loses the one it had [documented: notes]; the detach handlers run for the mod replaced or removed and the attach handlers for the new one [documented: OnAttachItemMod]; an undefined one nothing, 0 [observed: attach]. Returns 1 when something was done [hypothesis] |
| GetAllAttachedMods `weapon` | a string map, slot name → item mod, of the vanilla slots on and the named ones [documented]; empty for a base form [observed: allmods.pistol] |
| GetAllAttachedWeaponMods `weapon "slot"` (page GetAttachedMod) | the mod in that slot; no slot, none [hypothesis] |
| GetAllModsInSlot `object [slot]` (page GetAllSlotMods) | the mods that can go in the slot as a string map of slot → mod, every slot's without one [documented]; without Devkit's lists a vanilla slot can take its WMI and a named slot the mod attached there [hypothesis] |
| HasExtendedMods `object [checkVanillaMods]` | the object has a mod in a named slot [hypothesis: "uses the extended mod system" read as having an extended mod], or, with the flag, its weapon has vanilla mods (a WMI slot filled): the 10mm pistol's base form gave 0 without the flag and 1 with it [observed: hasext.pistol, hasext.pistol.vanilla] |
| OnAttachItemMod, OnDetachItemMod `priority udf runOn… "1" itemMod "2" object`, or `udf 0` | the handler, with its filters, or removed [documented: both pages' examples]; the udf gets the item mod and the object [documented]. runOnReconstruct/runOnDeconstruct are kept; named-slot attachments do not outlive a load, so they never fire on one |

## 4. The player, weapons, inventory and dialogue

| Function | NVForever |
|---|---|
| IsThirdPerson | 1 when the camera is not first person, as the engine tells it (`Game::set_first_person`) [documented; observed: third, 1 in the probe save, which is in third person] |
| GetPlayerCameraRotation `x y z` | the three variables get the player's angles in degrees, roll 0 [documented: "pitch, roll and yaw" in degrees; observed: camrot, roll 0]; the first-person camera looks where the player does [hypothesis: the probe save is in third person, where pitch and yaw were 8° and 11° off the player's angles, the camera's orbit; the engine does not tell an orbit to the game side] |
| FindWeaponAmmo `weapon [ammo]` on an actor | none for a weapon without ammo [observed: ammo.bat]; the first of the weapon's ammo (NAM0, an ammo or a form list of them) the actor carries; given an ammo, the next carried one after it, going round to it last; none carried, no form [documented: the page's 9mm example]. An ammo not in the list starts from the top [hypothesis] |
| ReplaceItemInInventory `newBase [copy] [count]` on an inventory reference | the item replaced by the new base form, keeping whether it is worn [documented: "This works on equipped items"]; `copy` keeps its condition and mods too, `count` how many (all when omitted) [hypothesis: the arguments' meaning] |
| SetSightingNode `name`, GetCurrentSightingNode | `##SightingNode` until a node is set [observed: sight]; the node set until the player's weapon changes [documented; observed: sight.set]. While one stands, further sets do nothing: after "Bip01 Head", setting "##SightingNode" or another name left it reading "Bip01 Head" [observed: sight.set, sight.set2; hypothesis: a weapon change clears it, as the page says] |
| SetWeaponDegradationMult `multiplier [weapon]` (or `weapon multiplier`, as the example) | the weapon base's per-shot wear [documented]. GetWeaponDegradationMult (catalogued under the setter's name, so a call with no number reads) gives it, else the record's Damage to Weapon Mult when its override flag is on, else fDamageToWeaponValue (0.2) [hypothesis: what it reads with nothing set] |
| GetTopicPrompt `info` | the name (FULL) of the info's topic, not the info's own prompt (RNAM) the page names: for a line of Doc Mitchell's with a 27-character prompt the game gave the 26 characters of its topic's name, and for a line with no prompt the 19 of its topic's [observed: prompt, prompt.none]; "" without a topic or a name [hypothesis] |
| EvaluateString `"condition"` | 1 when the text holds as an `if` condition, compiled once per text as a function script [documented]; a text that does not compile is false and goes to the trace [hypothesis] At most 1,024 distinct texts are compiled in one game (a text that does not compile is remembered as such and counts); a new text past that gives 0 and goes to the trace. A compiled unit lives as long as the game (the VM runs units borrowed for the game's lifetime, and lambdas keep theirs), so the cache cannot evict [decision: PR #1291 review, memory bounded] |
| CallLoop `delay udf args…` on a reference (the player when none) | the function called every `delay` seconds of game time; a function that sets its value sets the next delay, below 0 ends the loop; `-1` as the delay removes it [documented]. The first call after one delay; the same udf on the same reference again replaces it [hypothesis]. Not kept across a load [documented: the page's example text, "auto-removed when loading a game"] |

## 5. Files

pReadFile `path [start] [end]` reads lines as strings, start counted from 0, end left out
[documented: "0 5" reads the first five lines; observed: readfile.lines, 3 lines for
"0 3"; readfile, every line of Fallout_default.ini without them], from under the game's
folder [documented: pWriteFile's notes; observed: readfile]; a missing file gives an
empty array [hypothesis]. pWriteFile `path array [append]` writes one line per element
under the game's folder and gives 1 [documented; observed: file.write, file.read read
the two lines back] and pWriteFileAlt under Data [documented: "relative
to the game's DATA path"; hypothesis: a mod's staging folder is its place in Data]. Both
write into NVForever's own layer, never the game's folder (CLAUDE.md charter;
`mod_files` module notes), and readers read that copy first.

A path is taken only when every part is a plain name: one starting at a root (`\x`),
naming a drive or stream (any `:`), or with a `.` or `..` part is refused by the readers
(an empty array) and the writers (0, nothing written), and a writer also refuses a disk
path inside the game install, the original game's user folder
(`fnv_install::guard_write`) or the game folder above the Data read root
[decision: CLAUDE.md charter; PR #1291 review]. This holds for every `mod_files` reader
and writer, the INI functions and the array files included.

## 6. Not read: Devkit

ppNVSE's Devkit text files define form traits, item-mod slots and the mods they take,
akimbo sets, and kit commands (the catalogue's `Kit::` entries, which are not script
functions and not counted). NVForever does not read them [decision: a loader is a
separate design; it would fill `PpState` and a trait store]. Until then:

- GetFormTrait (the catalogue's one name for the pages GetFormTrait, GetFormTraitType and
  GetFormTraitSize), HasFormTrait: no trait exists: value 0, type −1 [documented: "Trait
  does not exist"; observed: trait.pistol, 0 with no Devkit], size 0 [hypothesis].
  SetFormTrait has no catalogue entry. HasFormTrait with the catalogue's arguments (index
  first) did not compile in the game, so its argument list is a follow-up (§7).
- IsAkimboForm, HasAkimboSet: 0 [observed: akimbo.pistol, akimboset.pistol, no Devkit];
  GetAkimboWeaponsAlt writes no forms.

## 7. Follow-ups

- **Engine**: the iron sights (`nvforever::iron_sights`) look for the default node, not
  `Game::pp_sighting_node`; the weapon wear (`fnv_combat::condition::weapon_wear`) does
  not read `Game::pp_degradation_mult`.
- **Catalogue**: GetWeaponDegradationMult, GetFormTraitType and GetFormTraitSize share
  another function's catalogue name (their pages carry its template name), so scripts
  calling them by their own names do not compile yet; `nvse_pp` answers the names.
- **Catalogue**: HasFormTrait's arguments (above); AX_Find's deep flag and
  CreateFormInstance's key are optional in the catalogue but the calls without them did
  not compile in the game [observed]; the catalogue is left as the pages give it.
- Devkit (§6).

## 8. Lab

`L5.ext.pp` (parity-lab/scenarios/layer5_ext.json) runs the `pp` set's 37 probes through
`fnvtool ext queries` against the private golden `jip/pp.json` (third run, 2026-11-01):
35 of 37 equal. The two that differ are the probe save, not NVForever: `third` (the save
is in third person, the headless candidate starts in first person) and `camrot` (the
third-person camera orbit, 8° and 11° off the player's angles; first person gives 0).
Three runs were needed: the first crashed the game on AX_Exists with no form, and calls
that did not compile in the game were taken out of the set (the set's own notes).
