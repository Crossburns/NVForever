# Spec note: the base-game script language as the editor documentation describes it

Clean-room notes on the script language of the shipped game (the GECK language, called
"obscript" by the extender community), for Layer 5 and for a later mod-compatibility
layer. The source is a private export of 53 GECK wiki pages (geckwiki.com) the owner made
on 2026-10-09, kept at `C:\fnv-private\geckwiki\GECK-20261009093011.xml` and never
committed. The pages used most are: Scripting for Beginners, Object scripts, Quest
scripts, Reference scripts, Non-reference scripts, Reference functions, Calling
reference, Base Form, Ref Variable, Inventory Reference, Boolean, Int, Float, Globals,
Category:Variables, Function syntax, Edit Scripts, Script Validator, Useful Scripts,
Adding an Options Menu and Category:Conditions; the extender pages (Let, Eval, While,
ForEach, NVSE Expressions, Script Compiler Override, Inline Expression, NVSEScript and
others) feed §6 only. Category:Dialogue comes from the actor export
(`GECK-20261009092949.xml`). Everything below is our own summary; no wiki text is copied
(checked mechanically: no run of 8 lower-cased words in common with either export).

Tags:
- **[documented: P]** stands for "[documented: GECK wiki P, owner export 2026-10-09]";
  **[documented: catalogue notes]** is our paraphrase of a function's wiki page in
  `schemas/script/functions.json` (script-functions.md §1);
- **[observed]**: our in-game captures (layer5-captures.md) or the shipped masters
  (scripts.md §1–3, or a count made for this note with `fnvtool script units|decode`
  over the ten masters on 2026-10-09);
- **[hypothesis]**: neither.

Where the documentation and a capture disagree, the capture wins; every difference is in
§7.2. "Runtime" means `crates/fnv-script/src/vm.rs` and `crates/fnv-game` as of `main`
8c56374 (scripts.md §4–10), read only.

## 1. Kinds of script and what they run on

### 1.1 Script kinds

- A script form is one of three kinds, chosen in the editor: **object**, **quest** or
  **effect** (the editor says "Magic effect") [documented: Edit Scripts; Scripting for
  Beginners]. The kind decides what the script can be attached to. These are SCHR types
  0, 1 and 0x100 [observed: scripts.md §1.2].
- **Result scripts** are script fragments stored inside other records. They need neither a
  ScriptName line nor Begin/End blocks [documented: Scripting for Beginners]. The pages
  name quest stage results and dialogue results [documented: Non-reference scripts;
  Reference scripts]; the masters also carry package begin/end/change, terminal item and
  patrol-marker results [observed: scripts.md §1.3]. Every SCPT begins with the
  ScriptName statement and embedded units have none [observed: scripts.md §2.2], which
  agrees.
- `scn` is an accepted short form of `ScriptName` [documented: Scripting for Beginners].
- **Object scripts** go on forms that can be placed or carried (actors, activators,
  items, …) and can use almost every block type [documented: Object scripts].
- **Quest scripts** are attached to quests, cannot use the reference-specific blocks, and
  run at the quest's processing delay [documented: Quest scripts; Scripting for
  Beginners]. Their timing is in quests-dialogue.md §1 and §5.
- **Effect scripts** are attached to base effects (used by object effects, actor effects
  and ingestibles). They run **on the effect's target**, not on the effect, can use only
  ScriptEffectStart, ScriptEffectUpdate and ScriptEffectFinish, and their variables cannot
  be read or written from other scripts [documented: Scripting for Beginners]. The
  runtime gives each active effect its own instance on the target (scripts.md §7.6):
  agrees. Their timing is captured (layer5-captures.md §9.3 b).

### 1.2 Reference and non-reference scripts; the calling reference

- Most functions act on a reference ("reference functions"). A **reference script** may
  call them without naming the reference; the call then applies to the reference the
  script runs on. Reference scripts are object scripts and dialogue result scripts
  [documented: Reference scripts; Reference functions]; effect scripts count as reference
  scripts too, running on the target [documented: Scripting for Beginners].
