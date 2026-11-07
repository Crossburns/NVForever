# Spec notes: radios

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525), **[hypothesis]**
needs verification, **[design]** is our choice where the game's rule is not known.
Counts come from `fnvtool world radio` (fnv.world.radio/1, §7) over FalloutNV.esm alone
unless "ten masters" is said. Sources: the owner's GECK wiki export (pages Radio Stations,
Creating Radio Stations, Radio Tab, Reference, Activator, SetBroadcastState,
GetBroadcastState, StartRadioConversation, SetNPCRadio, PipBoyRadio, PipBoyRadioOff,
ForceRadioStationUpdate, SetRadioToDialogueOnlyMode, GetFormFlag, GetFlagsHigh,
IsRadioRef; 2026-10-09), xEdit's record definitions (schemas `objects.json`,
`placed.json`, `world.json`), the masters' compiled scripts (`fnvtool script decompile`,
read privately; nothing quoted beyond function names and FormIDs).

Code: `fnv_world::radio` (records, placed stations and receivers), `fnv_game::radio`
(receivers switched on, broadcast state, the programme as dialogue), `nvf-play`
`radio.rs` (playback). Related: audio.md (sounds, music), dialogue.md (lines, topics),
voice.md (voice files).

## 1. The pieces

- **A station** is a talking activator (TACT) marked as a radio station; each station is
  its own base object [documented: Radio Stations]. Its placed reference is the
  station's voice: the radio quests' lines are said by it (§3). TACT fields: `VNAM`
  voice type, `INAM` radio template (a SOUN), `SNAM` looping sound [documented: xEdit].
- **A receiver** ("radio") is an activator (ACTI) whose `RNAM` names a station; it plays
  that station "when turned on" [documented: Radio Stations, Activator]. Its own `INAM`
  radio template "appears to not work properly when applied to activators, must be
  applied to Talking Activator" [documented: Activator].
- **A placed station reference** carries `XRDO` "Radio Data": range radius, broadcast
  range type (0 Radius, 1 Everywhere, 2 Worldspace and Linked Interiors, 3 Linked
  Interiors, 4 Current Cell Only), static percentage, exterior position reference
  [documented: xEdit REFR; GECK wiki Reference "Broadcast Range Type" and
  GetRadioBroadcastType].

## 2. Records [observed]

