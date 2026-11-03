# Detection states: Alert, Combat, Lost

Owner request 2026-10-11: NPCs go on alert when they sense a threat nearby, from
hearing a hostile move or seeing an enemy at a distance without having confirmed it yet,
as the foundation of the combat state of NPCs. Builds on detection (docs/design/
detection.md): the numbers there say how well an actor perceives another; this step
says what the actor does with them. Facts and sources: docs/spec-notes/actor-behaviour.md
§2.2–§2.3 (the GECK wiki Category:Detection page in the owner's export).

Tags as in the spec notes: [documented: source], [observed], [hypothesis], and
[engineering choice] where the original's rule is unknown and parity does not matter
(combat is not a parity target, owner 2026-10-09; only what scripts see must match).

## 1. What is built

| Part | Where | What |
|---|---|---|
| The states | `fnv_game::awareness` | `State` (Normal, Alert, Combat, Lost), each actor's `Awareness` (state, alert list, target list), the transitions (`Awareness::settle`), `Timers` |
| Who is a threat | `Game::would_attack` | an actor's aggression against its factions' reaction to the other (§3); `rest::hostile_to_player` is now this with the player |
| Feeding it | `Game::observe_threat`, `hear_event`, `hurt_by`, `settle_awareness` | called by the engine's detection after each round of measurements |
| Detection events | `Game::queue_detection_event` | CreateDetectionEvent, the impacts of the player's shots |
| Barks | `Game::detection_bark` | the Detection topics on each change and now and then while searching (§4) |
| Script functions | same module | IsInCombat, GetCombatTarget, CreateDetectionEvent, SetAlert, GetIsAlerted (§6) |
| Searching | `fnv_ai::alert` | actors run on their packages (`--npc-packages`) search in Alert and Lost and stand facing their target in Combat; Continue During Combat packages go on (§5) |
| How it looks | engine `alert.rs` | weapon drawn in a combat state; head turned to the alarm or the target |
| Measuring | engine `detection.rs` | the detector's state enters the formula (alert, busy); events measured; `alert:` log lines and `alert summary` |

## 2. The states

    Normal ──noticed hostile / event heard / hurt──▶ Alert ──hostile detected──▶ Combat
       ▲  ╲______________hostile detected_______________________________________▲  │
       │                                                                           │ every target undetected
       │◀── alarms ran out ── Alert                    Lost ◀──────────────────────┘ for `lost` seconds
       │◀── targets hidden and forgotten, alarms ran out ── Lost ──detected──▶ Combat
       │◀── no targets and no alarms ── Combat

[documented: Category:Detection, the transitions in actor-behaviour.md §2.3]. In words:

- A hostile **noticed** (above fSneakNoticedMin −20, at or below 0) puts an **alarm** on
  the actor's alert list at the hostile's position, with the detection value as its
  size. While the hostile stays noticed the alarm follows it and does not run out (the
  documented re-centring). Normal → **Alert**.
- A hostile **detected** (above 0) goes on the target list. From any state → **Combat**.
- **Events** (§4) the actor hears and **hurts** from a hostile are alarms too.
- An alarm runs out `alarm` seconds after it was last raised or refreshed; with no alarm
  left, Alert → Normal.
- A target hidden (−20 or below) for `forget_target` seconds leaves the target list. In
  Combat, when no target has been detected for `lost` seconds, the actor is **Lost**: an
  alarm at each target's last known position, and it searches there. Lost → Combat when
  one is detected again; Lost → Normal when every target is hidden or gone and the alarms
  have run out. Combat → Normal when the target list and the alert list are empty
  (targets dead, gone, no longer hostile).
- Both timers count a target as detected while a member of the actor's combat group in
  Combat still detects it (#972, docs/spec-notes/combat-groups.md §3) [hypothesis].
- An event heard while searching puts its owner on the target list, undetected
  [documented].
- Back in Normal the lists are cleared.

The timers [engineering choice: the documentation names them, gives no lengths, and
FalloutNV.esm has no game setting for them]: `alarm` 20 s, `forget_target` 15 s, `lost`
10 s (3 s until 2026-10-19: fighters said CombatToLost almost as soon as they broke line
of sight; `lost` stays below `forget_target` so a target hidden the whole time is lost
and searched for, not forgotten). They are `fnv_game::awareness::Timers`. The state is not saved (the engine measures
again after a load).

**"No Combat Alert"** (package flag 27): an actor whose current package has it does not
become Alert; it still goes to Combat [hypothesis: the flag's name; no source describes
it; 192 of FalloutNV.esm's 4,163 packages set it, observed 2026-10-11].

## 3. Who is hostile

`Game::would_attack(actor, other)`: "Enemy actors are attacked on sight by Aggressive and
Very Aggressive actors", "Neutrals will be attacked by Very Aggressive actors", allies and
friends are not [documented: GECK wiki GetFactionCombatReaction], the factions combined as
`GetFactionRelation` does (docs/spec-notes/factions.md §4); Frenzied attacks anyone
[hypothesis]. Neither may be dead, disabled, unconscious or restrained; the player's
teammates do not attack the player. Two actors with a faction in common, or placed from
the same base form, never attack each other [engineering choice: without it the Very
Aggressive creatures of one kind, neutral to each other, would fight]; since 2026-10-12
not when either's factions name the other's an enemy, and a shared faction that names no
reactions (GeneralIdleFaction) only counts when one of them has no faction that does
(`Game::kin`, combat-ai.md §12.4).

Only hostiles count: a non-hostile actor noticing the player stays Normal. Being hit by
the player (assault, friendly fire) and an ally's fight (Assistance) make an actor
hostile to the attacker through a grudge (`fnv_game::provocation`, combat-ai.md §10);
crimes do since (their witnesses and victims, `fnv_game::crime`,
docs/spec-notes/crime-karma.md); the player's reputation does not.

## 4. Events and barks

- **Detection events** [documented: Category:Detection, GECK wiki CreateDetectionEvent]:
  an owner, a sound level and a type (bullet, beam, explosion, generic). Every actor
  within reach measures it like a target that makes that sound and cannot be seen: the
  owner's Pair (its sneak when it sneaks), the distance to the event, the line of sight
  to it (for the sound's falloff), no visual term [hypothesis: an event is heard]. Above
  −20 an actor that would attack the owner and is not in Combat is alarmed at the
  event's place [documented].
- Raised by: CreateDetectionEvent (at the calling reference); every impact, at its IPCT's
  sound level, else the firing weapon's (100 / 50 / 10); every projectile in flight, once
  a measurement where it is, at its PROJ's level; every explosion with a known cause, at
  its EXPL's level, as an Explosion event [hypothesis for all three; detection.md §8].
- **Hurt**: an actor the player hits (and does not kill) that would attack the player is
  alarmed at the player's position [documented: Normal → Alert when a hostile hurts the
  detector]. Only the player fires, so the player owns every impact and hit until actors
  shoot.
- **Barks** [documented: GECK wiki Detection Tab]: on each change the actor says its line
  of the topic `<From>To<To>` (NormalToAlert, AlertToCombat, CombatToLost …), about the
  alarm's owner or its target (the conditions' target); while Alert it says AlertIdle,
  while Lost LostIdle, every 5–15 s (fCombatDetectionDialogueIdleMin/MaxElapsedTime,
  FalloutNV.esm's values) [documented; per actor, not per combat group as the original:
  engineering choice]. Not while it is already speaking, nor while a menu or a
  conversation holds the game. Not said yet: NormalToAlertResponse, StartCombatResponse.
- **Group cooldown** (2026-10-11, owner request: detection and combat lines were said
  "way too frequently … if an enemy in a group says a line … all the others don't shout
  the same line type"): after an actor says a Detection line, neither it nor a member
  of its group within 4,096 units (`BARK_GROUP_RADIUS`) says one of the same kind for
  15 s (`Timers::bark_cooldown`: the longer idle wait,
  fCombatDetectionDialogueIdleMaxElapsedTime) [design for all three]. A group is the
  same actor, the same base form or a faction in common (`Game::bark_mates`, the rule
  that keeps them from fighting each other) [design, written before combat groups were built: combat-ai.md §12]. The
  kinds (`bark_kind`): NormalToCombat, AlertToCombat and LostToCombat are one ("to
  combat"); AlertToNormal, CombatToNormal and LostToNormal one; AlertIdle and LostIdle
  one; NormalToAlert and CombatToLost each their own [design]. A line held back is
  logged at debug level (`alert: <actor> does not say <topic>: <who> said a line of
  that kind <s> s ago (group cooldown)`) and counted in the summary's `held back by the
  group cooldown`.

## 5. What actors do

- **Weapon** (engine `alert.rs`): in a combat state, or after SetAlert 1, the actor draws
  its weapon; back in Normal it holsters it [documented: Alert is "weapon out"].
- **Head**: in Alert and Lost it looks at its latest alarm (chest height), in Combat at its
  target; never over a look something else set (a conversation, a script's Look).
- **Searching** (`fnv_ai::alert`, actors run on their packages): in Alert and Lost the
  actor leaves furniture, walks to its latest alarm, looks about there for 4 s, then
  searches on: at the alarm again if it has moved more than 64 units, else at a random
  point within fCombatSearchInteriorMinRadius (256) or fCombatSearchExteriorMinRadius
  (512) of it [observed: the values in FalloutNV.esm; hypothesis: the nearest search
  ring's radius; the Combat Search page is not in the export]. It walks. A walk that has
  not arrived after 30 s ends where it is. [engineering choice for the times and
  distances.]
- **Combat**: it closes in to where it can attack and faces its target; the attacks are
  the combat AI's (docs/design/combat-ai.md). Cover and fleeing were built later (combat-ai.md §12.2, §12.6). A target
  it no longer detects (behind a rise, say) it keeps closing in on, whatever the
  distance, halving the gap each time the way is planned, until it sees it again or
  goes Lost; the combat AI keeps its distance to fight from meanwhile [design: below].
- **Continue During Combat** (package flag 26, owner's request 2026-10-11): an actor
  whose current package has it carries its package out in every combat state as if
  nothing had happened: no search, no stand [hypothesis: the flag's name; no source
  describes it. 399 of the 4,163 packages set it, among them sniper posts, "stand in
  place" and "flee the scene" packages, observed 2026-10-11]. Its state, weapon and barks
  are as for any other actor.
- Back in Normal the actor goes back to its package's place, as after a random
  conversation. Actors not run on their packages stand where they are and only turn
  their heads.

## 6. What scripts see

| Function | Now | Source |
|---|---|---|
| IsInCombat | 1 in Alert, Combat and Lost; for the player, 1 while an actor in a combat state has the player on its target or alert list | [documented: Category:Detection; GECK wiki IsInCombat, "will return 1 if enemies are actively searching for the player"] |
| GetCombatTarget | in Combat, the detected target with the highest value; else none | which of several: [engineering choice] |
| CreateDetectionEvent | queues an event at the calling reference (§4) | [documented] |
| SetAlert / GetIsAlerted | SetAlert's flag is kept on the reference (savebaked); GetIsAlerted is 1 with it or in a combat state | [documented: "purely an animation state"]; the combat state drawing it: [hypothesis] |

Furniture's fast getting-up set, chosen under `GetIsAlerted == 1` (furniture.md §2), now
applies to alerted actors.

Not yet when this was written: OnStartCombat / OnCombatEnd blocks, IsHostilesNearby,
GetPCDetectionState, combat groups, crimes. Built since: OnStartCombat and OnCombatEnd
(`fnv_game::awareness`), combat groups (combat-ai.md §12), crimes (crime-karma.md).
IsHostilesNearby (JohnnyGuitar) and GetPCDetectionState (JIP) are extender functions, left
to the extender work (extender-compat.md). StartCombat, StopCombat, GetAttacked and allies joining are in
combat-ai.md §6 and §10.

## 7. Line of sight, fixed on the way

Two faults in detection's line of sight showed in the first test (an alerted Fiend
switching between Combat and Lost every second):

- The ray stopped at the first actor's capsule and called the view clear, without
  looking behind it; the detector's own capsule (the ray starts inside it while physics
  moves the actor) made everything behind visible. The ray now goes on past actors.
- Small transparent things blocked sight: a "Transparent Small" fence beside the Fiend's
  path. Sight now passes the transparent layers, invisible walls, clutter, small debris
  and shell casings [hypothesis: by the layers' names].

The log says which: `out of sight (blocked)` or `out of sight (out of view)`.

## 8. Checks

- Unit tests: `fnv_game::awareness` (transitions, timers, events, No Combat Alert);
  `fnv-ai` tests `an_alerted_actor_searches_where_the_alarm_is_and_goes_back_after`,
  `continue_during_combat_keeps_the_package_going`,
  `an_actor_in_combat_stops_and_faces_its_target`.
- In the game, Violet among the Fiends, the player 3,000 units east at noon, standing:
  `nvforever --data <Data> --cell Goodsprings --pos=-43900,64896,7000 --play --hour 12 --no-voice --npc-packages FiendVioletRef --run-seconds 45`
  [observed 2026-10-11]: `alert: 1EFiendViolet 000F570B Normal → Alert: noticed the
  player, value -10.6, 3019 units away`, her NormalToAlert line, `searches for the player`,
  AlertIdle, `Alert → Combat: detects the player` at about 2,650 units, `stops and faces
  the player`, and `alert summary: 1 actors became Alert; changes: Normal → Alert 1,
  Alert → Combat 1; …; 2 Detection topic lines said`.
- The same 3,600 units off, sneaking, firing at her, with `NVF_PLAY_TEST_SCRIPT_MENUS=1
  NVF_PLAY_TEST_EQUIP=000E3778 NVF_PLAY_TEST_GIVE=0008ED03:50
  NVF_PLAY_TEST_CONTROL=0.1:sneak,14:weapon,17:reload,19:aimnpc,20:fire,20.2:fire` and
  `--pos=-43300,64896,7000 --run-seconds 40`: no alert before the shot; then `heard a
  bullet event of the player (level 50 …, 273 units away)`, `Normal → Alert: heard a
  bullet event`, a search at the impact, and `1 detection events, heard 1 times`.

- **Group cooldown** (2026-10-11): Cook-Cook's camp, the player 1,700 units south at
  noon, god mode: `NVF_PLAY_TEST_GOD=1 NVF_PLAY_TEST_SCRIPT_MENUS=1 nvforever --data
  <Data> --cell Goodsprings --pos=-43200,73000,7000 --play --hour 12 --no-voice
  --run-seconds 45`. Before: 12 Detection lines in 45 s (four NormalToCombat within
  29 s, eight CombatToLost from two fiends, one every 2–4 s); after: 4 lines (two "to
  combat", two CombatToLost, 15 s or more apart in the group) and 32 held back. The
  camp's fiends switch between Combat and Lost every few seconds (17 times each way in
  45 s); that flicker is older than this change and not looked into here.
- **The camp's Combat ↔ Lost flicker, fixed** (2026-10-12, playtest list): the fiends
  ran at the player, dropped behind a rise (`out of sight (blocked)`), and the combat AI
  let go of them the moment they did not detect the player: without a distance to fight
  from, the AI had them stand and face the player where they could not see, so after
  `lost` (3 s) they went Lost, searched, saw the player again and went back to Combat.
  Now the combat AI keeps a fighter (and its distance) for as long as it is in Combat,
  fighting the target it detected last, and an actor that does not see its target closes
  in on it. The camp, 45 s [observed 2026-10-12]: `alert summary: … Normal → Combat 4,
  Alert → Combat 1;` and no Combat → Lost at all (before: 17 each way).
- The Violet check above, on main 9ecb1be as on this branch (2026-10-11): she becomes
  Alert and sets off to search, but stays within 200 units of where she stood for the
  whole run (`ai summary … 1 on their way`) and never reaches Combat; reported, not
  looked into.

## 9. Next

- Combat AI on top of Combat: attacking with the actor's weapon, combat styles, cover,
  fleeing; OnStartCombat and the other script hooks; combat groups.
- Crimes and assault making actors hostile; NormalToAlertResponse; explosions as events.
- Tuning the timers and the search against the game (captures of GetDetected and
  IsInCombat over time, actor-behaviour.md §8).