- A **non-reference script** (a quest script or a quest stage result) has no reference of
  its own, so every reference function in it must name one: `SomeRef.Func` [documented:
  Non-reference scripts; Quest scripts]. Runtime: stage results and quest scripts run with
  no self reference (`RunCtx::self_ref` None): agrees.
- **But a stage result's reference-less call acts on the player**: VDoctors stage 20
  (`RemoveSpell WithdrawalJet` … with no reference, the doctors' addiction cure) cleared
  the player's Jet withdrawal within 10 frames of `SetStage VDoctors 20` [observed: o5_03,
  2026-10-31, nvfairport, IsSpellTarget 1 then 0; observer-checks.md check 42]. Which
  other functions do so, and in quest scripts, is not captured [hypothesis: the player is
  the default reference]. Runtime: RemoveSpell with no reference in a script that has none
  acts on the player (`magic::remove_spell`, `Ctx::subject_or_player`; #1110); other
  reference functions still fail such a call.
- When a script runs with a calling reference, every call written without a reference
  goes to it, and GetSelf returns it [documented: Calling reference]. A dialogue result
  script runs on the speaker [documented: Reference scripts, as a reference script;
  Category:Dialogue; the runner's `say_info` does the same: agrees].
- The explicit reference is either the Editor ID of a **persistent** reference or a
  local `ref` variable [documented: Scripting for Beginners]. In the bytecode these are an
  `SCRO` entry and an `SCRV` entry of the reference list [observed: scripts.md §1.2]:
  agrees.
- An item in an inventory runs its object script, and implicit calls work on it
  [documented: Scripting for Beginners], although inventory items are not references
  [documented: Inventory Reference]. GetContainer gives the holder [documented: Scripting
  for Beginners]. Runtime: one instance per (holder, item) (scripts.md §7.1).

### 1.3 Base forms and references

- A base form is the template in the object window or inside an inventory; a reference is
  one placed instance with its own position and current state [documented: Base Form].
- A `ref` variable may hold either. A base form in it is still not a reference: using it
  as the calling reference fails and may crash, while passing it as a parameter is fine
  where the function accepts a form [documented: Ref Variable; Base Form; Calling
  reference; Function syntax].
- Runtime: a base form used as the calling reference is passed to the function like a
  reference; what each function does with it is not specified [hypothesis: harmless,
  since no shipped call should do it].

### 1.4 Instances and variables of other scripts

- Every reference that runs an object script has its own copy of the variables
  [documented: Object scripts; Scripting for Beginners]. Runtime: one instance per
  scripted reference: agrees.
- Another script's variable is written `Owner.Variable`. For a quest it is the **quest's**
  Editor ID, not the script's [documented: Useful Scripts; Adding an Options Menu; Quest
  scripts]; for a reference it is the reference's Editor ID, which needs the reference to
  be persistent [documented: Object scripts]. Bytecode: `r` + reference list entry +
  variable index of the script that the form runs [observed: scripts.md §2.5]: agrees.
- Dialogue conditions read object-script variables through GetScriptVariable [documented:
  Object scripts].
- A quest's variables stay readable and writable while the quest is stopped [documented:
  Quest scripts]; already in quests-dialogue.md §1.

## 2. Variables

### 2.1 Types

- **short, long, int** are the same integer type; **float** holds fractions; **ref**
  holds a form [documented: Scripting for Beginners; Category:Variables; Int]. The
  compiled form keeps only Integer or FloatOrRef per variable (`SLSD`) [observed:
  scripts.md §1.2]: agrees. The keyword `reference` is also accepted [observed:
  scripts.md §3.4].
- Integer range −2,147,483,648 … 2,147,483,647 [documented: Category:Variables]; the
  capture agrees (no 16-bit wrap; out of range gives −2147483648) [observed:
  layer5-captures.md §5.5].
- **A fraction stored into an integer is dropped.** The Int page calls this flooring;
  the capture shows truncation toward zero (−2.7 → −2, −0.5 → 0) [observed: §5.5]. The
  capture wins; the runtime truncates (§7.2 row 1).
- **Float precision**: Category:Variables gives single precision (about 7 digits); the
  capture shows doubles (16777217 and 0.1 read back unchanged) [observed: §5.5]. The
  capture wins; the runtime keeps doubles (§7.2 row 2). The Int page itself says the
  integer types are kept as doubles behind the scenes [documented: Int], which fits the
  capture.
