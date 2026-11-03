# Weapon wheel: hold a hotkey for a slow-time wheel

Status 2026-10-23: **built** on `claude/weapon-wheel` (`engine/nvf-play/src/hotkeys.rs`,
`weapon_wheel.rs`, `weapon_wheel/draw.rs`, `weapon_wheel/slice.wesl`; smoke scenario
`weapon-wheel`). The Pip-Boy's hotkey panel (§3) since 2026-10-27
(`pipboy/hotkey_panel.rs`, smoke `pipboy-hotkey-assign`). The sounds (§8) are built. Owner request: every hotkey (1–8)
works as a quick key when tapped, and opens a wheel of all eight when held, modelled on
Red Dead Redemption 2's weapon wheel (the owner's reference picture is kept privately in
`C:\fnv-private\references\weapon_wheel\owner-reference.webp`, never committed).

The original has hotkeys but no wheel on the HUD: its only hotkey "wheel" is the Pip-Boy's
assignment panel (§3). So the wheel itself is NVForever's own design, like VATS mode
(docs/design/vats-mode.md); what the hotkeys hold and how slot 2 behaves follow the
original.

Tags: `[documented]` cites a file, `[observed]` is something we looked at in the game
data or a reference, `[hypothesis]` is unconfirmed, **`[design]` is our own choice where
the owner's request leaves something open**. The owner's request is quoted as "owner".

Mockups made from the game's own pictures (private, never committed:
`C:\fnv-private\references\weapon_wheel\mock1.png` slot 1 held, `mock4.png` slot 4 picked
with the mouse, `mock2.png` slot 2; `mock.py` draws them).

## 1. The owner's design

- **Tap** (owner): pressing a hotkey 1–8 uses or equips the item in it at once.
- **Hold** (owner): holding a hotkey opens the wheel; time slows; the view behind it
  blurs; the held key's slice is lit as the one picked.
- **Release** (owner): letting go of the key does what a tap does for the slice that is
  picked when it is let go.
- **Pick another** (owner): pressing another hotkey while the wheel is open, or pointing
  the mouse in a direction, picks that slice instead (mouse to the right picks slot 4).
