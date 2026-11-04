# kNVSE's AnimGroupOverride folders (#1461)

kNVSE replaces actors' and weapons' animations with files a mod puts in a folder, with no
plugin and no script. NVForever reads the folders in `crates/fnv-game/src/nvse_anim_folders.rs`
and plays what they give through the override layer of the kNVSE functions
(docs/design/knvse-anim.md §2).

Sources: the GECK wiki export's CopyAnimationsToForm page, the one page that mentions the
folder [documented: "Copies the kNVSE AnimGroupOverride animations, including conditions
and other attributes"]; the file trees of the 25 mods installed in the owner's two Mod
Organizer folders that have one (8,837 files, 299 JSON files, 778 entries; survey of
2026-11-03) [observed]. No kNVSE source, readme or mod page was read. What the files show is
the layout mods use; how kNVSE reads it is [hypothesis] unless said otherwise.

## 1. Layout [observed]

- `meshes\AnimGroupOverride\` holds JSON files directly (`RNC_AMAKFlamethrower.json`) and
  one folder per set of animations (`RNC_AMAKFlamethrower\`). Folder names are the mods'
  own; nothing in the name says which form they are for.
- In a set's folder, `_male\` holds the third-person files and `_1stPerson\` the
  first-person ones (3,974 and 4,441 files; any case: `_Male`, `_1stperson`). No `_female`
  folder was seen. Under them the files sit flat or in subfolders mirroring the game's
  (`_male\Locomotion\mtidle.kf`, `_male\human\2haaim.kf`) or with any name
  (`_male\New folder\2haEquip.kf`).
- A set's folder can hold further sets, named by their path (`Rock_ServiceRifle\JamLow\`
  with its own `_1stPerson\`); entries name them as `"folder": "Rock_ServiceRifle\\JamLow"`
  (14 entries).
- Files are `.kf` (8,517), with `.bak` copies, a `chances.txt` beside some jam files and
  `_male.mohidden` folders (Mod Organizer's hidden files). File names need not be the
  group's (`1hpReloadL v2.14.kf`, `2haJamF_a.kf`).
- `_1stPerson\Mod1\` … `Mod3\` folders sit beside the plain files of a weapon
  (`Ani_DesertEagle\_1stPerson\Mod1\1hpReloadL v2.14.kf`) [hypothesis: the files for the
  weapon with that mod attached].

## 2. The JSON files [observed]

A file is a list of entries, or one entry; 59 files are empty. An entry's keys:

| Key | Seen | Value |
|---|---|---|
| `folder` | 778 | the set's folder, under `AnimGroupOverride` |
| `mod` | 710 | a plugin's file name (`FalloutNV.esm`, `mil.esp`, `Fallout3.esm` for TTW) |
| `form` | 708 | an ID in that plugin, hex without `0x`: one (`"7E56B"`) or a list (254) |
| `condition` | 422 | a script condition (below) |
| `priority` | 84 | a whole number: 1, 2, 10 |
| `pollCondition` | 12 | `true` |
| `comment` | 10 | text |

- The forms are weapons (`FalloutNV.esm` `"432C"` with the folders `Rock_FatMan…`, and
  the mods' own guns) and the player (`"form": "14"`, PlayerREF's ID); 68 entries have no `mod`/`form` and are for every actor whose condition
  holds (`GetCreatureType == -1 && GetIsSex Female …`). [hypothesis: base actors, races and
  creatures work as forms too; none was seen.]
- Conditions are script expressions as an `if` takes them: `GetCurrentAmmoRounds == 0`,
  `GetPCIsSex Female == 1 && GetWeaponHealthPerc >= 85`, `(Goo1.AuxVarGetFlt "*_BnBEqSocket")
  == 1`, `GetINIFloat "Perk:b357MagnumrevolverCowboy" "RevolverAimAnimations.ini"`,
  `IsIdlePlayingEx ProtectronTurnedOff == 1`. `this` names the actor (`this != Player &&
  IsSwimming`), and functions without a reference run on it.
- A form-wide set comes first and its special cases after, with a condition and often a
  priority (the Fat Man: the plain set, then `_empty` on `GetCurrentAmmoRounds == 0`, then
  jam sets by `GetWeaponHealthPerc`). Files are named to sort first or last
  (`1Another_Millenia_base.json`, `zzz_heavy_fatman.json`).
- 20 entries name a folder that is not there.

## 3. What NVForever does

- **Reading:** once, when the game's Data folder is known: the JSON files directly under
  `meshes\AnimGroupOverride` in every read root (NVForever's layer first; Mod Organizer's
  virtual Data is one root), read in name order without regard to case. An entry is kept
  when its plugin is loaded and its folder has animation files; its `form` IDs become the
  plugin's load-order IDs. A file that is empty is skipped; one that is not JSON, an entry
  without a folder, one naming a plugin but no form [hypothesis: a slip; only entries
  with neither are for every actor], and one whose folder is missing or empty are logged
  and skipped
  (the broken-files rule). The start-up line `AnimGroupOverride: N entries from M files`
  says what was read. Only loose files are read [hypothesis: mods ship them loose; none
  was seen in an archive].
- **A set's files:** every `.kf` under its `_male` (third person) and `_1stPerson` (first
  person) folders, at any depth, except under `Mod1`–`Mod3` (left for a follow-up).
- **Which hold:** four times a second, for the player and each actor the engine draws: the
  entries for the actor's reference, its base or its equipped weapon, and those for every
  actor, whose condition holds on the actor (EvaluateString's compiler; a condition that
  fails to compile or run counts as false and is logged once). [decision: the original
  checks a condition when the animation starts, or continually with `pollCondition`
  (hypothesis); a quarter second covers both closely enough, so `pollCondition` is read
  but changes nothing.]
- **Which wins:** the holding entries in order of priority (none is 0), then of reading;
  their files are given in that order, and of two files of the same group the later wins
  (as the engine applies SetActorAnimationPath's paths) [hypothesis: the higher priority
  wins, and at the same priority the later entry]. SetActorAnimationPath's paths come
  after the folders' [hypothesis: a script's call wins over what a mod installed]
  (`Game::anim_overrides`).
- **CopyAnimationsToForm** adds the target form to every entry of the source, conditions
  and priorities with them [documented].
- **The engine** plays the third-person files whose name is a standing locomotion group
  (`mtidle.kf`, `mtforward.kf` …) on drawn actors, as it plays SetActorAnimationPath's
  (`engine/nvforever/src/anim_paths.rs`). A file's group is read from its name, not its
  sequence [hypothesis, as knvse-anim.md §2; the mods' `1hpReloadL v2.14.kf` shows names
  can differ].

## 4. Not built (follow-ups)

- The engine plays no first-person override, no weapon group (attacks, reloads, aims,
  equips) and no idle from them: it has no player for SetWeaponAnimationPath's either.
  Weapon sets are read and matched game-side, ready for it.
- The group of a file from its sequence name instead of its file name; several files of
  one group (`2haJamF.kf`, `2haJamF_a.kf`) and `chances.txt` [hypothesis: variants picked
  at random].
- `Mod1`–`Mod3` folders by the weapon's mods.
- An observer check in the original: which of two holding entries wins at equal and at
  different priorities, and whether a script's SetActorAnimationPath beats a folder.
