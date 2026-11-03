# Spec notes: combat groups (alarm reach, whom joiners fight, shared targets)

How the actors of one side of a fight help each other: who hears an alarm, whom an
alarmed actor is sent against, and how long a group goes on fighting a target only some of
its members see. Code: `crates/fnv-game/src/combat_group.rs` (`Game::raise_alarms`,
`Game::first_starter`, `Game::share_sightings`) and `crates/fnv-game/src/awareness.rs`
(`Awareness::settle`). The rest of the combat groups (threat ratios, flight, searches) is
docs/design/combat-ai.md §12; issue #972.

What the original does is from one observer run of the Goodsprings gunfight and one at
Cook-Cook's camp, with the JIP functions [observed: docs/handoff/observer-jip-audit-2026-10-25.md
§3.3, §3.5, §6 item 5]. Combat mechanics are not a parity target (owner: modern design is
allowed); what matters for quests is that townsfolk and animals join a fight on their
town's side.

## 1. Who hears an alarm

- **The original** [observed: §3.5]: in the Goodsprings gunfight an actor's combat allies
  stood a median 1,044 units from it, 4,864 at the 95th percentile and up to 9,026
  (19,752 pairs).
- **NVForever** [design]: an actor that starts fighting or notices a hostile alarms
  comrades within `ALARM_RADIUS`, **9,216 units** (the farthest observed pair rounded up
  to a quarter cell; 2,048 until playtest list 5 item 31, 4,096 until #972).
- Hearers are the actors measured in that detection round (drawn, in the high process)
  **and** the other actors of the loaded cells, at the position the world keeps for them
  [design: the original's allies reached past the actors near the player; our engine
  measures only drawn actors]. Who among them answers is unchanged (combat-ai.md §12.3:
  its own kind, or an ally or friend whose Assistance helps the sender; never against its
  own ally or friend; a coward stays out).

## 2. Joiners fight the first of the foes to start

- **The original** [observed: §3.5]: the first OnStartCombat events were Gangers 3, 6, 2
  and 5 against Sunny; every townsperson's own OnStartCombat then named **GSPG03Ref**, the
  first Ganger to start, whichever Ganger the sender was fighting.
- **NVForever**:
  - Each fighter's first move to Combat in a fight is remembered (`Tactics::started`,
    forgotten when it calms) [design].
  - The first starter of a foe's side is the member of the foe's combat group (its
    comrades in a combat state with a foe in common) that went to Combat earliest, ties by
    FormID; the foe itself when none of them is fighting (the player, for one)
    [design on the observation]. A member dead or out cold (knocked out, or set
    unconscious) is passed over: the next to start is named [design].
  - An alarm names that first starter, at its position when it was measured (or where
    the world has it in the loaded cells), instead of the sender's own foe, unless the
    starter is the hearer or the hearer's ally or friend (then the sender's foe, as
    before) [design].
  - A hearer that would not attack them otherwise (a neutral helping by its Assistance)
    holds an Assist grudge against both the sender's own foe and the starter, so it can
    still fight the foe in front of it once the starter is gone [design].
  - An actor alarmed by an ally (`Cause::Ally`) that is not fighting anyone yet takes the
    alarm's owner as its target once it detects it, and holds it for the target
    commitment (alert.md, `TARGET_COMMIT`), even when it detects another foe better
    [design: so its fight is against the starter, as the townsfolk's was]. Until it
    detects the starter it fights whatever it detects best, as before.

## 3. A group keeps a target while a member detects it

- **The original** [observed: §3.3]: at Cook-Cook's camp five Fiends stayed in combat with
  the hidden player for the whole half hour, kept it as their combat target and closed
  from 1,466 to about 110 units, though only 0 to 5 of them (often 1 or 2) detected it at
  any moment.
- **The reading built here** [hypothesis, not yet captured: observer-jip-audit §6 item 5
  says the "some member detects" reading is unconfirmed; observer-checks.md has the
  question]: a fighter neither loses nor forgets a target while a member of its combat
  group still detects it.
- **NVForever**:
  - Before each round's settling, every fighter in Combat learns, for each target it does
    not detect now, the latest time a comrade in Combat with the same target detected it
    and where (`Awareness::shared`; the target's last known place moves there, so the
    fighter closes in on it) [design].
  - The Lost timer (10 s) runs from the later of its own last detection and the group's;
    the forget timer (15 s) keeps a hidden target on its list while the group detected it
    within that time [design on the hypothesis].
  - It does not count as seeing the target: it fires only at what it detects itself
    [design].
  - When nobody in the group detects the target any more, every member goes Lost the Lost
    timer after the group's last sighting, together.
