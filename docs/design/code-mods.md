# Design: code mods (WebAssembly plugins inside the engine)

**Status: approved by the owner 2026-11-02 (#1398), with every §17 recommendation taken;
revised the same day after the review on #1399 (fuel limits, bone overrides, authoring
tools, script host).** Nothing here is built yet. §17 records the rulings and the one
open decision (item 8, not needed before M5). Implementation follows in phases
(§16), each on its own issue and branch.

## 1. Goal

Let users run their own code inside NVForever without changing the engine. A **code mod**
is a WebAssembly component plus a manifest. The engine loads it into a sandbox and gives
it a fixed, versioned API. Through that API it can:

- add script functions that ESP scripts, the console and other code mods call;
- react to game events;
- read and change game state through the same verbs the script functions use;
- keep its own data in the save;
- drive visual-only effects, such as bone physics for capes, hair and straps.

The API decides what a code mod can do. Mods gain new abilities when the API grows, not by
reaching into the engine.

### 1.1 Terms

"Plugin" already means an ESM/ESP in this project (plugins.txt, load order). These are
called **code mods**, and their API the **code mod API**. "Extender" keeps meaning the
xNVSE / JIP LN / JohnnyGuitar function libraries (extender-compat.md).

### 1.2 Non-goals

- Loading NVSE plugin DLLs, or any native code (extender-compat.md §2). A native-DLL
  tier could be designed later on top of the same API (§17 item 7). It is not part of
  this design.
- Access to engine memory, Rust types or Bevy's internal data (the ECS world, the render
  graph).
- Replacing whole engine systems (physics, renderer, pathfinding). Code mods use the
  hook points the API offers (§8.7).
- New asset types. Meshes, textures and records still come from ESPs, BSAs and loose
  files.

## 2. Charter fit

- **Clean room:** the API is NVForever's own design. Nothing comes from NVSE's plugin
  interface. The game behaviour that code mods reach is the behaviour already specified
  in the spec notes.
- **Game folder:** code mods are read from NVForever's home (§4.1). The engine never
  writes into the game folder.
- **Licence:** wasmtime is Apache-2.0 WITH LLVM-exception [documented: its repository's
  LICENSE], which is compatible with GPL-3.0-or-later. Whether code mods themselves must
  be GPL is an owner decision (§17 item 1).
- **Determinism:** gameplay traces are compared bit for bit (rapier is pinned for that,
  dependencies.md). Gameplay code mods run at a fixed point, in a fixed order, with
  seeded randomness and canonical NaNs (§6.4). Visual code mods cannot change simulation
  state at all (§7).

## 3. Architecture

```
 ESP scripts ─┐                          ┌─ code mod A (own Store, own memory)
 console ─────┤                          ├─ code mod B
 extenders ───┼─ function registry ──────┤   ...
              │  (by name, §10)          │
              └─ event dispatcher ───────┘
                  (extender-compat §5.5)
                         │
                    &mut Game  (gameplay tier, PlaySet::Scripts)
                    bone poses (visual tier, after animation)
```

| Piece | Where | Contents |
|---|---|---|
| API definition | `schemas/codemod/wit/*.wit` | The WIT package `nvforever:api` (§8), the single source for host and guest bindings |
| Host core | `crates/fnv-codemod` (new) | Manifest, discovery, wasmtime engine and Stores, limits, gameplay-tier interfaces over `fnv-game`'s `Game`, the save section. Headless, so `fnvtool` and tests run it without Bevy |
| Engine binding | `engine/nvf-play` (`codemods.rs`, new) | Bevy systems that drive the host at fixed points (§9), the visual and UI tiers, notices, console command |
| Guest SDK | `sdk/rust/nvforever-codemod` (new, outside both workspaces) | Generated bindings plus a small hand-written layer (`export!` macro, typed events) |
| Examples | `examples/codemods/{hello,bounty,cape}` | §14 |
| Tools | `fnvtool codemod check/list` | §13 |

## 4. Package format

### 4.1 Where code mods are found

1. `<home>\codemods\<id>\`. The home is `Documents\My Games\NVForever` (install-layout.md).
2. `Data\NVForever\CodeMods\<id>\` (§17 item 2), read through the virtual file
   system, so Mod Organizer 2 can install and order code mods like any other mod
   (mod-organizer-2.md). This is read-only; nothing is written there.

If the same `id` is found in both places, the home copy wins and a log line says so.

### 4.2 Folder contents

```
codemods/cape-physics/
  codemod.toml        manifest (required)
  mod.wasm            the component (required, name set in the manifest)
  config.toml         default settings (optional, §11)
  ui/                 menu XML for the tile tree (optional, §8.10)
  README.md, LICENSE  (recommended)
```

### 4.3 Manifest (`codemod.toml`)

```toml
id          = "cape-physics"          # [a-z0-9-]{3,40}, unique
name        = "Cape Physics"
version     = "1.2.0"                 # semver of the mod
api         = "0.1"                   # code mod API it was built against (§12)
component   = "mod.wasm"
authors     = ["..."]
license     = "MIT"
description = "Bone-chain physics for capes and coats."

# What the mod may use. Interfaces it imports but was not granted fail at load (§7).
permissions = ["visual.bones", "visual.nif-data", "events", "config"]

# Script functions it defines must start with this prefix (§10.1).
function-prefix = "CapeSim_"

