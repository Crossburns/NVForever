# JohnnyGuitar NVSE, batch 3 (2026-11-01, #1007)

Effect shaders' traits and textures, media sets' traits, casinos' chips and winnings,
terminals' menu items and the quest script delay, in
`crates/fnv-game/src/nvse_jg3.rs`. Sources: the GECK wiki export [documented: the page
named]; the record layouts from `schemas/fnv` (xEdit wbDefinitionsFNV.pas). Not probed in
the game yet: every value but the casino winnings comes from a record, and the earlier
JohnnyGuitar probe sets (jg1, jg2) were run on the other machine. Setters change the
record for the session, read by the getters. Tests: `crates/fnv-game/tests/jg3.rs`.

| Function | NVForever |
|---|---|
| GetEffectShaderTraitNumeric, SetEffectShaderTraitNumeric `shader trait [value]` | EFSH DATA by the page's trait table, 0 flags to 46 scale key 2 time; the blends and Z tests (1-3, 23-27) as their numbers [documented: the table; the fields and enum numbers: schemas/fnv/effects.json]. The setter takes the traits its own page lists (0-42) and returns 1 when done [documented: "returning 1 on success"]; flags and enums keep whole numbers [hypothesis]. The fill and edge colours (4, 14) read 0 and are not set [decision: the pages do not say how three channels make one number; a probe would] |
| GetEffectShaderTexturePath, SetEffectShaderTexturePath `shader id [path]` | 0 fill (ICON), 1 particle shader (ICO2), 2 holes (NAM7); "" for none [documented: the table; the subrecords: xEdit EFSH] |
| GetMediaSetTraitNumeric, SetMediaSetTraitNumeric `set trait [value]` | 0 the type (NAM1: -1 none, 0 battle, 1 location, 2 dungeon, 3 incidental), 1-6 the dB values, 7-12 the boundaries, 13 the enable flags (PNAM), 14-17 the times (DNAM-GNAM); 0 for a missing subrecord or another trait [documented: the page's table names each subrecord] |
| GetMediaSetTraitString, SetMediaSetTraitString `set trait [path]` | 0-5: NAM2-NAM7, the sound paths [documented] |
| GetMediaSetTraitSound, SetMediaSetTraitSound `set trait [sound]` | 0 HNAM (intro, daytime), 1 INAM (outro, nighttime) [documented]; the setter takes a sound form or none, anything else changes nothing [hypothesis] |
| GetCasinoChip, SetCasinoChip `casino [chip]` | the casino's chip (CSNO DATA currency) [documented: the page; the field: xEdit CSNO]; the setter takes a CHIP form. The tables (blackjack, roulette, slots) play with the chip set (`casino::chip`) |
| GetCasinoWinnings, SetCasinoWinnings `casino [chips]` | the player's net chips won at the casino (`casino::add_earnings`, blackjack.md §4) [documented]; setting it also raises the winnings level the casino's scripts read, as a game's round does [hypothesis] |
| GetTerminalMenuItemCount `terminal` | the number of menu items (ITXT) [documented] |
| GetTerminalMenuItemText, SetTerminalMenuItemText `terminal textID item [text]` | 1 the item's line (ITXT), 2 its result text (RNAM) [documented]; "" for no such item |
| Get/SetTerminalMenuItemNote, …Submenu, …Flags `terminal item [form / flags]` | the item's display note (INAM), sub menu (TNAM), flags (ANAM: 1 Add Note, 2 Force Redraw; the setter keeps those two bits) [documented]. A note setter takes a NOTE or none, a sub menu setter a TERM or none [hypothesis]. The changes reach the terminal screen: the listed lines, the result text, the note shown and given, and the sub menu opened (`terminal::item`) |
| GetQuestDelay `quest` | SetQuestDelay's delay for the session, else the record's (DATA), 5 s when that is 0 or missing [documented: the page; the default: `QuestDef::script_delay`, observed in `L5.quest.delay_*`] |

Not in this batch: SetProjectileSound and SetExplosionSound (nothing reads a projectile's or an
explosion's sounds yet, neither a getter nor the engine), GetWorldspaceEncounterZone and SetWorldspaceEncounterZone (WRLD XEZN is
not in the schema yet), HSVtoRGB and RGBtoHSV (outputs into script variables), GetRGBColor
(how the three channels pack is not documented), AddTerminalMenuItem, RemoveTerminalMenuItem
and RefreshTerminalMenu (they change the list's length while the menu shows; a design of
their own). The engine's effect shaders do not read a changed trait or texture yet; the page
warns that most changes do not show while the shader plays.
