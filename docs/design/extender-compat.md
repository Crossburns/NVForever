# Design: script extender compatibility (xNVSE, JIP LN, JohnnyGuitar) as native engine code

**Status: approved by the owner 2026-10-25, core first.** Written 2026-10-20. The work goes in
phases on their own branches; the core (compiler, values, registry, events, saves) is broken
into tasks in `docs/handoff/extender-core-tasks.md`. §8's decisions other than go/no-go are
still open.

## 1. Goal

Mods that say "requires xNVSE / JIP LN NVSE Plugin / JohnnyGuitar NVSE" should run on
NVForever unchanged. NVForever does this by **implementing those extenders' script
functions, syntax and events as part of the engine**, in Rust. It does **not** load their DLLs.

From a mod's point of view nothing changes: its scripts call `ar_Map`, `GetPCTrait` or
`SetOnHitEventHandler` and get the documented result. From the engine's point of view these
are just more script functions in the existing function table, next to the vanilla ones.

What this gets us, compared with the original game plus DLLs:

- One engine. No DLL load order, no clashing address hooks, no crashes inside a plugin.
- The same evidence standard as vanilla functions (spec notes, captures, coverage tables).
- Clean room. Nothing is copied from the extenders, and nothing depends on the original
  exe's memory layout.

## 2. Non-goals

- **Loading NVSE plugin DLLs**, whether in process or through a 32-bit host. Plugins are 32-bit
  x86 and our engine is 64-bit. More to the point, the useful plugins patch `FalloutNV.exe` at
  fixed addresses and read game objects at fixed field offsets. Supporting that would mean
  rebuilding the original's memory layout, which is only known from disassembly. That rules
  it out under the charter.
- **Recreating the extenders' engine patches** (bug fixes, INI tweaks, performance hacks). These
  fix the original exe, not ours. Where a patch changes behaviour players rely on, it becomes
  a separate, named NVForever option, decided case by case (§9).
- **Reading or importing `.nvse` co-saves** of the original game. NVForever's own saves carry
  the extender state (§5.4). An importer could come later, alongside any `.fos` import.
- **Other extenders**, such as ShowOff (173 function pages), kNVSE (28) and the long tail. The
  design leaves room for them (one module per origin) but they are not in this plan's scope. (Later work built several of them anyway: ShowOff,
  kNVSE, ITR, One111, ppNVSE and others, release v0.6.0; see docs/status.md.)

## 3. Scope, measured

Function pages in the GECK wiki extract (`C:\fnv-private\geckwiki\functions.xml`, 3,486
pages), counted by each page's `origin` field [observed, 2026-10-20]:

