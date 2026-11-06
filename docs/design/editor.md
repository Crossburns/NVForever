# In-game plugin editor and quest debugger (design)

Work in progress on the branch `local/editor-encoder` (backed up to GitHub 2026-11-03; no issue or pull
request yet: the owner keeps it off the repo until it is known to work, 2026-11-01). The
owner's decisions are in §21.2.

An editor mode inside `nvforever`: the player types `editor` in the console, the game freezes, and the
world, its records, its terrain, its navmeshes, its quests and its dialogue can be
inspected and changed in place, tested at once by unfreezing, and saved into an ESP or
ESM that the original game, the GECK and xEdit can open. Alongside it, a quest and
dialogue debugger that explains why a quest is stuck or why an NPC will not say a line.

This is NVForever's own design, not a behaviour of the original [design]. Facts about the
file formats are tagged as in the rest of the repo and cite the spec notes they come from;
anything not yet checked is a [hypothesis] and is listed in §21.

Contents:

1. Goals and non-goals
2. What the engine already has, what is missing
3. Architecture: the edit session
4. Writing plugins
5. The schema encoder
6. Editor mode: entering, camera, input, layout
7. Selecting and placing references
8. Editing records
9. Terrain
10. Navmeshes
11. Lighting, weather, image spaces, water
12. Scripts
13. Quest debugger
14. Dialogue debugger: "why won't this NPC say that?"
15. Dialogue authoring
16. Checks before saving (lint)
17. Undo, history and crash safety
18. Settings and controls
19. Testing
20. Phases and what done means for each
21. Open questions (facts to check, decisions for the owner)
22. Risks

---

## Handover (2026-11-06, fourth session; the earlier ones below)

**Start here.** `origin/main` merged again (406 commits, no conflicts). The dialogue
debugger's core is in (progress log, fourth session): `editor why [topic]` in editor
mode explains the selected actor's lines of a topic from the game's own verdicts. Next:
the quest view (§13.1-§13.2, `editor quest <quest>`), then the "what if" overrides
(§14.4) and the follow-up rule for chosen topics in the explainer. Back up with
`git push origin local/editor-encoder`; still no issue or PR.

## Handover (2026-11-03, third session; the second session's below)

**Start here.** `origin/main` is merged in (2026-11-03), and all 11 editor smoke scenarios
pass on it, `editor-find-place` for the first time. Clean masters and archives in
`mods\` are built (progress log, third session). The branch is backed up to GitHub as
`origin/local/editor-encoder` on the owner's request; still no issue or PR. Back it up
with `git push origin local/editor-encoder` (the branch's `pushRemote` is `no_push`).
Next open task: the quest and dialogue debugger (list below).

## Handover (2026-11-01, end of the second session)

The record picker's smoke `editor-find-place`, left unrun here, passed in the third session.

**Second session, in short** (details in the progress log, newest last):
- `56d53aa3` MO2 saving: into the owning mod, else overwrite; on in the MO2 profile.
- `33ccae44` the target plugin: `editor open`, `editor save` with no name, remembered in
  `[Editor] Target=`.
- `86d32752` mouse drag, End / `editor drop`, and an **engine fix**: SetPos on a single
  dynamic body (loose clutter) snapped back at once because the settle write-back undid
  it. `phys.rs` `repose_ref` with `RefRepose` from `script.rs` fixes it. Because it
  affects game scripts on `main` too, it is being ported to `main` with an issue and PR
  by the session "Port SetPos clutter-collision fix to main" (branch
  `claude/funny-yalow-018453`), engine code only, no editor code. When that merges, this
  branch's copy of the change in `phys.rs`, `script.rs` and `lib.rs` will conflict on
  the next merge of `main`; keep `main`'s version.
- Record picker (this handover's commit): `editor find`, `editor place #n | <Editor ID>`;
  actor bases refused, by FormID too.
- `knockdown-player` fails on this branch with and without the engine fix (each time a
  different get-up timing check). It is not caused here; left alone.

**Where everything is**
- Worktree `C:\Users\Loot\source\repos\NVForever-editor`, branch `local/editor-encoder`, on
  `origin/main` at b87893d4. Its `pushRemote` is `no_push`, so a push fails. **The owner keeps
  this off the repo**: no issue, no PR, no push until they say (memory `editor-workstream`).
- Build caches: `F:/nvf-target/editor-root` (root crates, fnvtool) and
  `F:/nvf-target/editor-engine` (engine). Build the engine with
  `cd engine && CARGO_TARGET_DIR=F:/nvf-target/editor-engine cargo build --release -p nvforever`.
- Commits (first session): `96569a33` encoder; `c1c6dd6d` mods-folder plugins + writer +
  editor mode; `37574624` delete/undo; `46a528e6` placing, INI lock, this doc;
  `3ff57a51` verified.
- The owner's decisions: §21.2. Read them before choosing behaviour.

**Verified at the end of the first session** (all six editor smoke scenarios pass:
`editor-save-write`, `editor-save-load`, `editor-delete-undo`, `editor-place-write`,
`editor-place-load`, `editor-locked`; `cargo test -p nvf-play editor`; engine clippy clean).
- Placing: `editor place <base>` / P put a new reference where the camera's ray hits, undo
  removes it, and a save writes it as the plugin's own (`0x0A000800` = ten masters, then the
  plugin). The next start finds it at the same place.
- The INI lock (`editor=1` under `[Settings]`; test aid `NVF_PLAY_TEST_EDITOR=1`).
- Scenario notes: the console names references with an Editor ID by that name, and prints
  `<ref>.<function> >> ran` for functions that return a form (so check numbers, not
  GetBaseObject). The smoke homes (`target/smoke/editor-home*`) persist between runs (hence
  `NVF_PLAY_TEST_EDITOR_REPLACE`).
- To rerun: build the engine (command above), then `python tools/smoke run --exe
  F:/nvf-target/editor-engine/release/nvforever.exe --data "F:/SteamLibrary/steamapps/common/Fallout
  New Vegas/Data" --area editor`.

**Open tasks, in the order I'd take them**
- [x] MO2 saving (§21.2 item 4, §4.1, §4.6; second session, see the progress log). Left
  for the owner: one save from a real MO2 start (sessions cannot start MO2), then check
  in MO2 that the plugin shows ticked in the right pane and the file is in the mod's
  folder (or in overwrite for a new one).
- [x] `editor open <plugin>` / choose the target plugin when editor mode starts (§6.1;
  console form, see the progress log). The "New plugin… / Open…" dialog waits for the UI
  toolkit (§8).
- [x] Mouse drag (§7.2; across, Shift for up and down, Ctrl for the grid) and snap to
  surface (End / `editor drop`); see the progress log. Still open from §7.2: drawn
  gizmo handles, rotating by mouse, local space, snap to reference, aligning to the
  surface normal, duplicate, physics drop, scale. The mouse itself has no test aid, so
  the real mouse drag is untried until the owner uses it. Static collision of a moved
  reference stays at its old place until its cell reloads (progress log).
- [x] A record picker for placing (console form: `editor find`, `editor place #n`; see
  the progress log; **its smoke run is still to do**, top of this section). Open: the
  visual list with type and plugin filters and the model preview (needs the UI toolkit,
  §8), and actor bases (needs the writer to write ACHR/ACRE).