[load]
after    = ["some-other-mod"]          # soft ordering hints
requires = [{ id = "core-lib", version = ">=1.0" }]
plugins  = ["MyCapes.esp"]             # ESPs it expects; a warning if not loaded

[limits]                               # requests, capped by the engine (§6.2)
memory-mb = 32
```

## 5. Discovery, load order, enabling

- `NVForever.ini`, section `[CodeMods]`: `Enabled=1` (master switch, default 1), then one
  `<id>=0|1` per mod. A newly found mod is **off** until the player turns it on. That
  matches how new plugins are handled (install-layout.md §Plugins). The first time it is
  turned on, the settings page shows its permissions.
- The load order is a topological sort over `requires` and `after`, with ties broken by
  `id`. It is stored as `codemods.txt` in the home, in the same style as `plugins.txt`, so
  it is stable and editable. A cycle or a missing `requires` turns the mods involved off,
  with a message.
- The load order decides the order of `init`, of event delivery and of hook chains
  (§9.3).
- Command line: `--no-codemods`, and `--codemod <path>` to load one folder for
  development.

## 6. Runtime

### 6.1 Engine

- wasmtime (owner ruling 2026-11-02, §17 item 5) with the Component Model and the
  Cranelift compiler, pinned to an exact version for determinism, the same rule as rapier.
- Compiled code is cached under `<cache>\codemods\`, keyed by the component's hash, the
  wasmtime version and the engine configuration. Later starts skip compilation
  [documented: wasmtime `Engine::precompile_component` / `Component::deserialize`].
- One wasmtime `Engine` for the process, one `Store` and instance per code mod. Code mods
  share no memory.
- Wasm features: SIMD on. Threads, shared memory and relaxed SIMD off: relaxed SIMD's
  results can differ between CPUs [documented: the relaxed-simd proposal].

### 6.2 Limits

| Limit | Default | Notes |
|---|---|---|
| Linear memory | 32 MB | The manifest may ask for up to 256 MB. Enforced by a `ResourceLimiter` [documented: wasmtime] |
| Work per gameplay call (event, function, hook) | 10 M fuel | **Fuel**, not time: wasmtime counts executed work, about one unit per wasm instruction [documented: wasmtime `Config::consume_fuel`], so the cutoff falls at the same point on every machine (§6.4) |
| Work per gameplay frame, per mod | 20 M fuel | All of a mod's gameplay calls in one frame together. Running out stops the mod (§6.3), the same on every machine |
| `init` | 200 M fuel | Once, at game start |
| Time per visual tick | 1 ms per mod | **Epoch interruption** (real time): one engine thread advances the epoch every 0.5 ms [documented: wasmtime `Config::epoch_interruption`]. Visual only, where real time cannot change a trace |
| Time for all visual ticks per frame | 2 ms total, `[CodeMods] VisualBudgetMs` in `NVForever.ini` | When it is used up, the remaining mods skip **this frame's visual tick only** (load order rotates the first mod each frame so none is always last). Nothing gameplay is ever skipped for time |
| Script-function call depth across mods | 8 | §9.2 |
| Save data | 1 MB per mod | §10.3 |

All the numbers are [hypothesis] starting values. M1 measures fuel's overhead and how
many fuel units a millisecond is on the owner's machine, then sets them. Gameplay time is
measured but **never enforced**: each mod's real time per frame (gameplay and visual) goes
into the existing frame telemetry (frame-telemetry.md), and a mod averaging over 1 ms of
gameplay time gets a log warning. Real time decides nothing in the gameplay tier.

Why not time for gameplay too: a mod close to a time limit would be stopped on a slow
machine and not on a fast one, and the traces would then differ (review on #1399).

### 6.3 Failure

A trap stops the mod. A trap is a panic inside the mod, running out of time or memory, or
a host call that returns a fatal error. Then:

1. The call is abandoned. Script functions return `0` (the same as an unhandled function
   today). A hook keeps the value it had before this mod.
2. The mod is **turned off for the rest of the session**. Its later events are not sent,
   and its script functions become `Stub("code mod <id> stopped")`, so they keep showing
   in the trace.
3. The log gets the mod id, the reason and the wasm backtrace (with function names when
   the component carries a name section). The player sees a notice once (HUD message:
   "Code mod Cape Physics stopped. See the log.").
4. The game carries on. Its saved data stays as it was (§10.3).

**Exception:** a mod stopped for real time in its visual tick (§6.2) loses only its
visual tier for the session. Its gameplay side keeps running, because stopping it there
would make gameplay depend on the machine's speed.

### 6.4 Determinism rules (gameplay tier)

- NaN canonicalisation on [documented: wasmtime `Config::cranelift_nan_canonicalization`].
- No wall clock. `game-time` gives game time and the frame number. WASI's clocks are given
  a fixed source for gameplay mods: monotonic time counts frames, and wall time is the
  game's start time.
- Randomness: WASI `random` and `api.random` are a per-mod `rand_chacha` stream, seeded
  from the save's seed and the mod id (dependencies.md already chose `rand_chacha` for
  the simulation). This stream is saved.
- Calls run in load order at fixed points (§9). Nothing runs on other threads.
- Limits are counted in fuel, never in time (§6.2). Running out of fuel or memory happens
  at the same point on every machine.

Visual-tier mods may read the real clock. They cannot touch the simulation, so they
cannot break a trace.

## 7. Permissions and tiers

Each interface in §8 belongs to a permission. When the engine links a component, an
import whose permission is not in the manifest **fails the load**, with a message naming
it. Nothing traps at runtime. The settings page shows the permissions in words.

| Permission | Tier | Interfaces | Shown as |
|---|---|---|---|
| (always) | all | `types`, `log`, `game-time`, `random`, `codemod` | — |
| `game.read` | gameplay | `forms`, `refs`/`actors`/`inventory`/`quests`/`globals` (read functions) | "Reads the game world" |
| `game.write` | gameplay | the same interfaces' write functions, `script.call` | "Changes the game world" |
| `events` | gameplay | `events` | "Reacts to game events" |
| `script.functions` | gameplay | `script.register` | "Adds script functions" |
| `hooks` | gameplay | `hooks` (§9.3) | "Changes game rules" |
| `save` | gameplay | `storage` | "Keeps data in your saves" |
| `config` | all | `config` | "Has settings" |
| `ui` | ui | `ui` | "Changes menus and the HUD" |
| `visual.bones` | visual | `visual-bones`, `visual-tick` | "Moves character bones (looks only)" |
| `visual.nif-data` | visual | `nif-data` | "Reads model data" |
| `visual.world` | visual | `world-query` (read-only raycasts) | "Looks at the world around actors" |

**Visual tier rule:** the visual interfaces have no write access to anything that is
saved or traced. A mod may hold both visual and gameplay permissions, but each interface
can only be called from its own stage (§9). Calling `actors.set-av` from the visual tick
returns `error.wrong-stage`.

**WASI:** components built for `wasm32-wasip2` import WASI interfaces (stdout, clocks,
random, environment). The host provides them through `wasmtime-wasi` with nothing
granted: no preopened directories, no sockets, an empty environment, stdout and stderr
sent to the log with the mod's id, clocks and random as in §6.4. Code mods get no file
system and no network.

## 8. The API, version 0.1 (`nvforever:api@0.1.0`)

What follows is the shape of the API. The `.wit` files written in phase M1 become the
reference. Every gameplay verb should be a thin wrapper over an existing
`crates/fnv-game/src/functions` implementation, so ESP scripts, the console, extender
functions and code mods share one code path, its spec note and its tests.

### 8.1 `types`

```wit
interface types {
  type form-id = u32;        // load-order form ID, as scripts see it
  type ref-id  = form-id;    // a placed or created reference

