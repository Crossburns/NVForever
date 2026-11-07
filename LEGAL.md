# Legal notice

NVForever is a free, open-source project: a clean-room reimplementation of the engine
behaviour of *Fallout: New Vegas*, written from scratch in Rust. It is an engine only. To
do anything with it you need the data files of your own lawfully purchased copy of the
game.

This page describes how the project is run. It is not legal advice.

## Independence

NVForever is an independent, non-commercial project made by volunteers. It is not
affiliated with, endorsed by, sponsored by or otherwise connected to Bethesda Softworks,
ZeniMax Media, Microsoft, Obsidian Entertainment, Valve, or any other company involved in
making or publishing *Fallout: New Vegas* or the middleware it uses.

- NVForever contains no code from the original game, from its editor (the G.E.C.K.) or
  from the Gamebryo engine.
- NVForever is not a replacement for buying the game. It does not run without the
  original data files and does not help anyone obtain them.
- NVForever does not circumvent copy protection. The Steam executable's code section is
  encrypted and the project does not unpack it.
- NVForever does not modify the original game. It never writes into the game folder or
  into `Documents\My Games\FalloutNV`; the game's `Data` folder is read-only input
  ([docs/design/install-layout.md](docs/design/install-layout.md)).

## Trademark acknowledgements

*Fallout*, *Fallout: New Vegas* and related names and logos are trademarks or registered
trademarks of Bethesda Softworks LLC, a ZeniMax Media company, in the United States
and/or other countries. *Fallout: New Vegas* was developed by Obsidian Entertainment.

Gamebryo, Havok, SpeedTree, FaceGen, Bink, Steam, Windows and all other product and
company names mentioned in this repository are trademarks of their respective owners.

These names are used only to say what NVForever is compatible with and which file formats
and behaviours it describes. Their use does not imply any affiliation or endorsement, and
the project claims no rights in them. The name "NVForever" refers to this project only.

## No game files distributed

This repository and its releases contain no content from the game:

- no plugins or archives (`.esm`, `.esp`, `.bsa`);
- no meshes, textures or animations (`.nif`, `.dds`, `.kf`, FaceGen `.egm`/`.egt`/`.tri`);
- no audio, voice or lip-sync files (`.ogg`, `.wav`, `.mp3`, `.lip`);
- no saves, no executables, no screenshots or frames rendered from game data.

`.gitignore` blocks these file types so they cannot be committed by accident.

Testing does need material derived from the game: reference outputs ("goldens"), dumps,
captures from the running game and reference screenshots. These are produced by each
developer from their own copy and stay private, in a folder outside the repository. They
are never committed or published.

What the repository does contain is our own description of the game's file formats and
behaviour (`docs/spec-notes/`, `schemas/`) and the tools that read your copy of the data.
Those descriptions name records, fields, Editor IDs and file paths where that is needed to
explain a format or cite an observation. Quotations from documentation are kept short and
cite their source; descriptions of scripting functions are written in our own words.

The engine itself shows a handful of short on-screen phrases that are not in the data
files and so cannot be read from your copy, such as a terminal's prompts. These are kept
to plain functional words and are tagged where they are defined (for the terminals,
[docs/spec-notes/hacking.md](docs/spec-notes/hacking.md) §8). Where the original's
wording names a brand from the game's fiction, NVForever uses its own wording instead,
and built-in labels the data files lack are written by the project, not transcribed
from the original. A built-in label may be a name or term that the data files' own shown
text already uses, such as "S.P.E.C.I.A.L.", which FalloutNV.esm writes in perk
descriptions and messages; it is spelled as the data spells it and the record is cited
where the label is defined. It is then taken from the player's own files, not from the
original's screen or executable. Abbreviations or phrases found only on the original's
screen are not covered and still get the project's own words. The
game's own dialogue, quest text, messages, names, notes and menu layouts are read at run
time from your game's files and are not in this repository.

## How we learnt the game's behaviour and recreated it

NVForever follows a clean-room process, in the spirit of OpenMW.

**Sources we use**