- [x] Clean masters on save, and a `.bsa` beside a plugin in `mods\` mounted (third
  session, see the progress log).
- [ ] The quest and dialogue debugger (§13–§14). Dialogue core done in the fourth
  session (`editor why`); open: the quest view, history (§13.3), the stuck-quest helper,
  checkpoints, "what if" (§14.4).
- [ ] The record property panel (§8): needs a UI toolkit (bevy_egui; add it to
  docs/design/dependencies.md first).
- [ ] Terrain (§9) and navmesh (§10) editing: later phases.
- [ ] The facts to check in §21.1 (block numbering is no longer needed by the writer;
  item 10 is answered: plugins in `mods\` did not load and now do).

## Progress log

### 2026-11-01: phase 0 checks

Work is on the local-only branch `local/editor-encoder` in the worktree
`C:\Users\Loot\source\repos\NVForever-editor` (its `pushRemote` is `no_push`, so a push
fails). Commit `96569a33`. Build cache: `F:/nvf-target/editor-root`.

**Plugins from `<home>\mods\` (§21.1 item 10): they do not load** [observed: code].
`fnv_install::plugins::sync` lists plugins only from `game.data`
(`crates/fnv-install/src/plugins.rs:90`), and the engine opens them from Data. `mods\`
is used only for files that scripts write and read (`fnv_game::mod_files`, set up in
`nvf-play/src/script.rs` and `script_input.rs`). So §2.2 item 9 is real work: the plugin
list and the opener must also look in `mods\`, with a rule for a plugin of the same name in
both places (`mods\` should win, as it does for files).

**Schema encoder (§5): built, round trip over the ten masters**
(`fnv_esm::encode`: `encode_type`, `encode_sub`, `round_trip`; `fnvtool esm encode-check
<plugins> --schema schemas/fnv`, 6 s for all ten):

| Outcome | Subrecords |
|---|---|
| decoded, then encoded back byte for byte | 2,997,789 |
| not decoded by the schema (Unknown or Error, kept raw by the editor) | 58,142 |
| encoded to other bytes | 310 |
| refused | 0 |

- The 310 are TERM RNAM (309, Dead Money) and TERM ITXT (1, FalloutNV.esm): an empty
  string stored as four NUL bytes [observed: e.g. DeadMoney.esm 0x010138E9 RNAM
  `00000000`]. They decode as `""` and encode as one NUL. That's harmless for the
  editor, because unchanged fields keep their source bytes (§5.1). It's the "bytes after
  the NUL" loss the encoder's notes predicted.
- CSTY CSTD failed all 132 before the fix: its struct had six fields all named `unused`,
  so the decoded JSON object kept only the last one. The fields are numbered
  `unused_1`…`unused_6` on the local branch, and the schema loader now rejects a struct with
  a repeated field name. It was the only such struct in `schemas/fnv`. No code reads those
  fields (only named CSTD fields are used in `fnv-game`).
- Not checked here: floats that are NaN or infinite. None turned up refused, so no
  decoded float in the masters is NaN or infinite. Also unchecked: enums with two names for
  one number (none differed).
- Still to do for §5.3: the "edit one field of each type per record type" test, and the
  lab scenario (`L1.esm.encode.*`) when this goes public.

### 2026-11-01: plugins from mods, editor mode v0, the first plugin writer

On the same local branch, after the encoder commit:

- **Plugins from `<home>\mods\`** (§2.2 item 9): `fnv_install::plugins::present_plugins`
  lists `Data` and `mods\` together (the mods copy's name wins for a name in both),
  `plugin_path` opens the mods copy first, and `sync` reports `from_mods`. The engine's
  `world::open_plugins` takes the mods folder; setup logs `setup: plugins from <mods>: …`.
  A plugin's archives (`Name.bsa` beside it in `mods\`) are **not** mounted yet. Developer
  runs (`--data`) open from `mods\` too when a listed plugin is there.
- **`fnv_install::plugins::turn_on`**: turns a plugin on in the home's `plugins.txt`.
- **Writer v0** (`fnv_esm::edit::write_placements`, §4): placed references at new
  positions and rotations as overrides. The masters are every plugin before it. Each
  reference goes under the winning plugin's own group chain, labels remapped (so §21.1
  item 1, the block numbering, isn't needed). The WRLD and CELL owners are copied, and
  FormIDs are remapped through the encoder (`encode_type_mapped`). It also writes ONAM and
  the HEDR count. Synthetic tests cover a master, an add-on with a non-identity index, and
  re-saving into a loaded plugin. `fnvtool esm edit-move` writes such a plugin from the
  command line.
- **Editor mode v0** (§6–§7, `engine/nvf-play/src/editor.rs`): `editor` in the console
  switches it on and off. It uses the free camera in mode 1 (the world frozen), hides the
  HUD, and looks round while the right mouse button is held. Left click selects
  (`console::pick_under`, pulled out of the console's click) with a cyan box and a details
  panel. The arrows move along the world axis nearest the view, Page Up/Down move up and
  down, Ctrl+Left/Right turn, Alt gives fine steps; moves go through SetPos/SetAngle.
  `editor save <Name.esp>` writes the moved references into `<home>\mods\Name.esp`
  (keeping a `.bak`, reading the file back) and turns it on in `plugins.txt`.

**Checked 2026-11-01** (commit `c1c6dd6d` on `local/editor-encoder`):
- Smoke `editor-save-write` and `editor-save-load` (in the local branch's
  `tools/smoke/scenarios.toml`, home `target/smoke/editor-home`) both pass. A Goodsprings
  bottle (0x0010A202) is selected, moved 64 along X and turned 45° from the console,
  saved into `EditSmoke.esp` (1,136 bytes, 10 masters), and the next start finds it at
  x −68311.4, 222.6°.
- `fnvtool esm edit-move` on the masters (a FalloutNV.esm and a HonestHearts.esm
  reference): `esm verify` reads the output back byte for byte, with no warnings.
- Engine tests (nvf-play 711, nvforever 304) and clippy (root packages and the engine
  workspace) pass.
- Bug found on the first in-game run: the script frame swaps the run's world into the
  game only while scripts run, so outside it the live world is `SimState::world`. Editor
  mode had read `rt.game.world` and saved the plugin's own position. It's fixed.
- Test aids and console forms for scripted runs: `editor select|move|turn`,
  `NVF_PLAY_TEST_EDITOR_REPLACE=1`.

**Delete and undo** (commit `37574624`): Delete / `editor delete` disables the selection
in the run; when saved, it's Initially Disabled (flag 0x800 [documented]) at z −30000
[hypothesis: the community's sinking height]. Ctrl+Z / `editor undo` puts back moves,
turns and deletes. Smoke `editor-delete-undo` passes on 0x0010A206.
- Found on the way: a reference with an enable parent ignores its own enabled state
  (`World::is_enabled` follows the parent), and `Enable`/`Disable` do nothing to it.
  Undo therefore switches it back on directly with a `RefEnabled` event. The Goodsprings
  bottle 0x0010A202 follows a disabled parent, so it's never visible: fine for the
  writer tests, wrong for a delete test.
- Open question for §4.5: editor mode lets you delete a reference that follows an enable
  parent. In the run that does nothing visible, but the save writes it Initially Disabled
  and sunk. Whether to warn, or refuse, for such references is still to decide.

### 2026-11-01 (second session): saving under MO2

- `fnv_install::mo2::Layout` reads the instance's mods, overwrite and profile folders from
  `ModOrganizer.ini` (MO2's defaults when unset, which is how both of the owner's
  instances have it [observed]). `save_target` looks for the plugin in overwrite first,
  then in the enabled mods by `modlist.txt` priority, matching names without case and
  keeping the file's own spelling; otherwise it picks a new file in overwrite. `turn_on`
  adds the plugin to the profile's `plugins.txt` (ANSI, CRLF) and `loadorder.txt`.
- `editor.rs` `Target`: under MO2 the save goes there and the `.bak` goes to
  `<home>\editor-backups\`. Without MO2 nothing changes.
- Test aid `NVF_PLAY_TEST_EDITOR_MO2=<instance folder>`, because sessions cannot start
  MO2. Smoke `editor-save-mo2` passes: the plugin is written to `overwrite\` and listed
  in `profiles\Smoke\plugins.txt`. The other six editor scenarios still pass.
- Bug found on the first run: a relative instance folder was doubled
  (`editor-mo2\editor-mo2\overwrite`), because `%BASE_DIR%` already held it. The instance
  is now made absolute first, and the scenario's pattern is strict enough to catch it.
- Not checked: a save from a real MO2 start, and whether MO2 keeps the `plugins.txt` line
  when it refreshes after the game closes [hypothesis].

### 2026-11-01 (second session): the target plugin

- `editor open` lists what it could open: the loaded `.esp` files and the plugins in
  `<home>\mods\`, each with its reference count, or "not yet" when it holds other
  records. `editor open <Name.esp>` checks the plugin as a save would and makes it the
  target. `editor save` without a name writes into the target, and a save with a name
  makes that plugin the target. The panel shows the target.
- The last target is kept as `Target=` under `[Editor]` in the home's `NVForever.ini`.
  The first `editor` of the next run opens it again and says so, or says why it can't.
- Smoke `editor-open-write` (lists the plugins, refuses FalloutNV.esm, opens a new
  plugin, saves with no name) and `editor-open-last` (the next run reopens it and saves
  into it).

### 2026-11-01 (second session): drag and set down

- **Drag**: a left press on a reference selects it. Moving the pointer 4 pixels with the
  button held starts a drag: the reference slides on the horizontal plane at its own
  height, keeping the grabbed point under the pointer. Shift at the press moves it on
  the upright plane through it that faces the camera, so up and down only. Ctrl snaps
  the new place to the 16-unit grid. A whole drag is one undo step. A click without a
  drag on the selection unselects it, as before. The plane maths (`ray_at_height`,
  `ray_at_upright`, `drag_place`) has unit tests; a point past reach or behind the eye
  is ignored.
- **Set down** (snap to surface): End or `editor drop` casts a ray straight down from
  the bottom of the selection's drawn bounds, leaving its own collision out, and moves it
  so that bottom rests on the hit. One undo step. The log names what it landed on.
- Test form `editor drag <x0> <y0> <x1> <y1>` (fractions of the screen) puts the
  selection through the same maths, because the mouse has no test aid. Smoke
  `editor-drag-drop` places a bottle, raises it 64, sets it down, undoes that, then
  drags it.
- **Engine bug found and fixed on the way** (`phys.rs` `repose_ref`, `RefRepose` from
  `script.rs`): SetPos/SetAngle on an object whose collision is one dynamic body (loose
  clutter, a placed or dropped item) did not move the body, and the settle write-back
  copied the body's old pose back over the move straight away. The raised bottle read
  `GetPos Z 8396.80` after `SetPos Z 8461`, and every editor move of clutter would have
  snapped back. Now any reference a script moved, turned or resized has that single
  dynamic body rebuilt at the simulation's pose when the two differ. This holds for
  game scripts too, not only editor mode. A second cause: console lines run before the
  tick, which clears `Game::turned` first, so the console's moves are collected before
  the tick. Not covered: collision that isn't one dynamic body (static or multi-part)
  stays at the old place until its cell loads again, though it is drawn at the new one.
- Checks for that change: the `nvf-play` tests (716), engine clippy, and the 29 smoke
  scenarios that watch `phys.rs`/`script.rs`/`lib.rs`, plus the editor ones: 37 of 38
  pass. `knockdown-player` fails with and without the change (twice with it, once
  without, each time on a different timing check of the get-up). The rebuild never
  fired in its runs, so it is not this change. Left for whoever owns knockdowns.

### 2026-11-01 (second session): the record picker

- `editor find <words>` lists up to 20 placeable base records whose name or Editor ID
  holds every word, using the console's record index (the one `help` uses, built on a
  thread when the console opens, under a second). Named records come first. `editor
  place #<n>` places the n-th, `editor place <Editor ID>` one by its exact Editor ID, and
  `editor place <FormID>` works as before.
- Only §7.3's types less the actors can be placed (`PLACEABLE`): NPC_, CREA, LVLN and
  LVLC are refused, by FormID too. Before this, `editor place` took any base, and the
  writer would have saved an actor as a REFR, which breaks the compatibility line
  (§21.2 item 2). Actors need the writer to write ACHR/ACRE first.
- Smoke `editor-find-place` (written, not run yet: the build was stopped for the
  handover). Unit tests: `picker_finds_placeable_records_only`, `place_words`.

**Next** (phase 3 on): place new references from a picker (needs new FormIDs and
next object ID upkeep), delete as disable-and-sink, undo, a gizmo drag instead of keys,
`editor open <plugin>` to pick the target at the start, the record property panel
(§8, needs a UI toolkit: bevy_egui), then the quest and dialogue debugger (§13–§14,
which needs no writer and could start any time).

### 2026-11-03 (third session): main merged, clean masters, archives in `mods\`

- `origin/main` merged (437 commits). The SetPos clutter fix had landed on `main` as
  `c1e38465` with the same code, so `phys.rs` took `main`'s copy; `console.rs`
  (`bat` beside `editor`), `lib.rs` and `scenarios.toml` kept both sides.
- **Clean masters** (§4.2): `fnv_esm::edit::write_plugin` writes in two passes. The first
  decodes every copied record and notes which masters the file's FormIDs (record
  headers, group labels, subrecords) belong to; the second writes with only those, in
  load order, and renumbers. When a copied subrecord does not decode exactly, its
  FormIDs are unknown and every master stays (unit test
  `keeps_every_master_when_a_subrecord_is_not_decoded`). `Written::new_load_ids` gives
  the new references' load-order FormIDs (what the console takes), which the save line
  now prints; `new_ids` stays in the file's numbering.
