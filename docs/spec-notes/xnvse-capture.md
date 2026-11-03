# Spec notes: in-game capture plugin (`nvf_capture`, xNVSE)

A capture tool for the parity lab, not part of the engine: NVForever has no xNVSE
support and needs none. The plugin runs inside the **original** game (FNV 1.4.0.525 with
xNVSE) to record the observations that Layer 4+ scenarios compare (item counts, actor
values, globals, disabled state, positions, leveled-list draws), replacing hand-typed
console sessions (spec §8.1 "behavioural capture plugin"; docs/spec-notes/simulation.md
§6). Code: `tools/capture/nvf_capture/` (Rust, 32-bit DLL) and
`tools/capture/capture_steps.py` (job files in, goldens out).

## 1. Source and licence

The interface below is re-expressed from xNVSE's public plugin header
`nvse/nvse/PluginAPI.h` and `nvse/nvse/nvse_version.h` (github.com/xNVSE/NVSE, master as
of 2026-10-07, release 6.4.9) [documented]. The repository has no licence file, so its
code is treated as all rights reserved: it is read as interface documentation only and
nothing is copied. The plugin uses no game addresses and no
game data structures: everything goes through xNVSE's interface and the game's own
script language.

## 2. Plugin interface used [documented: PluginAPI.h]

All calls use the C calling convention of 32-bit MSVC (cdecl); `bool` is one byte.

- The DLL exports two functions by undecorated name: `NVSEPlugin_Query(const
  NVSEInterface*, PluginInfo*) -> bool` (fill in the info; return false to decline) and
  `NVSEPlugin_Load(const NVSEInterface*) -> bool`.
- `PluginInfo`: `infoVersion` u32 (= 1), `name` (C string pointer), `version` u32.
- `NVSEInterface` starts with four u32 — `nvseVersion`, `runtimeVersion`,
  `editorVersion`, `isEditor` — followed by function pointers in this order:
  `RegisterCommand`, `SetOpcodeBase`, `QueryInterface(u32 id) -> void*`,
  `GetPluginHandle() -> u32`, then further members this plugin does not use.
- Versions are packed as `major << 24 | minor << 16 | build << 4 | sub` (each masked to
  8, 8, 12 and 4 bits). xNVSE 6.4.9 is `0x06040090`; game 1.4.0.525 is `0x040020D0`, its
  "no gore" edition `0x040020D1`.
- `QueryInterface` ids: 0 serialization, 1 console, 2 messaging, 3 command table, 4
  string var, 5 array var, 6 script, 7 data, 8 event manager, 9 logging, 10 player
  controls. A null result means "not available".
- **Console interface** (id 1): `version` u32, `RunScriptLine(const char*, ref*) -> bool`,
  `RunScriptLine2(const char*, ref*, bool suppressOutput) -> bool` — run one console
  line; the reference may be null.
- **Messaging interface** (id 2): `version` u32, `RegisterListener(handle, const char*
  sender, callback) -> bool`, `Dispatch(...)`. A callback receives a `Message*`:
  `sender` (C string), `type` u32, `dataLen` u32, `data` (pointer). With sender `"NVSE"`,
  message types are numbered from 0 in this order: PostLoad, ExitGame, ExitToMainMenu,
  LoadGame, SaveGame, ScriptPrecompile, PreLoadGame, ExitGame_Console, **PostLoadGame
  (8; data: true if the save loaded)**, PostPostLoad, RuntimeScriptError,
  DeleteGame, RenameGame, RenameNewGame, NewGame, DeleteGameName, RenameGameName,
  RenameNewGameName, DeferredInit, ClearScriptDataCache, **MainGameLoop (20; sent every
  game loop)**, and later ones not used here. For PostLoadGame the header says only
  "data: bool"; the flag is carried **in the `data` field itself**, not behind it:
  reading `data` as a pointer crashed the game on the first save load (access violation
  in `nvf_capture.dll`, 2026-10-07) [observed]. The plugin reads the low byte of the field.
