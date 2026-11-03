# vheyes-nvse's functions (2026-11-01, #1016)

The 21 vheyes-nvse functions, built in `crates/fnv-game/src/nvse_vheyes.rs`. Sources:
the GECK wiki export [documented: the page named]. vheyes-nvse is not installed in the
probe game, so nothing here is `[observed]`.

## 1. The moon

| Function | NVForever |
|---|---|
| SetMoonFull, SetMoonDoom, SetMoonDoomSet `enabled` | flags kept [documented]; texture precedence when full: doom > doom set > vanilla [documented] |
| GetMoonDoomState | 2 doom, else 1 doom set, else 0 [documented] |
| SetMoonScale `scale` | a multiplier above 0; 0 or below hands the size back [documented] |
| SetMoonGrowth `base perDay max anchor` | the scale `base × perDay^(days − anchor)`, at most `max`, never below `base` [documented] |
| SetMoonShadowMult `mult [always]` | kept [documented] |
| SetMoonFade `duration doomSet doomTex full` | the three flags change at the midpoint; a call while one runs is ignored [documented]; real seconds, menus included [hypothesis] |
| GetMoonAngle | what the engine's sky tells (`Game::set_moon_angle`), 0 before [documented: 0 with no sky; follow-up: the sky does not tell yet] |

The engine's sky reads the look through `Game::moon_look(days)` (flags, scale, the fade's
alpha) — follow-up: not wired yet.

## 2. Reference 3D refresh

NVForever loads a reference's 3D without the original's background clone queue
[decision]: GetRefCloneQueueCount reads 0 and IsRefMidClone 0 (the "settled" answers).

| Function | NVForever |
|---|---|
| DetachRef3D | 1 for a placed reference other than the player: its scene mirror forgotten; else 0 [documented] |
| QueueRef3D | 1: the reference waits for the engine's rebuild (`Game::take_rebuild_3d`); 0 for the player or no reference [documented] |
| StampRef | the reference's stamp is its base's model path now; 1, or 0 with no model path [documented] |
| QueueRefClone | 0 when the stamp matches, 1 when the path changed; first touch stamps and gives 0 [documented]; a reference with no model path 0 [hypothesis] |
| QueueRefCloneHard | as QueueRefClone, but an unstamped reference gives 1 and is not stamped [documented] |
| DeferRefUpdate3D `[stampFlag]` | 1 registered (0 for the player, no reference, or a full list of 4096 [hypothesis: the size]); the next frame each still valid reference is rebuilt and, with the flag, stamped [documented] |
| ClearRefStamps | every stamp and the waiting list wiped; 1 [documented] |
| ArmQueuedConsoleCommand `[delayTicks]` | 1; `delayTicks` (2) frames later the lines (up to 8) of `Data\Config\vheyes-nvse\QueuedCommand.txt` run as console lines, once [documented]; the file is read when they run, NVForever's layer first (`crate::mod_files`) |

## 3. Stored ammunition

GetStoredAmmoRounds (GetStoredClip) −1 and GetStoredAmmoType (GetStoredAmmoForm) 0, the
pages' "no stored data" [decision: NVForever keeps no Stewie's Tweaks "Remember Weapon
Ammos" data].

Follow-ups: the engine's sky does not read `moon_look` or tell the angle, and its model
loader does not take `take_rebuild_3d`. Until the engine takes them, the three engine
queues of these batches (`take_rebuild_3d`, `take_clamp_requests`, `take_allow_attack`)
hold a reference once however often it is asked for, and at most `QUEUE_LIMIT` (1,024),
the oldest going first, so a script asking every frame cannot grow them
(`nvse_vheyes::queue_once`) [decision].

The moon fade's clock is the same real-seconds clock as CacheUI's typewriter (frame
lengths added up, `Game::time_real`); a zero fade changes the looks at once without
running the rest of the frame's work (the deferred 3D flush, the queued console lines)
[decision].