- **Archives beside a plugin in `mods\`** (§2.2 item 9, second half):
  `fnv_vfs::Vfs::archive_order_in` also looks in the mods folder for `<stem>.bsa` and
  `<stem> - *.bsa`, names them by full path, and prefers the mods folder's copy of a name
  in both (as `plugin_path` does for plugins); the base list and `Update.bsa` stay
  Data's. The engine passes the mods folder (`GameData::open_for_plugins_in`). Unit test
  `mods_folder_archives_follow_their_plugins`; no smoke, since an archive cannot be
  committed.

### 2026-11-06 (fourth session): main merged, the dialogue debugger's core

- `origin/main` merged (406 commits, no conflicts; `tools/smoke verify` fine).
- **One evaluation for the game and the debugger** (§14.3). `Game::eval_conditions`
  returns each condition's value, operand, result and run-on reference
  (`cond::CondResult`); `conditions_pass` is `cond::combine_results` over it, and
  `explain_conditions` (runner and `run_info` diagnostics) formats it, so it now sets the
  condition target as the game does (before, it had its own copy of the loop without it).
- **Line verdicts**: `Game::line_verdict` holds the rules `can_say_as` used and returns
  the line's conditions or the first rule that blocks it (`dialogue::Blocked`: Say Once
  said, intelligence, Say Once a Day with the hours, fixed speaker, quest not running,
  quest conditions, voice type without Allow Default Dialog, the line's conditions);
  `can_say_as` is its `is_ok`. The grouping of plain lines and Random pools is one
  function (`dialogue::group_end`) for `select_among` and the explainer.
- **`fnv_game::debugger`**: `Game::explain_topic` judges every line of a topic in the
  order the game tries them and gives the line (or Random pool) the game takes now;
  `explain_topic_text` makes console lines. It follows `select_line` (greetings, SayTo,
  Say); the chosen-topic rule that passes over follow-up lines is not shown yet.
  Evaluating runs the condition functions, so a GetRandomPercent condition draws here
  too [design: read-only otherwise].
- **`editor why [<topic>]`** (console, editor mode only): the selected reference (the
  editor's selection, else the console's) as speaker, the player as listener, GREETING
  when no topic is named; a topic by Editor ID or FormID. By default it lists only the
  lines written for the speaker (`Game::line_fits`: fixed speaker, voice-type gate and
  the "who says it" conditions of the line and its quest allow it; display only):
  Sunny's GREETING goes from 5584 lines to 67. `editor why <topic> all` lists every line.
- Tests: `the_debugger_agrees_with_the_line_the_game_takes` (every topic of the dialogue
  fixture, three gate values: the debugger's winners hold the line `select_line` took),
  `the_debugger_shows_pools_and_why_a_line_is_held_back`; smoke `editor-why`.

## 1. Goals and non-goals

### 1.1 Goals

- **Edit while playing.** One key toggles editor mode; leaving it resumes the same run
  with the changes in effect. No restart, no separate tool.
- **Real plugins.** Changes are saved as an ESP (or ESM) in the format the masters use,
  so that the GECK and xEdit open it and the original game plays it. The editor is a way
  to make mods, not an NVForever-only format.
- **Test as you build.** Dialogue, quests, scripts, placement and navmeshes are tried in
  the running game the moment they change. This is the main thing the GECK cannot do
  (its loop is: save, launch the game, load, walk back, test, quit, edit).
- **Explain, don't just show.** The debuggers say why: which condition failed, which line
  won and why the others lost, which script set a stage.
- **Never lose work, never damage files.** Every edit can be undone, the session survives
  a crash, a save never overwrites a plugin without a backup, and the masters and the game
  folder are never written (charter: NVForever never writes into the game folder).

### 1.2 Non-goals

- Editing models (NIF), textures (DDS), sounds or animations. The editor places and
  configures them; it does not author them.
- Generating the original game's distant LOD (object LOD meshes, terrain LOD textures)
  for the plugins it writes. NVForever generates its own LOD at run time (generated-lod
  design); for the original game the editor warns and points at the community tools.
- Replacing xEdit's conflict resolution or the GECK's every window. The editor covers what
  is best done in the world; anything else stays in the existing tools.
- Reading the GECK, xEdit or any other editor's source. Behaviour comes from the spec
  notes, the documentation they cite, and observation (charter).

---

## 2. What the engine already has, what is missing

### 2.1 Already built

| Piece | Where | What it gives the editor |
|---|---|---|
| Lossless plugin container: records, groups, subrecords, compression, XXXX | `crates/fnv-esm/src/raw.rs` (`write_subrecords`, `write_nodes`), `plugin.rs` (`Plugin::to_bytes`) | Saving: every shipped plugin reads and writes back byte for byte (lab `L1.esm.roundtrip.*`) [observed: status.md, layer1.md] |
| Schema: 106 of 107 master record types decoded into named, typed fields | `schemas/fnv/*.json`, `fnv_esm::decode::decode_record` | Field names, types, enums, flags and FormID targets for every record: the property panel's data (§8). NAVI is read by hand in `fnv-nav` [observed: status.md] |
| Load order, plugin header, masters, ONAM | `fnv_esm::loadorder`, `plugin.rs` (`PluginHeader`) | The edit plugin's place in the order; the header to write |
| xEdit cross-check of every record | lab, status.md | A second reader to validate what the editor writes |
| Landscape decoding | `crates/fnv-world/src/land.rs` | Heights, normals, colours, layers in, so terrain edits start from the same model the renderer draws |
| Navmesh records and graph | `crates/fnv-nav` (`record.rs`, `graph.rs`, `doors.rs`, `obstacles.rs`) | Triangles, edge and door links; the graph rebuilt per navmesh (EnableNavMesh already swaps navmeshes in and out at run time) |
| Collision geometry | `crates/fnv-collide`, `fnv-phys` | Raycasts for selection and snapping; input for navmesh generation |
| Script compiler, byte-identical on 14,489 of 14,511 vanilla scripts | `fnv_script::compile`, `fnvtool script compile` | Script editing that saves source and compiled bytes together (§12) |
| Conditions | `crates/fnv-game/src/cond.rs` (`Condition`, `Cmp`, `RunOn`, `combine`) | The dialogue debugger's per-condition results (§14) |
| Dialogue selection | `crates/fnv-game/src/dialogue.rs` (`can_say`, `select_line`, `build_choices`, `start_dialogue`) | The exact point where a line is chosen; the debugger hooks here |
| Quests and quest targets | `crates/fnv-game/src/quest.rs`, `quest_targets.rs`; console `sqv`, `ShowQuests` | The quest debugger's data (§13) |
| Script and dialogue trace | `NVF_PLAY_SCRIPT_TRACE`, `dialogue:` log lines | Stage and variable history (§13.3) |
| Console selection and record index | console design (`help`, picked reference with box and details) | Picking, searching records by Editor ID and name |
| No-clip flight | console `tcl` (`player::NoClip`) | The editor camera's movement model |
| Saves, including load into another load order | `crates/fnv-save`, save-load design | Checkpoints and roll back (§13.5); saves still loading after the edit plugin changes |
| Plugin list and home folders | `<home>\plugins.txt`, `<home>\mods\` (install-layout.md) | Where edit plugins live and how they join the order |
| Settings and controls in `NVForever.ini` | `[Settings]`, `[Controls]` | The editor's key and switches (§18) |

### 2.2 Missing (the work)

1. **Schema encoder**: decoded fields back to subrecord bytes (§5). Everything that edits
   a record goes through it.
2. **Edit plugin model and writer**: header upkeep, FormID allocation, group placement,
   overrides, deletions (§4).
3. **Live overlay**: the running game reads the edit plugin's records over the load order
   and reloads only what changed (§3).
4. **Editor UI**: a toolkit for panels (none in the engine today; `bevy_egui` is the
   obvious choice, §6.4), gizmos, a picker.
5. **Terrain writer**: VHGT/VNML/VCLR/ATXT/VTXT re-encoding and neighbour seams (§9).
6. **Navmesh writer**: NAVM encoding, NAVI upkeep, link repair; later generation (§10).
7. **Per-condition results** from the condition checker (§14).
8. **Dialogue authoring model**: topics, lines, links, order, voice files (§15).
9. **Plugins loaded from `<home>\mods\`**: install-layout.md says mods are layered over
   Data by the VFS, "planned with Layer 2/3"; whether loose plugins in `mods\` already
   join the load order has to be checked before §4.6 can rely on it [hypothesis].

---

## 3. Architecture: the edit session

### 3.1 The edit plugin

An **edit session** has exactly one **target plugin**: either a new file, or an existing
ESP/ESM the user opens for editing. In memory it is an `EditPlugin`:

- the plugin header (`PluginHeader`: flags, masters, author, description, next object
  ID, ONAM);
- its records by FormID, each kept as decoded fields (the schema's JSON value) plus the
  raw record it came from, if any (so untouched records write back byte for byte);
- its group tree position for each record (§4.4), computed, not stored by hand;
- a dirty set (records changed since the last save);
- the command history (§17).

The target plugin is the **last plugin in the load order** for the session: its records
win every conflict, as the last-loaded plugin's do [documented: esm.md, load order;
the engine's own rule in `fnv_esm::loadorder`]. If the user opens a plugin that has later
plugins after it in `plugins.txt`, the editor says so: those later plugins would win over
the edits in a normal run, and the editor offers to (a) edit with the later plugins
unloaded, (b) show the edits as they would play (later plugins winning), or (c) cancel.

### 3.2 The overlay

The game reads records through the load order. The editor adds an overlay layer:

```
winning record(FormID) = edit plugin's record, if it has one
                         else the load order's winner, as today
```

The overlay is a single lookup in front of the existing winner lookup, so every system
(`fnv-world`'s index, `fnv-game`'s data tables, `fnv-nav`, scripts) sees edited records
without knowing about the editor. Systems that cache derived data from records (cell
contents, navmesh graphs, dialogue tables, script tables, actor templates) register a
**refresh handler** for the record types they derive from.

### 3.3 Refresh, not reload

After each command, the session lists what it touched (record FormIDs and types, cells,
worldspace grid squares) and calls the refresh handlers:

| Change | Refresh |
|---|---|
| A reference's position, rotation, scale, flags, base | despawn and respawn that reference's entity; physics body rebuilt; if it is flagged Obstacle (navmesh.md §4c), the navmesh cut updated |
| A reference added or deleted | spawn or despawn it; cell contents index updated |
| A base record (STAT, MSTT, FURN, DOOR, LIGH, CONT, WEAP, ARMO …) | respawn every loaded reference of that base; inventories holding it re-read on next open |
| An NPC/creature record | respawn loaded actors of that base (their AI and inventory restart: said in the UI) |
| LAND | that cell's terrain mesh and collision rebuilt; its grass regrown; the neighbours' seams if shared edges changed; generated distant terrain for the grid square marked stale and rebuilt in the background |
| NAVM / NAVI | that navmesh's graph part rebuilt (as EnableNavMesh does today); walkers on it plan again |
| CELL lighting, WTHR, IMGS, CLMT, WATR | the renderer's current values re-read |
| DIAL / INFO / QUST | dialogue and quest tables rebuilt for those records; an open conversation offered its choices again |
| SCPT or a record's embedded script | recompiled; running instances keep their variables where names and types still match (§12.4) |
| GMST, GLOB | re-read; GLOB values in the running game are only changed if the user asks (§3.4) |
| Anything else | a "needs reload" mark: the change is saved, and the session offers a full reload (§3.6) |

### 3.4 Plugin state versus game state

The running game keeps state that differs from the plugins: a reference the player
knocked over, an NPC that walked away, a door opened, a quest at stage 40, a global
changed by a script. The save system already separates the two (form changes in the save,
base values in the plugins; save-load design).

The editor always **shows and edits plugin values**. For each edited object the panel also
shows the live value where it differs ("plugin: (100, 200, 0); in this run: (130, 190, 0),
moved by physics"). On leaving editor mode, each edited object is offered one of:

- **Reset to plugin**: the run's changes to the object are dropped and it takes the new
  plugin values (default for references the editor moved, added or changed).
- **Keep run state**: the run's change stays; the plugin edit applies from the next new
  game or the next time the object resets.

A **"Reset cell"** command resets every reference of the current cell to its plugin
state (the same as the cell reset the game does after its timer, [hypothesis] about the
original's exact rules; ours is a direct reset).

### 3.5 Freezing the simulation

In editor mode the game clock, AI, physics, scripts and timers stop (the same freeze the
pause menu uses). Three exceptions, each a toggle on the toolbar:

- **Preview animation**: idles and animated statics keep playing so placement can be
  judged.
- **Preview time of day**: a slider moves the game hour for lighting only (the clock is
  restored on leaving).
- **Live mode**: the simulation runs while the editor panels stay open. Needed for the
  quest and dialogue debuggers (§13–§15), where the point is to watch the game react.

### 3.6 Full reload

Some changes cannot be refreshed in place (a master added to the edit plugin, a record
type with no refresh handler, a race or body change on the player). A full reload:

1. saves the edit plugin to its session file (§17.3) and makes an in-memory save of the
   run;
2. rebuilds the load order with the edit plugin as its last plugin;
3. loads the in-memory save into it (the save loader already handles a changed load
   order: FormIDs follow their plugin by name, save-load.md);
4. returns to editor mode with the selection and camera as they were.

---

## 4. Writing plugins

### 4.1 Where files go

- New plugins and edited copies are written to `<home>\mods\` (install-layout.md). The
  game's `Data` folder and `Documents\My Games\FalloutNV` are never written (charter,
  `crates/fnv-install`).
- Opening a plugin from `Data` for editing makes the target a copy in `<home>\mods\`
  with the same name; the original in `Data` is untouched. The UI says this plainly.
- To play the plugin in the original game, the user copies it into `Data` themselves.
  The editor can open the folder in Explorer; it never copies.
- Under Mod Organizer 2 (owner 2026-11-01, §21.2 item 4; built: `fnv_install::mo2::Layout`,
  `editor.rs` `Target`): when MO2 started the game, a save goes into the real file of
  the enabled mod that gives `Data` the plugin (MO2's overwrite folder first, then the
  mods from the highest priority down, by the profile's `modlist.txt`), else into a new
  file in the overwrite folder. The folders come from the instance's `ModOrganizer.ini`
  (`mod_directory`, `overwrite_directory`, `profiles_directory`, `base_directory` under
  `[Settings]`, MO2's defaults beside the INI when unset [documented: MO2 `PathSettings`])
  and the profile from `selected_profile`. The file a save replaces is kept in
  `<home>\editor-backups\`, so no mod folder gains a stray file. A plugin that only the
  game folder has is never written there (`guard_write`); its save is a copy in
  overwrite, which shadows it as MO2 does [hypothesis].

### 4.2 Header

| Field | Rule |
|---|---|
| Record flags | ESM flag set when the user saves as master (the file's extension follows: `.esm`) |
| HEDR version | the highest version among its masters (1.32–1.34 across the shipped masters [observed: plugin.rs]) |
| HEDR record count | recomputed on every save: records + groups − 1, as every shipped master has it [observed: plugin.rs doc comment] |
| HEDR next object ID | above the highest object ID the plugin has used (§4.3) |
| CNAM author, SNAM description | from the save dialog; remembered per user |
| MAST/DATA | every plugin whose records the edit plugin overrides or references, in load order; DATA written as 0, as in every shipped master [observed: plugin.rs] |
| ONAM | the overridden master records that sit in cell temporary-children groups (type 9), the set every shipped DLC lists exactly [observed: esm.md §ONAM]; why the original needs it is a [hypothesis] (esm.md) |

Masters are added automatically when an edit first references a record from a plugin not
yet a master, and removed on save when nothing references them any more ("clean
masters"). Adding a master mid-session needs a full reload (§3.6) only if FormID indexes
in already-written records must be renumbered; the edit plugin keeps FormIDs in their
load-order form in memory and renumbers to the file's master list when writing.

### 4.3 FormIDs

- A new record gets FormID `(index of the plugin in its own master list) << 24 | object
  ID`, where the index is the number of masters (a plugin's own records use the index
  after its masters [documented: esm.md, FormID high byte]).
- Object IDs start at the header's next object ID and count up; never reused within a
  session, even after an undo, so that a crash recovery or an outside copy can't collide.
- Editor IDs: required for base records, optional for references. The editor proposes
  one (`<prefix><type><n>`, the prefix per plugin, set in the save dialog) and refuses
  duplicates across the whole load order.
- **Renumbering on save** is never done for records already saved once: saves and other
  plugins may reference them.

### 4.4 Group placement

The writer places each record in the group the masters use for it [documented: esm.md
group types]:

| Record | Group |
|---|---|
| Base records | top group of their type |
| Interior CELL | block (type 2) and sub-block (type 3) by the cell's FormID, then the cell's children |
| Exterior CELL | its worldspace's children (type 1), exterior block (type 4) and sub-block (type 5) by grid (two i16, y then x), then the cell's children |
| Persistent references | the cell's persistent children (type 8); persistent exterior references in the worldspace's persistent cell |
| Temporary references, LAND, NAVM, PGRE etc. | the cell's temporary children (type 9) |
| DIAL and INFO | the topic in the DIAL top group, its lines in the topic's children (type 7) |

The block and sub-block numbers for exterior cells come from the grid; the rule is in
esm.md and has to be confirmed against every shipped exterior cell before the writer
relies on it (a unit test over the masters: recompute every cell's block and sub-block
and compare) [hypothesis until that test passes].

A reference moved across a cell border in the editor **changes cell**: it is written in
the new cell's children. For an override of a master reference this means the override
sits in a different cell than the master's record; how the original treats that is an
open question (§21); the safe default is to keep master references in their original cell
record and only write the new position (the engine places a reference by its position,
not its cell, for exteriors [hypothesis: to be checked in ours and the original]).

### 4.5 Overrides and deletions

- **Override**: the first edit of a record from another plugin copies the winning
  record into the edit plugin, then edits the copy. An override carries the whole record,
  not a diff (the format has no diffs).
- **Override of a reference**: its cell record is overridden too (a reference's group
  sits under its cell), as a copy with no changes unless the user changed the cell.
- **Deleting a record the edit plugin added**: removed from the file.
- **Deleting a master reference**: the format has a Deleted flag (record flag 0x20
  [documented: esm.md]). The community's long-standing advice is not to delete placed
  references from masters but to mark them Initially Disabled and move them far below the
  ground, because deleted references are said to crash the original game when another
  plugin or a save refers to them [hypothesis: community practice, to be cited from the
  xEdit documentation before relying on it]. The editor does the disable-and-sink by
  default and offers the Deleted flag only behind a warning.
- **Deleting a base record from a master**: refused; the editor offers "remove all
  placed references of it" instead.
- **Reverting**: "revert to master" drops the override from the edit plugin.

### 4.6 Joining the load order

On first save of a new plugin, the editor adds it to `<home>\plugins.txt` (active, after
every other plugin) when the user agrees (a checkbox in the save dialog, on by default).
Under MO2, the home's `plugins.txt` is not read (install-layout.md); the editor turns
the plugin on in the MO2 profile instead: a line at the end of its `plugins.txt` (ANSI,
CRLF, as MO2 writes it) and of its `loadorder.txt` when it has one. That MO2 keeps the
line when it refreshes after the game closes is a [hypothesis] (observer check below).

Prerequisite: the engine must load plugins from `<home>\mods\` (§2.2 item 9).

### 4.7 Saving

1. Run the lint checks (§16); errors stop the save, warnings are listed.
2. Encode every dirty record (§5); untouched records keep their original bytes.
3. Build the group tree (§4.4), header (§4.2).
4. Write to `<name>.tmp` beside the target, flush, then rename over the target; the
   previous file is kept as `<name>.bak` (one level), plus a timestamped copy in
   `<home>\mods\.editor-backups\` (last 10 per plugin).
5. Read the written file back through `fnv-esm` and compare record by record with the
   in-memory plugin; a mismatch restores the backup and reports the record.
6. Compression: records the source had compressed stay compressed; new records are written
   uncompressed (the format allows either per record [documented: esm.md]).

### 4.8 Compatibility with the original game, the GECK and xEdit

Goal: a plugin written by the editor opens in xEdit with no errors in its "Check for
Errors", in the GECK without warnings that a hand-made plugin would not also give, and
plays in the original game.

Checked by:

- our own reader (round trip, §4.7 step 5);
- the existing xEdit cross-check tooling, run on the written plugin;
- owner checks in the GECK and the original game for each phase's sample plugin (observer
  checks, §19.4).

---

## 5. The schema encoder

### 5.1 What it does

`fnv_esm::encode` (new): the reverse of `decode`. Input: a record signature and the
decoded fields (the JSON value `decode_record` produces); output: the subrecords, in the
order the schema lists them, ready for `write_subrecords`.

Rules:

- Every field type the decoder reads, the encoder writes: integers of every width,
  floats, FormIDs (with the master index rewritten, §4.2), strings (zero-terminated,
  CP-1252 via `fnv_util::cp1252`; a character outside CP-1252 is refused, not replaced),
  enums by name or number, flags by names or bits, arrays, structs, unions, optional
  trailing fields.
- **Byte-exact for unchanged values**: encoding a decoded record gives the same bytes. Two
  sources of difference have to be handled:
  - floats: write the exact bits that were read (keep the raw bits alongside the decoded
    number until the user edits the field);
  - unused or padding bytes, and trailing bytes the schema does not cover: kept from the
    source record and written back unchanged.
- Subrecord order: the schema's order for new subrecords; the source record's order for
  existing ones (the shipped records follow xEdit's order [observed: dialogue.md §2.1]).
- XXXX for subrecords over 65,535 bytes, as `raw` already writes.

### 5.2 Records not covered by the schema

- NAVI and NAVM are read by hand in `fnv-nav`; their encoders live there too (§10).
- Condition blocks and scripts embedded in records use the common definitions in
  `schemas/fnv/common.json`; the encoder covers them like any other struct.
- Any subrecord the schema does not decode is kept as raw bytes and shown read-only in
  the property panel ("not decoded").

### 5.3 Done when

A test encodes every record of the ten masters from its decoded form and gets the source
bytes back (lab scenario `L1.esm.encode.<master>`, the encoder's twin of
`L1.esm.roundtrip`). A second test edits one field of each type per record type and checks
the decoder reads back the new value.

---

## 6. Editor mode: entering, camera, input, layout

### 6.1 Entering and leaving

- A key (`[Controls] Editor`, default to be chosen, §18) or the console command `editor`.
- On entering: the simulation freezes (§3.5), the HUD hides, the mouse pointer shows, the
  camera detaches at the player's eye.
- On first entering in a run, the target plugin is chosen: "New plugin…", "Open…" (a list
  of the load order's plugins and `<home>\mods\`), or the last one used. Built for now
  as console commands: `editor open` lists, `editor open <Name.esp>` picks, and the last
  one used (`[Editor] Target=` in `NVForever.ini`) is opened on the first entry.
- On leaving: the reset choices (§3.4), unsaved changes remain in the session (saved to
  the session file, §17.3; nothing is lost by leaving without saving), the camera returns
  to the player.
- Optionally **move the player to the camera** on leaving (a toggle), for testing a spot
  just built.

### 6.2 Camera

- Free flight as `tcl` flies (forward/back along the view, strafe level), with speed on
  the mouse wheel and a fast key; right mouse held to look, so the left button stays for
  selecting.
- **Orbit** around the selection (Alt + drag), **frame selection** (F), **top view** for
  terrain and navmeshes (an orthographic camera looking down).
- Camera bookmarks per plugin (numbered keys), kept in the session.
- Through walls: the camera ignores collision; interiors are drawn whole in editor mode
  (the room and portal culling is off, status.md "interiors draw only the rooms seen
  through portals"), with a toggle to see what the player would see.

### 6.3 Layout

```
+---------------------------------------------------------------+
| toolbar: mode (Select | Place | Terrain | Navmesh | Light |    |
|          Quest | Dialogue), snap, gizmo space, live mode, save |
+-----------+-------------------------------------+-------------+
| outliner  |                                     | properties  |
| (cell     |            3D view                  | (selected   |
|  contents,|                                     |  record's   |
|  search)  |                                     |  fields)    |
+-----------+-------------------------------------+-------------+
| bottom: record picker | history | lint | log                   |
+---------------------------------------------------------------+
```

Panels dock, collapse and remember their sizes per user (in `NVForever.ini` or a
`<home>\editor.toml`; §18).

### 6.4 Toolkit

- Panels: `bevy_egui` (immediate-mode, widely used with Bevy, MIT/Apache licences;
  dependencies.md would get a row). The game's own menus stay as they are; the editor is a
  developer surface, not a styled game menu.
- Gizmos: Bevy's own gizmo drawing for lines and boxes; a transform gizmo crate
  (`transform-gizmo-bevy` or similar) for move/rotate/scale handles, or our own if the
  crate does not fit the engine's Bevy version.
- Text editing for scripts and dialogue: egui's multi-line editor with our own
  highlighting from the script tokenizer (§12.2).

---

## 7. Selecting and placing references

### 7.1 Selecting

- Click: a ray from the camera through the pointer against the render meshes' bounds and
  then the collision shapes (the console's pick already does this; reuse it), nearest
  hit. Markers (XMarker, map markers, sound markers, light bulbs, patrol points,
  navmesh door markers) are drawn as editor icons and pickable even though the game hides
  them.
- Shift-click adds, Ctrl-click toggles, drag a box to select by screen rectangle.
- The outliner lists the cell's references grouped by type, with search by Editor ID,
  base Editor ID, name, FormID; double-click frames the camera on it.
- **Select by**: same base, same cell, linked references (XLKR), enable parent's
  children, references inside the selection's bounds.
- Hidden from selection: a lock toggle per record type (e.g. lock statics while placing
  clutter), and a lock per reference.

### 7.2 Moving, rotating, scaling

- Gizmo with three modes (W/E/R), in world or local space.
- **Snapping**: grid (units configurable, default 16 as a power of two common in modular
  kits [design]), angle (default 15°), and **snap to surface**: the object's bounds
  dropped along −Z (or along the view ray) onto the collision below, optionally aligned
  to the surface normal.
- **Snap to reference**: modular pieces snap by their bounds' corners to the nearest
  piece's corners (no snap points in the data; corners only [design]).
- Numeric entry in the properties panel (position, rotation in degrees, scale).
- Duplicate (Ctrl+D) places a copy at the same spot, selected, ready to move.
- **Physics drop**: for clutter, a "drop" command runs the physics on the selection only
  until it settles, then writes the resting position into the plugin.
- Scale limits as the format stores them (XSCL float [documented: world.md]); the editor
  warns above 10 and below 0.01 [design].

### 7.3 Placing

- The **record picker**: every placeable base record (STAT, MSTT, SCOL, FURN, DOOR,
  CONT, LIGH, ACTI, TACT, TERM, MISC, WEAP, ARMO, AMMO, ALCH, BOOK, NOTE, KEYM, IDLM,
  NPC_, CREA, LVLN, LVLC, GRAS excluded (grass grows), TREE, ADDN, PROJ excluded, SOUN for
  sound markers, plus the marker statics), searchable, filtered by type and plugin, with
  a turntable preview of the model.
- Click to place at the pointer's surface hit; the placed reference stays attached to the
  pointer until a second click, rotating with the wheel.
- **Brush placement** for clutter and rocks: paint a radius, density, random rotation and
  scale range, align to slope; each placed reference is a separate REFR (no batching in
  the format).
- New references get the cell they land in by their position (exteriors) or the current
  cell (interiors).

### 7.4 Reference properties

The properties panel for a REFR/ACHR/ACRE/PGRE/PMIS shows its subrecords from the schema
(world.json REFR lists them: base NAME, teleport XTEL, map marker XMRK, lock XLOC,
ownership, enable parent, linked reference, radiation, charge, ammo, lit water, open by
default, navmesh door link XNDP, portals, rooms, occlusion planes, emittance,
position/rotation DATA … [documented: schemas/fnv/world.json]), with purpose-built
widgets for the ones best set in the world:

- **Teleport door pairs**: pick the destination door in the world (the editor travels to
  it), both doors' XTEL written together, the arrival marker placed by dragging.
- **Enable parent**: pick a parent; the children of a parent are highlighted when it is
  selected; "opposite of parent" toggle.
- **Linked reference**: pick; the link drawn as a line.
- **Ownership**: NPC or faction picker, rank.
- **Lock**: level, key picker, leveled lock flag.
- **Map marker**: name, type icon, flags, reputation; shown on the Pip-Boy map preview.
- **Patrol points**: idle, topic; the patrol chain drawn as lines in the world.
- **Rooms, portals, occlusion planes, multibounds**: drawn as translucent boxes and
  planes; edited by handles. (The engine uses rooms and portals, not occlusion planes or
  multibounds yet: status.md; the editor still writes them correctly.)

### 7.5 Interiors and new cells

- New interior cell: name, Editor ID, lighting (XCLL) or light template (LTMP),
  music, acoustic space, image space, water; then a doorway pair to an existing cell.
- New exterior cells come into being when something is placed in a grid square that has
  no cell record in the load order for that worldspace (rare in the base worldspaces, all
  squares inside their bounds have cells [hypothesis: to check over WastelandNV]).
- New worldspaces: out of scope until phase 7 (heightmap, LOD, map, climate are each a
  project).

---

## 8. Editing records

### 8.1 The generic property panel

For any record of the 106 decoded types, a form built from the schema:

- one row per field, labelled by the schema's name, typed widget per primitive:
  number fields with range checks where the schema has them, enum dropdowns, flag
  checkbox groups, string fields (multi-line for long text), FormID pickers filtered by
  the allowed target types, arrays with add/remove/reorder, structs nested;
- the winning plugin and every plugin that defines the record (the conflict list), with
  each plugin's value per field coloured where they differ (xEdit's view, done by our
  own decoded values);
- "revert field", "revert record", "copy as override into the edit plugin";
- references to this record ("used by"): computed from the FormID index the lab's
  coverage tool already builds (`decode::...` `on_formid` hooks).

### 8.2 Purpose-built editors (on top of the generic panel)

| Records | Extra |
|---|---|
| NPC_, CREA | live preview of the actor in a turntable; inventory list; packages list with the package's schedule; face morphs as sliders over FaceGen (the engine's FaceGen code) |
| LVLI, LVLN, LVLC | a roll preview: roll 100 times at a chosen level and show what came out |
| CONT | contents with counts and leveled lists |
| WEAP, ARMO, AMMO | stats table; the combat code's derived numbers (DPS etc.) shown beside the raw fields |
| LIGH | colour, radius, flicker previewed on a placed light |
| PACK | schedule, location and target pickers that point into the world |
| QUST, DIAL, INFO | §13–§15 |
| SCPT and embedded scripts | §12 |
| GMST, GLOB | a table with search |
| FLST | members list with picker |
| MESG | the message box shown live with its buttons |
| TERM | the terminal screen previewed on the monitor model (terminal-monitor design) |

### 8.3 New base records

"New from…" duplicates a record into the edit plugin with a new FormID and Editor ID;
"New" makes an empty record of a type with the schema's defaults (where the schema has
none, zero, and the lint step flags required fields left empty, §16).

---

## 9. Terrain

### 9.1 The format, as the engine reads it

From world.md §3 and `land.rs` [documented; layout observed]:

- A LAND record per exterior cell: a 33 × 33 vertex grid, spacing `CELL_SIZE / 32`.
- **VHGT**: an f32 offset, then 33 × 33 signed bytes; each row's first value is a delta
  from the previous row's first value (the first row from the offset), every other value
  a delta from its left neighbour; height units are 8 world units per step
  (`HEIGHT_SCALE`). Then trailing unused bytes.
- **VNML**: per-vertex normals.
- **VCLR**: per-vertex colours.
- **BTXT**: base texture per quadrant (LTEX, quadrant); **ATXT** + **VTXT**: alpha
  layers, each naming an LTEX, quadrant and layer number, with opacity per vertex of a
  17 × 17 quadrant grid (`QUAD_SIDE`).
- Neighbouring cells share their edge vertices and agree exactly (heights) [observed:
  world.md §3].

### 9.2 Height editing

- Brushes: raise, lower, smooth, flatten (to the height under the first click, or a typed
  height), noise, ramp (between two points), set (exact value).
- Radius, strength, falloff curve; a circle drawn on the ground.
- Works on vertices across cell borders: the brush edits every cell it covers; shared
  edge vertices are written to both cells so seams stay exact.
- **Range of a cell's heights**: the offset is an f32, but each step from the neighbour
  is a signed byte, so the slope between neighbouring vertices is limited to ±127 × 8 =
  ±1,016 world units per vertex spacing (`SPACING` = 4096 / 32 = 128 units) [derived from
  the layout above]. The brush clamps to that and shows where it clamped. The f32 offset
  and the row-start chain make the absolute range effectively unlimited.
- **Encoding**: rebuild the deltas from the edited heights; quantise to the 8-unit step;
  keep the trailing bytes. Normals (VNML) recomputed from the heights for the edited
  vertices and their neighbours, by the same rule the shipped normals follow (to be
  derived and checked against the masters: recompute every shipped cell's normals and
  compare; [hypothesis] until it matches within a byte).
- Objects on edited ground: an option "move references with the ground" lifts or lowers
  references whose base touched the old surface by the same amount.
- Water: the cell's water height (XCLW) shown as a plane; editing it is a CELL field.

### 9.3 Texture painting

- Pick an LTEX from the load order's landscape textures (picker with swatches).
- Paint: adds opacity to that LTEX's layer in each quadrant the brush touches; creates the
  ATXT layer in the quadrant if absent; erasing lowers opacity and removes a layer whose
  opacity is all zero.
- **Layers per quadrant**: there is a limit the original honours (commonly said to be 8
  alpha layers per quadrant in the GECK [hypothesis]); the editor counts layers and refuses
  a new one over the limit, offering to replace the weakest layer. The limit is checked
  against the masters (the most layers any shipped quadrant has) before enforcing.
- Edge agreement: opacities on shared quadrant edges are written equal on both sides, as
  the shipped data agrees within 1/255 for almost every edge [observed: world.md §3].
- Layer order: new layers are appended; the engine draws layers in order (world.md §3),
  so the panel shows the order and lets the user move a layer.
- Base texture: per quadrant, set from the picker.

### 9.4 Vertex colour painting

Brush on VCLR, colour picker, blend modes (paint, multiply, lighten), on the same 33 ×
33 grid.

### 9.5 After terrain edits

- Grass regrows on the edited cells (the engine grows grass from landscape textures,
  status.md).
- Generated distant terrain and object LOD for the grid square rebuild in the background
  (generated-lod design).
- Navmeshes over edited ground: their triangles no longer follow the surface. The editor
  marks those navmeshes "out of date" (§10.6) and offers to regenerate or to lift the
  vertices onto the new ground.
- For the original game: its terrain LOD would show the old shape from far away; the save
  dialog warns when LAND records changed.

---

## 10. Navmeshes

### 10.1 The format, as the engine reads it

From navmesh.md (NAVM read by hand in `fnv-nav/src/record.rs`, NAVI §1.2):

- NAVM per cell: vertices, triangles (vertex indices, neighbour edges, flags such as
  water and door), edge links to triangles of other navmeshes, door links, a grid for
  lookup (NVGD), cover data.
- NAVI, one record for the load order's navmeshes: NVMI per navmesh (flags: Initially
  Disabled bit 4, Is Island bit 5; location), NVCI per navmesh (FormID and three
  FormID arrays of connected navmeshes) [documented: xEdit NAVI, navmesh.md §1.2].
- NVForever builds its graph from NAVM alone; NAVI is read and checked only (navmesh.md).
  Whether the original uses NAVI's islands and connections at run time is not known
  (navmesh.md §open questions). So a written plugin must keep NAVI exactly consistent,
  or the original may path wrongly even when ours does not.

### 10.2 Viewing

- Navmesh overlay: triangles drawn over the ground, coloured by flags (water, door,
  preferred, island); edges linking to other navmeshes in another colour; door links as
  arrows to the door; cover edges marked.
- **Path test**: pick two points, the editor asks `fnv-nav` for a path and draws it,
  with its cost; pick an actor and see the path it would take to its package target.
- Problem markers (§10.6) shown in red.

### 10.3 Hand editing

- Select vertices, edges, triangles (box select in top view).
- Move vertices (snap to ground below), add a triangle from three vertices, split an
  edge, merge vertices, delete triangles, flip a triangle's flags.
- Edges across cell borders: the editor finds the matching edge in the neighbour's navmesh
  and writes the edge links on both sides.
- Door links: drag from a triangle to a door reference; the door's XNDP written to point
  back (world.json REFR XNDP: Navmesh Door Link [documented]).

### 10.4 Generation

- Input: the collision geometry of terrain and references in the cell (and a margin of
  the neighbours), from `fnv-collide`; water planes; Obstacle-flagged objects excluded
  when disabled-able (navmesh.md §4c).
- Generator: a Recast-style voxel generator from the Rust ecosystem (candidates:
  `oxidized_navigation` (Bevy, MIT/Apache), `recast-rs` bindings; to be judged in
  dependencies.md), with the agent radius, height, climb and slope set to match the
  game's actors (from the walkers' capsule sizes in `fnv-phys` [design]).
- Output: polygons triangulated, simplified, then converted into NAVM triangles with
  the flags set from the surface (water from water planes, door triangles under doors).
- Cell borders: generated per cell with the shared border vertices snapped to the same
  positions on both sides, then edge links built.
- Regenerating a cell that has a master navmesh: an override of the master NAVM with new
  triangles. Other plugins' references to its triangles (edge links from neighbouring
  navmeshes, cover) become stale: the editor regenerates the links on its side and warns
  about navmeshes it does not own.

### 10.5 NAVI upkeep

On save, for every navmesh the edit plugin adds, overrides or deletes:

- NVMI: flags, location, and the remaining NVMI fields recomputed as the masters have
  them (each field's rule derived from the masters and checked: recompute every shipped
  NVMI from its NAVM and compare [hypothesis] until it matches);
- NVCI: connected navmeshes from the edge links;
- islands: a connectivity pass over the whole loaded graph.

NAVI is one record shared by every plugin; the edit plugin's override carries the full
list (every master navmesh too). Two plugins that both edit navmeshes conflict on NAVI;
xEdit users know this and merge it; the editor warns when another active plugin also
overrides NAVI.

### 10.6 Checks

- Triangles with no neighbours (islands too small to matter), edges that should link to a
  neighbour cell but do not, door links to doors that no longer exist, vertices far
  above or below the ground, overlapping triangles, triangles under solid objects.
- "Out of date": the ground or objects under the navmesh changed since it was built.

---

## 11. Lighting, weather, image spaces, water

- Cell lighting (XCLL: ambient, directional, fog, fog power, light fade) and light
  templates with live sliders; the cell redraws as values move.
- Weather (WTHR) editing with the current weather forced on; time-of-day colours as
  four-stop gradients with the preview hour slider (§3.5).
- Image space (IMGS) and image space modifiers (IMAD) previewed.
- Water (WATR) properties on the visible water.
- Placed lights: radius drawn, colour and flicker previewed; light fade; shadows on/off
  as the engine supports.
- Our own engine's look can differ from the original's (owner preference for darker
  interiors, memory: interior-brightness-keep); the editor shows the values, and
  the save dialog does not judge looks.

---

## 12. Scripts

### 12.1 Where scripts live

SCPT records (object, quest, effect scripts), and scripts embedded in INFO (begin and end
result scripts), QUST stages and targets, PACK, TERM menu items, and others (common.json
script definitions). Each stores source (SCTX), compiled bytes (SCDA), header (SCHR),
variables (SLSD/SCVR) and references (SCRO/SCRV) [documented: esm.md, script spec notes].

### 12.2 The editor

- Text editor with highlighting from `fnv-script`'s tokenizer, line numbers, find and
  replace, go to definition for Editor IDs (opens the record), completion of function
  names and their parameters from the function catalogue (`C1` names with aliases,
  extender-compat), and of Editor IDs from the load order.
- Compile on every pause in typing; errors and warnings inline from the compiler's
  diagnostics.
- The extender grammar (let, eval, loops, UDFs, string and array variables …) is accepted
  only when the user turns on "uses extenders" for the plugin; the plugin then needs the
  extenders in the original game, and the UI says so. NVForever's own byte forms for
  extender syntax are never written to a plugin (status.md: "NVForever's own byte
  forms, never written to a plugin"); a plugin script using extender syntax is saved with
  its source and the bytes the original extenders' compiler would produce only if our
  compiler can produce those bytes exactly — otherwise saving is refused with the
  reason. [Open: §21; how extender-compiled bytes look is known from the GECK runs the
  owner is to do (status.md G2/G3), not yet.]
- Vanilla grammar scripts save source + compiled bytes; the compiler is byte-identical
  on 14,489 of 14,511 shipped scripts [observed: status.md], so a script that the GECK
  would compile gets the GECK's bytes in nearly all cases. The 22 others are listed in the
  compiler's notes; a script hitting one of those forms gets a warning.

### 12.3 Running scripts while editing

- **Run now**: in live mode, a quest script's GameMode block runs on the next frame with
  the new bytes; an object script on its next event.
- **Console in context**: the console's commands with the selected reference as
  `this` (the console already selects references).
- **Breakpoints and stepping** (later): the script VM gains a hook to stop at a line,
  show the variables of the instance and its locals, step to the next line. Built from
  our VM, nothing from the original.
- **Watch**: pin variables of quest scripts and references to a watch list that updates
  live.

### 12.4 Hot swapping a running script

When a script recompiles, running instances (quest scripts, object scripts on loaded
references) keep variables whose names and types are unchanged; new variables start at
0; removed variables are dropped. An instance mid-way through a block finishes the old
bytes first and uses the new ones from its next run. Shown in the log: "script X
recompiled: 3 instances kept 5 variables, 1 new, 1 dropped".

---

## 13. Quest debugger

### 13.1 Quest list

Every QUST in the load order, with:

- state: not started, running, completed, failed, stopped;
- current stage and the stages done (the log entries' text from CNAM, the stage flags
  from QSDT [documented: gameplay.json QUST]);
- objectives (QOBJ/NNAM) shown and completed, and their targets (QSTA) with whether each
  target's conditions pass now;
- priority, flags (start game enabled, repeated stages, allow repeated conversation
  topics …), the quest script and its delay;
- search and filters (running only, by plugin, by Editor ID or name).

### 13.2 Quest detail

- Variables of the quest script, live (the console's `sqv` data), editable in live mode.
- Each stage: its log entries, their conditions (passing or not now), their result
  scripts (opens in the script editor, §12), "set this stage now" (runs the result
  script as SetStage does).
- Each objective: its targets, "show target" (travels the camera to it; quest targets
  already resolved by `quest_targets.rs`), "complete" / "display".
- Topics belonging to the quest (DIAL records whose QSTI names it, gameplay.json DIAL
  [documented]) with a link into the dialogue view (§14).
- Actors with packages from this quest, and aliases-like links that FNV quests make by
  references in scripts (FNV has no quest aliases; references are named in scripts and
  conditions [documented: GECK wiki, Quest]).

### 13.3 History: who changed what

From the script and dialogue trace (the engine's existing trace events, extended):

- each SetStage with the time, the caller (script name and line, dialogue line, console,
  editor) and the stage before;
- each change of a quest variable, a global, a reference's enabled state, with the
  caller;
- each dialogue line said, with the topic and quest;
- filterable by quest; kept for the run (a ring buffer, 10,000 events, [design]),
  saved with the session for crash reports.

This answers "why did stage 30 never fire?": the history shows the last thing that
touched the quest, and the stage view shows which condition of the next step fails.

### 13.4 Stuck-quest helper

For a quest that stops progressing:

- the current stage's ways onward: stages whose result scripts are reached from dialogue
  lines (an INFO's result script calling SetStage on this quest), from object scripts
  (OnActivate, OnDeath …), from triggers; found by searching compiled scripts for
  SetStage calls on this quest (our compiler and decompiler know the opcodes);
- for each way, whether it can happen now: the dialogue line's conditions (§14), the
  object's existence and enabled state, the trigger's place;
- the walkthrough notes' stalls (mq-walkthrough-2026-10-27.md) become test cases (§19).

### 13.5 Checkpoints

- **Checkpoint**: an in-memory save of the run (fnv-save, without writing a file), named,
  several kept.
- **Roll back**: load a checkpoint into the same session; edits to the plugin are not
  rolled back (they are in the edit plugin, not the save).
- Checkpoints can be written as real saves on request.

---

## 14. Dialogue debugger: "why won't this NPC say that?"

### 14.1 Opening it

- Aim at an actor in editor mode, or open it from a topic in the quest view, or from a
  running conversation (live mode).
- Choose the speaker and the listener (default: the player).

### 14.2 What it shows

For the speaker, every topic it could ever have a line in (topics with lines whose
conditions, speaker ANAM, quest, or top-level flag could fit it), grouped:

- **Greeting** (GREETING topic and force greets), **topics the player has** (added by
  AddTopic or NAME on said lines, or top-level), **topics not yet added**, **combat,
  detection, service, misc** (DIAL DATA types 0–7 [documented: dialogue.md §DIAL DATA]).

For each topic, its lines in the order the game tries them (quest priority, then the
stored order, owner ruling on selection order; within a quest PNAM order where PNAM
exists [documented: dialogue.md §3.1]), and for each line:

```
INFO 0x0010A2B3  "You're the one who..."   quest VMS38 (priority 55)   ✗ fails
  flags: Say Once (already said by this actor)                          ✗
  speaker ANAM: none
  conditions:
    1  GetIsID  NPC: VMS38RedLucy           == 1     run on Subject   1.00  ✓  AND
    2  GetStage Quest: VMS38                >= 20    run on Subject  10.00  ✗  AND
    3  GetQuestVariable VMS38.bMet          == 0     ...              1.00  ✗  OR
    4  GetPCIsSex Female                    == 1     ...              0.00  ✗
  combined: (1 AND 2) AND (3 OR 4) = false
  winner of this topic now: INFO 0x0010A2C0 "What do you want?" (first passing line)
```

- every condition: function, parameters by name (Editor ID and name), run-on target
  (Subject, Target, Reference, Combat Target, Linked Reference … `cond::RunOn`), the live
  value, the comparison, the operand (a value or a global), pass/fail, and the AND/OR
  joining, with the group results as `cond::combine` computes them;
- line flags that block it (Say Once, Say Once a Day, Goodbye side effects, "Run
  immediately"), the quest's state, the speaker restriction;
- the winning line of the topic and, for each line above it that lost, the reason.

### 14.3 What it needs from the engine

- `cond.rs`: a version of the check that returns, per condition, the evaluated value and
  the pass/fail (`check_explained` beside the existing check; same code path, no second
  implementation so the explainer can never disagree with the game).
- `dialogue.rs`: `can_say_explained` and `select_line_explained` returning the reason
  each line lost (condition index, flag, quest not running, speaker mismatch, said
  already) — wrappers around the same logic.
- Both cheap enough to run every frame for one actor in live mode.

### 14.4 Trying things

- **What if**: override a condition's result, a quest stage, a variable or a global for
  the debugger only (nothing in the game changes) and see which line would win.
- **Make it so**: apply that change to the running game (set the stage, set the
  variable) and talk to the NPC. Roll back with a checkpoint (§13.5).
- **Say this line now**: force the speaker to say a chosen line (the engine's
  `start_dialogue` with a topic), conditions ignored, to hear and see it.

---

## 15. Dialogue authoring

### 15.1 The records

From gameplay.json and dialogue.md [documented]:

- **DIAL** (topic): name (FULL), priority (PNAM), type and flags (DATA: type 0–7,
  bit 1 Top-level), quests (QSTI, QSTR), INFO order bookkeeping (INOM/INOA, xEdit's,
  absent in the masters and ignored by the engine).
- **INFO** (line): DATA (type, next speaker, flags), quest (QSTI), previous topic
  (TPIC), previous INFO (PNAM: the line above in the editor's list), add topics (NAME),
  responses (TRDT: emotion, value, response number, sound; NAM1 text; NAM2 script notes;
  NAM3 edits; SNAM/LNAM speaker and listener animations), choices (TCLT), link from
  (TCLF), follow ups (TCFU), prompt (RNAM), speaker (ANAM), speech challenge skill or
  perk (KNAM), challenge (DNAM), conditions, begin and end result scripts.
- Line flags: Goodbye 0x0001, Say Once 0x0004, Say Once a Day 0x0100, and the others
  in dialogue.md §2 [documented].

### 15.2 Editing existing lines

- Response text, prompt, emotion and value, speaker and listener animations, flags,
  conditions (the condition editor: function picker from the catalogue with parameter
  pickers by type; run-on; comparison; value or global; AND/OR), choices, add topics,
  follow ups, result scripts.
- Responses: add, remove, reorder (response numbers rewritten in order).
- Text in curly brackets is an acting note and not shown [documented: dialogue.md NAM1];
  the editor shows it greyed.
- Every edit to a master's line is an override of that INFO, and its topic is
  overridden too (the INFO sits in the topic's children group, §4.4).

### 15.3 New topics

A new DIAL with lines. How it reaches the player decides how compatible the plugin is,
and the editor offers the ways, most compatible first:

1. **Top-level topic** (DATA flag bit 1) with conditions on the speaker (GetIsID, or
   ANAM on each line): the topic appears in that NPC's list without touching any master
   record. Requires its quest to be running (§15.5).
2. **Added by a script**: AddTopic in a new quest's stage or script; no master record
   touched.
3. **Added by a new line** of the NPC's (NAME add topics on a new INFO in an existing
   topic, e.g. GREETING): adds an INFO to a master topic, so the master DIAL is
   overridden (the topic record, not other plugins' lines; two plugins adding lines to
   GREETING both override the DIAL, which xEdit users merge).
4. **Linked as a choice** of an existing line (TCLT on a master INFO): the most direct,
   the least compatible (the master INFO is overridden; another plugin editing that line
   loses one side). The editor warns and suggests 1–3.

For each new topic the editor fills: Editor ID, name (what the player sees unless a line
has a prompt), type, priority (default 50, xEdit's default [documented: dialogue.md
DIAL PNAM]), quest.

### 15.4 Building while talking

In live mode, during a conversation with the debugger open:

- **+ New choice** on the current line: creates a new topic linked from this line (way 4,
  or way 3 when the current line is the plugin's own), opens a small form: player prompt,
  the NPC's response text, emotion, conditions (with live pass/fail beside each, §14),
  flags (Goodbye …), result script. Saving the form adds it to the choices on screen at
  once; clicking it plays it.
- **+ New response** to the topic just chosen: another line for this topic with its own
  conditions (the NPC answers differently later in the quest).
- **Edit this line** on whatever the NPC just said: opens its form; changes apply when
  the line is said again ("say again" button).
- **Branch map**: a graph view of the conversation built so far (topics as nodes, choices
  and follow ups as edges), coloured by whether each line can be reached now.
- The new records go into the edit plugin like any other edit; undo works per form save.

This is the part that does not exist in any current tool for this game [design: the GECK
needs a game restart to test dialogue].

### 15.5 Quests for new dialogue

- New topics need a quest. The editor offers to create one per plugin ("<plugin>
  Dialogue" quest, Start Game Enabled, priority chosen by the user) the first time a
  topic is created, or to pick an existing quest of the plugin.
- A topic whose quest is not running is flagged in the lint list and in the debugger
  ("this topic's quest VMS38 is not running: the line can't be said"), the most common
  cause of "my new topic doesn't show up" [hypothesis: from modder experience, not a
  measured fact].
- Priority: a dialogue quest's priority decides which line wins against the masters'
  lines (owner selection order ruling); the editor shows where the new line sits against
  the lines it competes with.

### 15.6 Line order

The game tries a topic's lines by quest priority, then stored order (owner ruling);
the stored order within a quest is the PNAM chain where PNAM exists (dialogue.md §3.1:
FalloutNV.esm's 23,247 lines have no PNAM, every add-on line has one [observed]). The
editor:

- shows the order as a list with drag to reorder;
- writes PNAM for the plugin's own lines as the add-ons do;
- never rewrites PNAM of master lines unless the user moves one (then it is an override).

### 15.7 Voice and lip files

- Voice file path: `Sound\Voice\<plugin>\<voice type>\<file>.ogg`, the file name built
  from the quest, topic, INFO FormID and response number (`fnv_voice::VoiceBank::file`;
  holotapes.md §voice [documented/observed there]). The editor shows each response's
  expected path for each voice type that can say it.
- **No recording yet**: the engine plays the line silent with its subtitle for a time
  from the text (`line_seconds`); how the original times a line with no voice file is to
  be checked [hypothesis]. The editor marks such responses "no voice".
- **Recording**: dropped (owner, §21.2 item 7). Was: record from the microphone into the expected path under
  `<home>\mods\`, or import an `.ogg`/`.wav` (converted with an Ogg Vorbis encoder from
  the Rust ecosystem); play it back in place.
- **Changed text on a voiced master line**: the recording no longer matches. The editor
  warns, offers to keep the audio, or to play silent (writes nothing about audio into the
  plugin either way; the choice is the user's at release time).
- **Lip files** (`.lip`): the engine reads them (lipsync spec). Generating them is out
  of scope for now (no clean-room source for the format's generation; reading only);
  lines without one get the engine's fallback mouth movement [hypothesis: whatever
  lipsync.md describes for a missing file].
- **Placeholder voice**: dropped (owner, §21.2 item 7). Was: text-to-speech for drafts, never saved as the
  plugin's voice unless the user exports it.

### 15.8 Speech challenges, barter, and results

- Speech challenge (DNAM, KNAM): skill or perk, difficulty; the editor shows the player's
  chance now and lets the debugger force success or failure.
- Result scripts: the script editor (§12) inline; common snippets offered (SetStage,
  AddItem, StartCombat, ShowBarterMenu, SetObjectiveDisplayed).
- Follow ups (TCFU) and link from (TCLF): edited as lists with pickers; dialogue.md
  §3.4–§3.5 for what they do.

---

## 16. Checks before saving (lint)

Errors (stop the save):

- a record's required field empty; a FormID pointing at nothing in the load order; a
  FormID pointing at a record of a type the field does not allow;
- duplicate Editor ID across the load order;
- a script that does not compile;
- a CP-1252-unencodable character in a string;
- a door with a teleport whose destination has no teleport back;
- navmesh edge links not matching on both sides (§10.6).

Warnings (listed, the save goes ahead):

- overrides of master records that change nothing ("identical to master": offered to
  clean);
- deleted master references (§4.5); master INFO lines linked as choices (§15.3 way 4);
- a topic whose quest is not start-game-enabled and not started by any script;
- lines with changed text on voiced responses (§15.7);
- LAND changes (original game's LOD stale, §9.5); navmeshes out of date;
- NAVI overridden by another active plugin (§10.5);
- extender syntax in scripts (§12.2);
- references outside their cell's bounds (exteriors);
- persistence: a reference used by a script or a package but not flagged Persistent
  (esm.md: Persistent flag on placed references [documented]).

---

## 17. Undo, history and crash safety

### 17.1 Commands

Every edit is a command: `{ record FormID, before (decoded fields or "absent"), after,
label }`, grouped into one undo step per user action (a gizmo drag, a brush stroke, a
form save). Undo restores `before` and runs the refresh (§3.3); redo applies `after`.

### 17.2 History panel

The list of steps with labels ("Moved 3 references", "Raised terrain in 4 cells",
"Added topic MyTopicWho"), click to go back to that point.

### 17.3 Session file

The edit plugin's dirty records and the history are written to
`<home>\mods\.editor-sessions\<plugin>.session` (our own format, versioned, not a
plugin) after every step (debounced, at most every 2 s [design]). After a crash, the next
entry into editor mode offers to restore it.

### 17.4 Never losing a plugin

§4.7: temp file + rename, `.bak`, timestamped backups, read-back verification.

---

## 18. Settings and controls

In `NVForever.ini`:

```
[Settings]
bEditorEnabled=1          ; editor mode available (see §21 on release builds)
sEditorAuthor=            ; default CNAM
sEditorIdPrefix=          ; default Editor ID prefix
fEditorGridSnap=16
fEditorAngleSnap=15
iEditorBackups=10

[Controls]
Editor=<key>              ; toggle editor mode
EditorFrame=F
EditorGizmoMove=W
EditorGizmoRotate=E
EditorGizmoScale=R
```

Panel layout per user in `<home>\editor.toml` [design]. Keys rebindable on the settings
page like the others (key-rebinding design).

---

## 19. Testing

### 19.1 Unit and property tests (`crates/`)

- Encoder round trip over synthetic records of every primitive and struct shape
  (proptest, as `fnv-esm/tests/synthetic.rs` already does for decoding).
- FormID allocation, master renumbering, group placement for synthetic plugins.
- VHGT encode/decode round trip and slope clamping; normals recomputation.
- NAVM encode/decode round trip; edge link pairing.
- Condition explain: same pass/fail as the plain check for every condition in the
  masters against a set of synthetic states.

### 19.2 Lab scenarios (with game data, private)

- `L1.esm.encode.<master>`: encode every decoded record, bytes equal (§5.3).
- `L1.esm.groups.<master>`: recompute every record's group placement, equal to the file.
- `L3.land.normals`: recomputed VNML equal to shipped (within a stated tolerance agreed
  before the run, never loosened afterwards: parity-lab rules).
- `L4.navi.recompute`: NVMI/NVCI recomputed from NAVM equal to shipped NAVI.

### 19.3 Smoke scenarios (`tools/smoke/scenarios.toml`)

Run headless where possible, through a test aid (like the existing `NVF_PLAY_TEST_*`
aids) that drives the editor by commands:

- `editor-place-save`: enter editor mode, place a static, save a new plugin, restart with
  it, the static is there (log line from the cell spawn).
- `editor-move-override`: move a master reference, save, restart, its position matches.
- `editor-delete-disable`: delete a master reference, the saved plugin has it disabled
  and sunk.
- `editor-terrain-raise`: raise a vertex, save, restart, the height and both neighbours'
  seams match.
- `editor-topic-add`: add a top-level topic to Sunny Smiles, talk to her, the line is
  said (dialogue log line).
- `editor-condition-explain`: the explainer's lines for a known line equal the expected
  list.
- `editor-crash-restore`: kill the run mid-session (own PID only), restart, the session
  restores.

### 19.4 Observer checks (the original game, owner's machine)

Added to `docs/handoff/observer-checks.md` only when this design leaves `local/`:

- a plugin from each phase loads in the GECK without warnings beyond a hand-made one's;
- xEdit's Check for Errors finds nothing;
- the original game plays it: the static is there, the moved reference is moved, the
  topic is said, the navmesh is walked by an NPC;
- a deleted (Deleted flag) master reference with a save referring to it: does the original
  crash? (settles §4.5's hypothesis).

---

## 20. Phases and what done means for each

Each phase is usable by itself; later phases build on earlier ones. They can run beside
the other engine work, since most of phase 1–2 is `crates/` work with no game-side risk.

| Phase | Content | Done when |
|---|---|---|
| 0. Groundwork | plugins load from `<home>\mods\` (§2.2.9); schema encoder (§5) | `L1.esm.encode.*` pass for all ten masters; a hand-made plugin in `mods\` loads |
| 1. Plugin writer, headless | `EditPlugin`, FormIDs, groups, header, overrides, deletions, save with backup and read-back (§4); `fnvtool edit` commands to script edits (place, move, set field, save) | a plugin made by `fnvtool edit` loads in ours, xEdit and the GECK (owner check) and plays in the original |
| 2. Dialogue and quest debugger (read-only + session-only changes) | §13, §14; per-condition explain in `cond.rs` and `dialogue.rs`; checkpoints | the explainer agrees with the game on every line it is shown in a full main-quest walkthrough save set; the walkthrough stalls are explained by it |
| 3. Editor mode, references | §6, §7; overlay and refresh for references (§3) | place, move, delete, duplicate, undo, save, restart: smoke `editor-place-save`, `-move-override`, `-delete-disable` pass |
| 4. Record editing and scripts | §8 generic panel and the purpose-built editors in priority order; §12 script editor and hot swap | edit a weapon's damage and see it in combat without leaving the run; recompile a quest script live |
| 5. Dialogue authoring | §15 in full except recording; building while talking | a five-topic conversation with a quest stage built entirely in game, saved, playing in the original game |
| 6. Terrain | §9 heights, textures, colours; seams; LOD rebuild | smoke `editor-terrain-raise`; a painted cell matches in the original game (near; LOD stale is expected) |
| 7. Navmesh | §10 view, hand edit, NAVI upkeep, then generation | an NPC walks a new navmesh in ours and the original; `L4.navi.recompute` passes |
| 8. Polish | lighting/weather panels (§11), voice recording, breakpoints, branch map, brush placement, new worldspaces | per feature |

Phase 2 can start before phase 1 (it needs no writer), and gives the main-quest work a
tool at once.

---

## 21. Open questions

### 21.1 Facts to check (against the data, the documentation or the original)

1. Exterior block/sub-block numbering rule (§4.4): recompute over the masters.
2. A master reference overridden into another cell's group (§4.4): what ours and the
   original do.
3. Deleted master references crashing the original (§4.5): cite the xEdit documentation;
   observer check §19.4.
4. VNML rule from heights (§9.2): recompute over the masters.
5. Alpha layer limit per quadrant (§9.3): the most layers any shipped quadrant has; the
   GECK's limit from its documentation.
6. NVMI field rules (§10.5): recompute over the masters.
7. Whether the original uses NAVI islands/connections at run time (navmesh.md open
   question).
8. Silent line timing in the original (§15.7).
9. Whether every exterior grid square inside the base worldspaces has a CELL record (§7.5).
10. Whether loose plugins in `<home>\mods\` already load (§2.2.9).

### 21.2 The owner's decisions (2026-11-01)

1. **Locked behind an INI setting**: editor mode is in every build, allowed only with
   `editor=1` under `[Settings]` in the home's `NVForever.ini` (built: `editor.rs`
   `allowed`; not a row of the settings page; smoke `editor-locked`).
2. **Compatibility is a hard line**: every ESP the editor writes works everywhere from
   the base game to NVForever. §4.8's checks (read-back, xEdit, GECK, the original game)
   are requirements, not goals.
3. **Extender syntax stays separate** until all of the NVSE logic has been migrated
   into NVForever: saved scripts hold vanilla syntax only (§12.2); extender syntax is
   refused when saving.
4. **MO2**: under Mod Organizer 2 a save goes into the plugin being edited, or else into
   MO2's overwrite folder, as part of its virtual file system (§4.1, §4.6; built
   2026-11-01, `fnv_install::mo2::Layout`; not yet tried from a real MO2 start).
5. **No key**: `editor` in the console opens editor mode and `editor` again closes it
   (as built).
6. **New plugins are added to `plugins.txt` and on**, so a plugin is part of the load
   order at once; edits act in the running session as they are made (§3, as built).
7. **No voice recording, no TTS**: new and changed lines are silent, with subtitles
   (§15.7's recording and placeholder parts are dropped).
8. **This design lives in `docs/design/editor.md`** (moved from the local draft).

---

## 22. Risks

- **Refresh coverage**: every system that caches record data needs a refresh handler;
  a missed one shows stale data and looks like an editor bug. Mitigation: the full reload
  (§3.6) as a fallback; a debug check that compares a refreshed system with a cold-built
  one.
- **Writing plugins that break the original game**: subtle header, group, NAVI or ONAM
  mistakes. Mitigation: read-back, xEdit cross-check, owner checks per phase before
  anyone else uses the files.
- **Scope**: this is several months of work beside the engine. Mitigation: the phases are
  each useful alone; phase 2 pays off for the main-quest work at once.
- **Clean room**: the editor must not be built by looking at the GECK's internals or any
  other editor's code. Behaviour from the documentation the spec notes cite and from our
  own observation of files (charter).
- **UI dependency**: `bevy_egui` must keep up with the engine's Bevy version (Bevy 0.20
  now, memory: bevy020-perf-check); if it lags, the editor waits or we pin.
- **Performance**: the explainer every frame, overlays of navmeshes and markers in big
  exteriors. Mitigation: explain only the debugged actor; draw overlays for the camera's
  cells only.
