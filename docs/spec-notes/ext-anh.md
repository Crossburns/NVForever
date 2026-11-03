# The Anh NVSE plugin's functions (2026-10-30, #1015)

Anh's script functions as NVForever builds them: the pure ones in the script VM
(`crates/fnv-script/src/vm/anh.rs`), the ones on the game in `crates/fnv-game/src/
nvse_anh.rs`. Sources: the GECK wiki export [documented: the page named]. Anh is not
installed in the probe game, so nothing here is `[observed]`; what a page leaves open is
marked `[hypothesis]` and is the first thing to check if Anh is ever installed for a probe
run (`tools/capture/jip_probes.py`).

## 1. Arrays, angles and strings

| Function | NVForever |
|---|---|
| Ar_Max, Ar_Min, Ar_RangeMath `array` | the largest, the smallest and their difference of the array's numbers; other elements left out [documented: "*numeric*" values; (6, 2, 10, 7) gives 10, 2, 8]; 0 for no number [hypothesis] |
| DegToRad, RadToDeg `angle` | [documented] |
| GetAngleQuadrant `radians` | 1-4, the angle taken modulo a turn, each quadrant from its lower edge [documented: 0.785 gives 1; hypothesis: the edges] |
| Sv_Capitalize `text` | the first character upper case [documented: "pepeglare snig" gives "P…"] |
| Sv_PadStart, Sv_PadEnd `text length pad` | the pad repeated, cut to fit, before or after the text until it is `length` long; a text already as long, or an empty pad, unchanged [documented] |
| ScancodeToChar `code` | the character the DirectX scan code types on a US keyboard, unshifted; "" for a key that types none [documented: the scan codes; hypothesis: the layout, lower case] |

## 2. Vectors and quaternions (arrays)

Vector3s are three-number arrays and quaternions `[w, x, y, z]` [documented: QNormalizeAlt].
Results are new arrays.

| Function | NVForever |
|---|---|
| V3Mult, V3MultByScalar | element by element; by the number [documented: (1,2,3)·(4,5,6) = (4,10,18); (1,2,3)×2 = (2,4,6)] |
| V3Dotproduct | a number [documented: "the dot product"; the page's return type says array] |
| V3NormalizeAlt, V3CrossproductAlt | unit length; A×B [documented: "the crossproduct of vectorA and vectorB"] |
| V3Lerp `a b t` | a + (b − a)·t [documented: "linearly interpolates between two points", t 0-1; hypothesis: a and b arrays, the page calls them floats] |
| GenerateBezierPoint `p0 p1 p2 t` | the quadratic Bézier point (1−t)²p0 + 2(1−t)t·p1 + t²p2 [documented: "a Bezier curve created by 3 points"] |
| QNormalizeAlt | unit length [documented] |
| QMultQuatQuatAlt `a b` | the Hamilton product A·B [documented: "the product of quatA and quatB"] |
| QMultQuatVector3Alt `q v` | v turned by q (q·v·q⁻¹), a vector3 [hypothesis: the direction and the result's shape; the page calls it a quaternion] |
| QFromAxisAngleAlt `axis angle` | the angle in radians [hypothesis: Anh's angles are radians: GetAngleQuadrant's example, DegToRad beside it] |
| QInterpolateAlt `a b ratio [spherical]` | normalised linear by default, spherical with the fourth argument, the shorter way round [documented: the two methods; hypothesis: the shorter way] |

xNVSE's own forms (QFromAxisAngle …) are different functions with observed conventions
(B·A, the conjugate turn, degrees; `vm/quat.rs`); Anh's are not assumed to share them.

## 3. On the game

| Function | NVForever |
|---|---|
| GetArmorArmorRatingAlt `[armor]` (GetArmorARAlt), SetArmorArmorRatingAlt `dr [armor]` (SetArmorARAlt) | as GetArmorArmorRating and SetArmorArmorRating: the argument, else the calling reference's base [documented] |
| GetQuestCompletedAlt `quest` | 1 when completed and not failed [documented] |
| GetZoneLevel `zone` | the level the zone took when its first cell loaded (encounter-zones.md); 0 before [documented: "the current level"; hypothesis: 0] |
| GetZoneMinLevel, GetZoneMatchLevel, GetZoneOwner `zone` | the ECZN record's minimum level, "Match PC Below Minimum Level" flag and owner [documented] |
| SetZoneMinLevel `zone level`, SetZoneOwner `zone owner` | for the session, read by the getters; a zone already entered keeps its level [documented: the pages' examples; hypothesis: not applied to entering] |

## 4. The last three (2026-11-01)

ArrayToString (the VM), GetCameraFOV and UpdateTubeMesh (`nvse_small.rs`): ext-small.md
§1.
