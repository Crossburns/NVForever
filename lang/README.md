# Translations of NVForever's own words

NVForever writes some words itself: the settings page, a few messages, the terminal
hacking log. The original game's text is not copied (LEGAL.md). The game's own text
(items, dialogue, quests) comes from the plugins of your install, so a localized install
already shows it in its language. Only NVForever's own words need a translation, and
this folder holds them, one file per language (#1443).

## A language file

`<id>.toml`, UTF-8, for example `de.toml`:

```toml
name = "Deutsch"

[words]
"Language" = "Sprache"
"Accepted." = "Angenommen."
"{n} of {len} right" = "{n} von {len} richtig"
```

- `name` is how the settings page lists the language, in the language itself.
- Each line under `[words]` is the English text exactly as NVForever writes it, then
  your translation. A word with no line, or an empty one, stays English.
- `{n}`, `{len}`, `{needed}` and the like are filled in by NVForever. Keep them, in any
  order your language needs.
- Write your own words. Don't copy the original game's strings into a file
  (LEGAL.md).

## Where NVForever looks

1. This `lang` folder beside `nvforever.exe` (the translations that come with NVForever).
2. `lang` in NVForever's home (`Documents\My Games\NVForever\lang`), for your own.
3. The folder `NVF_LANG_DIR` names, if set.

When two folders have the same id, the first one wins. The language is chosen on the
settings page (Interface, Language) and acts at once. It is kept as `language=<id>` in
`NVForever.ini`.

The game's fonts hold 256 glyphs in the install's code page (docs/spec-notes/fonts.md),
so a translation can only use the letters that code page has: Cyrillic on a Russian
install (`[Text] CodePage=1251`), but not Chinese.