  variant value {
    none, int(s32), number(f64), form(form-id), text(string),
    // array(array-id) added with extender Phase 2 arrays
  }

  enum error-code {
    not-found, wrong-type, not-loaded, invalid-arg, denied, wrong-stage, failed,
  }
  record error { code: error-code, message: string }

  record vec3 { x: f32, y: f32, z: f32 }
  record quat { x: f32, y: f32, z: f32, w: f32 }
  record transform { translation: vec3, rotation: quat, scale: f32 }
}
```

- Form IDs are load-order IDs. A mod that needs a form from a plugin looks it up with
  `forms.resolve("MyCapes.esp", 0x000123)`, so the plugin's position does not matter.
- Units are game units and radians (fnv-phys converts internally).
- Text is UTF-8. Game strings are converted from Windows-1252 at the boundary.

### 8.2 `log`, `game-time`, `random`, `codemod`

```wit
interface log { enum level { trace, debug, info, warn, error }  write: func(l: level, msg: string); }
interface game-time {
  frame: func() -> u64;                 // simulation frame number
  game-days-passed: func() -> f64;      // GameDaysPassed
  game-hour: func() -> f32;
  time-scale: func() -> f32;
  in-menu-mode: func() -> bool;
}
interface random { next-u32: func() -> u32; next-f32: func() -> f32; }   // §6.4
interface codemod {
  self-id: func() -> string;
  is-loaded: func(id: string) -> option<string>;   // version if loaded
  api-version: func() -> string;
}
```

### 8.3 `forms` (`game.read`)

```wit
interface forms {
  use types.{form-id, error};
  resolve: func(plugin: string, local-id: u32) -> result<form-id, error>;
  by-editor-id: func(edid: string) -> option<form-id>;
  form-type: func(f: form-id) -> result<string, error>;   // "WEAP", "ARMO", ...
  editor-id: func(f: form-id) -> option<string>;
  name: func(f: form-id) -> option<string>;
  value: func(f: form-id) -> option<s32>;
  weight: func(f: form-id) -> option<f32>;
  model-path: func(f: form-id) -> option<string>;
  keywords-or-flags: func(f: form-id) -> list<string>;     // named flags only, never raw bits
}
```

Writing to base forms (weapon damage and the like) waits for the extender form family
(extender-compat §7 Phase 3), and is exposed through `script.call` until then.

### 8.4 `refs`, `actors`, `inventory`, `quests`, `globals`

```wit
interface refs {
  use types.{ref-id, form-id, vec3, error};
  base: func(r: ref-id) -> result<form-id, error>;
  position: func(r: ref-id) -> result<vec3, error>;
  angle: func(r: ref-id) -> result<vec3, error>;
  cell: func(r: ref-id) -> result<form-id, error>;
  is-disabled: func(r: ref-id) -> bool;
  is-3d-loaded: func(r: ref-id) -> bool;
  distance: func(a: ref-id, b: ref-id) -> result<f32, error>;
  // game.write:
  move-to: func(r: ref-id, target: ref-id, offset: vec3) -> result<_, error>;
  set-position: func(r: ref-id, p: vec3) -> result<_, error>;
  enable: func(r: ref-id, fade: bool) -> result<_, error>;
  disable: func(r: ref-id, fade: bool) -> result<_, error>;
  place-at: func(base: form-id, at: ref-id, count: s32) -> result<ref-id, error>;
  player: func() -> ref-id;                                 // 0x14
}
interface actors {
  use types.{ref-id, form-id, error};
  get-av: func(a: ref-id, av: string) -> result<f64, error>;      // by name, AV_NAMES
  get-base-av: func(a: ref-id, av: string) -> result<f64, error>;
  is-dead: func(a: ref-id) -> bool;
  level: func(a: ref-id) -> result<u16, error>;
  race: func(a: ref-id) -> result<form-id, error>;
  is-female: func(a: ref-id) -> result<bool, error>;
  faction-rank: func(a: ref-id, f: form-id) -> option<s8>;
  equipped: func(a: ref-id) -> list<form-id>;
  in-combat: func(a: ref-id) -> bool;
  combat-target: func(a: ref-id) -> option<ref-id>;
  // game.write:
  set-av: func(a: ref-id, av: string, v: f64) -> result<_, error>;
  mod-av: func(a: ref-id, av: string, delta: f64) -> result<_, error>;
  damage-av: func(a: ref-id, av: string, amount: f64) -> result<_, error>;
  restore-av: func(a: ref-id, av: string, amount: f64) -> result<_, error>;
  equip: func(a: ref-id, item: form-id, silent: bool) -> result<_, error>;
  unequip: func(a: ref-id, item: form-id, silent: bool) -> result<_, error>;
  kill: func(a: ref-id, killer: option<ref-id>) -> result<_, error>;
  start-combat: func(a: ref-id, target: ref-id) -> result<_, error>;
}
interface inventory {
  use types.{ref-id, form-id, error};
  record stack { item: form-id, count: s32 }
  count: func(holder: ref-id, item: form-id) -> s32;
  items: func(holder: ref-id) -> list<stack>;
  // game.write:
  add: func(holder: ref-id, item: form-id, count: s32, silent: bool) -> result<_, error>;
  remove: func(holder: ref-id, item: form-id, count: s32, silent: bool) -> result<_, error>;
}
interface quests {
  use types.{form-id, value, error};
  stage: func(q: form-id) -> result<u16, error>;
  is-stage-done: func(q: form-id, stage: u16) -> bool;
  is-running: func(q: form-id) -> bool;
  variable: func(q: form-id, name: string) -> result<value, error>;
  // game.write:
  set-stage: func(q: form-id, stage: u16) -> result<_, error>;
  set-variable: func(q: form-id, name: string, v: value) -> result<_, error>;
  start: func(q: form-id) -> result<_, error>;
  stop: func(q: form-id) -> result<_, error>;
}
interface globals {
  use types.{form-id, error};
  get: func(g: form-id) -> result<f32, error>;
  set: func(g: form-id, v: f32) -> result<_, error>;        // game.write
}
```

### 8.5 `script`: calling and adding script functions

```wit
interface script {
  use types.{ref-id, value, error};
  enum param-type { int, number, form, ref, text, actor-value, any }
  record param { name: string, kind: param-type, optional: bool }
  record call-ctx { calling-ref: option<ref-id>, self-ref: option<ref-id>, unit: string }

