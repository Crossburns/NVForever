# Spec note: camera field of view (Layer 3)

Tags as in `nif.md`. Measured against the owner's reference frames of the original game
(FNV 1.4.0.525, `tools/capture/README.md` §7); the frames and the per-camera renders are
private and not committed.

## 1. The setting

- The game's field of view is a single number in degrees, 75 by default: `fDefaultFOV=75`
  in the owner's `FalloutPrefs.ini` for every capture [observed], and the script function
  `SetCameraFOV` falls back to 75 when its argument is omitted or 0 [documented: GECK wiki
  via `script-functions-catalogue.md`].
- Which angle the number measures (horizontal or vertical, and at which aspect ratio) is
  not stated in any source the project has. §2 measures it.

## 2. Measurement (2026-10-09) [observed]

Reference frames: the four starter cameras of `tools/capture/frames/cameras.json`
(`goodsprings_noon`, `goodsprings_dusk`, `goodsprings_hill_lod`, `lake_shore_noon`),
captured in first person with the game windowed at 1280 × 720 (16:9), `fDefaultFOV=75`.

Method: `nvforever` renders each camera with the lab scenario's own arguments
(`L3.frame.*`: pose, `--eye-height=120`, hour, weather, `--size=1280x720`, no text
overlay) and only `--fov` changed. `--fov` was then the horizontal angle at the window's
aspect, so each value below is the horizontal angle at 16:9. Score: the lab's image SSIM
(`parity-lab/fnvlab/image.py`: luminance, 192-pixel working width). The remaining
differences after alignment are rendering (sky and clouds, grass, tone), so absolute SSIM
stays low; the peak position is what matters.

Candidate rules for "75":

| Rule | Horizontal at 16:9 | Vertical | noon | dusk | hill_lod | lake |
|---|---|---|---|---|---|---|
| 75 horizontal at the window's aspect (old `nvforever`) | 75.0° | 46.7° | 0.181 | 0.237 | 0.182 | 0.179 |
| 75 horizontal at 16:10, vertical kept | 80.9° | 51.2° | 0.222 | 0.267 | 0.239 | 0.201 |
| **75 horizontal at 4:3, vertical kept ("Hor+")** | **91.31°** | **59.8°** | **0.417** | **0.411** | **0.490** | **0.391** |
| 75 horizontal at 5:4, vertical kept | 95.0° | 63.1° | 0.306 | 0.326 | 0.405 | 0.322 |
| 75 vertical | 107.5° | 75.0° | 0.181 | 0.270 | 0.239 | 0.241 |