1. *Public documentation.* Community and vendor documentation of the file formats and
   the scripting language: the UESP format pages, the G.E.C.K. wiki, Microsoft's DDS
   documentation, the xEdit project's record definitions (Mozilla Public License 2.0) and
   the niftools `nif.xml` description of NIF blocks. xEdit's definitions and `nif.xml`
   are read as documentation and re-expressed in this project's own data format
   (`schemas/`); no code from either is included and none is generated from them
   ([schemas/README.md](schemas/README.md)).
2. *The data files of a lawfully purchased copy.* We read the game's own archives and
   plugins and record what is in them: counts, layouts that decode with nothing left
   over, files that can be rewritten byte for byte.
3. *The running original game.* We observe what the unmodified game does from the
   outside: `nvf_capture`, a plugin for the xNVSE script extender, runs unattended jobs
   and records results such as item counts, quest stages, positions over time and
   screenshots. It works only through xNVSE's public plugin interface and the game's own
   script language; it uses no game memory addresses and no game data structures
   ([docs/spec-notes/xnvse-capture.md](docs/spec-notes/xnvse-capture.md)).

**Sources we do not use**

- No leaked source code or debug symbols, from any platform.
- No decompiled or disassembled game or editor code. Of the game's executable, only
  unencrypted data was read (version resource, import table, plain strings and the type
  names they include), to identify the game build and the third-party components it uses
  ([docs/spec-notes/layer0-install.md](docs/spec-notes/layer0-install.md)).
- No code or notes from other projects whose provenance does not meet these rules; such
  projects are not opened. Before this rule was tightened (2026-10-30), one such project,
  nv-rs, whose own policy admits leaked debug symbols and translated decompiled code, was
  used once, to cross-check NIF animation layouts already taken from `nif.xml` and the
  data. No code or layout was taken from it, and that cross-check has since been
  replaced by the data alone: every animation file in the game and its add-ons decodes
  exactly and re-encodes byte for byte ([docs/design/layer2.md](docs/design/layer2.md)).

**From notes to code**

- Every format and behaviour is first written down in plain language in
  [docs/spec-notes/](docs/spec-notes/). Each factual claim is tagged `[documented]` (with
  the source cited), `[observed]` (measured on a real install or in the running game) or
  `[hypothesis]` (not yet verified). Offsets, opcodes and field meanings are never
  invented.
- The implementation is written from those notes, not from the original program.
- Each layer is checked twice: against independent readers written from the same notes
  (`tools/capture/oracle_*.py`), and against captures from the original game, by the
  parity lab ([parity-lab/README.md](parity-lab/README.md)).

Contributions are accepted on the same terms: by contributing you confirm that your work
follows these rules and that you have the right to submit it, and each commit is signed
off under the [Developer Certificate of Origin](https://developercertificate.org/)
([CONTRIBUTING.md](CONTRIBUTING.md#licence)).

## Licence

NVForever is free software, licensed under the **GNU General Public License, version 3 or
(at your option) any later version** (`GPL-3.0-or-later`). The full text is in
[LICENSE](LICENSE). Copyright belongs to the NVForever contributors.

The program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.

This licence covers NVForever's own code, schemas and documentation. It does not cover,
and grants no rights in, *Fallout: New Vegas* or its data files, which remain the property
of their owners and subject to the terms under which you bought the game.

Third-party Rust crates that NVForever depends on keep their own licences (MIT,
Apache-2.0 and similar permissive licences, and a few MPL-2.0 crates). The allowed set is
enforced by `cargo deny` ([deny.toml](deny.toml),
[docs/design/dependencies.md](docs/design/dependencies.md)). Every release archive
carries `THIRD-PARTY-LICENSES.txt`: each package the programs are built from, its
licence and where its source is, and the licence texts the packages ship.

The capture plugin `nvf_capture` is a separate program that runs inside the original
game. It is under the same licence with an additional permission under GPLv3 section 7
to be combined with the game and the xNVSE script extender
([tools/capture/nvf_capture/LICENSE-EXCEPTION.txt](tools/capture/nvf_capture/LICENSE-EXCEPTION.txt)).
