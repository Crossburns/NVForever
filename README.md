# NVForever

An open-source, clean-room reimplementation of the **Fallout: New Vegas** engine, in the
spirit of OpenMW. It runs on the data files of *your own copy* of the game; it ships no
Bethesda content and contains no Bethesda or Gamebryo code.

NVForever is not affiliated with or endorsed by Bethesda Softworks, ZeniMax Media,
Microsoft or Obsidian Entertainment. See [LEGAL.md](LEGAL.md).

## Getting started

1. Own and install *Fallout: New Vegas* (base game; DLC optional).
2. Download a build from the [releases page](https://github.com/LootJunkieAzza/NVForever/releases)
   and unpack it **outside** the game folder.
3. Run `nvforever.exe` (or `./nvforever.sh` on Linux) and point it at your game folder
   when asked.

NVForever never writes into the game folder. Its settings, saves and logs live in
`Documents\My Games\NVForever`.

## Mods

- **Mod Organizer 2:** add `nvforever.exe` to MO2's executables (the drop-down beside
  **Run**, then **<Edit…>**) and start it from there. It loads the plugins ticked in MO2,
  in MO2's order, and the game folder MO2 manages. See
  [docs/mod-organizer-2.md](docs/mod-organizer-2.md).
- **Without a mod manager:** put the mod in the game's `Data` folder, then turn its plugin
  on in `Documents\My Games\NVForever\plugins.txt` with a `*` in front of its line. The
  order of the lines is the load order. NVForever keeps its own list, so the original
  game's mod list is not changed.

Script extenders (xNVSE, JIP LN, JohnnyGuitar and several smaller ones) are built into the
engine: their script functions run without the plugin `.dll` files, which NVForever never
loads. A mod that needs a plugin's engine patches, or a function not built yet, may not work.

## Building from source

Needs [Rust](https://rustup.rs) and Python 3.11+.

```bash
cd engine
cargo run --release --bin nvforever -- --data "<path to Fallout New Vegas>/Data"
```

More: [docs/development.md](docs/development.md) (build, test, options, tools) ·
[docs/status.md](docs/status.md) (what's built) ·
[docs/outstanding.md](docs/outstanding.md) (what's left).

## Community

Chat, questions and playtesting: [Discord](https://discord.gg/3HsVrsXSb).

## Bugs

Report bugs and differences from the original with the
[issue forms](https://github.com/LootJunkieAzza/NVForever/issues/new/choose). Crash
reports are written to `Documents\My Games\NVForever\logs`. Never attach game files.

## Contributing

Pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) first: the clean-room
rules there decide what can be merged.

## Support

NVForever is free. If you'd like to support it: [ko-fi.com/lootjunkieazza](https://ko-fi.com/lootjunkieazza).
Donations buy nothing extra; you still need your own copy of the game.

## Licence

GPL-3.0-or-later; see [LICENSE](LICENSE) and [LEGAL.md](LEGAL.md#licence).
