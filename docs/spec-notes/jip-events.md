# Spec note: JIP event handlers (`SetOn…EventHandler`, `SetGameMainLoopCallback`)

JIP LN NVSE's own event registrations: one function per event, each binding a UDF
script to that event, with filters as arguments. Task J2 of
[jip-tasks.md](../handoff/jip-tasks.md); input for J6. They sit beside xNVSE's
`SetEventHandler` (ext-events.md, task E1). JIP's events reachable through
`SetEventHandler` by name are in ext-events.md, not here.

Wiki statements are `[documented: <page>]` (private export `C:\fnv-private\geckwiki\`,
in our own words). Results from the original game (JIP 57.30, xNVSE 6.4.9) are
`[observed: jip-events/<id>]`: handlers registered as xNVSE lambdas by runtime-compiled
queries (`tools/capture/jip_probes.py --set jip-events`, 2026-10-26, 17 rows, the 12
documented values all match; golden private). See §4.

## 1. The common shape

`SetOn<Event>EventHandler <handler UDF> <1 set | 0 remove> [filters…]` [documented: every
page below].
- The handler is a UDF script form: `Begin Function {args}`. Its arguments are the
  event's values, in the order given in §2. Where an actor or a projectile is
  involved, that reference is the calling reference (`this`) [documented].
- Removing takes the same filters as setting: `… 0 SomeActor` removes the `… 1
  SomeActor` registration [documented: the examples].
- **Actor filters** (`actorOrList`) take one reference or a form list of references.
  For AnimAction and PlayGroup, an **empty** form list means any actor [documented:
  SetOnAnimActionEventHandler, SetOnPlayGroupEventHandler]. For FireWeapon the filter
  is optional; left out, it means any actor [documented: SetOnFireWeaponEventHandler].
- SetOnQuestStageEventHandler returns success 0/1. The others return nothing
  [documented: their pages].
- **Lifetime.** SetOnRagdollEventHandler's registrations are flushed on every game
  load, so they must be made again after a load [documented: SetOnRagdollEventHandler].
  For the other events the pages say nothing [hypothesis: the same, as JIP keeps them in
  memory only; NVForever clears every JIP registration on load and on return to the main
  menu, matching the one documented case].

## 2. The events

| Function | Filters after set/remove | Handler gets | Fires when |
|---|---|---|---|
| SetOnHitEventHandler | target actor or form list (optional) | this = struck actor; no arguments | an actor is hit. Runs **before** the damage and before the Verbose message; once per shotgun pellet; up to three times for an explosion: GetHitProjectile gives 0 the first time, then the projectile or the explosion, then the explosion [documented] |
| SetOnHealthDamageEventHandler | actor or form list | this = actor; damage (**negative**), source actor or 0 | an actor loses Health, including through DamageAV. Poison: the source is the victim. Collision: 0 [documented] |
| SetOnCriticalHitEventHandler | target, source, weapon (each optional; `XMarker` = no filter, since 0 is refused) | this = struck actor; source, weapon | a critical hit [documented] |
| SetOnCrippledLimbEventHandler | actor or form list | this = actor; limb 0 head, 1 torso, 2 left arm, 3 right arm, 4 left leg, 5 right leg, 6 brain | a limb is crippled [documented] |
| SetOnFireWeaponEventHandler | actor or form list (optional) | this = actor; weapon base form | a ranged weapon fires a round. Not when it jams; **not for the player in god mode** (demigod works) [documented] |
| SetOnReloadWeaponEventHandler | — | this = actor; weapon base form | any actor reloads [documented] |
| SetOnProjectileImpactEventHandler | projectile form or form list | this = projectile reference; no arguments. Details through GetProjectileRefImpactRef, …Weapon, …Source and GetPos | a projectile of that type hits anything, terrain included [documented; JIP 54.91] |
| SetOnRagdollEventHandler | — | this = actor | any actor ragdolls: death, paralysis, knockdown, low fatigue, unconsciousness [documented; JIP 55.26] |
| SetOnAnimActionEventHandler | actor or list, anim action 0–13 (GetAnimAction) | this = actor; action | the actor starts that action [documented] |
| SetOnPlayGroupEventHandler | actor or list, anim group keyword | this = actor; group index | the actor starts playing that group [documented] |
| SetOnUseAidItemEventHandler | ingestible or form list | this = actor; item | an actor consumes it [documented] |
| SetOnDialogTopicEventHandler | topic or info (an info by hex form ID) | this = speaker; the topic or info | it is spoken. With JohnnyGuitar 5.24 the topic may be left out [documented] |
| SetOnQuestStageEventHandler | quest, stage, skipScript (optional) | quest, stage | `SetStage quest stage` runs from any script. With skipScript non-zero, the stage's **result script does not run** [documented] |
| SetOnNoteAddedEventHandler | — | the note | a note is added to the player [documented] |
| SetOnLocationDiscoverEventHandler | — | the map marker reference | the player discovers a location [documented] |
| SetOnFastTravelEventHandler | — | the marker reference travelled to (not always a MapMarker-based reference, form 0x10; the page shows following linked children to the real marker) | the player fast-travels [documented] |
| SetOnCraftingEventHandler | — | item base form, quantity | an item is made in the Recipe menu [documented] |
| SetOnPCTargetChangeEventHandler | — | the new target actor, or null | the actor under the reticle changes (range: fAutoAimMaxDistance) [documented] |
| SetOnKeyDownEventHandler / …KeyUp… | DirectX scancode | the key | a key is pressed / released. Fires even when the key is disabled with DisableKey [documented] |
| SetOnControlDownEventHandler / …ControlUp… | control code | the control | a control is pressed / released [documented] |
| SetOnTriggerDownEventHandler / …TriggerUp… | 0 left, 1 right | the trigger | a controller trigger is pressed / released [documented; Controller.xml] |
| SetOnMenuOpenEventHandler / …MenuClose… | menu ID (optional: any menu) | the menu ID | a menu opens / closes. It must be a **specific** menu ID: a menu *type* such as 1 errors or crashes [documented] |
| SetOnMouseoverChangeEventHandler | menu ID (optional) | the menu ID | the highlighted item in that menu changes [documented] |
| SetOnMenuClickEventHandler | tile path: `"Menu#<id>"`, a full path or a prefix | menu ID, tile id (-1 if none), tile name **lower-case**; list items get `:<index>` | a matching tile is clicked [documented] |

Observed with SetOnHealthDamageEventHandler on the player and `DamageAV Health 1`:
- The handler gets -1 as the amount, and the actor is the calling reference (GetSelf is
  the player). The source passed for DamageAV is null: storing it with AuxVarSetRef
  leaves no element [observed: ev.amount, ev.this, ev.source.type]. All as documented.
- **Several handlers for one event run newest first**: the second registration ran
  before the first (two runs) [observed: ev.order.size = 2, ev.order.first = 2]. The
  pages do not say. NVForever follows the game for JIP's events. This is the reverse
  of SetEventHandler's tie order (ext-events.md §7).
- **A handler that declares fewer parameters than the event passes never runs**:
  handlers with no parameters and with one (the event passes two) were registered
  (the call succeeded) but did not run [observed: ev.noparams.ran, ev.oneparam.ran = 0].
  NVForever registers them and skips them, with an extender log line saying why
  [decision: the game gives no message].
- Not yet observed: whether one handler can be registered twice (a lambda is a new
  script each time, so it needs a GECK-compiled UDF). A second identical registration
  is kept once [provisional decision; ext-events.md leaves the same open for
  SetEventHandler, while SetEventHandlerAlt runs duplicates twice].

## 3. SetGameMainLoopCallback (alias SGMLC)

`[ref.]SetGameMainLoopCallback <UDF> <1|0> [delay frames, default 1] [mode flags,
default 3]` [documented: SetGameMainLoopCallback].
- The UDF takes no arguments. It runs every `delay` frames. Flags: 1 in game mode, 2 in
  menu mode, 8 removed on load or on return to the main menu.
- Called on a reference, the UDF runs with that reference as `this`; otherwise on the
  player.
- Example on the page: delay 30 is described as half a second, which assumes 60 frames
  per second. NVForever counts frames, as documented, not time [decision].
- Observed: a lambda registered with delay 1 and flag 1 ran, with GetSelf the player
  [observed: loop.ran, loop.this]. How many times per frame was not measured: a capture
  query does not run on every game frame.

## 4. Evidence still to get

Runtime queries reach the events through **lambdas** (§2, done for health damage and
the main-loop callback). Rules for writing those probes [observed: jip_ev_trial2,
jip_ev_trial3]:
- type the parameters in the braces, `Begin Function {float fD, ref rS}`; parameters
  declared before `Begin` do not compile;
- use `GetSelf`, not `this` (`this` does not compile there);
- the steps share the session: register, cause the event, then read.

Still open:
- **GECK-compiled probes** (when the owner is back): a quest registers handlers on
  GetGameLoaded, causes the event (SetStage, DamageAV, a forced reload), and prints
  `EXTPROBE|…` lines, in the same pattern as `tools/capture/ext-probes/udf`. Cases to
  settle: the order of two handlers, double registration, and whether a registration
  survives a load (the §1 hypothesis).
- More events by lambda, each with a harmless trigger inside the session: quest stage
  (SetStage with skipScript), use aid item (a stimpak added and used), fire and reload
  (an NPC's AI makes these slow to trigger by script).

## 5. Where NVForever fires them

Registration is one rule for every `SetOn…EventHandler` (`fnv_script::vm::jip`); an event
runs only where the game fires it (`Game::jip_event`):

| Event | Fired | Since |
|---|---|---|
| hit, healthdamage, crippledlimb | the hit and its damage (`combat.rs`); DamageAV for health damage | J6 |
| useaiditem | an actor consumes an ingestible (`aid.rs`) | J6 |
| noteadded, queststage, mouseoverchange, menuclick | the note, SetStage, the menus' tiles | J6, T8 |
| projectileimpact | a shot in flight hits anything, terrain included: `Game::projectile_impact`, on the shot's temporary reference; the filter is matched against its PROJ base | #1408 |
| menuopen, menuclose | `Game::open_menu`, `Game::close_menu`, the menu under the tutorial menu closing, and a Pip-Boy page left for another [hypothesis: each page is a menu of its own code]. The dialogue menu (1009) is not reported | #1408 |
| keydown, keyup | each frame, the keys and mouse buttons (256 + n) held by the player that changed since the last (`Game::jip_keys`), disabled keys included [documented] | #1408 |
| crafting | the recipe menu made an item: once per output, with all of it (`Game::item_crafted`) [hypothesis: the page does not say how a recipe made several times is reported] | #1408 |
| locationdiscover | the engine found a map marker, with the marker reference (`Game::challenge_marker_found`) | #1408 |

Not fired yet: criticalhit, fireweapon, reloadweapon, ragdoll, animaction, playgroup,
dialogtopic, fasttravel, pctargetchange, controldown/up, triggerdown/up.
