# Script extender integration: the next batch (2026-10-26)

Where things stand, and the next tasks for the whole extender integration (xNVSE, JIP LN /
LU, the plugin families the owner's mod lists use). Same rules as before
(docs/design/extender-compat.md): no extender, GECK or game source; behaviour from the
wiki export `[documented]` or the game `[observed]`, each family probed in the original
game (the peer machine's L5.ext sets, #828) before or right after it is built.

## 1. Where it stands

- **xNVSE core: done.** Every family of docs/handoff/xnvse-remaining.md §3 is built
  (this machine and the peer, #844); the UI functions run on the tile tree (#834). Left:
  TestExpr, GetPaired, the file writers, CompileScript, PrintVar/DebugPrintVar,
  GetSoldItemInvRef, GetCurrentQuestObjectiveTeleportLinks, GetFalloutDirectory.
- **Coverage of the owner's mod lists** (`fnvtool ext scan` counts, recounted against
  the code on dc19c50e) [observed]:

  | List | Extender calls | Built | Not built, by origin (calls) |
  |---|---|---|---|
  | Talon (TTW, 128 plugins) | 36,021 | 60% (was 44%) | LU 4,448 · JIP 3,653 · MCM 3,182 · kNVSE 1,686 · ShowOff 1,032 · JohnnyGuitar 248 · NVSE 21 |
  | VNV (42 plugins) | 23,958 | 55% | LU 3,684 · MCM 2,889 · JIP 2,505 · ShowOff 786 · kNVSE 620 · JohnnyGuitar 187 · ITR 23 |

  The rest is plugin families, not xNVSE. Five names carry a third of what is left:
  LeveledListAddForm (4,949 calls in both lists), SetMCMString / SetMCMFloat /
  GetMCMFloat (5,979), SetActorAnimationPath (2,154), the INI-file functions
  (GetINIFloatOrDefault_Cached, Get/SetINIFloat: 3,302), the sound/weather/image-space
  traits (1,700).
- **Known gaps from the probes:** Doc's reference reads GetFlagsLow 1033 where the
  game gives 1097: bit 6 (64) is set at run time on that ACHR and no documented flag
  table names bit 6 for actor references (our schema has it only for REFR, "Hidden From
  Local Map"); Doc's stimpak in jip-fifth does not reproduce on 02053f1d (waiting on
  the peer's rerun).

## 2. The batch, ranked

Size: S (a day's session or less), M (a session or two), L (several; split first).
"Probe" means the peer writes the set and captures it in the game first.

| # | Task | Size | Who | Done when |
|---|---|---|---|---|
| N1 | **Leveled lists (LU):** LeveledListAddForm and its family (remove, get count/entries, chance none, flags) on LVLI/LVLN/LVLC, read by the leveled-list resolver NVForever already has, so spawns and containers see the change. 4,949 calls. | M | probe: peer; build: here | the family matches its probe set; a mod's LeveledListAddForm item shows up in a container's respawn headless |
| N2 | **MCM script functions, headless:** SetMCMString, SetMCMFloat, GetMCMFloat, Get/SetModINISetting over the tile tree (UI task T7), so mods' MCM scripts run and keep their values. 5,979 calls. Needs the owner's MCM call (owner-review) and UI T3. | M | here, after UI T3 | an MCM mod's menu script runs headless with no NotBuilt and its values round-trip |
| N3 | **INI-file functions:** ShowOff GetINIFloatOrDefault_Cached / OrCreate / String forms, LU Get/SetINIFloat, GetINIString, SetModINISetting. Reads from `Data\Config\*.ini`; writes need the owner's call on where they go (owner-review). 3,302 calls. | M | probe: peer; build: here | the family matches its probes; writes land where the owner chose, never in the game folder |
| N4 | **Sound, weather and image-space traits (JIP):** Set/GetSoundTraitNumeric, SetSoundSourceFile, SetWeatherRGBColor and the other weather traits, Set/GetImageSpaceModTrait, Get/SetImageSpaceTrait, IsSoundPlaying, read by the engine's audio, sky and imagespace code. 1,700 calls. | M | probe: peer; build: here + engine | the probes match; a changed weather colour shows in a frame |
| N5 | **Actor animation path (kNVSE):** SetActorAnimationPath, IsAnimPlayingAlt/Ex, ActivateAnim/DeactivateAnim, PlayAnimSequence, StopIdle, IsIdlePlayingEx: a script-chosen KF replaces an actor's clip in the engine's animation system. 2,154+ calls. | L | design first (split into a game-side table + an engine hook) | a design note and a task split, then the first slice in a smoke scenario |
| N6 | **NIF blocks and models (JIP):** SetNifBlockFlag/Translation/Rotation/Scale, InsertNode, AttachModel, ModelHasBlock, GetChildBlocks, ReloadEquippedModels: by node name on the engine's scene graph (no memory layout; extender-compat.md §5.8). ~800 calls. | L | design first | classification per function (supported / refused) and the supported ones on the engine |
| N7 | **JIP actor and game odds:** HasKeyword/AssignKeyword, ToggleHardcoreTracking, SetHardcoreStageThreshold, SetActorVoiceType, SetBaseKarma, SetNthPerkEntryValue1, GetPerkModifier, MessageExAlt, LNSetName, MoveToEditorPosition, CastImmediate, IsInWater, IsInAir, GetKiller, SetSpeedMult, MoveToCell, SetLinkedReference, UnequipItemAlt, SetReferenceFlag, Disable/EnableNavMeshAlt, GetProjectileRefImpactRef. ~1,300 calls. | M (two halves) | probe: peer; build: here | each half matches its probe set |
| N8 | **Controller functions (LU):** IsButtonPressed, Enable/DisableButton, GetController, GetLeftTrigger/GetRightTrigger, GetPressedButtons, on the engine's gamepad input (controller workstream). ~300 calls. | S | here | the functions read the pad the engine already reads; a smoke scenario |
| N9 | **JohnnyGuitar, ShowOff and ITR leftovers:** GetPipBoyMode, Lerp, ModExtraMiscStat, SetCameraShakeNoHUDShudder, SendStealingAlarm, ClearArmorAltTexture, SetWeapon1stPersonModel, ActorHasBaseFlag, AuxTimerStart family, ClearFileCacheShowOff, FileExists/GetFilesInFolder (read-only, inside Data), ForceCrouch, ForceCombatTarget. ~600 calls. | M | probe: peer; build: here | matches its probes |
| N10 | **Runtime sweep of the real lists:** run the Talon and VNV load orders headless (`fnvtool script run` over the profiles' plugins) at a handful of cells for a few game minutes each, and count the NotBuilt errors and script errors by function. Ranks what actually runs, not what is written. | S | here (the mods are on this machine) | a table in this file, refreshed after each family |
| N11 | **Events that never fire:** list the events scripts can register (SetEventHandler names, JIP SetOn…EventHandler) against the ones NVForever dispatches; wire the missing ones (OnSell first: GetSoldItemInvRef needs it). | M | here; probe the order of firing with the peer | every registrable event either fires or is listed as refused, with a smoke scenario for the new ones |
| N12 | **Extender state across saves:** round-trip tests that aux variables, ref maps, arrays, strings, tokens and the session switches that ext-save.md says are saved survive save and load, and the "not savebaked" ones do not. | S | here | tests in fnv-game; ext-save.md's open items answered or listed |
| N13 | **Small leftovers:** GetPaired; PrintVar / DebugPrintVar (the compiler passes the variable's name); TestExpr; GetFalloutDirectory; GetCalculatedWeaponDamage; ResetAllVariables kept across frames; GetCurrentQuestObjectiveTeleportLinks. | S | here; peer probes | each matches a probe row or has its rule written down |
| N14 | **Probes for the open answers:** bit 6 of GetFlagsLow on references (Doc's 64): GetFlagsLow on a persistent and a non-persistent ACHR, an ACRE, the player, a dead actor, Doc with his cell loaded and unloaded, a REFR with Hidden From Local Map; the Doc stimpak rerun on the latest commit. | S | peer | the bit's rule written into nvse_other.rs (or a hypothesis with the rows) |

UI tile tree tasks T3–T10 (docs/handoff/ui-tile-tree-tasks.md) run beside this list in
their own workstream; T2 (wiring the UI functions) is done (dc19c50e). N2 waits on T3.

## 3. Order

1. In parallel: N1 (biggest single name), N10 (cheap, sharpens the ranking), N14 (peer).
2. N3 and N7 (probe-first, straightforward), N8.
3. N2 once UI T3 lands and the owner has made the MCM call; N4.
4. N11, N12, N13.
5. N5 and N6 start with design notes; they reach into the engine.

## 4. Owner decisions this batch needs

- **Where mods' file writes go** (N3's INI writes, the J9 file writers, Con_SCOF):
  NVForever never writes into the game folder. The proposal in owner-review.md (J9):
  NVForever's own folder, mirroring the path; readers prefer that copy. Built that way
  unless the owner says otherwise.
- **MCM** (N2): already asked in owner-review.md by the UI tile tree work.

## 5. N10: the runtime sweep (VNV, 2026-10-26)

`fnvtool script run` over the VNV profile's 41 plugins, 1,800 frames from the new-game
state (the mods' quest scripts and their GameMode, main-loop and event callbacks)
[observed]. What it found, fixed the same day:

- **Mods' scripts failing to compile from source**, so they fell back to bytes NVForever
  cannot run: one unknown name failed a whole unit. Fixed: an actor value the shipped
  scripts never name (`IgnoreCrippledLimbs`, from xEdit's full list); the Let macro on a
  subscripted or remote target (`aEvent["Count"] = …`, `Q.aItem[i] = 0`); a function on
  a quest's ref variable (`JLM.rContainer.LNGetName`); an undocumented plugin function
  at the start of a call line (TTW's `TTW_EnableRadioFix`) now compiles as a call that
  reports "not built" instead of failing the unit (`Seed::unknown_calls`, the engine
  only).
- **The source-compile rule** missed units whose bytes call extender opcodes below the
  highest one the table knows; it now asks whether the table has the opcode.

Before: 14,392 script errors and 1,044 unreadable calls in 30 s. After: none; what is
left is functions not built, ranked by calls at run time:

| Calls / 1,800 frames | Function (origin) | Task |
|---|---|---|
| 10,806 · 7,200 · 5,401 · 3,600 | IsButtonPressed, GetController, GetLeftTrigger, GetRightTrigger (LU) | N8 |
| 2,313 | GetWaterImmersionPerc (JIP) | N7 |
| ~1,800 each | IsInKillCam, GetPCUsingScope, GetPCUsingIronSights, GetAutoMove, GetActorVelocity, IsIdlePlayingEx (JIP) | N7 |
| 258 · 240 | GetCurrentWeather, ModelHasBlock (JIP) | N4, N6 |
| 65 · 53 · 36 | SetNthPerkEntryValue1, GetINIFloatOrCreate_Cached, GetINIFloat | N7, N3 |

## 6. Progress (2026-10-26, claude/ext-compiler)

| # | State |
|---|---|
| N1 | Done (516f58f5, 174bb870): the item resolver reads the edits; actor lists edited for the functions, spawning from an edited actor list not hooked yet. |
| N3 | Done (79cd56b1): `crate::mod_files`, writes under NVForever's mods layer only; `fnvtool script run --config`. |
| N7, N8 | Done (b470a964), with the sweep's player-state getters; Disable/EnableNavMeshAlt left (no observable state). |
| N9 | Done but ForceCrouch/ForceCombatTarget (ITR not installed in the probe game). |
| N10 | Done (6d7597ab), §5. |
| N14 | Done (6d7597ab): bit 0x40 = talked to the player. |
| N4 | Done (7cfe4d1f): the getters and setters; the drawn sky, sounds and image spaces do not read the changes yet. |
| N5 | Slice 1 done (1bf36331, docs/design/knvse-anim.md); slice 2 (the engine plays replacements) open. |
| N2 | Outside the pause menu MCM's functions give 0 [observed]; the in-menu rows wait on a way to open StartMenu (UI T7). |
| N11 | Events sent at the points they happen: OnEquip/OnUnequip/OnActorUnequip, OnActorEquip, OnDrop, OnEnable/OnDisable, OnHit, OnHitWith, OnMagicEffectHit, OnLoad, OnPackageStart/Done/Change, OnCombatEnd, OnStartCombat (through `Game::ref_event`); OnAdd not sent for AddItem [observed]; OnSell not yet. |
| N13 | TestExpr, GetFalloutDirectory, GetSoldItemInvRef, GetPaired (0, no tokens), GetCurrentQuestObjectiveTeleportLinks (targets, no doors); GetCalculatedWeaponDamage and PrintVar's names open. |
| N6 | Done (fc35f2d8, docs/design/nif-blocks.md): the mirror and the engine bridge; AttachModel not drawn yet. |
| N12 | Done: the peer's save-set/save-read rules hold (ext-save.md §7); main-loop callbacks of function scripts carry over a load. |
| gaps2–4 | The peer's sets of the most called unbuilt functions (`crate::nvse_gaps`, docs/handoff/modlist-function-counts.json): headless every row matches but the save's own state (inventory, game hour, active quest, the player's Guns in GetCalculatedWeaponDPS, processing-level counts, the stimpak's hardcore effects: two heal over time in hardcore, two at once otherwise). Floor/Ceil out of the 32-bit range read -2147483648 [observed]. Not built from those sets: GetRunSpeed, GetActorValueModifier, IsMusicPlaying, GetCurrentClimate, GetCursorPos, GetGroundMaterial, GetIsRagdolled, HasEffectShader, IsAttacking, GetFallTimeElapsed, IsCrimeOrEnemy, SetWeaponOut (the original did not draw in the probe's state), IsAnimPlayingEx. |
| N5 slice 2 | Done for locomotion (anim_paths.rs); smoke knvse-anim-replace. Idles, script-played sequences and first person open. |
| Engine events | Smoke nvse-engine-events: OnLoad (unscripted references too), OnHit/OnHitWith, OnMagicEffectHit (a used stimpak counts, observed), OnPackageStart, OnCombatEnd reach the handlers in the running game. |
| 2026-10-27 batch | Sound (sound1, `crate::nvse_sound`), live projectiles (proj1, `crate::nvse_proj`), PushObject / GetAvailableRadios (phys1–3, radio1), GetFallTimeElapsed, GetMineArmed, the peer's HUD/controls (hud1, controls1, `crate::nvse_hud`). A run of every flagged name in fnvtool found 118 not built (the rest matched by prefix or alias); the third JIP batch (`crate::nvse_jip3`, jip3a/jip3b captured here) built GetAshPileSource, GetIsRagdolled, IsCompassHostile, IsHostilesNearby, FailQuest, GetPCLastExteriorDoor, GetPCMovedCell, HasEffectShader, IsCrimeOrEnemy, RemoveMeIRAlt, DropMeAlt, MoveToContainer, GetActorTemplateFlag, GetIsImmobile, IsRaceInList, GetMenuTargetRef (1009), SetDelayElapsed, RefreshItemsList, GetDebugModeState: jip3a 33/35 headless. The two left read GetDebugModeState 1 across `tai`: the catalogue has no ToggleAI (TAI) because the wiki export has no page for it. |

## 7. Lab run of every set (2026-10-30, claude/nvse-integration)

ext-compiler and jip-lab merged with main (231 commits behind); `python -m fnvlab run
--scenario 'L5.ext.*'`: 18 of 60 scenarios match in full, and the merge changed no row
(the pre-merge tip 8e369ace gave the same 60 stats). The nine sets that had never been run
in the lab (gaps1, gaps7-11, plugins1-3) now have counts in probe-sets.md. Fixed from the
run: GetPluginHeaderVersion (built: the TES4 HEDR version), ActorHasBaseFlag through the
actor's base-data template, GetFilesInFolder on a missing folder (an empty array),
GenericAddForm's result (the index, -2 the last before it), ClearArmorAltTexture's result.

What is left, by cause [observed: runs/latest.md of that run]:

| Cause | Sets (rows) | What closes it |
|---|---|---|
| **The owner's save** (`nvfairport`): the player's inventory (27 kinds, ammo), skills (Guns 17, Melee 34, Unarmed 17, Luck 10) and through them every DPS, crit and damage row; game hour, days passed, active quest, factions, heading, Doc's position and packages, always-run, difficulty, known radios, the talked-to bit 0x40 | gaps2-7, gaps9-11, jip.fifth, n7, leveled2, loops, fourth, second, misc, plugins1, plugins3, flags, eighth, seventh, leftovers (~150) | the lab starting from the save: a reader of the original `.fos` (its layout is mapped, docs/spec-notes/save-files.md) feeding `ext queries --save`; the owner allowed the lab to read the save (owner-review) |
| **Engine rows**: hits, projectiles, NIF blocks, kNVSE animation, event firing order in a fight, ray casts, reticle range, cursor, run speed, ragdoll and crippling events | hits, projectile, nifblocks, knvse.anim, nvse.events, jip.events2, gaps8, gaps10 (~140) | a lab twin on the running engine (the smoke runner with nvf_capture-style steps) instead of `ext queries` |
| ~~HUD tile values~~ closed 2026-10-30 (#944): `ext queries` loads the menus and writes the HUD once; 21 of 22 rows match, rgbui in full. Left: gaps1 `ui.stringalt.z` (the game's runtime compiler passed "sText", the variable's name) | gaps1 (1) | nothing in NVForever |
| **Steps the game did not run** (rows we give and the golden lacks: the Ex quaternion forms, a hex BuildRef, RemovePackageAt under GetFormIDString, SetBaseKarma with a fraction, two ITR calls, INI and setting misses) | math, arrays, sixth, strings, fourth, seventh, n7, leveled, plugin.odds (~20) | nothing in NVForever (the compiler is kinder than the game's runtime one, owner-review GECK cases 24-25); seventh's pkg.* rows after it follow from the skipped step |
| **Small rules still open** | GetFontFile and GetNthPerkEntryString closed (#944, jip.forms in full); FATan/FATan2 closed (the game's approximation, fitted from probes rules1/rules2, #1025); the ref walk's extra reference is the save's (VCG01CasualHardcoreMessageREF taken, #1023); GetRunSpeed still open (338.8 whatever Speed Mult, Agility or the movement settings, ext-last-core.md §4) | per row |

After #944 (2026-10-30): 20 of 60 `L5.ext.*` scenarios match in full (rgbui and jip.forms
added), no regressions.

The save reader (2026-11-01, #1023): `fnvtool ext queries … --save <nvfairport.fos>` starts
the run from the save (`fnv_save::fos`, save-files.md §6.4): globals and game time, quest
flags, stages, objectives and variables, SPECIAL, tag skills, skill points, the inventory
and the place. From nvfairport, gaps2 now counts 27 item kinds and gaps5 reads Guns 17,
the save's values. Not yet carried over: faction relations, reputations, references'
changes (positions, packages, disabled state, Doc's place), the talked-to bits, always-run
and difficulty. The `L5.ext.*` scenarios do not pass `--save` yet: the extender goldens are
on the other machine, so pointing them at the save (a `save_path` lab variable, the
decision in owner-review.md) and the pass count wait for a run there.
