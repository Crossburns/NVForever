# Save files (`.fos`): layout as observed

Clean-room notes on the original game's save file, written to plan NVForever's own save
and load system ([design/save-load.md](../design/save-load.md)). The game does not read or
write `.fos` files; this note says what the original keeps, so our save keeps the same
state. One reader exists, for the extender lab only: `fnv_save::fos` and `fnvtool ext
queries --save` start a probe run from the save the probes ran from (§6.4, issue #1023). Tags: [documented] (public page named), [observed] (bytes in the saves below),
[hypothesis] (a reading that fits the bytes but is not proven). Offsets are byte offsets
in the file, little-endian unless stated.

No save bytes, dumps or screenshots are committed. The working copies and the parser used
for this note are in `C:\fnv-private\save-research\` (private).

## 1. The saves looked at

`C:\fnv-private\capture\saves-backup\` holds 27 folders, named by the Unix time at which
`tools/capture/capture_run.py` backed up the game's rolling saves (`autosave.fos`,
`autosave.fos.bak`, `autosave.nvse`) before a capture run (tools/capture/README.md). The
27 folders hold only **four different `.fos` files** [observed, MD5 of every file]:

| Name here | File (first folder holding it) | Size | Written by | Header place / play time |
|---|---|---|---|---|
| A | `1791540675\autosave.fos.bak` | 1,860,200 | the owner's game, 2026-10-09 05:43 (owner-review.md, "Read first") | Goodsprings, 0:11:58, save number 15 |
| B | `1791540675\autosave.fos` | 1,776,091 | VCG01 stage 55's scripted `Autosave`, capture round 4 from save `nvfnew` (layer5-captures.md §10) | Doc Mitchell's House, 0:01:50, save number 16 |
| C | `1791556130\autosave.fos` | 1,854,167 | capture `pc_drop_fall`, loaded `nvfdoc`, walked out of Doc Mitchell's door (owner-review.md; `run_pc_drop_fall.json`) | Goodsprings, 0:11:50, save number 16 |
| D | `1791556382\autosave.fos` | 1,854,167 | capture `pc_push`, loaded `nvfdoc`, same door (`run_pc_push.json`) | Goodsprings, 0:11:51, save number 16 |

A fifth save, the owner's **"max save"** (`saves-backup\Max Save\`, called M here), is a
late full playthrough: save number 292, level 50, 61:01:36 play time, 8,050,402 bytes,
file dated 2013 [observed, header and file time]. It is used for the range of record types and sizes (§6.3).

C and D start from the same save and leave by the same door a few real minutes apart, so
their difference shows what moves with a little game time (§7). A and C share the same
form ID array and changed-form list (§6), so A is from the same line of play [observed].
B is a new game, before the player ever entered an exterior worldspace (§5.3).

The xNVSE co-save (`autosave.nvse`) is ignored: NVForever has no use for it.

## 2. Top-level layout

The file is a sequence of sections; a table after the plugin list gives the absolute
offsets of the later ones [observed, all four saves; offsets for C]:

| Section | C offset | Notes |
|---|---|---|
| Magic `FO3SAVEGAME` (11 bytes, no terminator) | 0 | [observed]; the `.fos` extension is shared with Fallout 3 [documented: fileinfo.com ".FOS File Extension", no layout given] |
| Header size (u32) | 0x0B | counts the bytes from 0x0F to the start of the screenshot |
| Header (§3) | 0x0F | fields each followed by 0x7C |
| Screenshot (§3.2) | 0x0F + header size | width × height × 3 bytes |
| Form version byte, plugin block (§4) | after the screenshot | |
| File location table (§5) | after the plugins | 8 × u32 then 78 zero bytes |
| Global data table 1 (§5.1) | table[2] | 12 typed entries |
| Changed forms (§6) | table[3] | `changeFormCount` records |
| Global data table 2 (§5.2) | table[4] | 1 entry |
| Form ID array (§5.3) | table[0] | u32 count, then u32 form IDs |
| Worldspace array (§5.3) | right after the form ID array | u32 count, then u32 form IDs |
| Tail (9 bytes) | table[1] | `05 00 00 00 00 00 00 00 7C` in all four saves; meaning unknown |

The file ends exactly after the tail [observed: table[1] + 9 = file size in all five].

**The `|` byte.** Nearly every field in the header, the globals and the changed-form data
is followed by `0x7C` (`|`) [observed]. It carries no data: a reader skips it after each
field. The Oblivion (`TES4SAVEGAME`) and Skyrim layouts on UESP have no such separators
[documented: UESP "Tes4Mod:Save File Format", "Skyrim Mod:Save File Format"]; the rest of
the structure is close to Skyrim's (§5), which is the closest public description.

## 3. Header

### 3.1 Fields [observed, all four saves]

From 0x0F, each field followed by a 0x7C byte:

| Field | Encoding | Values seen |
|---|---|---|
| unknown | u32 | 0x30 in all four [hypothesis: header/format version] |
| language | 64 bytes, NUL-padded ASCII | `ENGLISH` |
| screenshot width | u32 | 512 |
| screenshot height | u32 | 288 |
| save number | u32 | 15 (A), 16 (B, C, D) |
| player name | u16 length, 0x7C, bytes | empty in B (a new game; the name is set in VCG01) [observed] |
| karma title | u16 length, 0x7C, bytes | "Drifter" |
| level | u32 | 1 |
| location name | u16 length, 0x7C, bytes | the cell's or worldspace region's display name |
| play time | u16 length, 0x7C, ASCII `HHH.MM.SS` | `000.11.50` |

Strings are length-prefixed (u16), a `|`, then the bytes without a terminator. The header
size therefore varies with the strings (137 bytes in A/C/D, 141 in B) [observed]. The
header holds no game date and no plugin list; the load menu's line is built from it alone
[hypothesis].

### 3.2 Screenshot

Width × height × 3 bytes, uncompressed, no padding [observed: 512 × 288 × 3 = 442,368
bytes ends exactly where the form-version byte starts]. Channel order RGB [hypothesis].

## 4. Form version and plugins [observed]

- u8 form version: 0x1B (27). Every changed form carries the same version byte (§6).
- u32 size of the plugin block (199 in all four), then:
- u8 plugin count (10), `|`, then per plugin: u16 length, `|`, name, `|`.

M lists nine (no `GunRunnersArsenal.esm`) in a different order (`FalloutNV.esm`,
`HonestHearts.esm`, `TribalPack.esm`, …) [observed], so the list is the load order the
save was made with, not a fixed list. The ten names in A–D are the base game masters in load order (`FalloutNV.esm`, the four story
DLCs, `GunRunnersArsenal.esm`, the four packs). A plugin's position is its mod index in
the form IDs that follow [hypothesis, consistent with every form ID resolving to a record
of the expected type in that master, §6].

## 5. File location table and global data

Right after the plugin block: eight u32 values, then 78 zero bytes up to the first global
entry [observed]. Matched against Skyrim's table [documented, UESP "Skyrim Mod:Save File
Format": six offsets, four counts, `unused[15]`], the New Vegas table is shorter:

| # | Meaning | C value | How it was matched |
|---|---|---|---|
| 0 | form ID array offset | 1,818,058 | points at a u32 count followed by that many form IDs [observed] |
| 1 | tail offset | 1,854,158 | equals the end of the worldspace array [observed] |
| 2 | global data table 1 offset | 442,834 | first typed entry starts there [observed] |
| 3 | changed forms offset | 445,396 | = end of table 1; the record walk starts there [observed] |
| 4 | global data table 2 offset | 1,818,050 | = end of the record walk [observed] |
| 5 | table 1 count | 12 | 12 entries tile table 1 exactly [observed] |
| 6 | table 2 count | 1 | |
| 7 | changed form count | 4,220 (A, C, D), 3,885 (B) | walking this many records lands exactly on offset 4 [observed] |

A global entry is u32 type, u32 length, then `length` bytes [observed, same as Skyrim's
documented layout].

### 5.1 Global data table 1 (types 0–11) [observed unless tagged]

| Type | Length (C) | Contents |
|---|---|---|
| 0 | 220 | (every value below is followed by 0x7C) u32 count 43, 43 × (u32). All zero in B (new game); first two are 1 in A/C/D. The misc statistics by code: the values of 31 saves fit the names (misc-stats.md §1) [observed] |
| 1 | 57 | **Player location and created-form counter**: u32 next created form ID (0xFF001313 in C, 0xFF001309 in B, 0xFF001317 in A); ref of the current worldspace (WastelandNV in C, a dummy worldspace FFEncounterWorld in B, which is indoors); two i32 cell grid coordinates (−18, 0 in C); ref of the worldspace or interior cell the player is in (GSDocMitchellHouse in B); three f32 position (no separator between them); ref of a quest (VCG01 in B, VCG02 in C) [hypothesis: the active quest shown in the Pip-Boy]; u32, f32, u32 unknown |
| 2 | 107 | a short list of mostly zero values; unchanged between all four saves; meaning unknown |
| 3 | 1803 | **Global variables**: vsval count (200), then per global: ref and f32, each followed by 0x7C. 199 resolve to GLOB records, one to form 0x63 (no record in the masters [hypothesis: a hard-coded global]). Game time is here: GameHour, GameDaysPassed, GameDay/Month/Year, TimeScale (30) |
| 4 | 30 | three f32 and six bytes; the third float moved between C and D (10.388 → 10.402); meaning unknown |
| 5 | 36 | u32 1, flags, floats; two floats moved between C and D; meaning unknown |
| 6 | 33 | u32 1003, small values; identical in all four |
| 7 | 2 | one byte (0) |
| 8 | 71 | **Sky and weather**: ref current weather, ref second weather (NVWastelandGS and NVWastelandClear in C; WastelandClear twice in B), then f32 values; two of them equal GameHour exactly (9.4697 in C) [observed: matches the GameHour global]; the rest unknown |
| 9 | 7 | u32 0 and a byte |
| 10 | 96 (C), 80 (B) | a vsval-counted list of refs (18 in C, 14 in B); meaning unknown |
| 11 | 4 | one ref (resolves to a SOUN record in C) [hypothesis: unknown; the resolution may be meaningless] |

### 5.2 Global data table 2

One entry, type 1000, length 0, in all four saves [observed].

### 5.3 Form ID array and worldspace array

- Form ID array: u32 count, then count × u32 form IDs (9,022 in A/C/D, 7,237 in B). Most
  are references (ACHR, REFR, ACRE) and their bases (NPC_, CREA), packages, quests,
  scripts, navmeshes, globals, cells [observed: resolving each against the masters].
  No entry has mod index 0xFF in any of the four.
- Worldspace array: u32 count, then form IDs. C has one (WastelandNV, 0x000DA726); B has
  none: the new-game player had not been outdoors [observed]. [hypothesis: the worldspaces
  the player has visited; Oblivion's page calls its equivalent of unknown purpose,
  documented, UESP Tes4Mod].

### 5.4 Refs and vsval

- **Ref** (3 bytes, big-endian): the top 2 bits are a kind, the low 22 bits a value.
  Kind 0: value is a 1-based index into the form ID array (0 = none); kind 1: a form ID in
  `FalloutNV.esm` directly; kind 2: a created form (0xFF000000 | value) [documented for
  Skyrim, UESP "Skyrim Mod:Save File Format"; observed for New Vegas: index − 1 makes every
  changed form resolve to a record of the type its record type says, §6, while the plain
  index does not]. Kind 1 was seen (e.g. world ref `00 01 95`), kind 2 was not.
- **vsval** (variable-size count): the low 2 bits of the first byte give the width (0 = 1
  byte, 1 = 2 bytes, 2 = 4 bytes), the value is the little-endian number shifted right by
  2 [hypothesis: fits every count seen: `21 03` = 200 globals, `98` = 38 quest stages,
  `04` = 1 item, `1C` = 7 variables].

## 6. Changed forms

Each record [observed: the walk tiles the section exactly in all four saves]:

| Field | Size | Notes |
|---|---|---|
| ref | 3 | §5.4 |
| change flags | u32 | which kinds of change follow |
| type | u8 | low 6 bits: record type; high 2 bits: width of the length field (0 = u8, 1 = u16, 2 = u32) |
| version | u8 | 27 in every record (= the form version byte, §4) |
| length | 1/2/4 | data size |
| data | length | the changes, in a fixed order; fields are each followed by 0x7C |

Skyrim's documented header has a second length (uncompressed size); New Vegas has only
one and no compression [observed].

The data has no per-field tags: which fields are present follows from the change flags and
the record type, as in Oblivion [documented, UESP Tes4Mod: "Bit order does not match the
order of subrecords", flags say which subrecords are present]. This note decodes the parts
needed to plan our own save; the rest is listed as unknown.

### 6.1 Types seen [observed, C; B has the same types]

Type numbers are matched by resolving each record's ref against the masters:

| Type | Record | Count C (B) | Change flag bits seen |
|---|---|---|---|
| 0 | REFR (placed objects) | 317 | 0, 1, 2, 4, 5, 12, 27, 28, 29, 31 |
| 1 | ACHR (placed NPCs, and the player ref 0x14) | 2,418 | 0, 1, 4, 5, 11, 18, 20, 21, 27, 28, 29, 31 |
| 2 | ACRE (placed creatures) | 900 | 0, 1, 5, 10, 11, 18, 20, 27, 28, 29, 31 |
| 7 | CELL | 15 | 1, 29, 30, 31 |
| 8 | INFO (dialogue responses) | 52 (13) | 31, always with no data |
| 9 | QUST | 466 | 1, 2, 29, 30, 31 |
| 10 | NPC_ (only the player base, 0x7) | 1 | 2, 5 |
| 11 | CREA | 1 | 1 |
| 33 | CLAS (PlayerClass) | 1 | 1 |
| 34 | FACT | 4 | 2 |
| 37 | FLST (form lists) | 41 | 31 |
| 40 | LVLI (leveled items) | 1 | 31 |
| 50 | CHAL (challenges) | 3 | 1 |

Not seen in A–D: created forms (kind-2 refs); M has them (§6.3).

Not every actor is listed: the masters hold 3,942 ACHR and 3,739 ACRE, the saves 2,418 /
900 (C) and 2,406 / 893 (B) [observed], so the original keeps a record for a large fixed
set of actors (most of them untouched by the player) [hypothesis: the persistent ones].

### 6.2 What is decoded per type

**QUST** [observed: VCG01 in B is at stage 55 per its capture; its record shows stages up
to 55 done and 60 not done]:

- flag bit 1: one byte of quest flags (0x31 in B where VCG01 is running, 0x32 in C)
  [hypothesis: the quest's running/completed flags].
- bit 2: an f32 first (9.0, 5.0, 0.05 seen) [hypothesis: the quest script's processing
  delay, SetQuestDelay].
- bit 31: **stages**: vsval count, then per stage: u8 stage index, `|`, u8 done, `|`, vsval
  log-entry count, then per entry: u8 entry index, `|`, u8 done, `|`, and when done 4
  bytes `23 01 E9 08` / `24 01 E9 08` [hypothesis: a date stamp, u16 day value then u16
  year 2281; the later stages in C carry the later value].
- bit 30 (465 of 466 quests): **script variables** (vsval count, then per variable: u32
  index, `|`, f64 value, `|`), then two more vsval-counted lists that were empty in every
  quest looked at. A quest with no changes but bit 30 stores `00|00|00`.
- bit 29: **objectives**: vsval count, then per objective: u32 objective index, `|`, u32
  state, `|`. States 1 and 3 seen [hypothesis: 1 = displayed, 3 = displayed and
  completed]. B: VCG01 objective 10 = 1; C: objectives 10, 30, 40, 50 = 3.

Variables are stored as f64 even though scripts declare short/int/float/ref [observed:
`00 00 00 00 00 00 F0 3F` = 1.0]. A **reference variable** has the top bit of its index
set (`41 00 00 80` = variable 0x41) and holds a 3-byte ref instead of the f64 (`00 14 19`)
[observed: nvfairport, VStreetCombat, VFactionSquad, NVDLC02RainQuest; every quest record
of that save then walks to its end].

The flags byte (bit 1), across nvfairport's 32 quests that have it: 0x21 VMQ01 and 0x31
on the five story DLCs' MQ00 (running), 0x32 VCG01 (done), 0x20 VCG02, 0x00 VCG04 and
VGenericTimer [observed] [hypothesis: bit 0 running, bit 1 completed; the other bits
unknown].

**REFR** [observed bytes; meanings hypothesis unless said]:

- bit 0: u32 that looks like the record header's flag word (0x409, 0x29, 0x800809 seen)
  [hypothesis: form flags, including the disabled state].
- bit 2: worldspace/cell ref, then six f32 with no separators (position, rotation), `|`.
  Seen on refs moved by physics (tumbleweeds, items) [hypothesis: "moved"].
- bit 4: one f32 (1.0 seen) [hypothesis: scale or health].
- bit 5 and bit 27: **item lists**: vsval count, then per item: ref, `|`, u32 count, `|`,
  then a vsval-counted list of extra-data lists (type byte 0x2F, 0x24, 0x25 seen with
  small payloads) [hypothesis: the container's inventory changes; extra types carry
  condition, ownership, count]. Vendor containers and Doc Mitchell's dresser have them.
- bit 12: door data (`2A`, a byte, a ref, two u32) on doors [hypothesis: lock or teleport].
- bit 28 and bit 31: animation/script data: vsval, a script ref, then u32 index + f64
  value pairs (as in quests) and short strings such as `Backward`, `Close`,
  `SpecialIdle` [hypothesis: bit 31 = script variables of the reference, bit 28 =
  animation state].
- bit 29: a short ref list ending `74 | ref` [unknown].

**ACHR / ACRE**: flag 0x40000 (bit 18) on 2,020 of 2,418 ACHR records, data 297 bytes for
most [observed]. These include an f32 near 17,634 that moved with game time between C and
D and a u32 that grew by 765 [observed]; meanings unknown. The player ref (0x14) record is
4,429 bytes in C (flags 0xB0000832) and starts with a ref and six f32 like REFR bit 2
[observed]. Not decoded further: the player's inventory, actor values, effects and
perks are somewhere in it [hypothesis].

**NPC_ 0x7 (player base)**: bit 2: seven bytes `05 05 05 05 05 05 0A` [hypothesis: the
seven SPECIAL values]; bit 5: the player's name (u16 length, `|`, string) [observed: the
same name as the header].

**CELL**: bit 1: one byte (cell flags) [hypothesis]; bits 30/31: blocks with the value
0x129 = 297 [observed: 297 = GameDaysPassed (12.39) × 24, so hours passed;
hypothesis: the cell's detach time, for respawn and reset] and runs of `0F 00` / `F0 00`
bytes [hypothesis: local map fog-of-war bits, interiors].

**INFO**: bit 31, no data. 13 in B (new game), 52 in C [hypothesis: "this response has
been said", the say-once flag and the "said" condition functions].

**FACT** (PlayerFaction and three others): bit 2: vsval count, then per entry: ref, `|`,
u32, `|`, u32, `|` [hypothesis: faction relations, ref + modifier + combat reaction].

**FLST** bit 31: vsval count, then refs [hypothesis: form lists changed by script,
AddFormToFormList]. **LVLI** bit 31: entries of ref, u16 level, u16 count, f32
[hypothesis: leveled list changed by script, AddItemToLeveledList]. **CHAL** bit 1:
u32 1, u32 0 [hypothesis: challenge progress]. **CLAS** bit 1 (PlayerClass): four u32,
three skill-like values and −1 [hypothesis: the player's tag skills].

### 6.3 The max save (M) [observed unless tagged]

M parses with the same rules: the record walk ends exactly at table offset 4 after
42,784 changed forms, the form ID array holds 60,872 IDs, and the file ends after the same
9-byte tail. Form IDs resolve against the masters **in M's own load order** (resolving them
with A–D's order gives wrong record types), confirming that a form ID's mod index is the
plugin's position in the save's list.

- **Worldspace array**: 23 worldspaces, including the four story DLCs' (Zion, the Villa and
  its town worldspaces, Big MT, the Divide) and the Strip, Freeside, Boulder City.
  Together with B (none, never outdoors) this supports "worldspaces visited" [hypothesis].
- **Created forms** (kind-2 refs, 0xFF…): 574 records: 556 REFR (type 0, 32–593 bytes),
  15 ACRE (type 2, about 0.5–1.8 KB) and 3 of type 3 (546 bytes each, holding short
  strings and a SOUN-like ref) [hypothesis: type 3 = placed projectile/missile]. A created
  REFR's data starts like REFR bit 2 (cell ref, six f32) followed by a u16 and a 2-byte
  value, then the usual `|`-separated fields [observed]; its base form is somewhere in that
  data [hypothesis]. The next created form ID is 0xFF00155E.
- **Global table 1 grows with play**: type 2 is 4,329 bytes: vsval count 603, then
  (ref, `|`, u16, `|`) entries [hypothesis: a count per base form, such as the per-actor
  death counts Oblivion keeps, documented UESP Tes4Mod]; type 9 is 221,569 bytes (7 in
  A–D; contents not decoded); type 10 is 2,793 bytes and holds a radio song's file path
  (`data\sound\songs\radionv\…mp3`) [hypothesis: radio and music playback state]; type 0
  (43 u32) holds large counts (up to 147,591) [hypothesis: misc statistics].
- **Record types beyond A–D**: 4 PGRE (placed grenades, 205), 13 TACT (talking
  activators, 21), 31 NOTE (16, no data) [hypothesis: notes the player has], 32 ECZN
  (encounter zones, 20, 17 bytes each: four u32 such as 0x622, 0x620, 0x620, 0x18)
  [hypothesis: game-day stamps and the zone's level, for respawn], 35 PACK (1),
  43 REPU (reputations, 9: bit 1, two f32, e.g. 2.0 and 0.0) [hypothesis: fame and
  infamy], and many more NPC_ (573, bits 2, 3, 9: seven SPECIAL-like bytes, then a block of
  14 skill-like bytes) [hypothesis: base stats of levelled NPCs fixed at first meeting].
- **Counts and sizes** (data bytes, min / median / max): REFR 26,683 (0 / 19 / 4,856);
  ACHR 3,560 (285 / 528 / 2,675); ACRE 2,861 (281 / 954 / 3,693); CELL 4,208
  (2 / 42 / 858); INFO 3,332 (all 0); QUST 483 (2 / 20 / 695); CHAL 130; FACT 46
  (5 / 44 / 870); FLST 36 (9 / 25 / 405). The player ref record is **108,445 bytes**
  (4,429 in C) [observed]; its contents were not decoded.

Size guide for our own format: a 61-hour game keeps state for about 42,800 forms; most of
it is per-reference (REFR, ACHR, ACRE: 33,000) and per-cell (4,200) [observed].

### 6.4 The player record, as far as `fnv_save::fos` reads it [observed unless tagged]

Read in the 19 `nvf*.fos` saves of the owner's `Saves` folder (2026-11-01; flags
0xB0000832, 0x90000832, 0xB0400832 and 0xB0400C32). The data starts the same in all of them:

| Part | Encoding | Notes |
|---|---|---|
| place | ref, six f32, `\|` | the worldspace outdoors (WastelandNV, TheStripWorldNew) or the interior cell (GSDocMitchellHouse), position, rotation in radians (nvfairport: z 1.309 = 75°) |
| actor values | 232 × (f32, `\|`) | three arrays of 77 (the actor value count) and one more entry. First array: 2.0 at codes 38 (Melee Weapons) and 43 (Speech) in 17 saves, 2.0 at code 5 (Strength) in the two level-50 saves (nvfLanius, nvfSlides). Second array: code 24 holds 10 in nvfairport, 143 to 3,419 in mid-game saves and 186,200 in the level-50 ones, code 23 holds 50 to 1,000 [hypothesis: the first is the permanent modifiers, where skill points spent go; the second the base values that play changes, experience (24) and karma (23)]. nvfairport's Melee is 34 = 2 + 2 × 5 + ⌈10 × 0.5⌉ + 15 tag + 2, which matches its probe |
| unknown | u8, `\|`, f32 (1.0), `\|`, vsval, `\|`, u8 (0x5E), `\|`, vsval n, `\|`, n × (ref, `\|`, u8, `\|`), u8 (0x18), `\|`, ref, `\|`, three f32, `\|`, f32, `\|` | the same shape in every save but nvfLanius, whose entries do not fit it |
| bit 29 | `74 \|` ref `\|` | present when change flag 29 is set; in four saves (nvf1topsym, nvf38door, nvf47suite, nvfBoone) it is followed by more entries (`1D \| 04 \| …`) whose sizes are not known, and the reader stops there |
| inventory | vsval count, `\|`, then per item: ref, `\|`, i32 count, `\|`, vsval lists, `\|`, per list: vsval extras, `\|`, per extra: type byte, `\|`, payload, `\|` | nvfairport: 27 items, the same 27 kinds its probes counted |

Extra data sizes seen in the inventory: 0x16 none (on the worn suit, Pip-Boy and glove),
0x21 a ref (3 bytes), 0x25 an f32 (on weapons and armour, 41.25 on a pistol)
[hypothesis: worn, owner, condition]. 0x0D (nvf31mainframe, nvfSlides) has a payload not
mapped; the reader gives no inventory for such a save rather than guess.

The Pip-Boy and its glove are listed worn with count 0 [hypothesis: the count is the
change against the base form's inventory, which holds them].

The player's base form (NPC_ 0x7, bit 2) holds SPECIAL: `05 05 05 05 05 05 0A` in
nvfairport, Luck 10 as its probes read. PlayerClass (CLAS, bit 1) holds the three tag
skills as actor value codes and −1 (38, 43, 44: Melee Weapons, Speech, Survival).

Global table 1 type 1, decoded further: next created ID `|`, world ref `|`, grid x (i32)
`|`, grid y (i32) `|`, world or cell ref `|`, three f32 `|`, quest ref `|`, u32 `|`, f32 `|`,
u32 `|`. nvfairport: grid (9, −29), which holds the player's position (37000, −117160)
[observed].

`fnvtool ext queries --save` gives the runner, in this order: the globals, each quest's
running/completed bits, stages done and not done, objectives and variables (by the
script's variable names), SPECIAL as the player's actor values, the tag skills, the skill
entries of the first modifier array as skill points spent, the inventory as changes on
top of the new game's, and the place. What it leaves out is counted in its output
(`save.skipped`, by reason): a quest record the reader does not understand (bytes left
over, a non-empty list after the variables), a form of a plugin not loaded, a form the save
created (0xFF...), a reference past the form ID array. The inventory's fixed markers
(0x00, 1.0, 0x5E, 0x18 and, with flag 29, 0x74) are checked: a record of another shape
gives no inventory. The tag skills are read only from PlayerClass's record (0x00057E6A).

## 7. What moves between C and D [observed]

C and D: same starting save, same door, about a second of game time apart (GameHour
9.46967 → 9.47007). File size, form ID array, worldspace array and the set of changed
forms are identical. What differs:

- header: play time 0:11:50 → 0:11:51;
- globals: GameHour and GameDaysPassed (table 1 type 3), the two GameHour copies in the
  weather entry (type 8), and floats in types 4 and 5;
- every ACHR and ACRE record (3,318) changes data without changing size: in the common 297-byte ACHR
  layout, an f32 at data offset 54 (17,634.11 → 17,634.16) and a u16/u32 at 240
  (11,688 → 12,453) [observed]; other layouts change the same kind of fields at other
  offsets; the player record changes;
- 38 REFR records, all with flag bit 28 (with or without 31 and 27): the animation/script
  data of activators [hypothesis: animation timers];
- one QUST record (vDialogueGomorrah): script variable 24 holds a copy of GameDaysPassed
  (12.39…) and moved with it;
- no stage, objective, INFO, CELL or FLST record changes.

So almost every actor record carries values that move with time even when nothing
happens to the actor [observed]. A versus C (the same line of play, different session)
differs in 82 REFR records (scripted animation/state data, the moved items), 7 QUST
records (script variables), the player and every actor.

## 8. Open questions

- The meaning of global types 2, 4, 5, 6, 9, 10, 11 and of the 9-byte tail.
- The ACHR/ACRE layouts (flags 1, 11, 18, 29, 31) and the rest of the player record:
  effects, perks, AI state, the second and third actor value arrays, the bit-29 block's
  other entries, the record of nvfLanius and nvfSlides (flag 10, a different shape after
  the actor values) and the inventory's other extra data types (§6.4).
- The full layout of created references (M has 574; their base form and extra data were
  not decoded), and of global types 9 and 10, which grow with play.
- The QUST date stamp's day encoding.

None of these is needed for NVForever's own format; they would matter only to import a
`.fos`, which is not planned (design/save-load.md §6). The lab reader (§6.4) reads only
what the probe sets need.
