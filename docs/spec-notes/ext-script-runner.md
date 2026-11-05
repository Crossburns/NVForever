# JIP LN's Script Runner: scripts without a plugin (2026-11-03, #1460)

Built in `crates/fnv-game/src/script_runner.rs`, run by the engine in
`engine/nvf-play/src/script.rs` and checked headless with `fnvtool ext script-runner`.
Sources: the GECK wiki export [documented: the page named]. The wiki's introduction page
("JIP LN NVSE Script Runner Introduction") is not in the export, so what it says about the
prefixes is not known here; the rest comes from the files mods ship [observed: the 203
`.txt` files under `NVSE\plugins\scripts` in the owner's MO2 mod lists, 2026-11-03, read
for their names, form and comments only]. None of it has been checked in the original
game yet (§5).

## 1. The files

- A mod puts plain-text scripts under `Data\NVSE\plugins\scripts\`; they run without a
  plugin [documented: Category:Functions (JIP), "allows scripts to be executed without an
  esp"; RegisterSRScript, "relative to ..\Data\NVSE\plugins\scripts\"].
- A file is a bare script body: statements, extender lines and declarations (`int i`,
  `ref r = …`, `let array_var a := …`) with no `scn` line and no `Begin` block [observed:
  none of the 203 has either]. NVForever compiles it as a result script, with the
  extender's grammar, once per game.
- Only the folder's own files run unless registered: of the 203, 202 lie directly in the
  folder and one in a subfolder [observed], and a subfolder's file needs RegisterSRScript
  (§3) [documented: "must include at least one subfolder"].
- The order of the files at one moment is not documented: NVForever runs them in the
  order of their names, any case [hypothesis].
- Text is UTF-8 when it is, else 8-bit text in the run's code page, as CompileScript reads
  its files.
- The folder is read in NVForever's own layer first, then in Data (an MO2 run sees the
  mods' files there); archives are not looked in [decision: no shipped file is in one].

## 2. The prefixes

The first three letters of the file name say when it runs [documented: RegisterSRScript,
"the prefix will determine when the script is executed"].

| Prefix | Files | When, in NVForever | Evidence |
|---|---|---|---|
| `gr_` | 126 | game restart: once a run, before any save is in place | [documented: RegisterSRScript, "Game Restart ('gr_' prefix)"; InitExtraMiscStat, a plugin uses a MenuMode 4 block (the main menu) where an SR file uses gr_] |
| `ln_` | 64 | a save loaded, and a new game | [hypothesis: the name; the files guard work they must do once a session with an aux variable flag, as code that runs at every load would] |
| `gl_` | 7 | a save loaded | [hypothesis: the name; RegisterSRScript's example registers `gl_Option2.txt`] |
| `gx_` | 4 | not run (§4) | [hypothesis: game exit; MCM Extender's file says "Saved cached INIs on game exit"] |
| `gn_` | 1 | a new game | [hypothesis: the name] |

A file with another prefix (or none) never runs [hypothesis].

**Moments in NVForever.** A load while playing starts a new run of the engine, so the
game restart comes again after every load, as GetGameRestarted does
(docs/design/save-load.md). With a save: the save is restored, then the `gr_` files run,
then the LoadGame and PostLoadGame events, then the `gl_` and `ln_` files. Without one:
the `gr_` files, then the `gn_` and `ln_` files. Both come after the menus' tile tree is
built, so a file can set UI traits [decision]. Without a save the `gn_` and `ln_` files
wait for the first frame the scripts run, when the new game begins: the world behind the
main menu is a new game held, and Continue or Load there starts another run, so running
them behind the menu ran them for a game that never began (#1548) [decision; in the
original no game exists behind its main menu, hypothesis]. The restart's files run after the restore
because a restore takes the game's state, lambdas included, from the save: run before it,
what a gr_ file made (handlers with lambdas) would be lost [decision; in the original the
main menu comes before the load, hypothesis].

## 3. RegisterSRScript `filePath` (JIP 57.15)

| | NVForever |
|---|---|
| When | only while the game restart's files run; elsewhere it does nothing and returns 0 [documented: "may only be called from a Game Restart ('gr_' prefix) SR script, and will have no effect otherwise"] |
| Path | relative to `scripts\`, with at least one subfolder [documented]; `..` and empty parts refused [decision] |
| 0 when | the file cannot be read, does not compile, or a file of that name was already registered [documented]; or its name has no known prefix [hypothesis] |
| Then | the file runs at its prefix's moments for the rest of the session; a registered `gr_` file runs after the folder's `gr_` files [hypothesis] |

## 4. What is not built

- **`gx_` files** (game exit): the engine has no moment where scripts still run as the
  game quits or returns to the main menu. Four files use it (MCM Extender saves its cached
  INI files there).
- **Mod index.** A Script Runner file has no plugin, so functions that act on the calling
  script's plugin find none [documented: PWriteFileAlt, "script runners have no relative
  mod index"]; NVForever names the unit `ScriptRunner\<file>`, which no plugin matches.
- **Arrays** a file makes are not freed when it ends [documented: Ar Null, "Should be
  called on all arrays that were created from Script Runner"], as a result script's are
  not; a lambda keeps the file's variables as they were when it was made (ext-udf.md).

## 5. To check in the original game

The prefixes other than `gr_`, the order of the files, whether `gr_` runs once per
session or again after a load from the pause menu, and where `gl_`/`ln_` run relative to
the LoadGame events (docs/handoff/observer-checks.md).

## 6. The files that do not compile yet

`fnvtool ext script-runner` over the owner's MO2 mods (109 distinct files) against the
ten masters: 101 compile, and every one of them runs without an error at its moment (73
gr_ at the restart, 23 ln_ and gn_ in the new game) [observed: 2026-11-03, #1470; 96
before it]. Of the 8 left, 6 name forms or functions of plugins not in that load order
(`Player.RefreshAnimData` twice, `Player.CalculateShotsPerSec`, `ced89` twice,
`AZGOMChickHadleyFaction`; expected), and 2 are written in ways the pages do not cover,
left as they are:

- `Player.IsAnimPlayingEx 5 0 0 1` inside an `if eval` (ln_B42Optics.txt): four arguments
  where the page lists three [documented: IsAnimPlayingEx; hypothesis: a newer JIP took a
  fourth after the export];
- `TempCloneForm (NVDLC04WeapSMMG WeapMinigun)` (ln_trueminigun.txt): two operands in
  parentheses with no operator between them; what the original does with it is open.

Five forms the files use, compiled since #1470 (each also in mods' ESP scripts compiled
from source; tests in `crates/fnv-script/tests/compile.rs`):

- **A type and a name alone in parentheses** declare the variable where a function writes
  a result into it: `Player.GetNifRotAlt "RadNeedle" (float f1) (float f2) (float f3) 2 2`,
  `Player.GetAngleEx (float x) (float y) (float z)` [hypothesis: the files are written so;
  the Inline variable declaration page shows only `=`, `:=` and `<-`].
- **A declaration inside an argument's inline expression**: `SetUIFloatAlt "…" (int i =
  GetPCDetectionState)` [documented: Inline variable declaration, "NVSE expressions can now
  declare variables"; Inline Expression: an inline expression is an NVSE expression].
- **The `…Ex` path and name setters take format values** after their string, up to 20, and
  their later parameters after those: `SetModelPathEx "a.nif%z…%z" s … s rVodka`
  [documented: SetModelPathEx, Bugs; "formatable string" on SetIconPathEx,
  SetBipedModelPathEx, SetBipedIconPathEx, SetNthFactionRankNameEx; String Formatting, 0
  to 20 values]. The catalogue lists no format parameter for them.
- **Arguments past those a call line's function takes are dropped**, with a warning
  (`surplus_arguments`), as the vanilla compiler drops them (script-compiler.md §3 rule 1):
  `SetJohnnyOnRenderUpdateEventHandler 1 (begin function {} … end) 0 0` hands a
  three-parameter function four [hypothesis: the extender's call lines do the same; the
  page lists three]. This was the `syntax: unexpected 0` the files reported, not
  DispatchEventAlt. Only a run of arguments is dropped; anything else is still an error,
  as is a dropped argument that cannot be read (`Nobody.GetAV Luck`). Reading the dropped
  arguments leaves nothing behind (no reference list entries, lambda numbers, variables
  or warnings). The cost: where the catalogue lists too few parameters, a real argument is
  lost with only a warning. Observer check 81 (observer-checks.md) settles whether the
  original drops them.
- **`( begin function`**, a space after the parenthesis, opens a block lambda as
  `(begin function` does (`CallWhen ( begin function {} … end) ({}=> …) (0b10100)`)
  [hypothesis: spaces between tokens do not matter; documented: Lambda shows
  `(begin function`].

Building the files found two compiler gaps that affect every script compiled from source,
now fixed: a type word after `let` (`let int i := 10`, `let array_var a := …`) and in an
extender condition (`if eval (int j = …)`, `while`, `foreach array_var e <- a`) declares
the variable [documented: Inline variable declaration; observed: ext-syntax.md §9, the GECK
compiled inline `int` declarations with `let`]; a block lambda's body declares its own
variables before it compiles; and a parameter typed `arg(up to 15)` (DispatchEventAlt, the
Call… family) takes that many arguments [documented: the pages' parameter types].
