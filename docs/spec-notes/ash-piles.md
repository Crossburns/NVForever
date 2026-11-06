# Ash and goo piles

What an energy weapon's critical kill leaves of a body: the body is turned to ash (lasers,
alien weapons) or green goo (plasma) and a pile lies where it fell, searched as the body
was. Code: `crates/fnv-game/src/ash_piles.rs`, `engine/nvf-play/src/combat.rs`
(`crit_effect`, `apply_hit`), `engine/nvf-play/src/script.rs` (`by_hand`),
`engine/nvf-play/src/activate.rs`. Smoke: `ash-pile-chet`.

## 1. A weapon's critical effect

- A weapon (WEAP CRDT) names an actor effect, the Crit Effect, "applied to the target on a
  critical hit"; with the **On Death** flag "the Critical Effect is only applied when the
  target is killed" [documented: GECK wiki Weapons, owner export `Items.xml`]. Only
  actor effects whose range is Self can be chosen [documented: same page].
- The shipped energy weapons carry On Death critical effects [observed: `fnvtool combat
  weapon`, FalloutNV.esm]: the laser pistol and rifle `LaserDisintegrationFXSpell`
  (000BDA30, "Disintegration"), the plasma pistol and rifle `PlasmaEffect` (00092C48,
  "Gooification"), the flamer `FlamerCritEffect02` (000A73D7). The alien weapons'
  `AlienDisintegrationFXSpell` (0005C6C3) and the Fat Man's `NukeCritEffect` (0005B141)
  are the others of the kind.
- NVForever: after a critical hit's damage (`Game::hit`), the weapon's critical effect is
  applied to the one hit (`Game::apply_crit_effect`, through `apply_magic_item`): an On
  Death effect only when that hit killed, any other on every critical hit of a living
  target. The hit's object effect (hit-effects.md) still goes on before the damage.
- The player takes no critical effect [hypothesis/design: the player's death camera
  (death_cam.rs) has no culled body to show; the original likely disintegrates the player
  too]. The hit aid (`NVF_PLAY_TEST_HIT … :crit`) makes a critical hit.
- Applied after the death, the effects are not dispelled by it; the disintegration
  effects carry "No Death Dispel" anyway [observed: MGEF 000BDA2F, 0004DFDA flags].

## 2. The disintegration scripts

The effects are Script effects; their scripts do the work [observed: SCPT source in
FalloutNV.esm, read with `fnvtool esm decode`]:

- `LaserDisintegrationEffectScript` (000BDA2E) and `AlienDisintegrationEffectScript`
  (0005C6C2): at ScriptEffectStart SetCriticalStage DisintegrateStart, two effect shaders
  (PlayMagicShaderVisuals), a 1.8 s timer; when 0.5 s are left `AttachAshPile`; at 0
  SetCriticalStage DisintegrateEnd and the shaders stop. The laser's skips Mr. House
  (`GetIsID NVCRMrHouse`).
- `PlasmaGooificationEffectScript` (000C7691): GooStart, two goo shaders, 1.4 s, then
  `AttachAshPile 2` and GooEnd at once.
- GooEnd and DisintegrateEnd "take ref out of the water system, cull its 3D, and remove
  its Havok data from the world" [documented: GECK wiki SetCriticalStage]; built before
  this note (the body is no longer drawn, has no hit volume and its ragdoll leaves the
  physics world: script-stubs.md, `Game::culled`). GooStart and DisintegrateStart "change
  constraints in the ragdoll": **not built** (the ragdoll stays as it is for the 1.4–1.8 s).
- The effect shaders play as membranes and particles (effect-shaders.md); their holes
  (the body burning away) are PR #540's, not merged when this was built: the body shows
  the glow and then vanishes under the pile.

## 3. AttachAshPile and the pile

- `ref.AttachAshPile [type]` "attaches an ash pile to the calling reference"; type 1 the
  normal ash pile, 2 the plasma goo pile [documented: GECK wiki AttachAshPile]. The piles
  are the activators `DefaultAshPile1` "Ash Pile" (0000001B, `Effects\AshPile01.NIF`) and
  `DefaultAshPile2` "Goo Pile" (00000022, `Effects\GooPile01.NIF`), both with a looping
  sound 00068ABF [observed: FalloutNV.esm]; "only two available in the game" [documented:
  GECK wiki AttachAshPileEx (JIP)]. A pile knows the actor it came from [documented: GECK
  wiki GetAshPileSource (JIP)].
- NVForever (`Game::attach_ash_pile`): a new reference of the pile (found by Editor ID,
  else by those FormIDs) at the actor's position in its cell, upright with the actor's
  heading [hypothesis: place and angles], drawn as created references are (`dropped.rs`).
  Any type other than 2 gives the ash pile [hypothesis]. The actor and the pile are linked
  both ways in the references' saved state (`RefExtra::ash_pile`, `ash_pile_source`). A
  second call on the same actor makes no second pile [hypothesis].
- The pile is searched: its prompt says Search, and activating it opens the inventory of
  the actor it came from, which keeps everything it carried [hypothesis: the original
  lets the player loot ash piles; whether the items are moved into the pile or the pile
  shows the body's inventory is not documented — the result for the player is the same].
  The rule is the shared default activation's, so a script's `Activate` of a pile does the
  same [design: `Game::default_activation`]. The culled body itself can no longer be
  targeted.
- When the player killed the actor, the statistic Disintegrations counts one
  [hypothesis: by its name; misc-stats.md].
- ResurrectActor on the actor removes its pile [hypothesis].
- Where the body lies: the pile goes to the actor reference's position, which is where it
  stood when it died; the ragdoll's drift in the 0.5–1.4 s before is not followed
  [hypothesis: small].

## 4. Not built

- The ragdoll changes of GooStart and DisintegrateStart.
- The piles' looping sound (SNAM) on created activators.
- The holes of the effect shaders (PR #540).
- Critical effects on the player (§1).