- Comparing floats with `==` is unreliable; the page advises flooring or `<=`/`>=`
  [documented: Float]. Advice only; no rule for the runtime.

### 2.2 Declaration and initial value

- Declarations go at the top, outside every Begin/End block [documented: Scripting for
  Beginners]. The compiler nevertheless accepts a declaration inside a block: six SCPT
  units of the masters do it, and their skip counts include the declaration line
  [observed: scripts.md §2.6]. See §7.2 row 4.
- A variable name must be unique within its script and must not equal any form's
  Editor ID [documented: Scripting for Beginners]. (Ten SCPT units of the masters have two
  declarations sharing one index [observed: scripts.md §1.2]; the documentation does not
  explain this.)
- A new variable reads 0 [documented: Scripting for Beginners; Ref Variable]. Runtime:
  instances start with every variable 0 (scripts.md §7.1): agrees.

### 2.3 Booleans and references as values

- There is no Boolean type. In an `if`, 0 (including an empty `ref`) is false and any
  other value is true; functions documented as Boolean return 1 or 0 [documented:
  Boolean].
- Comparison operators and `&&`/`||` give only 1 or 0 [documented: Scripting for
  Beginners; NVSE Expressions, which contrasts the extender's operators with this].
  Runtime: the same (scripts.md §4.2, a [hypothesis] there that this documentation now
  supports).
- An empty `ref` is 0; `if r` tests that it is set; `r == PlayerREF` compares references
  [documented: Ref Variable]. Scripting for Beginners prefers GetIsReference over `==` for
  references [documented]. Runtime: a reference read as a number is its FormID
  (scripts.md §4.1), so both forms work there.
- `player` and `PlayerREF` name the same reference in almost every case [documented:
  Scripting for Beginners]. Both compile to FormID 0x14 [observed: scripts.md §3.4]:
  agrees.

### 2.4 Globals

- A global (GLOB form) is visible to every script and condition without an owner
  [documented: Globals].
- Its declared type is short, long or float; short and long are the same, and **every
  global is stored as a floating-point number** internally, so very large values lose
  precision (the page's example: two billion and the next 64 integers all read as two
  billion, which is 32-bit float spacing) [documented: Globals]. The runtime stores
  globals as `f32` (`fnv-sim` `World::set_global`): agrees. Whether writing a fraction to a
  short global keeps the fraction is not documented; the runtime keeps it (§8 item 7).
- The value in the plugin is used only when the plugin is first loaded; after that the
  save holds it [documented: Globals]. Layer 7 (saves).
- **Constant** globals are reset to the plugin value whenever a save is loaded; a script
  that sets one from the main menu (MenuMode 4) changes that reset value for the session
  [documented: Globals]. Not modelled (Layer 7).

### 2.5 Result-script variables

- Variables declared in a **dialogue result script overwrite the quest script's variable
  with the same index** [documented: Category:Dialogue, listed as a bug]. In the masters,
  40 INFO result scripts declare a local, and so do 86 TERM, 13 PACK and 11 REFR
  (patrol) result scripts [observed: `fnvtool script units`, units with `variables` > 0].
  The runtime gives every result-script run fresh temporary locals (`Locals::Temp`), so
  it does not reproduce this (§7.3 row 3).

## 3. Blocks and when they run

- A script's code sits in `Begin <block> … End` blocks. Each frame in which the script
  runs, its blocks are checked from top to bottom and each one whose event holds runs
  [documented: Scripting for Beginners]. Runtime: `Game::run_event` runs every block of
  the event in order: agrees. In the masters 173 units have some block type twice
  [observed].
- **Block types** [documented: Scripting for Beginners, glossary]:
  - general: GameMode, MenuMode;
  - effect: ScriptEffectStart, ScriptEffectUpdate, ScriptEffectFinish;
  - for one reference (grouped here by theme): items — OnAdd, OnDrop, OnEquip, OnUnequip,
    OnActorEquip, OnActorUnequip, OnSell; world — OnActivate, OnLoad, OnRelease,
    OnDestructionStageChange, and the triggers OnTriggerEnter, OnTrigger,
    OnTriggerLeave; combat — OnStartCombat, OnCombatEnd, OnHit, OnHitWith,
    OnMagicEffectHit, OnDeath, OnMurder; AI and dialogue — OnPackageStart,
    OnPackageChange, OnPackageDone, SayToDone.
  Most reference-specific blocks only make sense on some forms (OnDeath on actors, OnAdd
  on items that can be carried) [documented]. The catalogue's `begin_blocks` has the codes
  (script-functions.md).
- **Block parameters** narrow a block: `Begin OnAdd player` runs only when the holder is
  the player [documented: Scripting for Beginners]. Runtime: `block_accepts`: agrees, and
  the MenuMode code works the same way (scripts.md §7.2).
- **Object-script GameMode and MenuMode run every frame**, and that rate cannot be
  changed [documented: Object scripts]: every frame in which the object, or for an item
  its container, is loaded [documented: Scripting for Beginners, glossary]. Observed every
  frame for the skull's object script and the Goodsprings windows [observed:
  layer5-captures.md §8.3 d, §9.3 c]. Runtime: every frame for enabled scripted
  references in loaded cells and for the player's items: agrees for those; items in other
  inventories are §7.2 row 6.