  // script.functions; only during init:
  register: func(name: string, params: list<param>, returns: param-type) -> result<u32, error>;

  // game.write:
  call: func(name: string, on: option<ref-id>, args: list<value>) -> result<value, error>;
}
```

`script.call` gives a code mod the whole function library at once: vanilla, the
extenders as they are built, and other code mods' functions. The typed interfaces above
are the fast and convenient path for the common verbs. `call` is the complete one.

### 8.6 `events`

```wit
interface events {
  use types.{ref-id, form-id};
  enum kind {
    game-loaded, new-game, before-save, cell-attached, cell-detached,
    load-3d, unload-3d,                 // an actor's or object's 3D was built or dropped
    equip, unequip, item-added, item-removed,
    hit, death, activate, combat-start, combat-end,
    quest-stage, menu-open, menu-close,
    message-box-answer,                 // the player answered a mod's ui.show-message-box
    tick,                               // once per simulation frame
  }
  record filter { kind: kind, subject: option<ref-id>, form: option<form-id> }
  subscribe: func(f: filter);
  unsubscribe: func(f: filter);
  // Payloads are delivered through the guest export `on-event` (§8.12).
  variant event {
    game-loaded, new-game, before-save,
    cell(tuple<kind, form-id>),
    load-3d(ref-id), unload-3d(ref-id),
    equip(equip-info), unequip(equip-info),
    item(item-info),
    hit(hit-info), death(death-info), activate(activate-info),
    combat(tuple<kind, ref-id, option<ref-id>>),
    quest-stage(tuple<form-id, u16>),
    menu(tuple<kind, string>),
    message-box-answer(tuple<u32, u32>), // (box id from show-message-box, button index)
    tick(f32),                           // simulated seconds this frame
  }
  record equip-info    { actor: ref-id, item: form-id }
  record item-info     { holder: ref-id, item: form-id, count: s32, from: option<ref-id> }
  record hit-info      { target: ref-id, attacker: option<ref-id>, weapon: option<form-id>,
                         projectile: option<ref-id>, damage: f32, body-part: option<string>,
                         critical: bool, sneak: bool }
  record death-info    { actor: ref-id, killer: option<ref-id> }
  record activate-info { target: ref-id, by: ref-id }
}
```

The firing points are the ones the extender dispatcher defines (extender-compat §5.5). A
code mod event fires in the same frame and order as the matching `Begin` blocktype or
extender event: vanilla blocks first, then extender handlers, then code mods
[hypothesis: our choice of order, §9.1]. Subscriptions are filtered on the host, so a mod
listening for one actor's hits is not called for everyone's.

### 8.7 `hooks`: changing a result (`hooks`)

Hooks let a mod change a value the engine is about to use, not just observe it. Version
0.1 has a short, chosen list. Each hook is a pure function of its input record.

```wit
interface hooks {
  enum hook { damage, hit-chance, barter-price, xp-reward, loot-roll, fall-damage }
  register: func(h: hook);              // init only
}
// Guest export: on-hook(h, input) -> output, with a record per hook, for example
//   damage: { target, attacker, weapon, base: f32, ... } -> { damage: f32 }
```

**Chain rule:** each registered mod gets the previous mod's output, in load order. The
last one's result is used. A mod that traps is skipped (§6.3). Each hook's firing point
and inputs are written into the spec note of the system it belongs to (combat.md and so
on) before it is added. The list only grows through issues.

### 8.8 `storage` (`save`)

```wit
interface storage {
  get: func(key: string) -> option<list<u8>>;
  set: func(key: string, data: list<u8>) -> result<_, error>;   // §6.2 size limit
  remove: func(key: string);
  keys: func() -> list<string>;
}
```

The data lives in the mod's in-memory map. It is written into the save (§10.3) and
restored before `game-loaded` fires.

### 8.9 `config` (`config`)

```wit
interface config {
  get-float: func(key: string) -> option<f64>;
  get-int: func(key: string) -> option<s64>;
  get-bool: func(key: string) -> option<bool>;
  get-text: func(key: string) -> option<string>;
}
```

Defaults come from the mod's `config.toml`. The player's values are in
`<home>\codemods.ini`, section `[<id>]`, and set on the settings page, which lists each
mod's keys with their types and descriptions from `config.toml`. Changes are delivered as
an `on-config-changed` export call.

### 8.10 `ui` (`ui`)

```wit
interface ui {
  show-message: func(text: string);                       // HUD corner message
  show-message-box: func(text: string, buttons: list<string>) -> u32;  // box id; answer: §8.6 message-box-answer
  set-float: func(path: string, v: f32) -> result<_, error>;   // tile tree paths
  get-float: func(path: string) -> result<f32, error>;
  set-text: func(path: string, v: string) -> result<_, error>;
}
```

`show-message-box` returns at once with a box id. The answer comes later as the
`message-box-answer` event, which is sent only to the mod that opened the box (no
subscription needed). A mod cannot wait for it inside the call: that would block the
frame.

The paths are the tile tree's (ui-tile-tree.md, extender-compat §5.6, owner chose option
A). A mod's `ui/` folder is injected like extender menu XML. A code mod therefore gets
the same UI reach as a JIP-style script mod.

### 8.11 Visual tier: `visual-bones`, `nif-data`, `world-query`

```wit
interface visual-bones {
  use types.{ref-id, transform, error};
  resource skeleton {
    bone: func(name: string) -> option<u32>;               // index, by NIF node name
    bone-names: func() -> list<string>;
    world-transform: func(bone: u32) -> transform;         // after animation, this frame
    local-transform: func(bone: u32) -> transform;
    parent: func(bone: u32) -> option<u32>;
    set-local-override: func(bone: u32, t: transform);     // applied before skinning
    clear-override: func(bone: u32);
  }
  skeleton-of: func(r: ref-id) -> result<skeleton, error>; // 3D must be loaded
  velocity: func(r: ref-id) -> types.vec3;
  is-first-person: func(r: ref-id) -> bool;
  camera-distance: func(r: ref-id) -> f32;
  is-ragdolled: func(r: ref-id) -> bool;
}
interface nif-data {
  use types.{ref-id, form-id};
  // NiStringExtraData / NiFloatExtraData / NiIntegerExtraData by name, on the
  // model of a worn item or a reference.
  string-data: func(model-of: form-id, node: option<string>, name: string) -> option<string>;
  float-data:  func(model-of: form-id, node: option<string>, name: string) -> option<f32>;
  int-data:    func(model-of: form-id, node: option<string>, name: string) -> option<s32>;
}
interface world-query {
  use types.{vec3};
  record hit { point: vec3, normal: vec3, distance: f32 }
  raycast: func(from: vec3, to: vec3) -> option<hit>;     // static world only
  wind: func() -> vec3;
  water-height-at: func(p: vec3) -> option<f32>;
}
```

Overrides belong to the mod that set them. They are cleared when the actor's 3D unloads
or the mod stops.

**Overrides and animation (prerequisite for M4).** Animation does not rewrite every bone
every frame. It starts from the bone's *current* transform, writes only the channels its
track has (translation, rotation, scale), writes through `set_if_neq`, and leaves out
tracks that never move (engine/nvf-assets/src/anim_play.rs `step`; model.rs `moves`)
[observed: code, 2026-11-02]. If an override were written straight into the bone's
`Transform`, three things would go wrong:
- a channel the clip does not animate would keep the override;
- a bone in a still track would never return after `clear-override`;
- extra cape bones (below) have no track at all, so nothing would ever reset them.

So overrides are not written into the animated pose. The host keeps, for each
overridden bone, its **base pose**: the pose animation produced this frame, or the bind
pose for a bone no clip drives. Each frame, after animation, it records the base pose,
then writes the override (all channels) and marks the transform changed. On
`clear-override`, or when the mod stops, it writes the base pose back. `local-transform`
and `world-transform` return the base pose, never the override, so a simulation does not
read its own last output as input. If two mods override the same bone, the later one in load order wins,
and a log line names both.

**Extra bones:** a worn armor's NIF may contain bones that are not in the skeleton (cape
chains). When the armor is attached, the engine adds them under their parent bone by name,
so the mesh can skin to them. Whether `engine/nvf-assets/src/actor.rs` already does this
is unchecked [hypothesis: probably not]. It is a prerequisite of the visual tier (phase M4).

### 8.12 The guest world

```wit
world codemod {
  import types; import log; import game-time; import random; import codemod;
  import forms; import refs; import actors; import inventory; import quests; import globals;
  import script; import events; import hooks; import storage; import config; import ui;
  import visual-bones; import nif-data; import world-query;

  export init: func() -> result<_, string>;
  export on-event: func(e: events.event);
  export call-function: func(id: u32, args: list<types.value>, ctx: script.call-ctx)
                        -> result<types.value, string>;
  export on-hook: func(h: hooks.hook, input: list<u8>) -> list<u8>;   // typed in the SDK
  export on-visual-tick: func(dt: f32);
  export on-config-changed: func(key: string);
  export on-shutdown: func();
}
```

A component imports only what it uses, so only those imports need permissions (§7).
Exports a mod does not need are filled in with no-ops by the SDK.

## 9. Calling model

### 9.1 Where code mods run in a frame

| Stage | Bevy placement | What runs |
|---|---|---|
| Init | Once, after plugins and the save are loaded, before the first `PlaySet::Scripts` | `init` in load order: subscriptions, registrations, hooks |
| Gameplay | `PlaySet::Scripts`, after the script VM's frame (engine/nvf-play/src/lib.rs `PlaySet`) | The queued events of this frame, in the order they happened. Each event goes to its subscribers in load order. Then `tick` |
| Inline | Wherever the engine calls them | Script functions (when a script calls them) and hooks (when the system computes the value) |
| Visual | `PostUpdate`, after animation is applied, before `TransformSystems::Propagate` and skinning [hypothesis: exact placement to check against the engine's animation systems] | `on-visual-tick` for mods with `visual.bones`, then the host applies overrides over the recorded base poses (§8.11) |

During gameplay calls the host holds `&mut Game`, so writes take effect at once, as a
script function's do.

### 9.2 Re-entry

- Events fired *during* a code mod call (for example `actors.kill` causes `death`) are
  queued and delivered later in the same gameplay stage. They are never delivered inside
  the call that caused them.
- A script function call can go script → mod A → `script.call` → mod B. The depth is
  capped at 8. A mod that is already on the call stack cannot be entered again: the call
  returns `error.denied`. That keeps one component instance from being re-entered, which the
  Component Model forbids anyway [documented: its reentrance rule; a re-entry traps].
- Hooks cannot call `game.write` functions (`error.wrong-stage`): they compute a value.

### 9.3 Performance notes

- A host call costs on the order of tens of nanoseconds. Copying strings and lists
  across the boundary costs more [hypothesis: to measure in M1]. The API passes ids and
  small records, and leaves bulk data with the host.
- Per-mod time is recorded in the existing frame telemetry (frame-telemetry.md) and shown
  by the console command `codemods` (§13). Real time is only measured, never enforced, in
  the gameplay tier (§6.2).

## 10. Integration with the engine

### 10.1 Function registry

- Today's registry is a static slice keyed by opcode
  (`crates/fnv-game/src/functions/mod.rs`, `registry()`, `Kind::{Impl, Stub, NoOp}`).
  Extender-compat §5.1 already plans a second key by name. Code mods need that table to
  be extendable at runtime: `Registry { by_opcode, by_name }`, built from the static slice
  plus the extender crates plus code mod registrations.
- A new entry kind, `Kind::CodeMod { mod_index, fn_id, sig }`. The VM converts its
  arguments into `value`s and calls `call-function`.
- Names must start with the manifest's `function-prefix`. A name that clashes with any
  existing function is refused at `register`.
- **ESP scripts reach code mod functions by name, so they need the script compiler**
  (extender-compat Phase 1, route 2). Compiled `SCDA` bytes carry opcodes only, and a code
  mod function has no opcode the GECK knows. Until the compiler exists, code mod
  functions are reachable from the console, from other code mods and from extender UDF
  calls by name. This dependency matters most for when M2 is useful.
- Coverage tooling: code mod functions are counted apart from game functions and never
  appear in `script-stubs` totals.

### 10.2 Event dispatcher

There is one dispatcher in `fnv-game` (extender-compat §5.5), and code mods are one more
kind of subscriber. Building the firing points for the extenders gives code mods the
same events at no extra cost. Building `load-3d`/`unload-3d` and `equip`/`unequip` first
also serves the visual tier.

### 10.3 Saves

- A new `codemods` field in `SaveBody` (save-load.md §4), `#[serde(default)]`, so older
  saves load:
  `{ "<id>": { "version": "1.2.0", "storage": { key: bytes }, "rng": state } }`.
