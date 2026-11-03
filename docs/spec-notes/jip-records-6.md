# JIP LN NVSE, batch 6 (2026-11-01, #1006)

Destruction data, creature data and messages, in `crates/fnv-game/src/nvse_jip6.rs`.
Sources: the GECK wiki export, each function's page [documented: the page named]; the
fields: xEdit's record definitions as the schema keeps them (`schemas/fnv/common.json`
DEST/DSTD/DMDL, `gameplay.json` CREA and MESG). Not probed in the original: the extender
goldens of this machine's probe game are kept on the other machine, so every value here
comes from the pages and the records; choices the pages leave open are tagged
[hypothesis]. Setters change the form for the session (nothing is saved: "the changes will
not be saved with the game", AddDestructionStage [documented]) and are read by the getters
and by the game.

## 1. Destruction data

`object` is a base form; a reference reads its base's [documented: the pages' `object:ref`,
"only usable with object types that have Destruction Data"]. A form without destruction
data reads 0, none or "".

| Function | NVForever |
|---|---|
| Get/SetDestructionDataTargetable `object [0/1]` | DEST's flag VATS Targetable |
| GetNthDestructionStageDebris / Explosion `object index` | DSTD's debris and explosion |
| GetNthDestructionStageReplacement `object index` | the stage's DMDL path, "" without one |
| Get/SetNthDestructionStageTrait `object index trait [value]` | 0 health percentage, 1 self damage per second, 2 model damage stage, 3 flags (1 cap damage, 2 disable, 4 destroy), 4 debris count [documented: the page's tables]; a trait past 4 reads 0 |
| SetNthDestructionStageDebris / Explosion `object index [form]` | left out: none [documented: the page's second example] |
| SetNthDestructionStageReplacement `object index path` | |
| AddDestructionStage `object index healthPercent damageStage` | a new stage at the index: the new stage count, -1 when nothing was added [documented]; an index past the end adds at the end [hypothesis] |
| RemoveDestructionStage `object index` | the new stage count, -1 when nothing was removed [documented] |

After an add or a remove, each stage's DSTD index is its place in the list [hypothesis].
A value a trait cannot hold is not set: a health percentage past 0-100, a byte trait past
0-255, NaN or infinity; AddDestructionStage with a NaN or infinite index adds nothing and
gives -1 [hypothesis: the game keeps the fields in range]. The int arguments reach the
functions already made whole (the VM's int conversion), so a number past the i32 field
arrives as its nearest end.
The setters return 0 [hypothesis: as the record setters of batch 5 were observed to,
jip-records-5.md].

The game reads the changed data too (`Game::destruction`: object damage, the stages'
models and explosions, VATS targeting, GetDestructionStage). The page says the changes
reach only references made after the call [documented: AddDestructionStage, "no effect on
existing references"]; NVForever reads a base's data each time a reference is damaged, so
existing references take them too [decision: one store per base, nothing kept per
reference].

## 2. Creatures

`[baseForm]`, else the calling reference's base [documented: the pages' examples,
`RexRef.GetCreatureDamage` and `GetCreatureDamage Rex`]. Not a creature: 0 / none.

| Function | NVForever |
|---|---|
| Get/SetCreatureDamage `[damage] [baseForm]` | DATA's damage (the unarmed attack's), which combat's creature attack reads too (`Game::creature_attack`); a value past the i16 field, NaN or infinity is not set [hypothesis] |
| GetCreatureReach `[baseForm]` | RNAM |
| GetCreatureWeaponList `[baseForm]` | LNAM, the melee weapon list |
| Get/SetCreatureSoundsTemplate `creature [template]` | CSCR ("Inherits Sounds from"); left out clears it [documented] |
| CreatureHasModel `path [baseForm]` | the model list (NIFZ) has the path, any case [documented]; a model toggled off is still in the list [hypothesis: "the model-list includes"] |
| GetCreatureModels `[baseForm]` | an array of the paths toggled on [documented] |
| ToggleCreatureModel `path on [baseForm]` | turns one on or off: 1 when the list has it, 0 otherwise [documented: `success`; hypothesis: 0 for a path not listed] |

Toggling a model changes the list the functions read; the engine still draws a creature
from its whole list (follow-up: drawing only the models toggled on).

## 3. Messages

| Function | NVForever |
|---|---|
| Get/SetMessageFlags `message [flags]` | DNAM: 1 Is Message Box, 2 Auto-display [documented]; ShowMessage shows a message as a box by the changed flag |
| SetMessageDisplayTime `message seconds` | TNAM, the seconds a corner message stays; a negative value, one past the u32 field, NaN or infinity is not set [hypothesis] |
| Get/SetMessageDisabled `messageOrList [0/1]` | a disabled message is never shown [documented]: by ShowMessage, nor as the box the engine shows on refusing power armour without the training; a form list disables or enables each message in it [documented] |
| ToggleCraftingMessages `0/1` | the recipe menu's "added" corner lines off or on [documented] |

ToggleCraftingMessages' page names its argument's type but not the argument, and the
compiler counts only named ones, so the catalogue row got a name
(`tools/capture/script_functions.py` PARAM_FIXES).