- **Disabled references still run their object scripts** [documented: Object scripts].
  GameMode runs for a disabled reference at the same rate as while it is enabled
  [observed: capture job `od_09a_disabled_ref_script`, 2026-10-18: the skull's `timer`
  fell from 998.19 to 992.94 over 300 loops while disabled, owner-decision-captures.md].
  Runtime: `Game::active_refs` runs GameMode and MenuMode for disabled references
  (MenuMode with them is [hypothesis]); `Game::set_loaded` runs OnLoad at a cell load
  for disabled references too [observed for VMS15's rising plane, layer5-captures.md
  §8.3 d]; see §7.2 row 5.
- **Quest scripts** run only while the quest runs, at its delay; a tiny delay (0.01, or
  0.001 in another page) amounts to every frame [documented: Quest scripts; Scripting for
  Beginners]. Captured the same (quests-dialogue.md §5).
- MenuMode replaces GameMode while a menu is open (scripts.md §7.2). One tutorial remark:
  the game goes back to GameMode as soon as the player starts putting the Pip-Boy away,
  about half a second before it is gone [documented: Adding an Options Menu]. Layer 7
  (menus).
- A message box is read by polling GetButtonPressed, which gives −1 until the player has
  chosen; menus built from ShowMessage are state machines in GameMode that return early
  while it is −1 [documented: Adding an Options Menu]. Runtime: the same reading of −1
  (scripts.md §7.2, engine API).

## 4. Statements, expressions and calls

- **Case**: the language ignores case [documented: Scripting for Beginners]. Editor IDs
  resolve case-insensitively in the compiler's lookup [observed: scripts.md §3.4]: agrees.
- **Comments** run from `;` to the end of the line; plain GECK scripts have no block
  comments [documented: Scripting for Beginners]. The masters' compiler also drops lines
  with no letter or digit [observed: scripts.md §3.4].
- **Set**: `set <variable> to <expression>` [documented: Scripting for Beginners].
- **If**: `if` … `elseif` … `else` … `endif`; only `if` takes an `endif`; there is no
  switch statement [documented: Scripting for Beginners]. Markers without a matching
  `if` compile and run in a way the documentation does not cover; the captures settled
  the stray `elseif` (scripts.md §4.2, layer5-captures.md §8.3 c).
- **Return** ends the script for the current frame [documented: Adding an Options Menu].
  Runtime: it ends the current block only (each block is its own run). The two differ
  only where a later block of the same event would run in the same frame (§7.3 row 2).
- **Operators**: `+ - * / %`, `== != > < >= <=`, `&& ||`; there is no NOT or XOR (write
  `A == 0`) [documented: Scripting for Beginners]. The bytecode also has unary minus
  [observed: scripts.md §2.3]. The vanilla pages give no precedence table; the masters
  show `||` binding tighter than `&&` (scripts.md §3.3), which is the reverse of the
  extender's expressions (§6).