- 87 TACT records, 39 with record-header bit `0x00020000` set; 28 of those also have
  `0x40000000`. Ten masters: 132 TACT, 49 and 38.
  - Bit `0x00020000` is set on every station named by a receiver (Radio New Vegas, Mojave Music Radio, Black Mountain Radio, the
    casino radios, the Tops performers, …) and on FO3 leftovers such as Galaxy News
    Radio; the 48 TACTs without it are terminals, intercoms, speakers and a few named
    "Radio" that no receiver names (`RadioTenpennyTower`, `MoriartysSaloonRadio`,
    `BMRadioReceiver`). It stands for the editor's "Radio Station" box [hypothesis].
  - Bit `0x40000000` is the "Continuous Broadcast" box [hypothesis]: among the 11 radio
    stations without it are those scripts start themselves — `SetBroadcastState 1` on
    Repcon's station, `StartRadioConversation RadioHello` on the NCR emergency radio,
    Hanlon's confession, Lily's grandkids' recording and the three Tops performers
    [observed: the masters' compiled scripts]; the others are the EDE frequency and FO3
    leftovers. Radio New Vegas, Mojave Music Radio, Black Mountain Radio and the casino
    stations have it.
  - Bit `0x10000000` is "Non-Pipboy" [documented: GECK wiki GetFormFlag and GetFlagsHigh,
    the header-flag table "adapted from FNVEdit": bit 28 "Non-Pipboy / Reflected by Auto
    Water"]. It is set on 14 stations: the casino radios (Tops, Ultra-Luxe, Gomorrah),
    the Strip and Goodsprings store radios, the Tops performers, Hanlon's and Lily's
    recordings, EDE's frequency, Repcon's station and two FO3 leftovers; not on Radio New
    Vegas, Mojave Music Radio, Black Mountain Radio, Camp Guardian or the NCR Emergency
    Radio [observed]. The same table names bit 30 (`0x40000000`) only "NavMesh Generation
    - Ground", so Continuous Broadcast stays a hypothesis.
  - 25 of the 39 radio stations lack Non-Pipboy (`radio_stations_on_the_pipboy`); six of
    those are placed: Radio New Vegas, Mojave Music Radio and the NCR Emergency Radio
    (Everywhere), Black Mountain Radio (Radius 50,000, 5 % static), Camp Guardian
    (Radius 5,000, 50 % static, from its exterior position reference) and the FO3
    leftover `FFEU07RadioBeacon` (Radius 3,000, in worldspace `00031E12`) [observed].
- Looping sound (`SNAM`): 10 stations name `OBJRadioStaticLP`
  (`fx\ui\radio\ui_radio_static_lp.wav`, Loop, 18.5 dB): nine FO3 `FFRadioSignal`
  stations (Continuous, no lines) and Hanlon's recording [observed]. A SOUN
  `UIRadioStaticLP` names the same file (Loop, 10.13 dB); `UIRadioSignalFound`,
  `UIRadioSignalLost` and `UIPipBoyStatic` exist too [observed]. No game setting (GMST)
  names radio static [observed: none with "radio" or "static" in its Editor ID].
- Radio templates: 10 TACTs name one (`AMLRadio` min 150, max 1800 units,
  13 dB; `AMLRadioLarge` 20 / 2500, 10.13 dB; `AMLRadioTheTops` 780 / 10000, 13.66 dB;
  `AMLRadioTopsPerformer` 20 / 2500, 2.64 dB). All four name the file
  `fx\xxx_placeholder_silence.wav` and have only the flag Mute When Submerged: they are
  attenuation settings, not sounds. Radio New Vegas and Black Mountain Radio name none.
- 24 receiver bases (ten masters: 39); every `RNAM` points at a TACT base, never at a
  placed reference.
- 220 placed receivers (ten masters: 392), 6 initially disabled (ten masters: 61). By the
  base's script: 134 `VintageRadioStartOnSCRIPT`, 24 `BMVintageRadioBMRScript`, 7
  `VCampGoldLoudspeakerScript`, 1 `GoodspringsRadioScript`, 54 without a script (ten
  masters adds 81 Dead Money speakers with `NVDLC01RadioSpeakerSCRIPT`).
- Placed stations: 43 TACT references (all TACTs). Radio New Vegas and Mojave Music Radio
  sit in the interior `Vault11a` with range Everywhere; Black Mountain Radio has range
  Radius 50,000 with 5 % static; Camp Guardian Radio Radius 5,000 with 50 % static and an
  exterior position reference.

## 3. What a station says [observed]

A station's lines are the Radio-tab lines of its quests [documented: Radio Tab, Radio
Stations: radios "use dialogue from the Radio tab" and start "radio conversations" from
RadioHello as conversations start from HELLO]. Every radio quest of the masters ties its
lines to its station with a quest condition `GetIsID <station>` (for example
`vGSRadioQuest` → `VGSStoreRadio`, `vCountryRadioQuest` → `vCountryRadio`,
`RadioNewVegas` → `RadioNewVegasTA`): the station's placed reference is the speaker the
conditions are run on.

| Station | Quest | Radio lines | With a sound (song) | Voice files |
|---|---|---|---|---|
| RadioNewVegasTA | RadioNewVegas | 216 | 20 | 181 (MaleUniqueMrNewVegas) |
| RadioBlackMountainTA | RadioBlackMountain | 186 | 30 | — |
| vCountryRadio (Mojave Music Radio) | vCountryRadioQuest | 15 | 15 | — |
| vCasinoTopsRadio | TopsRadioQuest | 8 | 8 | 8 |
| vCasinoUltraLuxeRadio | vUltraLuxeRadioQuest | 7 | 7 | 7 |
| vCasinoGomorrahRadio | vGomorrahRadioQuest | 6 | 6 | 6 |
| vNCREmergencyRadioTA | vNCREmergencyRadio | 33 | 0 | 33 |
| RadioNVCampGuardianRadio | vCampGuardianRadio | 8 | 0 | 8 |
| RepconRadioStation | RadioRepconQuest | 3 | 1 | — |

- **Songs** are lines whose response names a sound form (TRDT "Sound"); the sound "will
  play instead of the info's voice file" [documented: Radio Stations]. The 89 such
  responses of the masters' radio quests name SOUN records whose file is an `.mp3` under
  `songs\`; 87 are flagged 2D and Dialogue Sound. 45 distinct song files are named; 43
  exist. The two missing are temporary songs (`MUSNVTempBillyTheKid` in
  `songs\radio\_nv_temp\`, `MUSNVTempBlueMoonStrip` `songs\radionv\temp_bluemoon_strip.mp3`)
  of the one-line quests `vGSRadioQuest` and `StripRadioQuest`, whose stations no placed
  receiver names.
- **Mono files.** For in-world radios the game plays a `_mono` copy of the song beside
  the Pip-Boy's file [documented: Creating Radio Stations: "This mono file will be what is
  played out of speakers and radios in-game"]. All 48 loose MP3s in
  `Data\Sound\songs\radionv\` have a `<name>_mono.ogg` in `Fallout - Sound.bsa`
  [observed].
- **DJ lines** have no sound and are voice files in the station's voice type (Mr. New
  Vegas: `sound\voice\falloutnv.esm\maleuniquemrnewvegas\radionewvegas_…`).
- "Radio Infos can only have a single response" [documented: Creating Radio Stations];
  the masters' radio lines have one response each [observed].

## 4. Receivers on and off

- `VintageRadioStartOnSCRIPT` (134 receivers): OnLoad, once (a variable `init`), the
  reference activates itself; OnActivate passes the activation on (`Activate`)
  [observed: decompiled]. `BMVintageRadioBMRScript` and `VCampGoldLoudspeakerScript` do
  the same with `doonce`. So a receiver is off until activated, a default activation
  switches it [hypothesis: from these scripts' names and shape; the default action of a
  receiver is not described in the wiki], and these scripts make their radios "start on".
  Receivers without a script (the jukeboxes `JukeBoxDirty01`, `NewVegasRadio`, …) stay
  silent until the player switches them on [hypothesis]; the player's own activation
  counts as a switch as a script's does (`Game::default_activation_by_hand`) [design].
- `GoodspringsRadioScript` (the Prospector Saloon's radio `00109087`): OnLoad calls
  `SetDestroyed 1` unless the player knows about the radio; OnActivate by the player
  shows a message unless fixed; fixing it sets `bfixed` and activates it [observed:
  decompiled; layer5-captures.md §8.3 d]. A destroyed receiver is silent [hypothesis].
- Five receiver bases have neither a script nor a name (FULL): the three Tops performers'
  speakers (`marker_audio.nif`, one placed each), `vCasinoGomorrahSpeaker02` and the FO3
  `JukeBoxCleanVault101` [observed]. The player cannot activate an unnamed activator and
  no script activates them, yet the Tops performers' broadcasts (§5) can only be heard
  through them: they start switched on [hypothesis].
- In the Prospector Saloon both receivers play Mojave Music Radio: the jukebox
  `00107D73` (`JukeBoxDirtyNVCountry`, StartOn script) and the broken radio.

## 5. The programme

- **Broadcast state**: on or off per station; the default is the base's Continuous
  Broadcast box; `SetBroadcastState 0` stops continuous broadcast ("the station will play
  static except when playing scripted conversations"), `1` starts it ("the station will
  generate and play radio conversations one after another") [documented:
  SetBroadcastState, Radio Stations]. `StartRadioConversation [topic]` interrupts a
  broadcasting station with one conversation and returns to the loop, or plays one
  conversation on a silent station [documented]. `GetBroadcastState` reads the same
  state, so a station never set reads its Continuous Broadcast flag.
- **Who starts conversations** [observed: the masters' compiled scripts]: 13 scripts make
  16 StartRadioConversation calls, all but two naming RadioHello: the Tops show script
  (Bruce Isaac, the Rad Pack, the Lonesome Drifter by game hour, each station then set
  to 0 by the performer's OnHit, OnStartCombat and OnDeath), Hanlon's trigger (after
  `SetTalkingActivatorActor` names Hanlon), Lily's quest (with `SetNPCRadio 1` on Lily),
  the NCR Emergency Radio's quest every ten seconds counted from the end of its last
  line, Repcon's launch (with `SetBroadcastState 1` and the Pip-Boy tuned to it), a
  Black Mountain line's begin script (`RadioBMDontTakeMyRaul`, the station set to 0
  first) and an FO3 beacon script with no topic.
- **Conversation rules we use** (`fnv_game::radio`, `nvf-play` `radio.rs`): from the
  topic named (else RadioHello), the line the station can say now; then the links as
  in the programme; the conversation ends at a line that links nowhere [hypothesis;
  RadioGoodbye is "used to end a chain of radio links" [documented: Radio Tab]]. It
  starts at once, heard or not, and interrupts the line on the air, whose end script
  then does not run [design]. A call while the station is still saying a conversation's
  line is not taken [design: the NCR quest asks every ten seconds and its first line
  lasts 12 s]. Afterwards a broadcasting station starts again from RadioHello, a silent
  one falls silent [documented: "return to generating looping broadcasts as before"].
  A station that stops broadcasting finishes the line it is saying [design: some lines
  stop their own station in their begin script].
- Checked headless (`radio_data.rs`): with `vNCREmergencyRadio` running, twelve
  conversations of the NCR Emergency Radio are one line each, ten different lines, all
  its quest's; that station does not broadcast, Radio New Vegas does.
- **Order** [observed: the masters' lines]:
  - Mojave Music Radio: 15 RadioHello lines, all Random and Run Immediately, each a song;
    three conditions `GetQuestVariable vCountryRadioQuest <slot> != <song number>` over
    three slots, and a begin script that shifts the slots and stores the song: a song
    does not come back while it is one of the last three played.
  - Radio New Vegas: RadioHello (15 lines) → RNVNewsIntro → RNVNewsStory / Story2 /
    Story3 / DamStory / Transition → RNVNewsPost → RNVMusicIntroGeneric or
    RNVMusicIntroSpecific → RNVMusic (20 songs, each linking to RNVMusic again and
    conditioned on a counter below 3). Lines link on by their Link To topics (TCLT);
    RNVNewsPost links to both music intros.
- **Rules we use** (`fnv_game::radio`):
  - The station's placed reference says lines like an actor, with the player as the
    listener: within a topic the dialogue rule of dialogue.md §4 picks the line
    (quest priority, stored order, Random pools, Say Once per speaker), and the line's
    begin and end result scripts run with the station as their reference [hypothesis:
    the documented "like normal dialogue conversations"]. A line's fixed speaker (ANAM)
    does not keep the station from saying it; it names whose voice the line is in
    [documented: SetNPCRadio: a line an actor-radio is not the speaker of "plays with a
    filter effect to indicate it's not him"; observed: Black Mountain Radio's segment
    lines name Tabitha or Raul Tejada, so before this rule the station never got past
    RadioHello].
  - Black Mountain Radio [observed: its lines]: RadioHello lines are songs (Random, a
    begin script keeping the last three) listing segments 01–11 and 13 as choices; a
    segment is one topic whose lines a quest variable walks through (each line's begin
    script moves it on), each listing the same topic again, the last listing RadioHello;
    RadioBMMusic lines name all segments in their Link From and are gated by another
    variable.
  - The programme starts with RadioHello [documented]. After a line, of the topics it
    leads to (its Link To topics, and the topics whose lines name its topic in their
    Link From, answered by such a line, dialogue.md §3.5), those with a line the station
    can say now; one of them at random
    [hypothesis: how the game chooses among several links is not documented; taking the
    first would never play RNVMusicIntroSpecific]. With none (RNVMusic after three songs,
    a line without links), back to RadioHello [hypothesis].
  - Half a second of silence between two lines [design].
  - Checked headless (`crates/fnv-game/tests/radio_data.rs`, with game data): Mojave
    Music Radio plays 30 songs with no repeat within three; Radio New Vegas comes round
    news → intro → one to three songs → RadioHello again; Black Mountain Radio
    alternates songs with whole segments (eight or more in 120 lines, each line once,
    each ending with its line back to RadioHello), RadioBMSegment12 among them.

## 6. Playback (`nvforever --play`, `nvf-play` `radio.rs`)

- A receiver plays while it is on (§4), enabled, not destroyed, its station has a
  placed reference, and the player is within the template's maximum distance of it. It
  plays the station's line on the air; while the station is quiet (not broadcasting,
  in no conversation) it plays the station's looping sound when the station has one
  [hypothesis: SetBroadcastState 0, "the station will play static except when playing
  scripted conversations" [documented]; Hanlon's speakers play `OBJRadioStaticLP`
  until his confession], else nothing. A receiver does not need its station's placed
  reference to be enabled [design: Radio New Vegas's is enabled only when the opening
  ends].
- **Actors made radios** (`SetNPCRadio`, §9): the actor plays like a receiver at its
  current position, heard by the station's template, while it is enabled and in the
  player's cell or worldspace. The filter on lines the actor is not the speaker of
  [documented: SetNPCRadio] is not built.
- One programme per station, shared by its receivers; it starts when one of its
  receivers is first heard and runs on from then, heard or not [design]. A receiver that
  starts while a line is under way joins at the point reached (the file is sought to it)
  [design].
- File: a song's `_mono.ogg` when the archives have it, else the file the SOUN names; a
  DJ line's voice file in the voice type of the line's fixed speaker when it names one
  (Tabitha's `CreatureUniqueTabitha` for her Black Mountain lines; the station has no
  voice type), else the station's [observed: `fnvtool voice line`]; a line with neither lasts its
  fitted text length in silence. Length from the Ogg headers (voice.md §5).
- Loudness [design]: the song SOUN's static attenuation (12.33 dB for the RNV songs
  looked at); DJ voice files at full level. By distance: the station's radio template,
  else the receiver's, else 150 / 1800 units (AMLRadio's), as `sfx::distance_gain`
  (full within the minimum, falling as minimum ÷ distance, faded out at the maximum).
  The template's own static attenuation is not applied. Receivers and actors are panned
  as the sound effects are (`crate::sfx::Panned`, audio.md §14.5) [design].
- Pause: the Esc menu and loading screens hold sounds and programmes, as for placed
  loops (audio.md §7).
- No subtitles: a station's talk lines are heard and never subtitled (owner, list 5
  item 27; until then the loudest talk line was subtitled [design]).
- Log: `radio: <station> says <topic> <info>: <file> <seconds> s`, `radio: <receiver>
  tuned to <station> playing <file> [from <s> s]`, `radio: <receiver> (<station>)
  switched on|off`, `radio: <receiver> (<station>) is destroyed: silent`,
  `radio: <station> starts a conversation (StartRadioConversation)`, `… ends its
  conversation[: silent]`, `pipboy radio: …`, and at the end `radio summary: N receivers
  heard, N songs started, N DJ lines, …; receivers switched N times, N on at exit (N
  more start on)` and `pipboy radio summary: …`.
- Smoke test (CLAUDE.md): the Prospector Saloon with the jukebox switched off at 20 s
  and on again at 28 s. Observed: `00109087 (vCountryRadio) is destroyed: silent`; the
  jukebox switched on by its OnLoad, `vCountryRadio says RadioHello 0x0016B80E:
  sound\songs\radionv\mus_roundhouse_rock_mono.ogg 167.9 s`, off, then on again "from
  20.2 s"; `radio summary: 1 receivers heard, 1 songs started, 0 DJ lines, …; receivers
  switched 3 times, 1 on at exit`. In Boone's room in Novac (`0016CC35`, 90 s) Radio New
  Vegas played nine DJ lines from RadioHello through RNVMusicIntroGeneric and then a
  song.

- Looks: a receiver's model shows whether it is on (the radios' dials light by their
  "Open" sequence, the jukeboxes' idle plays): activators.md §3.

## 7. Tools

`fnvtool world radio <Data> <plugins> --schema schemas/fnv` (fnv.world.radio/1):
`counts` (talking activators and their header flags, placed station references by
range type, receiver bases and placements by script, the stations' lines and song
files, the radio stations on the Pip-Boy), `stations` (flags, `on_pipboy`, voice type,
template, looping sound, placed references with `XRDO` and `broadcasts_from`: the
exterior worldspace and position the range is measured from, the
quests that `GetIsID` them, their lines by topic and file type, receivers),
`receiver_bases` (station, template, script, placements) and `goodsprings_receivers`.
No texts are printed.

## 8. The Pip-Boy radio

- **What it receives**: radio stations without Non-Pipboy (§2) whose placed reference is
  enabled and in range; a script may tune it to any enabled station in range, Non-Pipboy
  or not [documented: PipBoyRadio, a disabled or out-of-range station "cannot be
  selected"; observed: Repcon's launch script tunes the Pip-Boy to its Non-Pipboy
  station]. The list is ordered by the stations' names [design].
- **Range** (`fnv_world::radio::reception`) [documented: GECK wiki Reference, Broadcast
  Range Type]: Everywhere always; Worldspace (and linked interiors) the station's
  worldspace and its connected interiors; Linked Interiors the connected interior cells;
  Current Cell its cell; Radius within the range radius of the station, or of its
  exterior position reference for a station in an interior. Our readings [design]:
  interiors "connected" to a worldspace are those the player walked into from it; doors
  are not followed for Linked Interiors, which is the station's own cell; Current Cell
  outdoors is the 4096-unit square; Radius is measured in three dimensions, from the
  player's last position outdoors while inside; an interior station without an exterior
  position reference is received in its own cell only. At Goodsprings Black Mountain
  Radio is out of range, about 70,000 units from its station [observed: positions in
  `fnvtool world radio`, `broadcasts_from`].
- **Static**: "plays static over this percentage of the radius" [documented]. Over that
  outer band the station falls evenly from full to nothing and the `UIRadioStaticLP`
  loop rises from nothing to full [design; hypothesis for the sound, by its name]. A
  tuned station that is quiet (§6) plays that static too [hypothesis].
- **Playback**: at the listener, no falloff, not panned; songs play their own (stereo)
  file, not the `_mono` copy [documented: Creating Radio Stations]; the song's static
  attenuation, DJ lines at full level, as for receivers. The station's programme is the
  one its receivers play (§6).
- **Controls** [design; there is no Pip-Boy screen yet]: `]` tunes to the next station
  received (off → first → … → last → off), `[` to the previous; not while a script has
  turned the Pip-Boy off (DisablePlayerControls). Turned on without a station it takes
  the last one tuned, else the first received. Out of range it falls silent until the
  station is received again.
- **Scripts** (§9): `PipBoyRadio 0` and `PipBoyRadioOff` turn it off, `PipBoyRadio 1
  [station]` on (to the station given, else the last one), `PipBoyRadio tune <station>`
  only while on [documented: PipBoyRadio notes]. The masters call PipBoyRadio three
  times (Repcon's launch: 1, tune, 0) and PipBoyRadioOff once (before the ending
  slides) [observed].
- Test aid `NVF_PLAY_TEST_RADIO` (`nvf-play` `radio.rs`) and smoke test (CLAUDE.md):
  at Goodsprings with Radio New Vegas and the NCR Emergency Radio enabled and its quest
  started, the Pip-Boy steps through Mojave Music Radio, a DLC station and back, then
  is tuned to the NCR Emergency Radio: its quest's StartRadioConversation every ten
  seconds, a 12 s line (subtitled then; no longer, list 5 item 27), one call not taken, static between conversations
  [observed 2026-10-11]. With the ten masters loaded, the DLC stations their scripts
  enable are listed too. Near the edge of Black Mountain Radio's range
  (`--pos=-47300,5300,9000`) its song played at 0.51 of its level under static at 0.49
  [observed].

## 9. Scripts' radio functions (`fnv_game::radio`)

- `SetBroadcastState` / `GetBroadcastState`: one state; unset, the Continuous Broadcast
  flag (§5).
- `StartRadioConversation [topic]`: kept for the engine, which starts it (§5).
- `PipBoyRadio`, `PipBoyRadioOff`: §8.
- `SetNPCRadio [0|1] station`: the masters always give the 0/1 first (six calls, Lily
  and EDE) [observed]; 0 turns the actor's radio off whatever station is named; an actor
  playing a station keeps it until turned off [documented: SetNPCRadio notes].
- `ForceRadioStationUpdate` "forces radio update" [documented]: not called by
  FalloutNV.esm; the DLC scripts call it (2 or 3 calls a run with the ten masters)
  [observed]. The engine looks at every radio each frame, so it is only counted
  [design].
- `SetRadioToDialogueOnlyMode`: not called by the masters' scripts; not built.

## 10. Loudness: what the data says

- Songs carry their SOUN's static attenuation (12.33 dB on Radio New Vegas's songs:
  ×0.242); DJ lines are voice files without a sound form, and nothing in the line, the
  voice type or the station sets their level [observed: the TACT and VTYP fields, INFO
  TRDT]. The data does not decide the DJ level; it stays at full [design], for the
  owner's ear.
- The radio templates (AMLRadio 13 dB, AMLRadioLarge 10.13 dB, AMLRadioTheTops
  13.66 dB, AMLRadioTopsPerformer 2.64 dB) carry a static attenuation we do not apply
  (§6); applied, it would lower DJ lines and songs alike from receivers.
- The install's `Fallout_default.ini` [observed]: `fDefaultRadioVolume=0.5000`,
  `fDefaultVoiceVolume=0.6000`, `fDefaultEffectsVolume=1.0000`, `fRadioDialogMute=0.50`,
  `fDBVoiceAttenuationIn2D=2.0`, `iRadioUpdateInterval=250`. What they do is a
  hypothesis (a radio slider at half the effects', the radio lowered to half during
  conversations, 2D voice lines 2 dB down, the radio looked at every 250 ms). None is
  applied: NVForever does not read the game's INIs (install-layout.md).

## 11. Open and not built

- Not built: a Pip-Boy screen (the radio is keys and the log), the filter on SetNPCRadio
  lines, `SetRadioToDialogueOnlyMode`, the signal found / lost sounds, reverb, the INI
  levels of §10.
- Capture questions for the owner: whether a scriptless jukebox starts off and one
  activation switches it on; whether the unnamed Tops speakers start on; whether a quiet
  station's receivers play its looping sound (Hanlon's speakers before his confession);
  how loud a radio is at a distance (the templates' curve and whether their static
  attenuation applies); whether DJ lines and songs are as loud as each other; whether a
  station runs on while no receiver is heard and where a receiver joins; which linked
  topic a station takes when several have lines; the pause between lines; the
  Pip-Boy's order of stations, what it plays out of range and in a static band, and
  whether Black Mountain Radio is received at Goodsprings; what a second
  StartRadioConversation does while a conversation plays.