Fine sweep (horizontal at 16:9 in 0.5° steps from 89° to 93°, plus 87° and 95°): the best
value is 91.5° for noon (0.420), dusk (0.415) and lake (0.393) and 92.5° for hill_lod
(0.499, a flat top from 91.5° to 93° within 0.003; the view is mostly open road with few
vertical edges). Every camera scores within 0.01 of its best at 91.31°. The score falls
by 0.06–0.11 at 89° and by 0.03–0.07 at 93° (hill_lod's flat top aside), so the
measurement resolves the angle to about ±0.5°.

Visually (50/50 blends of the reference and the 91.31° render): the store signs, power
poles, crates and bushes at Goodsprings, the Prospector Saloon and road at the hill
camera, and the rocks and the tower across the lake overlap; at 75° the render is a 1.33×
zoom of the game's view.

## 3. Rule [hypothesis]

The FOV setting is the horizontal angle of a 4:3 view; at other aspect ratios the vertical
angle is kept and the horizontal angle grows with the width ("Hor+"):

- vertical = 2·atan(tan(setting / 2) · 3/4) (59.84° for 75);
- horizontal at aspect *a* = 2·atan(tan(vertical / 2) · *a*) (75° at 4:3, 85.3° at 16:10,
  91.3° at 16:9).

`nvforever` implements this (`vertical_fov` in `engine/nvforever/src/main.rs`; `--fov`
keeps meaning the game's setting). Bevy's perspective `fov` is the vertical angle, so a
resized window keeps the rule without further work.

Only 16:9 has been measured, so a rule that gives 91.3° at 16:9 by another route (for
example a fixed horizontal angle regardless of aspect) is not ruled out. Test: capture
one camera at 16:10 (e.g. 1280 × 800) or 4:3 (1024 × 768); the rule predicts 85.3° and
75° horizontal. The first-person weapon's separate FOV (`SetCameraFOV`'s second argument)
is not covered: the frames show no weapon.

## 4. Aiming down the sights

- While a gun is aimed down the sights the view zooms in to the weapon's Sight FOV
  (WEAP DNAM `sight_fov`, combat-data.md §1.1), the field of view down the sights; 0
  "essentially disables iron sights" [documented: GECK wiki Weapons]. The zoom is per
  weapon: the Varmint Rifle zooms in further than the Sturdy Caravan Shotgun [observed:
  the owner's playtest, 2026-10-16; reference shots private].
- The Sight FOV is read in the units of the FOV setting (§3: the horizontal angle at 4:3,
  the vertical angle kept) [hypothesis]. Test: a reference shot down a weapon's sights
  at 16:9, matched as in §2, should peak at the horizontal angle §3 gives for its Sight
  FOV.
- The zoom takes 0.25 s each way, eased at both ends [design: not captured]; both points
  of view zoom [hypothesis]. Scoped weapons (Has Scope) use the same angle for now; their
  scope overlay is not built.
- Built in `engine/nvf-play/src/aim_zoom.rs`; smoke scenario `aim-zoom`.
- A conversation narrows the view too, and lays that over this angle without becoming its
  base: docs/design/dialogue-play.md §8.8.

### 4.1 True iron sights

- Every weapon model surveyed has a node `##SightingNode` under its root, behind the gun
  at the height of the sights, turned as the barrel's `ProjectileNode` [observed:
  `Fallout - Meshes.bsa`, model space (forward along +X, up, side)]:

  | model | `##SightingNode` | `ProjectileNode` |
  |---|---|---|
  | varmintrifle.nif | (−12.55, 8.14, 0) | (46.46, 7.00, 0) |
  | 10mmpistol.nif | (−25.76, 7.69, 0.18) | (17.84, 4.05, 0) |
  | huntingshotgun.nif | (−3.83, 7.25, 0.23) | (62.11, 5.90, 0.23) |
  | cowboyrepeater.nif | (−6.09, 7.71, 0) | (53.10, 6.55, 0) |
  | 357revolver.nif | (−25.78, 7.67, 0) | (23.44, 6.51, −0.02) |

  [hypothesis] The node marks where the eye goes down the sights. 18 more guns name it
  `SightingNode` (once `SightingNOde`) without the `##` (nif-node-audit.md row 1). The
  laser rifle, laser pistol and minigun models have no such node; the plasma rifle has
  one at (−13.39, 11.7, 0.17) [observed].
- As the game ships, the aim-down-the-sights clips (`2hraimis.kf`, `1hpaimis.kf`) hold
  the gun centred but low: the rear sight sits about 60–85 px below the middle of a
  720-line frame [observed: the owner's screenshots, Varmint Rifle and Weathered 10mm,
  2026-10-20]. With "true iron sights" the front post is at the middle and the gun is
  higher and nearer the eye; the world is framed the same, so only the weapon moves
  [observed: the same screenshots].
- In first person a gun whose model has no sighting node keeps the hip pose while aimed:
  the original plays no down-the-sights clip for it and only zooms the view [observed:
  the owner's screenshots, 2026-10-24, Laser Rifle at the hip and aimed, same pose,
  zoomed; the owner: "base game has no aim animation it literally just zooms the screen
  in"]. The third-person body still plays its `AimIS` clips [observed: the owner, "third
  person is still animated"]. [hypothesis] The original decides by the node, as true
  iron sights do; checked only on the laser rifle. NVForever does this whatever the
  true-iron-sights switch says (`player_body.rs`, `Rig::sights_clips`).
- NVForever (on by default, settings page, `--no-true-iron-sights`): the arms rig's
  `Bip01 Translate` is turned about the sighting node so the barrel's axis lies along
  the view and moved so the node lands on the camera node, eased in with the zoom
  [design]. Only weapons whose model has the node; the others keep the original aim
  [design: the owner, 2026-10-20, "check its nif for the sight node"]. The crosshair is
  hidden while the sights come up [design: the owner, 2026-10-20]. The switch is on the
  settings page's Gameplay tab.
- Built in `engine/nvforever/src/iron_sights.rs`; smoke scenarios `true-iron-sights`,
  `true-iron-sights-laser`.