- **No short-circuit**: the Eval page says short-circuiting `&&`/`||` came with xNVSE 6
  for `eval` [documented: Eval]. A plain `if` evaluates both operands [observed: o5_01,
  2026-10-31: `if 0 && player.ModAV Variable09 10`, `if 1 || …`, `if 0 == 1 && … == 0`
  and five more forms all ran the ModAV; observer-checks.md check 51]. The runtime
  evaluates the whole postfix expression: agrees.
- **Calls**: `[ref.]Function arg arg …`; arguments are separated by spaces and/or commas;
  omitted optional arguments take their defaults [documented: Scripting for Beginners;
  Function syntax]. The compiler reads commas like spaces [observed: scripts.md §3.4]:
  agrees.
- **A function's result cannot be used directly as the calling reference or as an
  argument of another function**; it must go through a variable first [documented:
  Scripting for Beginners; Script Compiler Override; Inline Expression]. The bytecode
  agrees: parameters are literals, variables, globals or reference-list entries, never a
  call, and a calling reference is always a reference-list entry [observed: scripts.md
  §2.3–2.4].
- **Inside an expression a function takes its arguments before any operator applies**:
  the plain compiler reads `Sin 14 == 1` as `(Sin 14) == 1`, which is why the extender's
  override mode, which reads it the other way, needs a warning [documented: Script
  Compiler Override]. The masters' compiler takes as many following atoms as the
  function has parameters [observed: scripts.md §3.4]: agrees.
- **Failures are silent**: a `set` whose assignment fails stops the script without a
  message [documented: Let]; an `if` that calls a function on an empty `ref` fails the
  same way [documented: Eval]; a reference function called on an unset `ref` variable
  stops the script [documented: Ref Variable]. Runtime: a call on an empty `ref` is
  skipped and gives 0, and the script goes on, with a `call_failed` trace event (§7.3
  row 1, scripts.md §4.2) [observed: the original does so, §7.3 row 1].
- A source line is limited to 512 characters [documented: Inline Expression; Lambda].
  A compiler limit; the runtime reads bytecode only.

## 5. Compiler and validator rules that explain the shipped bytecode

| Rule | Documented | What the bytecode shows [observed: scripts.md] | Runtime |
|---|---|---|---|
| Result scripts have no ScriptName and no blocks | Scripting for Beginners | ScriptName is the first statement of all 3,715 SCPT and of no embedded unit (§2.2) | result scripts run their top-level statements (`run_result`) |
| Function results are never arguments or calling references | Scripting for Beginners; Script Compiler Override | no call token among parameter encodings; calling references are reference-list entries (§2.3–2.4) | — |
| A named reference must be persistent, or a local `ref` | Scripting for Beginners; Object scripts | `SCRO` FormIDs and `SCRV` local indices (§1.2) | resolves both |
| Remote quest variables go through the quest | Useful Scripts | `r` + index of the quest's `SCRI` (§2.5) | §4.3 |
| Declarations belong before the first block | Scripting for Beginners | six units declare inside a block; the skip count includes the line (§2.6) | structured reading (§4.2) |
| Functions bind their arguments first | Script Compiler Override (by contrast) | a call takes as many atoms as it has parameters (§3.4) | postfix as stored |
| `player` = `PlayerREF` | Scripting for Beginners | both are 0x14 (§3.4) | — |
| The editor compiler misses errors an outside validator finds | Script Validator | stray `else`/`elseif`/`endif` markers compile (§2.6) [hypothesis: one cause] | §4.2 rules |
| "Recompile All" writes every master script into the plugin being edited | Edit Scripts | a plugin can override vanilla scripts with copies; the runtime resolves variable names per viewer (§2.5) | load-order viewer |

Rules the documentation states that the bytecode cannot show: names may not equal an
Editor ID; the 512-character line limit; case-insensitivity of keywords.

## 6. Script extender features (extender, out of scope for the base game)

Shipped masters use none of this (script-functions.md §2 [observed]). Summarised only for
a later mod-compatibility layer [documented: the extender pages of the export]:
- **Statements**: `let` (assignment with the extender's expressions; it reports a failed
  assignment and carries on, where `set` stops silently), `if eval`, `while … loop`,
  `foreach … loop` over containers, arrays, strings and form lists, `continue`, `break`,
  `Label`/`GoTo`.
