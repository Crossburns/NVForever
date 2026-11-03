# Layer 1 design: BSA reader and ESM/ESP parser

Spec section 12 deliverable, in the section 9 format. Status as of 2026-10-09: **implemented
and validated against the user's install** (FNV 1.4.0.525), and cross-checked against the
owner's FNVEdit export of all 10 masters: 6 plugins match on every row, the other 4 differ
by one row each, every one an xEdit normalisation of what the file holds
(docs/spec-notes/esm.md, "Cross-check against xEdit"). Lab: 111/115 Layer 1 scenarios pass.

## 1. Summary and assumptions

Task: design (and, given autonomy, build) the archive and plugin layer so that
FalloutNV.esm and every DLC master load, every FormID resolves, and a complete record
inventory matches a trusted reference.

Assumptions (defaults chosen while the user was away; easy to revisit):
- **Rust**, originally standard library only because the first sandbox had no crates.io
  access. Superseded 2026-10-06 by the owner's decision to use the Rust ecosystem fully:
  see `docs/design/dependencies.md` for what replaced the in-tree zlib, JSON and SHA-256.
- Licence placeholder **GPL-3.0-or-later** (OpenMW's choice). Not yet decided by the user.
- Ground truth = an independent Python reader written by a separate agent from the spec
  notes only, plus the HEDR record count authored by Bethesda's own tool, plus the
  owner's FNVEdit exports (2026-10-09).

## 2. Findings and design

### 2.1 Crates (format libraries have no engine dependencies, spec section 7)

| Crate | Role |
|---|---|
| `fnv-util` | Windows-1252 text (`encoding_rs`), SHA-256 (`sha2`), bounds-checked LE cursor |
| `fnv-inflate` | zlib decoding (`flate2`, zlib-rs backend) with our own header/trailer diagnostics; stored-block encoder |
| `fnv-bsa` | BSA v104 reader, name hash, canonical writer, byte-identical rewrite |
| `fnv-esm` | `raw` container layer → `plugin` → `loadorder` → `schema`/`decode` → `inventory`/`survey` |
| `fnvtool` | candidate CLI; every command prints one JSON document (`docs/cli-contract.md`) |

### 2.2 BSA (details and tags in `docs/spec-notes/bsa.md`)

- Header, folder records, file-record blocks, name block and data layout: [documented]
  by UESP, every rule **[observed]**: a writer using only these rules rebuilds 14 of 15
  staged archives byte-for-byte; the 15th is a mod-manager file.
- Name hash: [documented], verified on all 15,072 files and every folder [observed].
- Archive flag 0x80 does **not** mean embedded names in FNV [observed]; 0x100 never set.
- Two shipped zlib irregularities (missing Adler-32; size-0 compressed entry) must be
  accepted [observed].
- Data is read lazily by offset, and `bsa verify` streams its rewrite into a byte
  comparator, so even the 1.6 GB archives need only the directory plus one file in
  memory [observed on the staged archives].

### 2.3 Plugins (details in `docs/spec-notes/esm.md`)

- Record/group/subrecord container, compression, XXXX: [documented], round-trip
  **[observed]** on all 10 masters (FalloutNV.esm 245 MB in ~3 s).
- HEDR record count = records + groups − 1 in every master [observed] — a free oracle.
- ONAM = exactly the overridden records in cell temporary-children groups (REFR, ACRE,
  ACHR, NAVM; also 7 LAND in LonesomeRoad) in every DLC [observed].
- Pre-order packs inject new forms into FalloutNV.esm's FormID space [observed].
- Irregular data in the user's masters [observed], each tolerated and reported:
  one LAND with a wrong zlib checksum (FalloutNV.esm; shipped defect *or* local damage —
  [hypothesis], see §5); one REFR whose FormID uses a master index beyond the master list
  (GunRunnersArsenal.esm); one dangling reference (DeadMoney INFO 0x01011B23 →
  0x010108DD); two QUST DATA subrecords only 2 bytes long where the documented layout
  needs at least 4 (FalloutNV.esm 0x0001E5CA and 0x0002242A — both have old form versions
  1 and 2, so an older legacy layout is the likely explanation [hypothesis]).

Tolerance policy (spec section 7): structural overruns are hard errors with offsets;
everything else (unknown signatures, mis-sized data, checksum anomalies, unmappable
FormIDs) is preserved and reported. Nothing is silently dropped: records keep their raw
bytes, so a rewrite is byte-identical even when the schema knows nothing about them.

### 2.4 Load order and FormIDs

`LoadOrder::new(&[Plugin])` checks that each master is present and loads earlier, builds
per-plugin master-index → load-index maps, and indexes every record by global FormID
(`Vec<DefRef>` per FormID, last = winner). API: `to_global`, `to_local`, `winner`,
`exists`. Results on base + 9 DLC/packs [observed]: 628,463 unique global forms; 780 forms with
2–6 definitions.

**Reference resolution** [observed] (every FormID-typed field the schema knows): 1,002,308 fields;
102,646 null; 893,586 resolve; 6,074 point into the engine built-in range below 0x800
(4,566 of them 0x14 [hypothesis: the player reference]); **1 dangling**; **1** with an
invalid master index (the GRA REFR). So every FormID the schema can see resolves, except
those two irregularities. (2026-10-09, with the larger schema: 1,012,805 fields, 898,714 resolve, 107,918 null;
19 with a master index beyond the master list: the GRA REFR's base plus 18 FalloutNV.esm
ANIO DATA with a stale index 01; dangling: the DM INFO plus 6 REGN / 7 WTHR fields,
esm.md Unknowns 7.)

### 2.5 Schema-as-data strategy

- Layouts live in `schemas/fnv/*.json` (`fnv.schema/1`), loaded and validated at runtime;
  correcting a layout needs no recompile.
- Type system: primitives, `formid`, `zstring`, named types, structs with trailing
  optional fields (form-version growth), fixed or fill-to-end arrays, enums, flags.
- Lookup: (record type, signature) first, then `common`.
- Every entry carries `confidence` and a `note` citing its source (currently xEdit
  `wbDefinitionsFNV.pas` / `wbDefinitionsCommon.pas`, MPL-2.0, read as documentation and
  re-expressed in our own format).
- Coverage is measured, not assumed: `fnvtool esm coverage` decodes every subrecord and
  ranks undecoded (record, signature) pairs by instance count. That list is the work
  queue (spec section 7: logging of unhandled data drives the work).
- `fnvtool esm survey` gives purely observational statistics per pair (sizes, string-like,
  FormID-array-like) to check or derive layouts.

Current coverage [observed] (subrecord instances decoded exactly by a documented layout;
about 0.5% of FalloutNV.esm instances are fields the layout itself marks as opaque bytes,
e.g. compiled script data, counted separately as `opaque_bytes`):

Measured 2026-10-09 (`fnvtool esm coverage --schema schemas/fnv`, all 10 masters):

| Plugin | Decoded | Pairs fully decoded | Trailing | Errors |
|---|---|---|---|---|
| FalloutNV.esm | 94.74 % of 2,321,456 | 972 / 1,155 | 0 | 2 |
| DeadMoney.esm | 95.50 % of 160,880 | 747 / 889 | 0 | 0 |
| HonestHearts.esm | 93.80 % of 179,706 | 718 / 848 | 0 | 0 |
| OldWorldBlues.esm | 96.86 % of 225,669 | 720 / 854 | 0 | 0 |
| LonesomeRoad.esm | 96.03 % of 161,312 | 693 / 810 | 0 | 0 |
| GunRunnersArsenal.esm | 94.05 % of 6,853 | 231 / 259 | 0 | 0 |
| Caravan/Classic/Mercenary/TribalPack | 98.61 / 94.27 / 97.47 / 100 % | 63/64, 95/97, 70/72, 51/51 | 0 | 0 |

The 2 errors are the two 2-byte QUST DATA (§2.3). The 8 WATR DATA "trailing" of the
previous measurement were the older 186-byte DATA form (visual data plus damage, in the 8
FalloutNV.esm waters without DNAM); since 2026-10-09 the schema has size-decided unions
and decodes both forms [documented: xEdit WATR; observed: 70 × 2 bytes, 8 × 186 bytes].

Combat layouts (2026-10-09, `schemas/fnv/combat.json`, docs/spec-notes/combat-data.md):
PROJ, EXPL, BPTD, AMEF, ENCH and SPEL decode with zero unknown, trailing or errors in all
ten masters. FalloutNV.esm is now 94.89 %, DeadMoney 95.57, HonestHearts 93.88,
OldWorldBlues 97.06, LonesomeRoad 96.13, GunRunnersArsenal 96.03, ClassicPack 94.90; the
others are unchanged. DEBR DATA puts a NUL-ended string in the middle of a field and
stays undecoded.

**Record-specific layouts exist for 70 of the 107 record types present** (MSTT, GRAS,
PWAT, ADDN, ANIO, IPCT, IPDS and TACT added 2026-10-09, `schemas/fnv/placed.json`, each
fully decoded with zero trailing/errors in all 10 masters and spot-checked: their FormID
fields point at records of the documented types). Records of those 70 types are 97.2 % of
FalloutNV.esm's 465,016 records. The other 37 types decode only their shared subrecords
(EDID, FULL, OBND, models, scripts, ...); by FalloutNV.esm record count the biggest are
NAVM (4,771), PACK (4,163), CPTH (418), TERM (344), CAMS, SPEL, IDLM, LSCR,
EXPL, ENCH, MSET, ARMA. Largest undecoded subrecord instances by record type: PACK 53,603,
NAVM 30,306, NAVI 9,543, DIAL 5,058 (INFC/INFX), TERM 3,962, MSET, BPTD, CREA (NIFT),
REFR (TNAM). Schema v1 limits behind several gaps: no context rules (the same signature
meaning different things by position), no length-prefixed strings, no EditorID- or
sibling-decided unions (GMST DATA depends on the Editor ID's first letter), no
count-prefixed arrays (CREA NIFT). Size-decided unions (`{"union": [{"size": n, "type":
T}, ..., {"type": fallback}]}`) exist since 2026-10-09; NUL-separated string lists have
the primitive `zstring_array` (2026-10-08, KFFZ/NIFZ).

### 2.6 Records the schema cannot express, and readers still by hand (issue #1000)

Runtime readers take their values from the schema's decoded fields wherever the schema
has the layout (CLAS, CPTH, IDLE and RADS/DEHY/HUNG/SLPD moved over 2026-10-31/11-01).
The records below stay hand-read because schema v1 cannot say their layout:

| Record | Subrecords | What v1 lacks |
|---|---|---|
| PERK | DATA, EPFD | Context: DATA after the record header is the perk's data, DATA after a PRKE header is the effect's, laid out by that header's type; EPFD is a float, a FormID, a script or text by the EPFT before it (sibling-decided union) [documented: xEdit wbDefinitionsFNV.pas PERK; observed: docs/spec-notes/perk-entry-points.md, `data.rs` PerkEntry]. The headers (PRKE, PRKC, EPFT, EPF2, EPF3) are in `gameplay.json`. |
| DEBR | DATA | A NUL-ended model path in the middle of a struct, followed by a byte; `zstring` reads to the subrecord's end [documented: docs/spec-notes/combat-data.md §1]. |
| NAVM | NVGD | Per grid square a u16 count and that many u16 indices, for divisor² squares: an array whose count is computed from an earlier field [documented: docs/spec-notes/navmesh.md §1.1]. The other NAVM arrays are fixed-size elements to the end and could go in the schema; `fnv-nav` reads the whole record by hand. |
| NAVI | NVMI | The island block is present only when flag bit 5 is set (a flag-decided field) [documented: navmesh.md §1.2]. NVCI (a FormID, then three u32-counted FormID arrays) fits the count-prefixed arrays. |

Records the schema could express but has no entry for yet, read by hand at runtime
[observed: `esm coverage` lists them unknown; readers in `crates/fnv-game/src/data.rs`,
`crafting.rs`, `caravan.rs`]: RCPE, RCCT, CDCK, AVIF (ANAM), VTYP (DNAM); LSCT DATA is read
by nothing. CSNO and CCRD have entries (`objects.json`) but `casino.rs` and `caravan.rs`
still slice their bytes. Each needs an xEdit-sourced entry where missing (schemas/README.md)
and its reader switched to the decoded fields, as was done for CLAS; the masters' totals in
`tools/smoke/expected/esm-coverage.toml` move with each new entry.

## 3. Code and schema

- Code: `crates/*` (tests: `cargo test --workspace --release`).
- Schemas: `schemas/fnv/{common,world,objects,gameplay,sky,placed}.json` (70 record types,
  503 record-specific and 68 shared subrecord definitions, 56 named types; 2026-10-09).
- Lab: `parity-lab/` (Python stdlib; 26 self-tests), scenarios in
  `parity-lab/scenarios/layer1_*.json`.
- Capture: `tools/capture/` (oracles, xEdit script, inventory; 16 self-tests).

Parity result on the cloud-staged data [observed]: **84/84 capturable Layer 1 scenarios pass** (10
plugin inventories incl. all 465,016 FalloutNV.esm records, headers, stats, byte-identical
round-trips; 15 archive listings with SHA-256 of every decoded file, headers,
round-trips). The other 31 scenarios need data that only exists on the user's machine
(7 archives over 400 MB; 10 FNVEdit exports) and report as `error` until captured.

Parity result on the owner's machine, 2026-10-09 [observed]: **111/115 Layer 1 scenarios
pass**, 0 errors (every archive incl. the big ones, all 10 FNVEdit exports present). The
4 failures are `L1.esm.inventory_xedit.{FalloutNV,DeadMoney,OldWorldBlues,GunRunnersArsenal}`,
one row each, all documented xEdit normalisations (§4 step 7).

## 4. Step-by-step validation plan against FalloutNV.esm

Each step is a lab scenario or a command; all steps 1–6 pass today on the staged copy.

1. **Container integrity**: `fnvtool esm verify FalloutNV.esm` → byte-identical rewrite,
   0 decompression errors, 0 subrecord parse errors, payload round-trip identical
   (scenario `L1.esm.roundtrip.FalloutNV`).
2. **Bethesda self-check**: `fnvtool esm stats` → HEDR count = records + groups − 1
   (`L1.esm.stats.FalloutNV`, which also compares per-type and per-group-type counts with
   the independent oracle).
3. **Header**: masters, version, next object ID, ONAM size vs oracle (`L1.esm.header.*`).
4. **Record inventory**: all 465,016 records keyed by FormID; type, EditorID, FULL,
   flags, form version vs the independent Python oracle (`L1.esm.inventory.FalloutNV`).
5. **Load order**: `fnvtool esm loadorder` over base + DLC: no missing masters, every
   record FormID mappable (one known exception), overrides/injected forms/ONAM reported.
6. **Field decoding**: `fnvtool esm coverage --schema schemas/fnv` → zero `trailing`,
   errors only where noted; reference resolution via `loadorder --schema`.
7. **Third-party cross-check (done 2026-10-09)**: FNVEdit export vs candidate
   (`L1.esm.inventory_xedit.*`), the M1 exit criterion: 6/10 pass with every row equal;
   FalloutNV, DeadMoney, OldWorldBlues and GunRunnersArsenal differ by one row each, all
   four documented xEdit normalisations (three missing required subrecords that xEdit
   adds as empty, one out-of-range master index that xEdit folds into the file's own),
   esm.md "Cross-check against xEdit". `oracle_esm.py` agrees with us on all four rows.
8. **Repeat on DLC masters** (steps 1–7 per plugin; scenarios exist for all 10).

## 5. Open questions and verification steps for the user

Answered (2026-10-09), kept for the record:

- ~~Run the FNVEdit export~~: done by the owner for all 10 masters; result in §4 step 7.
  The predicted GRA difference (0x02000801 shown as 0x01000801) is one of the four rows.
- ~~Big archives~~: validated on the owner's machine (2026-10-06) and now part of every lab
  run there (all `L1.bsa.*` pass).
- ~~INI `[Archive]` section~~: the owner's `Fallout.ini` and `FalloutPrefs.ini` hold the
  shipped default `SArchiveList` (the six base archives), `bInvalidateOlderFiles=1`,
  `SInvalidationFile=ArchiveInvalidation.txt` (bsa.md, layer2.md §2.5).
- ~~Language~~: Rust, using the ecosystem fully (owner decision, dependencies.md).

Still open:

1. **xEdit differences (owner decision)**: the four `L1.esm.inventory_xedit.*` rows are
   documented xEdit normalisations, not reader errors (esm.md), so those scenarios stay
   failing under the rule that comparators, scenarios and goldens are not changed to pass.
   A clean option, if wanted: a *known differences* list in the scenario format (each
   entry: key, field, expected oracle value, candidate value, and a link to the spec-note
   paragraph that explains it), reported separately by the lab as "documented difference"
   instead of pass or fail, and only ever edited by the owner. Not implemented.
2. **Checksum anomaly**: the SHA-256 of the owner's current `FalloutNV.esm` (re-hashed
   read-only 2026-10-09) is still
   `50991d36804b7d1e70df1afd7471b72f0e29d1b456ee2516a9717c002564e7c1`, the copy in which
   LAND 0x00150FC0's bad Adler-32 was found. A Steam "Verify integrity of game files"
   (owner-only) or another copy would tell a shipped defect from local damage.
3. **In-game behaviour checks**: the out-of-range FormID is **settled** by an
   unattended capture (2026-10-09, job `l1_oob_formid`): the game places GRA's REFR
   0x02000801 in `TestMentats` as GRA's own 0x..000801 with GRA's own base 0x..000800,
   exactly as xEdit displays it. **Owner decision**: make `LoadOrder::to_global` read an
   out-of-range index as the plugin's own (done 2026-10-15, PR #281; it fails the two `L3.world.*.interiors`
   scenarios by exactly this reference because `oracle_world.py` and its golden follow the
   old rule, so oracle and golden would be updated with it; esm.md). Still open:
   whether the dangling DM INFO "Previous INFO" link (0x01011B23 → 0x010108DD) changes
   the order of `NVDLC01RadioElijahEnd`'s lines; the capture needs a save at the end of
   Dead Money (owner), plan in esm.md Unknowns 4.
4. **Licence**: the owner's decision (GPL-3.0-or-later assumed as placeholder).

## 6. Updated spec notes

Written to `docs/spec-notes/bsa.md`, `docs/spec-notes/esm.md` and
`docs/spec-notes/layer0-install.md` (plain-language, tagged).