- **Event manager interface** (id 8, plugin version 9): function pointers from its start
  [documented: xNVSE 6.4.9 PluginAPI.h]: `RegisterEvent`, `DispatchEvent`,
  `DispatchEventAlt`, **`SetNativeEventHandler(const char* name, handler) -> bool`**,
  **`RemoveNativeEventHandler(name, handler) -> bool`**, then members this plugin does not
  use. A native handler is `void (ref* thisObj, void* params)`; xNVSE passes a null
  reference and an array of the event's two arguments [documented: EventManager.cpp].
- **Script interface** (id 6, runtime only): `CallFunction(Script*, ref* calling, ref*
  container, Element* result, u8 numArgs, ...) -> bool`, `GetFunctionParams`,
  `ExtractArgsEx`, `ExtractFormatStringArgs`, `CallFunctionAlt`, **`CompileScript(const
  char*) -> Script*`** (a "Begin Function {} … End" user-defined function from source
  text; null on failure), `CompileExpression`, `DecompileToBuffer`. `CallFunction` runs
  the compiled function and stores its `SetFunctionValue` result.
- **Result element** (`NVSEArrayVarInterface::Element`): an 8-byte union (string
  pointer / array / form pointer / double) followed by a u8 type: 0 invalid (nothing
  returned), 1 number (double), 2 form, 3 string (C string owned by the game's heap),
  4 array; 16 bytes, 8-byte aligned. The caller initialises the type to 0.
- **Form and array results** (plugin version 10): the plugin passes a form or array
  result, unchanged, as the single argument of a compiled helper function (`ref rX` or
  `array_var aX` parameter), as it passes event arguments to event probes. A form becomes
  `{"form": "<GetFormIDString>"}`. An array is read by `ar_Keys` and then `aX[key]` per
  key: keys 0, 1, 2 … give a JSON list, others a JSON object; arrays in arrays are
  followed 3 deep, 256 elements at most [observed: job cap10, 2026-10-27, the player
  `00000014`, GetAvailableRadios as a list of 5 forms, nested and empty lists, a string-keyed
  map as `{"a": 1, "b": 2}`].

## 3. Script functions the jobs use [documented: GECK wiki function pages]

Base game: `AddItem` (third argument: add silently), `RemoveItem`, `RemoveAllItems`
(optional target container), `GetItemCount`, `Activate`, `Drop`, `EquipItem`,
`UnequipItem`, `GetActorValue`/`GetAV`, `SetActorValue`/`SetAV`, `GetDisabled`,
`Enable`, `Disable`, `Lock`, `Unlock`, `GetLocked`, `GetLockLevel`, `GetPos`, `GetAngle`,
`MenuMode <code>` (1008 container menu, 1014 lockpick menu [documented: GECK wiki
GetMenuTargetRef]) and the console-only `CloseAllMenus`. xNVSE/FOSE: `BuildRef(modIndex,
formID)` and `GetModIndex(name)` (forms by plugin name and plugin-local FormID, so jobs
do not depend on load order), `GetParentCell`, `GetFormIDString` (load-order FormID as
hex text; needs a ref variable), `GetNumItems`, `GetInventoryObject`,
`GetNumLoadedMods`, `GetNthModName`, `GetFirstRef` / `GetNextRef` (form type 201 =
inventory items; third argument: include taken references), `GetBaseObject`,
`GetSourceModIndex`, `GetRefCount`, `IsKeyPressed` (DirectX scan code: End = 207),
`MessageEx`, `SetFunctionValue`, `let`, `while … loop`, `$` (ToString).

## 4. Protocol (this project's design)

- **Capture directory**: `C:\fnv-private\capture\` unless the file `nvf_capture.ini` next
  to the DLL holds a line `dir=<path>`. The plugin writes only there (never into the
  game folder): `jobs\*.steps.json` (input), `jobs\done\` (finished jobs),
  `out\<job>.<unix-seconds>.jsonl` (results), `nvf_capture.log`.
- **Job** `fnv.capture.steps/1`: `name`, `title`, `start` (what to load first, shown in
  game), `steps` (array). A step is one of: `{"script": text}` (run a function script,
  ignore its result), `{"console": line}`, `{"query": text, "label": l, "field": f}`
  (run, record the result), `{"manual": text}` (show the text, wait for End), `{"frames":
  n}` (wait n game loops). From plugin version 3: `{"until": text, "max_frames": n}` (run
  the function script once per game loop until it returns a non-zero number, at most n
  times; records `{"kind": "waited", "step", "frames"}`, or an error when n is reached) and
  `{"remember": text, "as": name}` (run it and keep the number; records `{"kind":
  "remembered", "step", "name", "value"}`). In any later step text of the same job,
  `%{name}` is replaced by the remembered number before the step runs. Older plugins skip
  a job that uses these (logged as "skipping"; capture_run.py reports it).
- **Run**: after a save has loaded (PostLoadGame with success), the plugin announces the
  first pending job (alphabetical) with `MessageEx` and waits for End; it then runs one
  step per game loop; a failed step is recorded and the job continues (errors are
  behaviour too). Home skips the announced job for this session.
- **Output** (JSON lines): first `{"kind": "start", "job", "plugin_version",
  "nvse_version", "runtime_version"}`, then one `{"kind": "result", "step", "label",
  "field", "value" | "error"}` per query (`value`: number, string or null), then
  `{"kind": "end", "steps"}`; a job is moved to `jobs\done\` only after its end line.
- **Automatic runs** (plugin version 2): if the capture directory holds `auto.json`
  (`fnv.capture.auto/1`: `expires` Unix seconds, `menu_console` lines, `menu_frames`
  (default 300), `start_frames` (120), `quit_console` lines, `quit_frames` (60)) when the
  plugin loads, and it has not expired, the plugin renames it to `auto.used.json` (so
  the next launch is a normal session) and then: counts game loops before any save has
  loaded and runs the `menu_console` lines once after `menu_frames` of them; after a
  save load starts each pending job by itself `start_frames` loops after it is announced
  (End is not needed; **manual steps still wait for End**); when no job is left, runs the
  `quit_console` lines once after `quit_frames` loops. Every console line and its result
  is logged. A missing, expired or unreadable file means a normal session.
  `capture_run.py` writes the file, starts the game through `nvse_loader.exe`, waits
  for the end lines and removes the file again if the plugin never read it.
- **Observing** (plugin version 4): `{"observe": {…}}` samples actors repeatedly for a
  span of game time, with an optional per-loop `drive` script (walking the player); step
  format and output lines in docs/spec-notes/ai-observer.md §3. Older plugins skip jobs
  that use it.
  Version 5 adds the drive's result (`value`) to its `drive` line. Version 6 adds `hooks`
  (an action after a condition has held for N samples).
- **Screenshots** (plugin version 7): `{"screenshot": label}` saves a picture of the game
  window as `shots\<job>.<step>.<label>.<unix-seconds>.png` in the capture directory and
  records `{"kind": "screenshot", "step", "label", "file", "width", "height", "mean",
  "foreground"}` (`file` relative to the capture directory; `mean` the mean of all channels,
  0 black to 1 white; `foreground` whether the game window was in front). The pixels are
  copied with Windows' GDI from the screen area of the process's largest visible window
  (its client area), so they are what the desktop shows: this needs a windowed or
  borderless game (the owner's install: windowed, 1280 × 720 [observed: FalloutPrefs.ini
  `bFull Screen=0`]) and the window in front; a fullscreen-exclusive game may give black
  pictures [hypothesis]. No game memory and no Direct3D object is touched. The HUD is
  hidden by the job (console `tm` a few loops before, `tm` again after); whether `tm`
  works from `RunScriptLine` is checked by the first trial. Older plugins skip jobs with
  this step. `capture_frames.py` writes reference-frame jobs with it (README section 7).
- **Short scripts** (plugin version 8): a script text without `Begin Function`, a job's
  `forms`, scripts as lists of lines, and optional `label` / `field`, for jobs written by
  hand; see §8. An older plugin skips a job that has `forms`, a list of lines or a query
  without `label` / `field`; a job whose only new thing is a short text it runs as it is,
  and those steps fail to compile.
- Every job the tools generate starts with one query per master, label `__modindex__`, field = plugin
  name, value = `GetModIndex` (255 = not loaded), so load-order FormIDs in the output can
  be mapped back to plugin + local FormID (`capture_steps.py`).

## 5. What the runtime compiler accepts [observed, three diagnostic jobs, 2026-10-07]

Scripts given to `CompileScript` on the owner's install (xNVSE 6.4.9, game 1.4.0.525):

- **Compile and run**: `float`/`int` variables; `while … loop`; globals read, `set`/`let`
  on globals; `player.` calls with numeric results (`GetAV`, `GetPos`, `GetNumItems`,
  `GetItemCount <ref var>`); `GetModIndex "<plugin>"` (OldWorldBlues.esm = 3);
  `GetNumLoadedMods` (10); `ref` variables **declared before `Begin Function`**, set with
  `BuildRef <mod index> <decimal local FormID>`; `AddItem`, `RemoveItem`, `SetAV`
  statements with ref-variable arguments; returning strings directly:
  `SetFunctionValue (GetFormIDString rC)` ("00103DF9" for Doc Mitchell's house; slots
  beyond the inventory give "00000000"), `(GetNthModName 0)` ("FalloutNV.esm"), `($ 3)`
  ("3"), `("a" + "b")`; comparing refs (`rC == rD`). Console lines (`RunScriptLine`) with
  hex FormIDs: `player.additem 0000000F 3` added 3 caps.
- **Do not compile**: any Editor ID of a base form or a reference (`Caps001`,
  `GSDocMitchellHouseIntDoorREF`) — the runtime does not keep those names
  [hypothesis for the reason]; hex literals (`0x38F7`); `ref` and `string_var` variables
  declared inside the block; a `string_var` assignment to a variable declared there.
- **But** (2026-10-26, JIP 57.30 loaded): a `string_var` declared **before**
  `Begin Function` takes `let s := "Whatchamacallit"` and can be passed on
  (`sv_RegexMatch s "(What)(.*)"` = 1), and `(sv_Construct "…")` works as a string_var
  argument; a literal string where a string_var parameter is expected does not compile
  (`sv_RegexMatch "abc" "abc"`) [observed: jip_rx_trial, tools/capture/jip_probes.py].
- **Lambdas** compile at run time as arguments (`SetOnHealthDamageEventHandler (Begin
  Function {float fD, ref rS} … End) 1 rP`, `SetGameMainLoopCallback (Begin Function { }
  … End) 1 1 1`) and the handlers run later in the session. Their parameters must be typed
  in the braces; inside them `GetSelf` works and `this` does not compile
  [observed: jip_ev_trial2, jip_ev_trial3; jip-events set].
- Globals are set with console lines (`set <Global> to <n>`), as a player would.
- **Crash when the clock moves at a low TimeScale**: with TimeScale 1 the game crashed
  (access violation, FalloutNV.exe+0x477C6D, every time) when `set GameHour` moved the
  clock back (crash test 3, freshly loaded save) and at the end of a 6-hour Wait (a 24-hour
  Wait just before had worked); TimeScale 0 crashed the same way. Lowering TimeScale
  alone (30, 10, 5, 2, 1) never crashed, and at TimeScale 30 setting the clock back and
  waiting worked (first run). Jobs therefore leave TimeScale alone. Consequence: the game
  clock keeps running (×30) while the player is in menus, so a capture cannot reproduce
  fractional time exactly (the first run read GameHour 14.136 for 14 and GameDaysPassed
  6.2558 for 6.25); scenarios observe whole-number calendar values only (§6).
- Jobs pause after every action (10 game loops, 30 after equipping, activating,
  actor-value and bulk inventory changes); cheap insurance, not a proven need.
- **Call reported as failed**: a function that sets no return value, even when its
  statements ran (`player.RemoveAllItems`, `set GameHour to 8.0` alone); every action
  script therefore ends with `SetFunctionValue 1`.

`capture_steps.py` writes jobs to these rules and resolves Editor IDs to FormIDs from the
masters itself (`--data`, read-only). Still untested in game: `if <ref var>`, `$` of a
float variable (inventory slots); failures are recorded per step, not fatal.
A script variable named `fW` does not compile while `fSw`, `fH`, `fP` do (the swimming
jobs, 2026-10-08; why is not known); `IsSwimming` compiles but read 0 for the player
throughout, also while swimming; `GetAV Rads` does not compile, `GetAV RadiationRads`
does; scripts declaring the ref variables `rK` and `rS` did not compile at all, the same
scripts with `rF1` and `rF2` did, and `IsQuestItem <ref var>` compiles and runs (the
quest-item job, 2026-10-09; which of the two names fails is not known); `GetActiveMenuMode` does not compile. Arriving at some places (Boulder Beach by
`coc` or by `SetPos`) leaves `MenuMode` 1 with the game clock stopped until the console's
`CloseAllMenus` (physics.md §6.6). `Activate`, `EquipItem`, `Lock`, `Unlock`, `Drop` run as statements; `while rScan != 0`
loops over `GetFirstRef` / `GetNextRef`, `rScan.GetBaseObject` compared with a `BuildRef`
base, `rScan.GetSourceModIndex` and `MenuMode 1008` in an expression all compile and run
[observed 2026-10-08, the lock and drop captures]. `CloseAllMenus` as a console line
closes the container menu a scripted `Activate` opened (`MenuMode` 0 afterwards). A
function called on an empty ref variable is reported by xNVSE ("Attempting to call a
function on a NULL reference") and the step fails without harm.

## 6. Open points

- **Only the first full script of a job compiles (2026-11-01) [observed].** In
  o13_diag_forms (plugin version 10, nvftest) the first `query` with its own
  `Begin Function` (aa_auto_trial's GetParentCell) ran, and each of the seven after it
  came back "script did not compile", `let rV := rN.GetAV Health` after a BuildRef among
  them, though the same lines compile as the plugin's own short forms and in the
  observer's probes. o11_02 and o11_03 failed the same way. The cause in the plugin is
  not known [hypothesis: a name the runtime compiler gives each script, reused]. Until
  it is found, read forms with the observer (its `weapon`, `package` ... fields) or write
  one full script per job.
- **"cannot create a bitmap" (2026-10-18) [observed].** Every screenshot of od_21 and
  od_22 (from `nvfstrip`) failed with this error, while od_20's Ringo pictures were
  taken in the same run.
  - **Where it comes from.** In `src/shot.rs` the text is returned only when
    `CreateCompatibleDC` or `CreateDIBSection` gives no bitmap. That is before
    `BitBlt`, and it does not depend on the window being in front: a window that is not
    capturable gives "no visible game window" (minimised) or "BitBlt failed" instead.
  - **So the error means GDI could not make a 32-bit DIB of the client area's size.**
    That is about 3.7 MB at 1280 × 720.
  - **Candidates [hypothesis].** The game is a 32-bit process. After a long session, with
    the Strip loaded, its address space may have no room left for the DIB's mapping. Or
    the window picked as "largest visible" had an unexpected size.
  - **Next step.** A small diagnostic would settle it: add the width, height and
    `GetLastError` to the message. The plugin is not changed here, because the DLL needs
    the owner to copy it.
  - **Settled (2026-10-19, plugin version 9) [observed].** On the owner's desktop the
    game's window is 3,840 × 2,160: its 32-bit DIB is 33 MB and the RGB copy 25 MB. The
    first shots of a run came back black or failed with this error, and then the RGB
    buffer's allocation failed and aborted the game ("memory allocation of 24883200 bytes
    failed", three crashes of `knockdown_look`). Pictures are now copied scaled down to
    1,280 wide (`StretchBlt`, HALFTONE) and the buffer is reserved fallibly; every shot of
    the later runs was taken (ai-observer.md §12).

- First automatic trial (2026-10-08) [observed]: xNVSE sends MainGameLoop while the main
  menu is shown (the log's "first game loop" came 2 s after the plugin loaded, before any
  save load). The console line `load nvftest`, run from the plugin's MainGameLoop handler,
  started loading the save and did not return; the plugin still held its own lock, the
  game's next message to the plugin waited for that lock, and the game hung on the
  loading screen ("Not Responding") [hypothesis for the mechanism: the save loads inside
  the console call and xNVSE dispatches messages from there]. Plugin fix: console lines
  of automatic runs run with the lock released, and a message that arrives while the
  plugin is busy never waits (a save load is handled at the next message, a game loop is
  skipped). capture_run.py now gives up when the game stays "Not Responding" for 120 s.
- Second automatic trial (2026-10-08, fixed plugin) [observed]: `load nvftest` at the
  main menu loaded `nvftest.fos` (made with the console line `save nvftest`); the log's
  "save loaded" came *before* the console call returned "ok", so the save does load inside
  the call and xNVSE dispatches PostLoadGame from there (the mechanism above). The job
  `aa_auto_trial` then started by itself and ended with no failed step; `qqq` closed the
  game. Unattended runs work for jobs without manual steps.
- Fractional game time (GameHour, GameDaysPassed after waiting) needs a capture that can
  stop or account for the running clock without crashing the game; not observed yet.

- `advance_time`, `detach_cell` / `attach_cell` and `transfer` of single items have no
  console equivalent and become manual steps (Wait menu, walking through a door, the
  container menu).

## 7. Scripted substitutes for manual steps

Goal: jobs without manual steps can run unattended (§4, capture_run.py). A substitute is
used for a scenario only after a capture made with it agrees with the capture made by
hand (`capture_steps.py compare`, numbers within a stated tolerance); until then the
manual step stays. Results of the first checks (2026-10-08, all runs unattended, plugin
version 3) are at the end of this section.

- **Settling after a door** (`capture_steps.py actions --settle auto`, replaces the End
  confirmation of `--confirm-after`): scripted `Activate` on a load door already moved
  the player through it in `goodsprings_doc_mitchell` [observed 2026-10-07]; only the
  wait for the loading screen was manual. The substitute waits (`until`) for the player's
  parent cell to differ from the door's, then 300 game loops. Check: rerun
  `goodsprings_doc_mitchell` this way and compare with its golden, which includes the
  player's position after the door (z within the character controller's settling).
- **Waiting** (`--time`, replaces the Wait menu for `advance_time`). Two candidates, both
  [hypothesis] until compared: `hour` sets `GameHour` to its value plus the hours (at most
  24 at a time, possibly past 24) and leaves the rest of the calendar to the game; and
  `timescale` raises `TimeScale` to 3600 (one game hour per real second), waits (`until`)
  for `GameDaysPassed` to reach its remembered start plus hours/24, and restores the
  remembered TimeScale, so the game runs the time itself. Raising TimeScale was not part
  of the crash tests (§5: only lowering it, and moving the clock at a low TimeScale). The
  trial job `tools/capture/jobs/aa_time_substitutes.steps.json` records every calendar
  global after each candidate (and after setting `GameDaysPassed` alone); the scenario
  checks rerun `goodsprings_merchant_restock` (restock behaviour) and
  `goodsprings_weight_and_time` (calendar values) with each candidate.
- **Leaving and returning to a cell** (`detach_cell`, `attach_cell`) and **container-menu
  transfers** (`transfer`) stay manual: the actions name a cell, not the door to use, and
  `RemoveItem` + `AddItem` is not known to be the same as a menu transfer.

Results [observed 2026-10-08, saves `nvftest` (inside GSGasStation) and `nvfdoc` (Doc
Mitchell's house)]:

- `aa_time_substitutes` (calendar after each candidate; the clock also runs a few game
  seconds between steps): setting `GameHour` 24 hours forward (to 33.3) gave GameHour
  9.32 again, GameDaysPassed +1.000 and GameDay +1, so the game wraps the hour and carries
  the day itself; +6 hours moved GameHour +6.005 and GameDaysPassed +0.250. TimeScale 3600
  until GameDaysPassed + 1: 1,433 game loops, overshoot about 2 game minutes (GameDaysPassed
  +1.0013), GameDay +1, TimeScale back to 30, no crash; +6 hours: 360 loops. Setting
  `GameDaysPassed` alone (+1) left GameDay unchanged, so it is not a substitute.
- `goodsprings_merchant_restock` rerun with `--time hour` and with `--time timescale`:
  identical to the golden made with the Wait menu (0, 200, 0, 200 caps; 0 differences).
- `goodsprings_weight_and_time` rerun with each: identical to its golden (weights,
  capacities, GameDay/GameMonth across the month end; 0 differences).
- `goodsprings_doc_mitchell` rerun with `--settle auto`: the player had left the house's
  cell at the first check after `Activate` on the front door; after the 300-loop pause
  every observation matched the golden except the position after the door, which
  differed by 0.0002 (y) and 0.005 (z) game units, well inside the scenario's 1-unit
  tolerance (0 differences at `--tolerance 0.01`).

So for these scenarios `--time hour`, `--time timescale` and `--settle auto` give the same
observations as the manual steps, and the scenarios can be captured unattended. This is
equivalence for what they observe (item counts, merchant restock at 72 and 84 hours,
calendar values, weights, cell and position after a door), not a proof that the
substitutes do everything the Wait menu does (e.g. health regeneration, NPC schedules and
cell respawn were not compared); a new kind of observation needs its own comparison.
`hour` is the default choice for unattended jobs (no TimeScale change, no dependence on
frame rate); `timescale` lets the game run the time, if a scenario needs that.

Further agreement [observed 2026-10-08]: the backlog captures `bl_restock_timing`,
`bl_primm_restock` and `bl_container_respawn` were each run with `hour` and with
`timescale`; every refill, respawn and non-refill came out the same (only leveled
contents rolled differently). Merchant restocks and cell respawns happen at day
boundaries or on cell load, both of which `hour` passes through, so this agreement is
expected [hypothesis]. Travel with the console's `coc` (`--go-to-cell`, also used to leave
and re-enter a cell in `bl_container_respawn`) arrived on the first check every time; it
is a teleport, not a walk through a door.

## 8. Short scripts (plugin version 8; this project's design)

Goal: a job a person can write by hand. The plugin is being published for others
(2026-10-10), and a hand-written query in the form the tools generate is mostly
boilerplate. The short forms are a rewriting done by the plugin when it reads a job
(`src/short.rs`); the game is given the same kind of function script as before, so nothing
here is a new claim about the game.

- A script text that does not contain `Begin Function` (any case) is a short form; one
  that does is used unchanged. Scripts in `query`, `script`, `until` and `remember` may
  also be given as a list of lines (joined with line feeds, one added at the end).
- What is written around a short form, by where it stands:
  - a number (`query`, `remember`; in `observe`: `header`, `count`, `probe`, `drive`):
    `float fV` / `Begin Function { }` / `let fV := <text>` / `SetFunctionValue fV` / `End`;
  - a string (`query` with `"text": true`): `Begin Function { }` /
    `SetFunctionValue (<text>)` / `End`;
  - a condition (`until`; in `observe`: `until`, a hook's `when`): `float fV` /
    `Begin Function { }` / `if <text>` / `let fV := 1` / `endif` / `SetFunctionValue fV` /
    `End`;
  - statements (`script`; in `observe`: `drive_end`, a hook's `then`): `Begin Function { }`
    / `<text>` / `SetFunctionValue 1` / `End`.
- A job's `forms` maps names (`[A-Za-z0-9_]`) to `"<plugin file>:<FormID in hex>"`; only
  the low three bytes of the FormID are used. In a short form `@name` becomes a ref
  variable: the n-th form used in that script is `rF<n>`, declared before
  `Begin Function` together with `float fM<n>`, and set first with
  `let fM<n> := GetModIndex "<plugin>"` and `let rF<n> := BuildRef fM<n> <decimal>`. An
  `@name` that is not in `forms` is an error, and the job is skipped as a whole (logged
  with the step's number).
- A short `query` without `field` gets its own text as the field; `label` defaults to "".
- Five of these texts are, character for character, scripts that already ran in the game
  [observed]: the number form in `aa_crash_test` (`player.GetAV Strength`), the string
  form in `aa_diag3` (`GetNthModName 0`), the condition form in `aa_time_substitutes`,
  the statement form with one form in `bl_encumbrance` (`player.RemoveItem rF1 2`) and the
  number form with one form in `ld_lock_levels` (`rF1.GetLocked`); the plugin's tests
  compare its output with those texts. Other combinations (a string or a condition with
  forms, two or more forms: `fM2`, `rF2`, ...) are built from the same parts
  [hypothesis that each compiles; a failure is recorded per step, as for any script].
- The version 8 plugin has not run in the game yet: the rewriting is tested outside the
  game only (`cargo test`). Its first trial is the owner's, because the DLL has to be
  copied by hand; all committed jobs are full scripts (50,289 script texts checked) and
  reach the game unchanged.
