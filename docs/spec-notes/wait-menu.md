# Spec note: the Wait and Sleep menu

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` and
`[engineering choice]` mark what is NVForever's own. Implementation:
`engine/nvf-play/src/wait.rs` and `wait/model.rs`; what scripts see and ask,
`crates/fnv-game/src/rest.rs`; the people during a wait, `fnv_ai::Ai::pass_time` and
`engine/nvforever/src/residents.rs`.

## 1. Sources

- **The owner's screenshot** of the original game's Wait menu, 1280 x 720, taken
  2026-10-11 (kept privately, not in the repository). Every "[observed: screenshot]"
  below is read or measured on it.
- **The game's interface files** in the base archive `Fallout - Misc.bsa`:
  `menus\sleep_wait_menu.xml` and the shared pieces it includes
  (`menus\prefabs\scrollbar_horiz.xml`, `text_box.xml`, `top_bracket.xml`,
  `bottom_bracket.xml`, `menus\globals.xml`), and the two textures it names in
  `Fallout - Textures2.bsa`. Described here in plain words; nothing of them is shipped.
- **FalloutNV.esm**: game settings, globals, the Well Rested records and the scripts
  that mention sleeping, read with `fnvtool`.
- **The GECK wiki**, from the owner's export (function pages and the "Settings" page).
- The first-person memory of how the game plays is *not* a source here. Where nothing
  above answers a question, the note says so.

## 2. What the menu looks like

On the screenshot [observed: screenshot]:

- The world behind the menu is blurred, everywhere on the screen. Inside the box it is
  also much darker: the sky is at about a quarter of its brightness outside. The dark
  area starts a little above the box's top line with a soft edge.
- A box 643 x 300 pixels in the middle of the 1280 x 720 screen. Its outline is a thin
  amber line along the top and one along the bottom; at each of the four corners a short
  vertical line of about 30 pixels that fades out towards the middle of the side. The
  sides are otherwise open.
- A title line, centred, about 60 pixels under the top: "How long would you like to
  wait?".
- The slider, 140 pixels under the top: a horizontal line 3 pixels thick and about 516
  long, with short marks standing on it every 19.4 pixels. There are 26 marks: the first
  and the last are taller and have a small foot pointing outwards, the 24 between them
  are the hours. A filled triangle pointing down, about 23 pixels wide and 12 high,
  stands above the mark of the chosen value; at "22 hours" it is over the 22nd of the 24
  marks.
- Under the slider, centred, in the largest type: "22 hours (8:34)".
- Lower, centred, in smaller type: "Saturday, 19.10.81, 10:34".
- At the right, one above the other and right-aligned, their right ends a few pixels
  outside the box's right corner lines: "Wait E)" and "Cancel X)". "Wait E)" is at
  about 0.59 of the brightness of "Cancel X)".
- The mouse pointer is on the screen (outside the box in this shot).
- All of it is one colour, the amber of the interface; the slider's line measures
  (179, 140, 65).

In the interface file [documented: `menus\sleep_wait_menu.xml` and the files it includes]:

- The menu is one box 860 x 400 of the interface's own units, centred on the screen. The
  screenshot's box is 643 x 300, so one unit is three quarters of a pixel at 720 pixels
  of height [observed]: the interface is laid out for a screen 960 units high.
- Six parts are addressed by number by the game's code: 0 the title, 1 the slider, 2 the
  hours line, 3 the date line, 4 the accept button, 5 the cancel button. The three lines
  and the accept button have no text in the file ("set in code"); the cancel button
  names the game setting `sCancel`.
- The outline is two shared "bracket" pieces, one at the top and one at the bottom, each
  a full-width line with a fading vertical line 50 units high at both ends. This agrees
  with the screenshot.
- The slider's picture is one texture (`Interface\Wait\wait_meter.dds`, 512 x 64: a
  line, a taller post at each end and 24 short ticks between them [observed: the
  texture]) and its marker another (`Interface\Wait\wait_pointer.dds`, a triangle
  pointing down). The slider itself is the shared horizontal scroll bar with 24 items,
  one visible, a step of 1 and a "jump" of 6, without arrow buttons. Its value runs from
  0 to 23, and the marker's place is the value times a fixed step.
  [hypothesis: value 0 to 23 is shown as 1 to 24 hours; the marker's computed place for
  value 21 is where the screenshot has it at "22 hours".]
- The two buttons are the shared text button: the text is fully opaque while the button
  is "targeted" (the pointer is on it, or it is chosen with a gamepad) and half opaque
  otherwise; a frame is drawn around it only then. On PC a key hint is written after
  the label. [hypothesis: in the screenshot Cancel is the targeted one and Wait is not,
  which fits the 0.59.]
- The file has a black panel behind the box, a little larger than it, but at a low
  opacity, and nothing in it asks for a blur. The blur and most of the darkening on the
  screenshot therefore come from the game's code, not from the file [hypothesis].
- All colour is the interface colour; no part sets its own.

## 3. Wording

- The texts come from game settings [documented: GECK wiki ShowSleepWaitMenu: the menu's
  text comes from `sHowManyWait`, `sSleep` and `sWait`; "Settings" lists those and
  `sDaySunday` to `sDaySaturday`, `sMenuDisplayDayString`, `sAMTime`, `sPMTime`]. None of
  these has a record in FalloutNV.esm [observed], and the wiki gives no values, so their
  values are inside the game's program and are known here only as far as the screenshot
  shows them: the wait title, "Wait", "Cancel", "hours", "Saturday" [observed:
  screenshot].
- Not known from any source: the Sleep menu's title, the label of its accept button
  (the setting is named `sSleep`), the singular of "hours", the other six day names as
  the game writes them, and what `sHowManyWait` holds exactly (the whole title, or a
  pattern the verb is put into).
- NVForever writes "How long would you like to sleep?" and "Sleep" [hypothesis: the
  wait title with the other verb], "1 hour" [hypothesis] and the English day names. A
  plugin that sets `sHowManyWait`, `sWait`, `sSleep`, `sCancel` or a `sDay…` setting is
  followed.

## 4. The two lines of time

- The hours line is the chosen number of hours and, in brackets, a time of day.
  [hypothesis: the time of day at the end of the wait. 10:34 + 22 hours is 8:34, as
  shown.] Hours have no leading zero, minutes two digits, 24-hour clock [observed for
  8:34 and 10:34; hypothesis for the rest].
- The date line is the game's date and time now [hypothesis]: weekday, then day, month
  and year with dots, then the time. The month is the GameMonth global plus one:
  GameMonth counts from 0 and 9 is October (simulation.md §5b [observed]), and the line
  shows 10. The year shows two digits, 81 for 2281 [hypothesis: the last two].
- The weekday. `GetDayOfWeek` returns 0 for Sunday to 6 for Saturday [documented: GECK
  wiki GetDayOfWeek]. GameDaysPassed counts from "10/13/81 (October 13, 2281)"
  [documented: GECK wiki GetGameDaysPassed], and the plugin starts the game on the 18th
  with GameDaysPassed 5 [observed: FalloutNV.esm globals], which fits. The screenshot
  shows a Saturday on 19.10.81. On the Gregorian calendar 19 October 2281 is a
  Wednesday, so the game does not use that. [hypothesis: the weekday is the number of
  days since 13 October 2281, which is then a Sunday, modulo 7. One screenshot fits it;
  whether the game counts whole GameDaysPassed (which would change the weekday at the
  hour of day the count started, noon in an untouched new game) or dates (changing at
  midnight) is not known. NVForever counts dates, on the game's calendar of 365-day
  years.]
- This disagrees with `fnv-game`'s `GetDayOfWeek`, which uses the Gregorian weekday
  [hypothesis there]. That function also decides AI packages scheduled by weekday. It is
  not changed here: see docs/owner-review.md.

## 5. Scripts

- Menu code 1012 is this menu: six scripts in FalloutNV.esm have `Begin MenuMode 1012`
  blocks, among them a medicine timer whose comment speaks of the rest menu starting
  [observed: `fnvtool script decompile`]. (The wiki's table of menu codes is missing
  from the export.) NVForever opens and closes menu 1012 in the script runtime with the
  menu, so those blocks run while it is open.
- `ShowSleepWaitMenu isSleep [checkPreconditions]` opens it from a script: 0 the Wait
  menu, 1 the Sleep menu [documented: GECK wiki ShowSleepWaitMenu].
- `IsPCSleeping`: the player is "sleeping in bed (not waiting)". `GetPCSleepHours`: the
  hours set in the menu, 0 when not sleeping. `SetPCSleepHours`. `WakeUpPC`: closes the
  sleep menu as the Cancel button does, and does not work while waiting. `IsTimePassing`:
  1 while sleeping, waiting or travelling [documented: GECK wiki, the pages of those
  names].
- `GetSecondsPassed` "is unreliable" in an effect script during sleep, wait and fast
  travel [documented: GECK wiki GetSecondsPassed].
- How the game's own scripts use them [observed: `fnvtool script decompile`,
  FalloutNV.esm]:
  - `IsPCSleeping`: one script, the beds' (SCPT 0x000B5431, §8).
  - `SetPCSleepHours`: three quest scripts (0x001446AE, 0x0013F374, 0x001672C8), each
    in game mode, two of them right after setting the GameHour global themselves
    ("set GameHour to 5", then `SetPCSleepHours 1`).
  - `WakeUpPC`: one script (0x0014BDAF), every frame of its `MenuMode 1012` block and
    once in game mode.
  - `ShowSleepWaitMenu`, `GetPCSleepHours`, `IsTimePassing`: no script of FalloutNV.esm.
- Opcodes [documented: the function catalogue, `schemas/script/functions.json`]:
  `IsPCSleeping` 0x10AF, `WakeUpPC` 0x10AE, `SetPCSleepHours` 0x10BD,
  `ShowSleepWaitMenu` 0x1279 (all four observed in compiled scripts), `IsTimePassing`
  0x1109 (from its condition index). `GetPCSleepHours` has no opcode in the catalogue.
- NVForever (`fnv_game::rest`):
  - `IsPCSleeping` is 1 from the moment a sleep's hours are accepted until the menu
    closes [hypothesis: nothing says when it starts; the beds' script reads it in menu
    mode to learn that the player slept, which fits]. `IsTimePassing` is 1 for the same
    time of a sleep or a wait (there is no fast travel yet).
  - `SetPCSleepHours n`: the Sleep menu comes up with `n` hours already passing (at
    most 24), wherever the player is, with the fade of a sleep; no refusal applies
    [hypothesis: "forces the player to sleep" starts a sleep as if accepted in the
    menu]. Called during a sleep, the count starts again at `n`.
  - `WakeUpPC` stops a sleep as Cancel does; during a wait and outside the menu it
    does nothing [documented].
  - `ShowSleepWaitMenu isSleep [check]` opens the menu. With `check` set it is the
    Sleep menu whatever `isSleep` says [documented: the wiki's note on the original]
    and the refusals of §9 apply; without it they do not.
  - `GetPCSleepHours` is not registered: its opcode is not known. Its value is kept
    (`Rest::sleep_hours`).

## 6. Controls

- The control is "Rest" (T by default; controls.md §2). Control code 15 is "Rest"
  [documented: GECK wiki TapControl].
- The Wait menu cannot be brought up while a script has the Pip-Boy group of the
  player's controls off [documented: GECK wiki DisablePlayerControls; controls.md §5].
- In the interface file [documented]: with a gamepad, left and right move the slider one
  step, the two triggers six, one face button accepts and another cancels; the mouse
  wheel moves the slider (wheel up to the right), the marker can be dragged, and the two
  buttons can be clicked. For the keyboard the file only gives each button a "PC" slot
  with a hint text; the screenshot shows the hints as "E)" for accept and "X)" for
  cancel [observed].
- Which keyboard keys move the slider is not documented in any source read.
- NVForever [engineering choice where not marked]: E accepts and X cancels
  [hypothesis: the hints are the keys]; Enter also accepts; Esc and the Wait control
  itself also cancel; Left / Right and A / D move the slider one hour, Page Down / Page
  Up six [the file's step and jump sizes], Home and End go to 1 and 24; the wheel moves
  one hour, up to the right [documented]; a press anywhere on the slider puts the marker
  there and it follows the pointer while the button is held; the buttons can be clicked.
  The menu opens on 1 hour (what the original opens on is not known).

## 7. Passing time

- How fast the original passes the hours is not documented in any source read, and
  neither is what the world does meanwhile (whether AI packages are evaluated each hour,
  whether actors are moved to where their schedules have them).
- A sleep can be stopped part-way: `WakeUpPC` does it "as if the player had pressed the
  Cancel button" [documented]. For a wait nothing is documented.
- NVForever [engineering choice]:
  - After accepting, one hour passes every half second of real time: a day takes twelve
    seconds. Each hour is added to the one play clock, the simulation's calendar (the
    GameHour, GameDaysPassed, GameDay, GameMonth and GameYear globals,
    `fnv_sim::World::advance_hours`), before that frame's scripts run. The sky and the
    lighting follow the calendar as they do in play (`sky_follows_game`); the AI
    packages and scripts read the same globals. Merchants restock at the midnights
    passed, as with any passing of time (simulation.md).
  - Meanwhile the menu counts down: the hours line shows the hours left (its time in
    brackets stays), the marker moves down the slider, the date line shows the time
    now; the accept button is faint and the slider cannot be moved.
  - Cancel (X, Esc, the Wait control, the button) stops it; the hours passed stay
    passed. After the last hour the menu closes by itself.
  - Like every other menu it holds the game while it is open: the player and the actors
    stand still, the running game clock stops (only the waited hours are added), and
    scripts run their MenuMode blocks (code 1012), not their GameMode blocks. Whether the
    picture stands still as well is the menu pause switch (`--menu-pause`).
  - The people (actors living by their packages, `--npc-packages`). They stand still
    while the menu is up, like everything else. For each hour that passes in it:
    1. every actor the periodic choice covers chooses its package for the new hour
       (`Game::choose_packages_now`), with the old package's change script and the new
       one's begin script, in that order, as in play;
    2. each then walks for as long as an hour of game time lasts in play: 3600 seconds
       over the TimeScale global, two real minutes at 30. The walk is headless, on the
       navmesh, doors passed as if open (`fnv_ai::Ai::pass_time`); it ends as soon as
       nobody is on the way. An arrival runs the package's end script.
    So after a wait the actors are where their schedules have them, or as far on the
    way as an hour took them, and scripts ran hour by hour in the order of play. Behind
    the menu they change place from one hour to the next (a sleep's screen is black).
    An actor that sat on furniture and is to be elsewhere is off it at once, without
    the getting-up animation.
  - What does not happen during a wait: nobody starts a conversation or says a Say To
    line (a Dialogue package goes as far as its wait location and starts when the game
    goes on); a sandboxing actor keeps what it was doing and a patrol stays at the
    point it waits at (both count seconds of play, which stand still in a menu); a
    message box a script shows waits for the menu to close (the runtime's rule for any
    open menu).
  - When the menu closes, the game goes on at the new hour, the actors from where the
    wait left them.
  - The log has one line per hour passed, with the date line, GameHour and
    GameDaysPassed, and at exit a "wait summary" line.

## 8. Sleeping

What the sources say the original has beyond the wording:

- Scripts can tell sleeping from waiting (§5).
- **Well Rested** [observed: FalloutNV.esm]: a perk ("After sleeping in a safe bed, you
  will wake well rested. You earn 10% more experience points for several hours.") given
  by a spell whose effect script, when it starts, shows a message, restores the player's
  health "and limbs" (the script's own comment) and adds the perk, and when it ends
  removes it. The spell is cast by a script attached to nine bed base objects (safehouse
  and hotel beds and the like): it notes that the player activated the bed, sees
  `IsPCSleeping` in menu mode, and casts the spell back in game mode. Three room scripts
  cast it directly. Beds without that script give nothing by these records.
- **Hardcore mode** has a sleep need [observed: FalloutNV.esm]: the setting
  `fHCSleepDeprivationRate` (50.0), five "Sleep Deprivation" spells with rising
  penalties, and messages for its level changing. How sleeping lowers it is not in the
  sources read; NVForever's guess (100 a slept hour) and the needs over waited hours are
  in aid-items.md §6 and §11.
- A statistic "Times Slept" [documented: GECK wiki GetPCMiscStat].
- Healing per hour of plain sleep, the rule for beds someone owns (only the name of a
  refusal text, §9), and being woken by an attack: not found in any source read.

NVForever [owner request 2026-10-11]: while the player lies on a bed (from lying down
until getting up starts; `nvf_play::furniture::PlayerSeat::bed`), the Wait control opens
the same menu as a Sleep menu: its title and accept button say so, and the log says
"sleep". Time passes exactly as for waiting. Once the hours of a sleep are accepted the
world behind the menu fades to black over one second, the menu and its count staying on
top, and it comes back over one second when the menu closes [owner request 2026-10-11
for the fade out; the time, the menu staying visible and the fade back in are design;
what the original shows while sleeping is not in any source read]. A wait does not
fade. The fade is the image-space pass's fade to a colour (`nvf_play::wait::SleepFade`,
`imagespace.rs`), over any fade a script has on the screen.

**Activating a bed.** The beds' script (SCPT 0x000B5431, on nine bed base objects)
[observed]: when the player activates the bed it sets a variable to 1 and runs the
bed's default activation; in any menu, if the variable is 1 and `IsPCSleeping` is 1, it
sets it to 2; in game mode, if the variable is above 0, it casts the Well Rested spell
on the player when it is 2, and sets it back to 0. A frame of play between the
activation and the menu would clear the variable, so in the original the menu must
come up with the activation [hypothesis: the default activation of a bed opens the
Sleep menu at once; no capture].

NVForever [engineering choice, put to the owner in docs/owner-review.md]: the player
lies down first [owner request 2026-10-10], and the Sleep menu opens by itself once
they lie. Cancel leaves them lying; the Wait control opens the menu again; a movement
control or Activate gets them up, as before. Because seconds of play pass between the
activation and the menu, a bed with an `OnActivate` block is told of the activation
again as the Sleep menu opens (`Runtime::bed_activated_for_sleep`): the block runs with
the player as the activator and its default activation does nothing (the player is on
the bed already). A script's own `Activate` on furniture now uses it for the player,
which it did not before (the nine scripted beds could not be lain on).

**Well Rested** then needs nothing of its own: the script sees `IsPCSleeping` while
the hours pass, casts the spell when the menu closes, and the spell's effect script
shows its message, calls `ResetHealth` on the player (health and limbs) and adds the
perk; when the spell's time is up it shows the end message and removes the perk. The
three room scripts that cast the spell directly work the same way. The perk's own
effect, 10% more experience, is the perk system's and is not applied yet.

Not built, and why: see §10.

## 9. When the original refuses

Known only from the names and texts of its messages and from function pages; none of
the rules behind them was captured:

- Game settings for refusal texts [documented: GECK wiki "Settings" and
  SetPCHasSleepWaitOverride name them]: `sNoWaitInCombat`, `sNoSleepCombat`,
  `sNoWaitHostilActorsNear`, `sNoSleepHostileActorsNear`, `sNoSleepInOwnedBed`,
  `sNoWaitUnderWater`, `sNoWaitWhileAlarmSounding`, `sNoWaitInCell`. Two have records in
  FalloutNV.esm [observed]: "You cannot wait when enemies are nearby." and "You cannot
  sleep when enemies are nearby.".
- "Hostiles nearby" is the check of `IsHostilesNearby`, "called when Sleep/Wait key is
  pressed", with a distance setting each for outdoors and indoors [documented: GECK wiki
  IsHostilesNearby]; the two settings have no record in the plugin [observed].
- A script can forbid waiting (`EnableFastTravel`'s `canWait` argument), a flag on the
  player or on a cell can forbid sleeping and waiting there (`sNoWaitInCell`)
  [documented: GECK wiki EnableFastTravel, SetPCHasSleepWaitOverride,
  GetPCHasSleepWaitOverride].
- Being in the air, taking damage and trespassing are not named by any source read.

More from the sources, read 2026-10-11:

- The flag a script sets with `EnableFastTravel`'s `canWait` is the player's "can sleep
  or wait" flag, and the text shown for it is `sNoWaitInCell` [documented: GECK wiki
  SetPCHasSleepWaitOverride, GetPCHasSleepWaitOverride]. Cells have a flag of their own
  for the same [documented: the same page]; which cell flag it is, is not said (the
  list of cell flags on the GetCellFlag page has none by that name).
- "Enemy actors are attacked on sight by Aggressive and Very Aggressive actors";
  "Neutrals will be attacked by Very Aggressive actors"; allies and friends "cannot
  attack each other" [documented: GECK wiki GetFactionCombatReaction].
- Not found in any source read: the two distances' values; what "hostile" means to
  that check (in combat with the player, or would attack); the rule for an owned bed
  (whose owner, whether the cell's owner counts); what "under water" is; what an alarm
  sounding is; whether each refusal applies to waiting, to sleeping or to both where the
  setting's name names only one.

NVForever (`nvf_play::wait::refusal`): a request by the player (the Wait control, lying
down on a bed) or by a script that asks for the check is answered with a line on the
screen instead of the menu, in this order [engineering choice: the order]:

1. **A script turned waiting off** (`EnableFastTravel` with `canWait` 0): the
   `sNoWaitInCell` line, for a sleep too [documented].
2. **Swimming**: the `sNoWaitUnderWater` line [hypothesis: "under water" is the
   character's swimming state].
3. **In the air** (jumping or falling: the character controller is not on the ground,
   not swimming, and no furniture holds the player) [owner request 2026-10-17]: an
   NVForever line, no setting being known for it.
4. **Taking radiation damage** [owner request 2026-10-17]: the player takes rads from
   the world now (radiation markers, irradiated water, explosions' zones: rads a second
   after Rad Resistance above 0, [radiation](radiation.md) §4); rads from aid items do
   not count. Its line is NVForever's.
5. **A hostile actor near**: the "enemies are nearby" line for waiting or for sleeping
   (the two texts FalloutNV.esm has). An actor is hostile when it is drawn, alive,
   enabled, not a teammate, not unconscious or restrained, and, by its Aggression:
   Aggressive and the player is an enemy of its factions; Very Aggressive and the
   player is not an ally or a friend of its factions [documented rule of who attacks
   whom, applied to the player as the one seen]; Frenzied [hypothesis: attacks
   anyone]. Near is within 2048 units outdoors and 1024 indoors [engineering choice:
   the two settings have no record and no source gives their values]; a plugin that
   sets `fHostileActorExteriorDistance` or `fHostileActorInteriorDistance` is followed.
   NVForever's actors do not fight on their own yet, so this is "would attack", not "is
   attacking".
6. **In combat** [owner request 2026-10-17]: a drawn actor in a combat state (Alert,
   Combat, Lost) has the player on its target or alert list, which is what IsInCombat
   on the player reads (`fnv_game::awareness`), limited to the drawn actors
   [hypothesis: an actor in a cell no longer loaded keeps its combat state after the
   player left (observed in the corpse-respawn smoke run: cazadors 28000 units away),
   and the player who went away is not being fought]. It comes after rule 5
   [hypothesis: in a fight the enemy is near, and the "enemies are nearby" line is the
   original's own], so it catches a hunter farther away or one the player provoked.
   The line is `sNoWaitInCombat` or `sNoSleepCombat` [documented names; no record in
   FalloutNV.esm, so NVForever's wording unless a plugin sets them].
7. **Trespassing** [owner request 2026-10-17]: the player is in an owned cell, not
   public, that is not theirs (`fnv_game::crime`'s trespass cell, the one owners warn
   the player in): an NVForever line, no setting being known for it.
8. **Someone else's bed** (sleeping only): the bed reference has an owner that is an
   actor other than the player, or a faction the player is not in: the
   `sNoSleepInOwnedBed` line [hypothesis: the reference's own owner, as `IsOwner` reads
   it; the cell's owner is not looked at]. The player still lies down on it.

Where a text's setting has no record (all but the two about enemies), the line is
NVForever's own wording; a plugin that sets the setting is followed.

Known gaps of rule 3 [observed 2026-10-11]: an actor whose factions come from its
template (the template flag "Factions"; most creatures the wasteland spawns, whose
template is a leveled list) has no factions in NVForever's game data, and the Fiend
tested (FiendVioletRef) came out neutral to the player by its factions. Such actors
refuse a wait only when Very Aggressive or Frenzied: two coyotes 300 units away
(Aggressive) did not, the Fiend (Very Aggressive) did.

Not built: `sNoWaitWhileAlarmSounding` (nothing says what the alarm is), the cells' own
flag.

## 10. Not built

- Of the refusals (§9): in combat, an alarm sounding, the cells' own flag.
- (Built 2026-11-01, #1177.) **Healing by plain sleep.** No source read says that a bed
  without the Well Rested script heals: no game setting of FalloutNV.esm names sleep or
  rest healing [observed: its GMST records], and the wiki export has no page on it.
  The owner saw it 2026-10-31 [observed]: outside Hardcore a sleep heals Health and
  crippled limbs in full; in Hardcore it heals neither. So the first hour slept outside
  Hardcore restores all Health and every limb (`Game::pass_hours`, the log line
  `sleep: Health restored by N`); a wait heals nothing.
- **The hardcore sleep need.** The records [observed: FalloutNV.esm]: an actor value
  "Sleep Deprivation" (AVIF 0x000005F9), the setting `fHCSleepDeprivationRate` (50.0),
  five spells for its stages (their names end in 200, 400, 600, 800 and 1000), magic
  effects that restore and damage it, and three messages for it rising, falling and
  ending. The need builds up "as game time passes" [documented: GECK wiki
  ToggleHardcoreTracking]. Not in any source read: the unit of the rate, what the
  stage numbers are compared with, and how much an hour of sleep takes off. Built since
  (2026-10-12, aid-items.md §11) on stated guesses: each hour waited or slept passes for
  the effects' timers and the needs, a slept hour takes 100 off sleep deprivation; the
  captures of the actor values over waited and slept hours (aid-items.md §9 item 7)
  would settle them.
- The "Times Slept" statistic: the catalogue of statistic codes has no code for it.
- Being woken (by an attack, by a script other than through `WakeUpPC`).
- `GetPCSleepHours` as a script function (no opcode known, §5).
- The perk's 10% more experience (§8).
- The interface's own textures (`wait_meter.dds`, `wait_pointer.dds`, the brackets):
  the menu is drawn with plain shapes after the screenshot's measures. Its colour is the
  HUD's and its texts are in the game's fonts [documented: `menus\sleep_wait_menu.xml`:
  system colour `&hudmain;`, the question in font 1, the hours in font 6, the time in
  font 2; the buttons taken as font 2, hypothesis] since 2026-10-16 (playtest list 5 item 51).
  With `[UI] TileMenus=SleepWaitMenu` (T15b, off by default) the menu is drawn from the
  game's own file and its textures instead (§12).

## 12. Drawn from the menu file (T15b)

Behind `[UI] TileMenus=SleepWaitMenu` (`engine/nvf-play/src/tiles/
wait_menu.rs`) the open menu (1012) is drawn from `menus\sleep_wait_menu.xml` and the
hand-built drawing is not spawned; the menu's rules stay the ones above [design].

- **What the file leaves to the code** [documented: its "Set in code" comments]: the
  question (`SWM_HowManyText`), the hours line (`SWM_HoursChosen`), the date line
  (`SWM_CurrentTime`) and the Wait button's word (`SWM_WaitButton`; the file shows the key
  hint beside it from `_PCButtonText`, so the word is written bare). The Cancel button's
  word is the file's `&-sCancel;`. The background's opacity is the other T15 menus'.
- **The slider**: 24 items, one visible, a step of 1 and a jump of 6 [documented]; hour
  `h` is value `h - 1`. The code writes `_current_value` and pokes `_SetInCode`, as the
  prefab's comment asks [documented: scrollbar_horiz.xml]. In NVForever's tree an
  expression starts from 0, so the prefab's own `_current_value` counts the arrows' and
  page areas' clicks from 0 and cannot follow a value code set [observed: unit test
  `a_click_counted_by_the_file_is_put_back_to_the_models_value`]; the clicks on the
  slider's pieces are therefore the hand-built menu's commands (a page area six hours, an
  arrow one, the marker the hours under the pointer) and the value is written again after
  them [design]. Whether the original's expression starts from the trait's last value is
  not known [hypothesis].
- **Keys**: the file binds W to Wait and E to Cancel [documented: `_PCButton_W`,
  `_PCButton_E`]. NVForever keeps its own keys (§6: E accepts, X cancels); W, which they
  do not use, presses Wait through the file [design]. The mouse wheel stays the hand-built
  menu's.
- **Not changed**: while the hours pass, the hand-built Wait button is shaded faint; the
  file's is drawn as the file says (it has no rule for it) [design: not captured].
- Smoke: `tiles-sleep-wait`.

## 11. Checks

Unit tests: `cargo test --release -p nvf-play wait` (in `engine/`): the lines as the
screenshot has them, the slider's limits, hours passing one at a time, cancel, the
weekday and date over midnight, a month's end and a year's end, the test aid.

Unattended runs (`NVF_PLAY_TEST_WAIT`, wait.rs), each expecting exit 0:

- Standing: `NVF_PLAY_TEST_WAIT=16:open,17:hours=22,19:accept` with
  `nvforever --data <Data> --cell Goodsprings --play --hour 10.5 --run-seconds 36`:
  "wait menu: open as wait at …", 22 lines "wait: hour N of 22 passed: …" with the
  weekday and date changing at midnight, "wait menu: 22 hour(s) passed; closed", and
  "wait summary: the menu was opened 1 times (0 as sleep), 22 hours passed, 0 requests
  held by scripts".
- On a bed (the sleep smoke test): `NVF_PLAY_TEST_NEAR=00176ACE
  NVF_PLAY_TEST_ACTIVATE=16 NVF_PLAY_TEST_WAIT=23.5:hours=24,24.5:accept` with `--cell
  GSHouseInterior03 --play --hour 22 --run-seconds 40`: the menu opens by itself once
  the player lies ("furniture: the player is on 00176ACE", then "wait menu: open as
  sleep (the player lies on a bed) …"), 24 "sleep: hour …" lines, the last at the
  opening's time of day one date later.
- A Well Rested bed: `NVF_PLAY_TEST_NEAR=000CEB03 NVF_PLAY_TEST_ACTIVATE=16
  NVF_PLAY_TEST_WAIT=24:hours=3,25:accept` with `--cell NovacPlayersRoom --play --hour
  22 --run-seconds 36`: "script: the player uses furniture 000CEB03 (a script's
  Activate)", "wait menu: the bed 000CEB03 has a script: its OnActivate block ran for
  this sleep", three "sleep: hour …" lines, then "on screen: You are now Well Rested!"
  and "1 sleeps told to a bed's script" in the wait summary.
- Someone else's bed: `NVF_PLAY_TEST_NEAR=0010B20D NVF_PLAY_TEST_ACTIVATE=16` with
  `--cell GSHouseInterior02 --play --hour 22 --run-seconds 30`: the player lies down,
  then "wait menu: refused (sleep): the bed belongs to GoodspringsFaction; on screen:
  …".
- Enemies near: `NVF_PLAY_TEST_WAIT=20:open,22:open,24:open,26:open` with `--cell
  Goodsprings --play --pos -46700,64897,4600 --hour 12 --run-seconds 29` (beside the
  Fiend Violet): "wait menu: refused (wait): FiendVioletRef is hostile and within 2048
  units (outdoors); on screen: …" with the text of FalloutNV.esm, and "refused (… enemies
  near)" in the wait summary.
- The people through a wait: `NVF_PLAY_TEST_WAIT=30:open,31:hours=4,32:accept` with
  `--cell Goodsprings --play --npc-packages
  EasyPeteRef,GSChetRef,GoodspringsSettler04Ref --hour 18.5 --run-seconds 50`: four "ai
  […] an hour passed in the wait menu: N of 3 residents walked" lines; with the fourth
  (22:41) the three have their night packages, Easy Pete and the settler "used door …"
  and "arrived", "furniture: 00104C80 is off 0010634A at once", and the ai summary ends
  with "3 there, 0 on their way" and "4 hours passed in the wait menu (2 walks in
  them)".
- The mouse: `point=<x>:<y>`, `press` and `release` steps move a pointer through Bevy's
  own hit-testing (on a 1280 x 720 window, `17:point=0.4477:0.4722,17.3:press` is the
  9-hour mark, `0.7227:0.5764` the accept button, `0.7188:0.6292` cancel).
- Held by a script: `NVF_PLAY_TEST_SCRIPT_MENUS=1 NVF_PLAY_TEST_WAIT=16:open` with
  `--play --new-game --no-voice --run-seconds 26`: "wait menu: stays shut, a script has
  disabled the player's controls (Pip-Boy)".

Results of 2026-10-11 [observed]: all four as described. The standing run (started with
`--hour 10.5667`) opened the menu at 10:39 on Friday 18.10.81; its 14th hour read
"Saturday, 19.10.81, 0:39" and its 22nd "8:39". The bed run went from 22:07 Friday to
22:07 Saturday, and a second sleep of 10 hours cancelled after 3 stopped at 1:09 on
Sunday 20.10.81. The mouse run dragged the marker from 9 to 15 hours, accepted with a
click and cancelled with a click after 3 hours. With three Goodsprings residents
(`--npc-packages EasyPeteRef,GSChetRef,GoodspringsSettler04Ref --hour 18.5`, 4 hours
waited at 18:37) all three chose their night packages within three game minutes of the
menu closing at 22:37; two set off for their beds and the third was already at his.

Results of 2026-10-11, second session (sleeping, refusals, people) [observed]: the five
runs above as described. In the Novac room the sleep went from 22:06 to 1:06 and the
Well Rested message came with the first frame of play after the menu. In the people's
run the first three hours moved nobody (their evening packages held), and at the
fourth Easy Pete walked 4275 units through one door to his bed 00176ACE and the settler
6814 units through two doors to his, 0010B20D, both inside that hour's two minutes of
walking; Chet was at his. Beside two coyotes (`--pos -59400,5654,8300`) the wait was
not refused (§9, known gaps). Not run: swimming, a script's `EnableFastTravel` flag,
`SetPCSleepHours`, `WakeUpPC` and `ShowSleepWaitMenu` from a real script (unit tests
cover the menu's side of the last three, `a_script_can_make_the_player_sleep`, and the
state scripts read, `fnv_game::rest`).