- **Values**: `array_var` (list, numeric map and string map arrays, saved in the
  extender's co-save) and `string_var`; **inventory references**, temporary references to
  an inventory stack, valid for one frame.
- **Syntax added in xNVSE 6.1**: inline expressions in parentheses as function arguments,
  inline variable declarations, `x = …` / `x += …` as shorthand for `let`, dot calls on
  function results and array elements, lambdas, binary and hex literals; `/* */` comments
  (6.1.7; read as comments from 6.2.1 on).
- **The Script Compiler Override** (`Begin _GameMode` and so on, now deprecated) switches a
  block to the extender's compiler, which also changes how a line parses (`Sin 14 == 1`).
- **NVSEScript** (xNVSE 6.3.6): a second language with C-like syntax (`name`, `;` line
  ends, calls with parentheses, `for`, `match`), interoperable with obscript scripts.
- **User-defined functions** are object scripts with a `Begin Function {…}` block.
- **Expression semantics differ** from vanilla: `&&` binds tighter than `||`, `||` and
  `&&` can return an operand's value instead of 1/0, and there are bitwise, power, string
  and array operators [documented: NVSE Expressions].

What a compatibility layer would need: the extender's opcodes are assigned by the running
extender (script-functions.md §2), its statements have their own byte forms inside `SCDA`
[hypothesis], a co-save holds arrays and strings, and the two expression grammars must not
be mixed up when reading bytecode.

## 7. Cross-check

### 7.1 Agreements

The runtime and the captures agree with the documentation on: Booleans as 0/1 and
comparisons giving 1/0; variables starting at 0; short = long = int with a 32-bit range;
`player` = `PlayerREF`; implicit calls in reference scripts and explicit ones in quest
scripts; one variable set per reference; remote variables through the quest or the
persistent reference; effect scripts on the target; blocks checked in order with
parameters; object GameMode every frame; quest scripts only while running and every
frame at a tiny delay; globals as 32-bit floats; arguments bound before operators; no
function results as arguments.

### 7.2 Differences with a capture or with the masters (the capture wins)

| # | Topic | Documentation | Capture or masters | Runtime |
|---|---|---|---|---|
| 1 | Fraction into an integer variable | floored (Int) | toward zero: −2.7 → −2, −0.5 → 0 [observed: layer5-captures.md §5.5] | toward zero (follows the capture) |
| 2 | Float variable precision | single, about 7 digits (Category:Variables) | double: 16777217, 0.1, 123456789.123 unchanged [observed: §5.5] | double (follows the capture) |
| 3 | Out-of-range and literal integers | not covered | out of range → −2147483648; whole literals saturate at 2147483647 [observed: §5.5] | as captured |
| 4 | Declarations inside a block | not allowed (Scripting for Beginners) | six SCPT units compile with one, skip count one too large [observed: scripts.md §2.6] | structured reading |
| 5 | Disabled references | object scripts run even when disabled (Object scripts) | OnActivate ran on disabled, dead ED-E when activated with the run-OnActivate flag [observed: §8.3 c]; GameMode ran for the disabled skull at its enabled rate [observed: `od_09a_disabled_ref_script`, 2026-10-18, owner-decision-captures.md] | GameMode runs for disabled references (agrees; since 2026-10-09, test `a_disabled_reference_runs_its_game_mode_block`); no cell-load OnLoad for them [hypothesis, not captured]; OnActivate runs |
| 6 | Scripted items in inventories | run while the holder is loaded (Scripting for Beginners) | captured only for the player's inventory [observed: §8.3 d] | only the player's items run GameMode. **Candidate** (§8 item 2) |

### 7.3 Documented rules the runtime does not implement or contradicts (no capture yet)

| # | Documented | Runtime |
|---|---|---|
| 1 | A reference function on an unset `ref`, a failed `set` or a failed call in an `if` stops the script silently (Ref Variable; Let; Eval) | since 2026-11-05 a call on an empty `ref` is skipped, gives 0, and the script goes on (`call_failed` traced, game.rs `Exec::call`; scripts.md §4.2). In the original with xNVSE 6.4.9 a function script run through CallFunction went on after such a call, alone and in an `if` [observed: o5_00, 2026-10-31]. An object script run by the game does too: NellisArtilleryScript's GameMode calls `CurrentActor2.GetDead` on an unset `ref` every frame one actor is in its trigger, and its barrage fires in the original (owner, issue #1600); ours stopped there and never fired [observed: showvars, Boom 0 and timer1 0 while ActiveTime counted]. Failed calls that are not on an empty ref did stop it: SetReputation axis 2 (o5_00), an `if` leaving two values (#908) |
| 2 | Return ends the script for this frame (Adding an Options Menu) | ends the current block only. 12 SCPT units have a Return in an earlier block of an event that has a later block of the same event [observed: `fnvtool script decode`]. Both blocks can run in one frame in five: LonesomeRoad 0x0400DEBA and 0x0400DEBB (two GameMode blocks), FalloutNV 0x00143E27 (two OnActivate blocks), 0x001347D1 (OnHit for one reference, then OnHit for any) and 0x00125AEF (OnDeath for any, then for one killer). In the other seven the blocks' parameters differ (MenuMode 1 and 1056 in 0x00104C01 and 0x00059C86; OnTriggerEnter for two different references in 0x00154219–0x0015421C and 0x00127467) |
| 3 | Result-script locals overwrite the quest script's variable of the same index (Category:Dialogue) | fresh temporary locals; 150 embedded units of the masters declare locals (§2.5) |
| 4 | Constant globals reset on load; plugin values apply only on first load (Globals) | not modelled (Layer 7) |
| 5 | A base form as calling reference fails (Base Form) | passed to the function unchanged |

Nothing in this note contradicts quests-dialogue.md; its one difference (the "Default"
delay flag) stands.

## 8. Open questions to capture

Candidates for the capture plugin (`nvf_capture`, layer5-captures.md). Job scripts are
compiled by the game, so plain `set`/`if` statements in a job behave as vanilla.

1. **Disabled references' GameMode** (settled for GameMode by `od_09a_disabled_ref_script`,
   §7.2 row 5; OnLoad on a cell load read from the rising plane, layer5-captures.md §8.3 d,
   with "cell load or inside Enable" not yet told apart): disable the skull in Doc Mitchell's house (FalloutNV
   0x00104C10, the round 4 stopwatch, layer5-captures.md §10.4), start its timer and
   sample `timer`. Settles §7.2 row 5 for GameMode; OnLoad of a disabled reference on a
   cell load is a second job (read `bLoaded` of VMS15's plane after the cell load, before
   any Enable).
