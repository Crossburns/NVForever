# Spec note: the extender functions Tale of Two Wastelands calls

The functions of the TTW audit's §2 ([ttw-fo3-script-audit-2026-11-02.md](../handoff/ttw-fo3-script-audit-2026-11-02.md))
that nothing in NVForever ran before issue #1408. Code: `crates/fnv-game/src/nvse_ttw.rs`,
the event firing points named in [jip-events.md](jip-events.md) §5, ScriptWait in
`fnv_script::vm` (`Vm::resume`) and `Game::run_blocks`.

Wiki statements are `[documented: <page>]` (the owner's private export
`C:\fnv-private\geckwiki\`, in our own words). How TTW calls them is `[observed]` in the
source text of TaleOfTwoWastelands.esm 3.4 (read-only, nothing copied). No capture of the
original was made for this note.

## 1. Where TTW calls them [observed]

- TTWGenericHandlerScript (a quest's GameMode, once per run): SetTerminalUIModel on a
  form list of terminals, MarkActivatorAshPile on three activators, the JIP event
  registrations (jip-events.md §5), and SetMusicState 1 / 0 around Fallout 3's "Exit the
  Vault" music so a sound marker does not cover it.
- TTWGenericHandlerScriptMiscStatsScript (`Begin MenuMode 4`, under GetGameRestarted):
  InitExtraMiscStat for five stats; the bobblehead scripts then ModExtraMiscStat
  "Bobbleheads Found" by 1.
- Three disintegration effect scripts: AttachAshPileEx with one of the marked piles.
- DLC02ElevatorSCRIPT (The Pitt's elevator, GameMode): `ScriptWait 6` between the
  elevator's return and moving the player.
- DLC02NPCBaseSCRIPT (OnDeath): `DonnerReedKuruParty 1`, so the simulation's dead cannot
  be eaten.

## 2. The functions

| Function | Rule | Source |
|---|---|---|
| ScriptWait n | The block stops after the call; while it waits the block is skipped and the rest of the script runs; when the wait is over it goes on from the call. A cycle is one processing of the script: every frame for an object script, the quest's delay for a quest script. One waiting block per script: a new ScriptWait (from any block) ends the old wait. Not in effect, function or result scripts. | [documented: ScriptWait] |
| | NVForever counts a cycle as each run of the waiting block's event and goes on in the n-th one after the call (0 as 1). Elsewhere than a script's own block (a result script, a function, what a block calls) the call does nothing. Inside a loop (`while`, `foreach`) or a chain of stray ElseIf markers the place cannot be kept, so the call does not wait there. Waits are not saved. | [hypothesis / decision] |
| IsScriptWaiting [quest] | 1 while the script of the calling reference, or of the quest given, has a waiting block. | [documented: IsScriptWaiting] |
| StopScriptWaiting [quest] | Stops that wait. NVForever drops it: the block runs from its start the next time. | [documented: StopScriptWaiting; hypothesis: whether the rest runs] |
| MarkActivatorAshPile activatorOrList | The activators count as ash piles until the game is quit (kept across loads). Forms that are not activators are left out. | [documented: MarkActivatorAshPile; hypothesis: other forms] |
| actor.AttachAshPileEx activator | As AttachAshPile (ash-piles.md §3) with that activator as the pile. An activator not marked first gives no pile. | [documented: AttachAshPileEx, "must first be marked"; hypothesis: what happens without] |
| actor.DonnerReedKuruParty 0/1 (alias TheOtherOtherWhiteMeat) | The corpse is eaten (1) or not (0): HasBeenEaten reads it and an eaten corpse cannot be fed on. | [documented: DonnerReedKuruParty] |
| SetMusicState 0/1/2 | 1 pauses the music playing, 2 resumes it; 0 stops music a script played with PlayMusic, otherwise starts the current music again from its beginning. Stopping also ends a pause. Other numbers do nothing. | [documented: SetMusicState; hypothesis: a stop ends a pause] |
| SetTerminalUIModel terminalOrList [path] | The terminal menu draws that art file (a path under `meshes\`); without a path, back to the default. Kept for the session. The engine's terminal menu does not read it yet. | [documented: SetTerminalUIModel; hypothesis: how long it lasts] |
| InitExtraMiscStat "name" | A custom misc stat at 0; required before ModExtraMiscStat and GetExtraMiscStat use it (JohnnyGuitar 4.98 on). A stat already there keeps its value. | [documented: InitExtraMiscStat, GetExtraMiscStat; hypothesis: a second Init] |
| ModExtraMiscStat "name" n | Adds n to an initialised stat; one never initialised stays unknown (GetExtraMiscStat -1). The stats are kept in the save (NVForever's snapshot, `extra_misc_stats`). The Pip-Boy does not list them yet. | [documented: ModExtraMiscStat; observed: -1 after ModExtraMiscStat alone, nvse_jip2.rs] |

## 3. Open

- ttw_nvse's TTW_EnableRadioFix and InitRockItLauncher, and ShowOff's opcode 0x3800
  (audit §3): the names are guesses by source order, and the plugin is not built. A
  GetCommandOpcode capture with ShowOff and ttw_nvse installed would settle them.
- Whether ScriptWait counts cycles of the waiting block's event or of every script
  processing, and whether a wait survives a save, are not observed.
- The terminal menu does not draw SetTerminalUIModel's art; the Pip-Boy does not show the
  extra misc stats.
