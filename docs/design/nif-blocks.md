# JIP's NIF block functions in NVForever (N6)

JIP's functions that read and change a reference's model nodes by name (GetChildBlocks,
GetNifBlockTranslation/Rotation/Scale/Flag and their setters, RenameNifBlock,
RemoveNifBlock, InsertNode, AttachModel, ModelHasBlock). Facts are from their pages
`[documented]` and the peer's nif-blocks set in the original game `[observed]`. Nodes
are known by name only; no memory layout of the original is used (extender-compat.md
§5.8).

## The split

- **Game side (`fnv_game::nif_blocks`):** a mirror of the node tree of every reference
  a script asked about (`SceneMirror`), filled by the engine each frame, and the scripts'
  changes queued for the engine. A change shows in the mirror at once [observed: a set
  reads back in the same frame]. Until the engine has told a tree, the getters give
  ar_Null or 0, as the original does before a placed actor's 3D exists [observed: 5
  frames after PlaceAtMe].
- **Engine side (`nvforever` nif_mirror.rs):** a reference's root is its actor rig, else
  the topmost entity drawing it; actors' skeleton node names come from the rig (their
  entities carry no `Name`). Each frame the queued changes are applied; a set
  translation, rotation or scale is held over the animation, frame after frame [observed:
  set-then-read stays exact on an animated bone]; the trees of the watched references are
  told back, depth first, a repeated name shown as `name:0`, `name:1` … [observed:
  `Object`, `Object:0`].
- **ModelHasBlock:** the base form's model file is read through the host's reader
  (`fnv_game::mod_files::read_data`) and its header string table searched for the exact
  name [observed: case-sensitive].

## Conventions

- Rotation: degrees, X Y Z extrinsic (glam `EulerRot::ZYX` read backwards); the getter's
  "intrinsic" form (whichRotation 2) is not yet told apart.
- GetNifBlockTranslation / Rotation with an axis letter give that number; with a number
  or nothing, the three as an array [observed: JIP 57.3 returns an array].
- Name lookups: exact first, then without regard to case [observed: InsertNode took a
  lower-case target].

## Not built yet

AttachModel's model is not drawn (the mirror keeps the request); the first-person model
(`pcModel`); GetBlockTextureSet; the world-space translation (`getWorld`).