| Origin field | Pages | Belongs to |
|---|---|---|
| GECK1 | 565 | vanilla (already the engine's job) |
| NVSE | 395 | xNVSE |
| FOSE1 | 211 | xNVSE (functions inherited from FOSE) [hypothesis: the wiki uses FOSE1 for these] |
| JIP | 768 | JIP LN NVSE |
| LU | 282 | lutana's plugin, merged into JIP LN [hypothesis, to confirm from the pages] |
| JohnnyGuitar | 359 | JohnnyGuitar NVSE |
| **In scope** | **≈2,015** | about 3.5× the vanilla library |

The page count is an upper bound on *functions*. Some pages are aliases or deprecated. Phase 0
(§7) turns it into an exact list.

The function count matters less than **which engine systems a function reaches into**. That
splits them into five families (§5).

## 4. Charter rules for this work

- **Sources**: the GECK wiki pages (documentation), the extenders' public readmes and change
  logs (documentation), and **captures in the original game** with the extenders loaded.
  `nvf_capture` already runs inside FNV 1.4.0.525 with xNVSE (docs/spec-notes/xnvse-capture.md),
  so it can call any of these functions and record what it returns.
- **Not read**: the source code of xNVSE (no licence file, treated as all rights reserved), JIP LN, JohnnyGuitar or any other plugin. The interface header used
  for `nvf_capture` stays the only extender file read, and only for the capture tool.
- **Process**: spec note per function family (`docs/spec-notes/ext-*.md`), tagged
  `[documented]` (wiki page), `[observed]` (capture) or `[hypothesis]`. Then implement from
  the note. Behaviour comes from what the game does; nothing mirrors how a plugin was built.
- **Goldens** from extender captures stay in `C:\fnv-private`, like every other golden.

### 4.1 Where this plan could break the clean room, and the rule for each

Nothing in the plan needs a forbidden source. These are the points where work could drift
into one, with the rule that stops it:

1. **Looking up an extender's source when the wiki is unclear.** xNVSE, JIP LN and
   JohnnyGuitar sources are public on GitHub, so this is the most likely slip.
   *Rule*: an unclear behaviour becomes a capture probe, never a source lookup. A session
   that has read extender source does no implementation work in this scope; it says so,
   and its branch is dropped.
2. **Wiki pages that carry implementation details.** Some pages are written by plugin
   authors and mention internal structures, offsets, flag bits or code snippets.
   *Rule*: take the documented *behaviour* (arguments, results, side effects); never take
   offsets or memory layouts, even when a page states them. Wiki text is re-expressed,
   not copied (copyright as well as clean room).
3. **The opcode table and bytecode encoding** (route 1 of §5.2). The obvious place to find
   the opcode numbers is the extenders' own command tables in their source, which is
   ruled out. *Rule*: opcodes and argument encodings come only from documentation or from
   observing compiled output: write test scripts, compile them in the GECK with the
   extender loaded, and read the `SCDA` bytes (as was done for the vanilla bytecode).
   Never by disassembling an extender DLL or the game exe.
4. **Captures that peek at memory.** `nvf_capture` uses no game addresses or structures
   (xnvse-capture.md §1). Some extender functions read raw form data or offsets.
   *Rule*: probes may call only functions whose documented behaviour is game-level
   (values, flags by name, counts). Functions that expose memory are never used as probes,
   and their NVForever versions are "refused" (§5.8), not reverse-engineered.
5. **Engine-internal functions** (§5.8) and **extender patches** (§9). To map a function
   onto our engine or to decide whether a JIP or JohnnyGuitar fix becomes an option, we
   need to know what it does. *Rule*: that knowledge comes from the function's page, the
   plugin's change log or readme, and observing the game with and without the plugin.
   Never from its code or by diffing patched game memory.
6. **What AI sessions already know.** Models may have seen extender source in training,
   so code written from memory could reproduce it. This is a risk for the whole project,
   but higher here because the source is popular and public. *Rule*: every implementation
   cites its spec-note entry; code is written from that entry, not from "how xNVSE does
   it"; reviews reject code whose structure or naming follows an extender's internals
   rather than our function table's.

Not clean-room issues, but rules all the same:

- Real mods' scripts are read locally for the Phase 0 scan. Only counts and function names
  go into the repo, never mod script text.
- Captures with JIP LN and JohnnyGuitar need those DLLs in the owner's game install. The
  owner installs them. Sessions don't write into the game folder (install-layout.md).
- Reporting "JIP NVSE Plugin, version X" to mods (§5.7) is for compatibility only. The
  docs and the settings page call these modules "JIP LN-compatible functions"; they never
  present themselves as the plugin.

## 5. Architecture

### 5.1 Where it lives

| Piece | Crate | Notes |
|---|---|---|
| Compiler, extended syntax, new value types in the VM | `fnv-script` | §5.2, §5.3 |
| xNVSE functions | `fnv-ext-nvse` (new) | registers into fnv-game's function table |
| JIP LN functions | `fnv-ext-jip` (new) | depends on fnv-ext-nvse's runtime types |
| JohnnyGuitar functions | `fnv-ext-jg` (new) | same |
| Event dispatch, co-save section | `fnv-game` | §5.4, §5.5 |
| Engine hooks for events, UI tiles | `engine/nvf-play` | §5.5, §5.6 |

One crate per origin keeps the coverage tables per origin, lets a load order turn an
extender on or off (§5.7), and keeps the vanilla function code readable.

**Function identity.** The vanilla table registers by opcode
(`crates/fnv-game/src/functions/mod.rs`). Extender functions also have opcodes (each plugin
has an opcode base), but compiled mod scripts only use those numbers if they were compiled
against the extender. The registry gets a second key, **by name** (with aliases), and the
opcode keys come from the opcode table if one is available (§5.2). The same `Kind::Impl` /
`Kind::Stub` / `Kind::NoOp` statuses apply, so `script-stubs` and the coverage tooling work
unchanged.

### 5.2 Getting the scripts: compile from source

Mods ship both `SCDA` (compiled bytes) and, usually, `SCTX` (source text)
(docs/spec-notes/scripts.md §1). Bytes compiled against an extender contain its opcodes and
its own argument encodings (string and array expressions, inline expressions).

Two routes:

1. **Decode extender bytecode.** Needs each extender's opcode numbers and argument encoding.
   Whether a public, documented opcode table exists is unknown [hypothesis]. If it doesn't,
   the encodings have to be worked out by compiling test scripts in the GECK with the
   extender loaded and reading the bytes it writes (§4.1 item 3). That is slow and
   fragile, and it never means disassembling a DLL.
2. **Compile `SCTX` ourselves.** Needs the script compiler that owner-review.md item 3
   deferred. The compiler also serves the console, test aids and source-only mods.

**Decision: route 2 first**, with route 1 as the fallback for units without source. *Owner, 2026-10-26:* route 1 is not built; the only source-less extender units in the owner's lists (3, AMFAmata.esp) do not matter. Phase 0
measures how many extender-using units in real mods lack `SCTX`. That number decides
whether route 1 is ever needed.

The compiler accepts the vanilla grammar plus xNVSE's (docs/spec-notes/script-language.md
already lists most of it, lines 255–321): `Let` and `:=`, compound assignment, `eval` and
`If eval`, inline expressions as arguments, short-circuit `&&`/`||`, `ForEach`, `While`,
`Break`/`Continue`, `Begin Function {…}`, `Call`, `SetFunctionValue`, `Return` with a value,
lambdas (`Begin Function` inline), string and array operators (`[]`, `->`, `+` on strings,
`$` conversion). NVSEScript (xNVSE 6.3.6's C-like syntax) comes last, as its own parser that
produces the same IR.

Correctness check: the compiler's output for **every vanilla unit** must decode to the same
IR as the shipped `SCDA` (14,514 units, docs/spec-notes/scripts.md). That is a lab scenario
the compiler can't fake, and it proves the vanilla half before any extender code depends on it.

### 5.3 Runtime value types

xNVSE adds two value types beyond numbers and refs:

- **String variables**: a local holds an id into a global string table. Strings live until
  destroyed (`sv_Destruct`) or until no variable holds them (xNVSE 6 reference-counts them)
  [documented: wiki "String Variables"; the exact lifetime rules need captures].
- **Array variables**: also ids into a global table, three kinds (array, map with number
  keys, stringmap), elements of any type including nested arrays, reference-counted.

Both go into `fnv-script`'s VM as first-class values (`Value::Str(id)`, `Value::Array(id)`).
They are not encoded into floats. Ids are stable within a save so that scripts comparing
them keep working.

### 5.4 Persistence

The string and array tables, event handler registrations (JIP's and JohnnyGuitar's handlers
persist with the save; xNVSE's `SetEventHandler` ones do not [hypothesis, needs captures]),
and every extender "set" function that changes a base form (`SetWeaponDamage` style
changes, which JIP saves) go in a new **extender section of NVForever's save**
(`crates/fnv-save`). It is versioned, and a save without it loads with empty tables.

Which base-form changes persist and which reset on load is per function, and it is exactly
the kind of quirk mods depend on. Each function's spec entry records it, from captures.

### 5.5 Events

Three event systems, all landing on one dispatcher in `fnv-game`:

- xNVSE `SetEventHandler` / `DispatchEvent` with named events and filters;
- JIP's `SetOn…EventHandler` family (OnHit, OnCrosshair, OnPCTargetChange, OnFire, menu
  events, …);
- JohnnyGuitar's `SetJohnnyOn…EventHandler` family.

Each event needs a **firing point** in the engine: the place in our combat, activation,
menu or AI code where the original fires it, with the same arguments, the same order
relative to vanilla blocktypes (OnHit vs the script's `Begin OnHit`), and the same frame.
The ordering is captured, not guessed: `nvf_capture` already receives events through xNVSE's
event manager interface (plugin version 9, xnvse-capture.md §2).

### 5.6 UI functions (owner chose A, 2026-10-26)

**Decision:** (A), a tile tree, so modders can change the UI; design in
`docs/design/ui-tile-tree.md` (its own session).

JIP and xNVSE have a large UI family: `GetUIFloat`, `SetUIFloat`, `SetUIString`,
`GetActiveMenuMode`, tile injection (`InjectUIXML`, `InjectUIComponent`), menu-item lookups.
These assume the original's **menu engine**: menus built from XML files into a tree of
*tiles*, each with *traits*, addressed by path (`HUDMainMenu/_JIPHUD/…`).

As far as I can tell from the menu spec notes, NVForever's menus are laid out from
measurements of the original, not built by running its menu XML [hypothesis, to check
against `engine/nvf-play`]. That gives two options:

- **(A) A tile/trait model.** Build a runtime that loads the game's menu XML (and mods'
  injected XML) into a tile tree with traits, and draw the menus from it. UI mods (MCM,
  HUD mods, Vanilla UI Plus, JIP's own HUD elements) then work. This is a big piece of work
  and it would replace how menus are built now.
- **(B) Named shims.** Keep our menus and expose a fixed map of well-known tile paths
  (health, AP, compass, the common MCM paths) onto our widgets. Cheap, covers the functions
  gameplay mods use to read the HUD, and leaves real UI mods out.

**Recommendation: (B) in this plan, (A) as its own design doc** once Phase 0 shows how many
of the mods you want depend on UI injection. MCM is the one to look at: many mods' settings
only exist through it.

### 5.7 What the engine reports to mods

Mods check for extenders before using them: `GetNVSEVersion`, `GetNVSERevision`,
`IsPluginInstalled "JIP NVSE Plugin"`, `GetPluginVersion`, `GetJIPLNVersion`, and print
"requires version X" message boxes otherwise.

- The engine reports, for each extender module, **the version whose behaviour it
  implements**, chosen as the version the coverage tables were captured against.
- `NVForever.ini` `[Extenders]` can turn a module off (it then reports "not installed") for
  testing a mod's fallback path.
- A mod asking for a newer version than we report gets a log line naming the mod and the
  version, so the gap is visible.

### 5.8 Engine-internal functions

Some functions only make sense inside Gamebryo: NiNode and NiAV block access
(`GetNifBlockTranslation`, `SetNifBlockFlag`), raw form or memory offsets, havok and
render-state switches, the game's own caches. Each one is classified:

- **mapped**: has a meaning in our engine (NIF blocks by name map onto our scene nodes,
  since we load the same NIFs);
- **NoOp(why)**: nothing to do here, stated reason, returns the documented neutral value;
- **refused**: depends on the original's memory layout; returns 0, logged once per mod.

## 6. Evidence and testing

- **Spec notes** per family: `ext-strings.md`, `ext-arrays.md`, `ext-events.md`,
  `ext-forms.md`, `ext-ui.md` and so on. Each function entry: syntax, arguments, return,
  edge cases (null ref, disabled ref, in menu, unloaded cell), persistence, evidence tag.
- **Capture probes**: `tools/capture` grows an extender probe generator, in the same style as
  the function probes of script-function-coverage.md §3. It calls each function in the
  original game on chosen refs and forms and records the result. Goldens stay in
  `C:\fnv-private\goldens\ext\`.
- **Lab scenarios**: `L5.ext.<origin>.<family>` compares our results with the goldens.
  Compiler parity (§5.2) is `L5.script.compile.vanilla`.
- **Coverage tables**: `script-function-coverage.md`'s method extended with an origin column.
  Status per function as now: match / wrong / no-system / open.
- **Smoke scenarios** (tools/smoke/scenarios.toml): one per phase, running a small test
  plugin (our own, written for the test, never a third-party mod) that uses the phase's
  functions and logs the results.
- **Real mods** are only used on the owner's machine and are never committed. A list of
  target mods (§8) is the acceptance test.

## 7. Phases

Each phase has an exit test. Later phases don't start until the previous one's exit passes.

**Phase 0: demand scan (small).**
- `fnvtool ext scan <plugins…>`: reads mod ESPs, lists every extender function their
  `SCTX`/`SCDA` uses, with counts per mod, and how many units have no source.
- Build the function list from the wiki extract (`fnvtool ext catalogue`): name, aliases,
  origin, family, argument signature.
- Run it over the owner's chosen mod list (§8).
- *Exit*: a ranked table (function → number of mods and uses) and the source/no-source
  count, reviewed by the owner. This table sets the order of everything after it.

**Phase 1: compiler (large).**
- Vanilla grammar first, then the xNVSE syntax of §5.2.
- Console uses the compiler (several statements on a line come for free).
- *Exit*: `L5.script.compile.vanilla` passes on all 14,514 units; every extender-using unit
  from Phase 0's mods compiles, or fails only on functions not yet registered.

**Phase 2: runtime core (large).**
- String and array values, UDFs, lambdas, `ForEach`/`While`, the save section, xNVSE's
  `SetEventHandler` with a first set of firing points.
- The string, array and UDF functions of xNVSE (`sv_*`, `ar_*`, `Call`…).
- *Exit*: ext-strings/ext-arrays lab scenarios pass against captures; a save/load smoke
  scenario keeps strings, arrays and handlers.

**Phase 3: wrapper functions, ranked (large, can run in parallel).**
- Getters and setters over systems we already have (forms, refs, inventory, actor values,
  factions, perks, leveled lists, weapons, ammo, quests), in Phase 0's order across all three
  origins. One family per branch, so sessions can split the work.
- *Exit per family*: its lab scenario passes; its coverage rows are match or open with a
  stated capture.

**Phase 4: event hooks (medium).**
- JIP and JohnnyGuitar handler families, each with its firing point and the captured order.
- *Exit*: an event-order lab scenario per family.

**Phase 5: UI (medium for option B; option A is its own plan).**
- The named shims of §5.6 (B).
- *Exit*: the Phase 0 mods' HUD-reading functions return our widgets' values.

**Phase 6: engine internals and the tail (small to medium).**
- Classify every remaining function per §5.8; NIF block functions mapped.
- *Exit*: no extender function is unregistered; every one is Impl, Stub(system) or NoOp(why).

**Acceptance**: the owner's mod list (§8) plays through its main features with no
unregistered-function log lines.

Rough sizing [hypothesis]: Phase 1 and 2 together are about as big as Layer 5's script work.
Phase 3 is the bulk by count, but each function is small and the families are independent,
so it parallelises well across sessions.

## 8. Owner decisions needed

1. **Go / no go** for the plan as a whole.
2. **The mod list** for Phase 0 and acceptance: which mods you want to run (for example
   your usual load order, or a list of the most-used extender-dependent mods).
3. **UI**: option (B) now and (A) later, or (A) in this plan (§5.6).
4. **Extender patches** (§2, §9): treat them as "not our engine's bugs" by default, or go
   through JIP's and JohnnyGuitar's fix lists and decide which behaviours become options.
5. **Version to target**: the current releases of each extender at the time Phase 0 runs,
   captured against, or a fixed older version if your load order uses one.

## 9. Risks

- **Quirk dependence.** Mods depend on undocumented edge cases. The wiki often doesn't
  cover them. Mitigation: captures for every function family, edge cases included.
- **Moving target.** JIP LN and JohnnyGuitar still get releases. Mitigation: target a stated
  version (§5.7, §8.5) and re-run the probe generator when moving up.
- **UI mods.** Without option (A), MCM-dependent mods lose their settings menus. Phase 0
  measures how much that matters.
- **Behaviour mods that rely on extender patches.** Some mods assume a JIP or JohnnyGuitar
  engine fix is present. Those surface as differences in play, not as missing functions.
  The decision in §8.4 covers it.
- **No public opcode table.** Only matters for units without source (§5.2); Phase 0
  measures how many.
- **Licence of the wiki pages.** The function pages are documentation, used the same way as
  for vanilla functions. Wiki text is not copied into the repo, only re-expressed facts.

### T12b indexed UI paths (2026-10-29)

[observed: private jobs hud_hardcore hc0, hud_xp_limbs all, hud_prompt2 p0-p8;
menu-xml.md §5/§8, hud.md §14.8] `*:N` and `name:N` count from the last matching
child, zero-based. [design] fnv-ui applies this to all indexed UI function paths;
the HUD copies are created in reverse order to preserve their captured paths.
Unindexed lookup and draw traversal keep file order [hypothesis: unindexed lookup].
The captures used JIP GetUIFloatAlt; deprecated xNVSE GetUIFloat parity and other
menus/scripts remain untested [hypothesis]. No dependence on first-child indexing
was established in vanilla data by these captures; absence of such a dependency
is not claimed.