2. **Scripted items outside the player's inventory**: add JarPistolPowder to a container
   and to an NPC in the loaded cell and watch the holder's PowderPistol count
   (`L5.quest.onadd_gamemode` with another holder; §7.2 row 6). Also OnAdd once per call
   or per item (scripts.md §10).
3. **Calls on an unset `ref`**: a job line `set m to 1`, then a reference function on an
   empty `ref`, then `set m to 2`; read `m`. Repeat with the failing call inside an `if`
   and in an `&&` whose left side is 0 (§7.3 row 1; that `&&` does not short-circuit is
   settled, §4).
4. **Return across blocks**: drive one of the five units of §7.3 row 2 whose two blocks
   can run in one frame (FalloutNV 0x00143E27 with `Activate player 1`, or 0x00125AEF
   with a kill) in a state where the first block returns, and read what the second block
   sets.
5. **Result-script locals**: whether a dialogue (or terminal) result script's local
   overwrites the owning quest script's variable at that index (§7.3 row 3). Needs
   dialogue or a terminal menu to be driven; Layer 6.
6. **Expression edge cases** in job scripts: `/` by 0, `%` with negative and fractional
   operands, `==` against `<` without parentheses, `!=` on references (scripts.md §4.2
   hypotheses).
7. **Short globals**: `set` a short GLOB to 2.7 and to 2000000033 and read it back
   (truncated or not; 32-bit float spacing) (§2.4).
8. **Constant globals and plugin defaults** across a save/load: Layer 7, needs a save the
   owner allows.