- On load, a mod's data is restored before `game-loaded`. Data of a mod that is not
  loaded is **kept unchanged** and written back on the next save, so turning a mod off
  for one session loses nothing.
- The save header lists the code mods and their versions, so the load menu can warn of
  missing or older mods. This mirrors the plugin-list check (save-load.md §4).

### 10.4 Console and settings

- Console: `codemods` (list with state and time used), `codemod reload <id>` (dev only:
  drop the instance and load it again; storage survives, in-memory state does not).
- The settings page gets a Code mods tab: on/off per mod, permissions, `config.toml` keys.

## 11. Versioning

- The WIT package is semver. During `0.x`, a mod's `api = "0.1"` loads only on a `0.1.*`
  engine. From `1.0`, a mod built for `1.n` loads on any `1.m` with `m >= n`. Additions
  (new functions, new event kinds, new hooks) are minor releases. Changing or removing
  anything is a major release.
- `variant` and `enum` additions break older guests' bindings in the Component Model, so
  events and hooks are delivered only to mods whose API version knows them. The host keeps
  one binding per supported minor version [hypothesis: to confirm how wasmtime's bindgen
  handles linking older worlds; M1 checks it].
- Deprecations stay for one major version and log once per mod.

## 12. SDK and authoring

- Target: `wasm32-wasip2`, which builds a component directly with stable Rust
  [documented: the Rust target's platform-support page]. No `cargo component` needed.
- `nvforever-codemod` crate: the `wit-bindgen` output plus:
  - `export!(MyMod)` and a `CodeMod` trait with default no-op methods;
  - typed `Event` and hook input/output types;
  - `Value` conversions, and `FormId` helpers for `forms.resolve`.
- Other languages: anything with a Component Model toolchain (C/C++ with
  `wit-bindgen c`, Go, JavaScript through `jco`). Only Rust is supported and tested by
  this project at first.
- A template: `examples/codemods/hello` copied with a new id (`fnvtool codemod new`, §13).
- A **GitHub template repository** whose Actions workflow builds the `.wasm`, runs
  `fnvtool codemod check` and attaches the packaged zip to every tag. Authors who never
  want a local toolchain can use it.

### 12.1 Script host (an authoring route with no compiler)

Most Fallout: New Vegas modders come from GECK and xNVSE script, not Rust (review on
#1399). NVForever ships one prebuilt code mod, the **script host**. It embeds an
interpreter and runs the script files in an author's code mod folder.

- **Manifest:** the author's mod names the host (`runtime = "script-host"`) and its
  script files. Each script mod is its own instance of the host component, with its own
  id, permissions, storage and limits.
- **Files:** the engine hands the host the mod's script files at `init`, as read-only
  bytes. No file-reading call is added, so the "no file system" rule of §7 stays as it is.
- **Determinism:** the interpreter and its maths library are compiled into the
  component, so its results are the same on every machine. The §6.2 fuel limits count
  the interpreter's work.
- **Scope:** events, hooks, quests, UI and config, the bulk of gameplay mods. Heavy
  per-frame work (cape physics) stays in Rust.
- **Language:** open, owner decision §17 item 8.
- **Phase:** M5, after the API has settled, so the host's bindings are written once.
  Example: `hello` again in the chosen script language.

## 13. Tooling and diagnostics

- `fnvtool codemod check <folder>`: reads the manifest, compiles the component, lists its
  imports and exports, compares them with the permissions and the API version, and
  checks the function prefix.
- `fnvtool codemod list`: what would load, in which order, and why each mod is off.
- Authoring commands, so making a mod does not start with learning Cargo (review on
  #1399):
  - `doctor`: checks for rustup and the `wasm32-wasip2` target; prints the install
    commands, or runs them when asked.
  - `new <id>`: a working project from the template.
  - `build`: compiles, then runs `check`.
  - `watch`: rebuilds on save and reloads the mod in a running game (`--codemod` and the
    console's `codemod reload`).
  - `package`: a zip laid out as `NVForever\CodeMods\<id>\`, ready for Mod Organizer 2
    or a mod site.
- Log lines are prefixed `codemod[<id>]:` so smoke expectations can match them.

## 14. Examples (shipped with the SDK, also the tests)

1. **hello**: logs on `game-loaded`, registers `Hello_Add <a> <b>`, keeps a counter in
   `storage`. Covers loading, registration, the console call and the save round trip.
2. **bounty**: on `hit` by the player with damage above a `config` threshold, adds caps.
   Registers a `damage` hook that scales damage by a setting. Covers events, `game.write`,
   hooks and config.
3. **cape**: bone-chain cloth. On `load-3d` and `equip` it reads a cape's `NiFloatExtraData`
   settings (stiffness, damping, gravity) and finds bones `Cape1..CapeN`. Each visual
   tick it runs a Verlet chain with distance constraints and body capsules, then writes
   bone overrides. Turned off in first person and beyond a camera distance. Covers the
   whole visual tier. Needs a test cape NIF of our own (never game data) with extra bones.

## 15. Testing

- **Host unit tests** (`crates/fnv-codemod`): small components built from the examples in
  a build step and checked in as test inputs. They are our own code, so committing their
  binaries is fine; or they are built in CI with the `wasm32-wasip2` target added to the
  toolchain. They cover: permission refusal at link time, memory and fuel limits (a
  looping gameplay guest is stopped at the same instruction count on every run), the
  visual time limit (a looping visual tick loses only the visual tier), traps that turn a mod off, load order
  and cycles, storage round trips, and re-entry refusal.
- **Determinism test:** the same scenario run twice with the `bounty` example gives the
  same trace bit for bit.
- **Smoke scenarios** (`tools/smoke/scenarios.toml`): `codemod-hello` (load, call from
  the console, save and load, counter kept) and `codemod-cape` (the cape test NIF on the
  player, bones move, no `stopped` line). Each lists the `codemod:` log lines it expects.
- No lab goldens: code mods are not behaviour of the original game.

## 16. Phases

| Phase | Contents | Exit test |
|---|---|---|
| **M0** | Owner rulings on §17 (done 2026-11-02) | Doc approved |
| **M1** Host core | wasmtime added, with its licence expression in `deny.toml`; `fnv-codemod`: manifest, discovery, load order, wasmtime engine and cache, limits, failure handling, `log`/`game-time`/`random`/`codemod`, `fnvtool codemod check/list/doctor/new/build`, `hello` without registration | Host unit tests; a looping gameplay guest is stopped by fuel at the same count on every run, a looping visual guest within 1 ms ± one epoch; fuel's overhead, fuel units per ms and host-call cost measured and written down, and the §6.2 numbers set from them |
| **M2** Gameplay API | Registry by name (shared with extender Phase 1/2 work), `Kind::CodeMod`, `forms`/`refs`/`actors`/`inventory`/`quests`/`globals`/`script`, `storage` and the save section, console commands, `fnvtool codemod watch/package` | `codemod-hello` smoke passes; save round trip; determinism test |
| **M3** Events, hooks, config, UI | The `events` interface over the extender dispatcher (as many firing points as exist), the hook list of §8.7 with spec-note entries, `config` and the settings tab, `ui` over the tile tree | `bounty` works in play; hook chain order test |
| **M4** Visual tier | Extra-bone attachment for worn armor, bone overrides over recorded base poses in `PostUpdate` (§8.11), `nif-data`, `world-query`, `visual-tick` | `codemod-cape` smoke passes; the cape example in the release |
| **M5** SDK release | The SDK crate published with docs, examples and the GitHub template repository; the script host (§12.1); the linking exception text (§17 item 1); API frozen as `0.1` | On a clean Windows machine with no Rust installed, an outside author goes from the shipped tools to a working mod in the game, following only the docs, by both routes: Rust through `doctor`/`new`/`build`, and the script host with no install at all |

M1 does not depend on the extender work. M2 shares the registry-by-name change with
extender Phase 1/2, so whichever comes first builds it. M3's events are as complete as
the dispatcher's firing points are at the time.

## 17. Owner decisions (items 1-7 ruled 2026-11-02, item 8 open)

The owner took the recommendation on items 1-7. Item 3 had no recommendation, so the
list the spec already proposed (§8.7) stands.

1. **Licence of code mods: a linking exception.** Code mods that use only the code mod API
   may use any licence. The SDK and the WIT files are MIT OR Apache-2.0. Not yet done:
   - The exception text goes into `LICENSE`/`LEGAL.md` before the SDK release (M5).
   - Under GPL-3.0 §7, only copyright holders can grant an additional permission for
     their own code. The history has another major author, so the exception needs their
     agreement, or it must be limited to code they did not write. The owner settles that
     before M5.
   - The wording is the owner's to approve; sessions do not draft legal terms on their
     own.
2. **Where code mods live: both.** `<home>\codemods\` and, read-only through the VFS,
   `Data\NVForever\CodeMods\` for Mod Organizer 2 (§4.1). The home copy wins.
3. **Hooks in 0.1: the six in §8.7** (damage, hit chance, barter price, XP reward, loot
   roll, fall damage). Each gets its spec-note entry before it is built (M3). Later
   hooks come through issues.
4. **New code mods are off** until the player turns them on, and see the permissions
   prompt first (§5).
5. **Runtime: wasmtime.** It compiles to native code, which the per-frame visual tier
   needs, and it has the most complete Component Model support. Its cost is several MB
   in the executable and a longer build. The interpreter `wasmi` was the alternative and
   is not used. `deny.toml` does not yet allow wasmtime's licence expression
   `Apache-2.0 WITH LLVM-exception`. Phase M1 adds it, with the crate list that
   `cargo deny` reports.
6. **Order: M1 now.** M1 is independent of the extender work. M2 goes together with the
   registry-by-name change that extender Phase 1/2 also needs; whichever starts first
   builds it.
7. **No native-DLL tier is planned.** Revisit only when a real mod cannot be done in
   WebAssembly.
8. **Script host language** (§12.1). *Open, added 2026-11-02 after the review on #1399.*
   - **Lua** (proposed in the review): small, widely known, easy to embed and to compile
     to WebAssembly.
   - **GECK/xNVSE-style script**, built from NVForever's own compiler (extender-compat
     Phase 1) for WebAssembly: the language these modders already write. It depends on
     the compiler being finished.

   A related option: expose hooks to ESP scripts the way extender event handlers are,
   which serves the same modders without a host. Not needed before M5.