- **Layout** (owner's picture): eight slices clockwise from the top left: 1 top left,
  2 top, 3 top right, 4 right, 5 bottom right, 6 bottom, 7 bottom left, 8 left. The
  picked item's name and figures in the middle; ammunition counts under the guns.

## 2. Timing: tap or hold

- A press shorter than [`HOLD`] = 0.25 s real time is a tap [design: long enough that a
  quick press never flashes the wheel, short enough that the hold feels immediate; RDR2
  opens its wheel on a hold of roughly this length (owner's reference)]. The tap acts on
  key-up, not key-down, so a hold never also fires the tap.
- At [`HOLD`] the wheel opens with the held key's slice picked.
- Measured in real time (`Time<Real>`), so the slow clock does not stretch it.
- Ready Item's tap/hold split (`control.rs`, `HOLSTER_HOLD`) is the model to copy.

## 3. What a hotkey holds

**[documented: menus\prefabs\hotkeys.xml, hotkey.xml]** The original's hotkey panel is
"a wheel of eight hot key elements": `HK_Item_0` … `HK_Item_7`, each a 60 × 60 box with
the item's icon and its number (`_HotKeyNumber` + 1). Item 1 (key 2) is commented out and
replaced by a fixed `IM_AmmoSwap` image (`interface\icons\typeicons\hot_keys_icon_ammoswap.dds`):
**key 2 is Ammo Swap, not an assignable slot.** The INI's second hotkey line is named
`Ammo Swap` too (docs/spec-notes/controls.md).

- **Slots 1 and 3–8 hold one item each**: a weapon, apparel, an aid item, a mine or
  grenade (anything the ITEMS page can equip or use) [documented: the panel's boxes;
  hypothesis: which kinds can be hotkeyed].
- **Slot 2 is Ammo Swap** [documented, above]. On the wheel it is the top slice with the
  game's Ammo Swap icon; the middle lists the carried ammunition of the equipped weapon
  (`mock2.png`). Tap 2 steps to the next ammo, as now (`combat`'s Ammo Swap); releasing on
  slot 2 does the same [design]. Keeping it keeps the original's controls and puts the
  ammo choice the owner's reference has under its title (`< Regular >`) in one place.
- **Assigning** [documented: menus\main\inventory_menu.xml `IM_HotkeyButton`,
  `&-sInventoryHotKey;`, opens `IM_HotKeyWheel`, the panel above over a compass rose
  `interface\hotkeys\hot_keys_rose.dds`]: in the original a "Hotkey" hint on the ITEMS
  page opens the panel, and a key or a box picks the slot. **Built** (2026-10-27, the
  owner: "hold your key and left click your item to assign"): while a hotkey's key is
  held on the ITEMS page the panel shows in the item picture's place (the file's boxes,
  rose and line, moved right to clear the list; `pipboy/hotkey_panel.rs`), and a left
  click on a row (or Enter on the selected one) puts that item in the held slot; the same
  item in another slot moves [hypothesis: the move rule]. Smoke `pipboy-hotkey-assign`.
- **Empty slot**: a tap says nothing happens [design: the HUD message "Hotkey N is empty"
  replaces today's "not available yet"]; on the wheel the slice reads EMPTY.
- **Item gone** (sold, dropped, used up): the slot keeps the item and shows it at half
  strength with count 0; a tap does nothing [hypothesis: the original clears it; design
  until a capture says]. A new copy picked up fills it again.
- **Saved** in NVForever's save (docs/design/save-load.md's "Hotkeys | not built" row):
  the slot's item form and, for a modded or damaged weapon, the stack it came from
  (`Extra`), so the same gun comes back.

## 4. What using a slot does

The same as Enter on that item in the Pip-Boy's ITEMS page (`pipboy/items.rs`) [design]:

| item | tap / release |
|---|---|
| weapon not equipped | equip it (`World::equip_at`) holstered: a weapon in hand is put away, and Ready Item draws the new one [design, the owner 2026-10-31, #1106; until then it was drawn, as RDR2's wheel readies the weapon; hypothesis: the original's hotkeys draw it] |
| the equipped weapon | nothing [design: RDR2 keeps it; no accidental unequip] |
| apparel | put on, or take off if worn (Pip-Boy behaviour) |
| aid item | use one (`aid::use_item`) |
| grenade / mine | equip as the weapon |

Equipping plays the normal equip clips (`fnv_actor::player_clips::equip_clips`), so the
weapon swap is visible, not instant.

## 5. The wheel open

- **Time** (owner): the game clock eases to [`WHEEL_SCALE`] = 0.2 over 0.15 s real time
  and back to 1 over 0.15 s on close, through `script::ClockScale` like VATS mode and the
  death camera [design: slow, not stopped, so the world is visibly still moving as in the
  reference]. Sounds slow with it (`audio_out::slowed_sound`), music and radio do not.
  If VATS mode is on, the wheel does not open (its keys are VATS's).
- **Blur** (owner): the image-space blur pass (`nvforever` `imagespace.rs`) at radius 3,
  the wait menu's (`WAIT_MENU_BLUR`), eased in with the clock; the view also darkens by
  about a third [design]. No new shader: the wheel's blur joins the `max(...)` the wait
  menu, Pip-Boy and death camera already feed.
- **The player** can still walk, but not look: the mouse picks slices while the wheel
  is open [design: RDR2 lets you walk; the mouse is the wheel's].
- **Attack, Block, Activate** are not passed to the game while it is open [design].
- It is not a `PlayState::Menu` (the world must keep running, slowly); a new
  `PlayerControl` flag that `read_controls` and the look code respect [design].

## 6. Picking a slice

- **Mouse** (owner): the mouse's movement since the wheel opened is summed into a
  pointer offset from the centre, clamped to the wheel's radius. Once it is past a dead
  zone of 40 units the slice is chosen by its angle, clockwise from straight up, in 45°
  sectors centred on the slots (slot 2 at 0°, slot 4 at 90°, slot 8 at 270°). Inside the
  dead zone the key's slice stays picked [design: the companion wheel picks by angle the
  same way, `companion_wheel.rs`]. The OS cursor stays hidden; a short line from the hub
  toward the pointer shows the direction (`mock4.png`) [design].
- **Keys** (owner): pressing another hotkey picks its slice and resets the pointer to
  the centre (so the mouse does not undo it) [design].
- **Release**: the wheel acts when the key that **opened** it is let go [design: the
  owner's "letting go of the key"; pressing other keys does not change which key closes
  it]. A click on the left mouse button also acts and closes [design].
- **Cancel**: Esc or the right mouse button closes it doing nothing [design].
- **Controller** (docs/design/controller.md): the D-pad carries hotkeys 1–4
  (`bindings::DPAD_HOTKEYS`); holding a D-pad direction opens the wheel and the right
  stick picks, by the same angles [design]; releasing the D-pad acts.

## 7. Look, from the game's own pictures (no new assets)

Everything on the wheel is either a game picture already in the base archives or drawn as
plain shapes in the HUD colour. Mockups: `mock1.png`, `mock2.png`, `mock4.png`.

| part | source |
|---|---|
| dark disc behind the wheel | `textures\interface\companion_wheel\hud_comp_bkg.dds`, a soft-edged black disc [observed], as the companion wheel draws its matte |
| the eight slices | drawn: annular sectors of 45°, 1.6° gaps, inner radius 178, outer 372 (interface units of the 960-high view) [design]. Unlit: near-black at 60 % with a dim amber outline; lit: amber at 27 % with a full outline and a bright arc just outside the rim (the reference's red arc, in the HUD colour) |
| item pictures | the item's own Pip-Boy picture (its record's `ICON`, `interface\icons\pipboyimages\…`, the Vault Boy art the ITEMS page shows), tinted with the HUD colour, 132 units; dimmed to 60 % when not picked |
| fallback picture | the type icons `interface\icons\typeicons\hot_keys_icon_*.dds` (small guns, melee, stimpak, chems, mine …) when an item has no `ICON` [observed: 17 kinds] |
| Ammo Swap | `interface\icons\typeicons\hot_keys_icon_ammoswap.dds` [documented: hotkeys.xml] |
| hub | drawn circle, radius 168, near-black, amber outline; everything in it stays 12 units inside the outline: each line is measured against the circle where it stands and takes the medium font, then a second line, then is cut short with dots; the kind line goes first when the whole is too tall (owner 2026-10-27: nothing may overlap the slices) |
| text | the game's fonts through `fnv-font` (`glow_monofonto_medium.fnt` for names, font 1 for the title) in the HUD colour (`interface::amber`) |
| figure bars | the Pip-Boy's own cards' figures (DAM or DPS, WG, VAL, CND, and range) as filled bars [design]; values from `pipboy/items.rs` `info_tiles` / `weapon_dps` |
| ammo line | `< .357 Magnum >` and `loaded / carried`, as the HUD's ammo counter |
| counts under slices | ammo carried for guns, count for aid items and throwables |

The game also has `hud_comp_pie.dds` (the companion wheel's ring) and `hud_comp_slice1–8`,
but they are cut for twelve 30° places with gaps at the top and bottom, so they do not fit
eight 45° slices [observed]; the slices are drawn instead. The hotkey panel's own
`hot_keys_box.dds` / `hot_keys_selection.dds` (small square boxes) are kept for the
Pip-Boy assignment panel (§3).

Drawing the slices needs one small 2D material (an annular sector by angle and radius, in
a shader, filled and outlined) or a mesh built once per slice; both are code, not assets.

The HUD (`hud.rs`) is hidden while the wheel is open [design: as the reference].

## 8. Sounds

The Pip-Boy's existing interface sounds (`pipboy/sounds.rs`): a click when the picked
slice changes, the equip sound on release [design; which sound files is a choice for the
owner's ear]. **Built** (#985): a changed pick plays the ITEMS page's row-change click
(`ui_sounds` Inventory / selection moved, `UIPipBoyScroll`). A slot used by a tap or from the wheel plays what the ITEMS page plays for
the same thing (owner 2026-10-27, `ui_sounds`, inventory events): putting on a weapon or
apparel plays the item's own put-on sound, taking apparel off its take-off sound, an aid
item its consume sound (ALCH `ENIT`), an item not carried or refused the refusal sound;
an empty slot or a weapon already in hand none.

## 9. Build order

1. Hotkey slots: storage in the game state, Pip-Boy ITEMS "Hotkey" panel to assign,
   save/load. Tap uses the slot (§3, §4). Unit tests for assign/move/use.
2. The wheel: hold timing, clock and blur, slices, pictures, text (§2, §5, §7).
3. Picking by mouse, keys, controller; release/cancel (§6).
4. Smoke scenario `weapon-wheel`: a test aid that assigns slots, taps 3, holds 1 for
   0.5 s, moves the pointer right and releases; expected log lines for the tap's equip,
   the wheel opening at `ClockScale` 0.2, slot 4 picked and equipped, the clock back at 1.

## 10. Open for the owner

- Slot 2 stays Ammo Swap (§3), as in the original. If all eight should be assignable,
  Ammo Swap needs its own key.
- Hold length 0.25 s, slow-motion 0.2, blur 3: all guesses to tune in play.
- Re-pressing the equipped weapon's slot does nothing; it could holster instead.
