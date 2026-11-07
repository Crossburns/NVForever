# FSR 3.1 upscaling

An optional temporal upscaler. The 3D view is drawn at a lower resolution with a sub-pixel
jitter, and AMD FidelityFX Super Resolution 3.1.4 (FSR 3.1) rebuilds it at the window's
resolution from this frame and its history. It is the temporal follow-up that #1639
(FSR 1, spatial) leaves out of scope: it brings the per-object motion vectors that issue
names as missing. The upscaler is the MIT-licensed wgpu-ffx port of the FidelityFX SDK 1.1.4
shaders, as SPIR-V, vendored with its shader sources and compile tool
(engine/vendor/README.md). No AMD DLL is used and there is no frame generation. What FSR
needs and what the engine gives it: [spec notes](../spec-notes/fsr-upscaling.md).

## Using it

**Settings > Graphics > FSR 3.1 upscaling** offers Off (the default), Native AA, Quality,
Balanced, Performance and Ultra Performance, saved by name. `--fsr <mode>` (`off`,
`native-aa`, `quality`, `balanced`, `performance`, `ultra-performance`) sets the starting
choice; a saved setting wins in play, as for every setting. The render size per axis is 1,
1/1.5, 1/1.7, 1/2 or 1/3 of the window's [documented: AMD's FSR 3.1 guide, see the spec
notes].

FSR does its own anti-aliasing, so while it is on, the anti-aliasing row is dimmed, reads
`Off (FSR active)` and takes no edits. The saved choice (MSAA, FXAA, SMAA or TAA) comes back
when FSR is off. The Sharpening switch drives FSR's own RCAS sharpening in place of the
separate pass. The HUD and menus are drawn after FSR, at the window's resolution.

FSR runs on wgpu's Vulkan backend only (SPIR-V passthrough). On another backend, or a GPU
without the formats and limits it needs, the log says `FSR 3.1 unavailable: …` once, the
frame is drawn natively, and the anti-aliasing row stays editable.

## Render contract

`fsr.rs` (`FsrPlugin`). The FSR context is made the first time a view uses it. Each view
with `FsrUpscaling` (the player's camera) has its own history and textures.

- **Size.** Bevy's `MainPassResolutionOverride` draws the main passes into the top-left,
  render-size part of the display-size HDR target, and FSR fills the whole target. Native
  AA draws at 1:1.
- **Jitter and mip bias.** FSR's Halton jitter for the render/output ratio goes through
  Bevy's `TemporalJitter`, negated (Bevy displaces the projection the other way), with a
  texture mip bias of log2(render/output) − 1 [documented: AMD's FSR 3.1 guide].
- **Inputs**, in `Core3dSystems::EarlyPostProcess` before bloom and tonemapping:
  - the HDR colour;
  - the depth prepass, reverse infinite Z, with the first-person squeeze undone
    (`fsr_depth.wgsl`, `FIRST_PERSON_DEPTH_SHARE`);
  - Bevy's motion-vector prepass, scaled by minus the render size. `nif_prepass.wesl` keeps
    the previous transforms and skinning of NIF meshes, and grass and trees also move their
    previous vertex by the previous frame's wind and billboard turn (`grass.wesl`,
    `tree.wesl`);
  - a reactive and transparency/composition mask (below);
  - the frame time.
- **History resets** (`FsrCamera::cut_from`): a quality or size change, a skipped frame, a
  frame over 250 ms, a move of 400 units or more or a turn over 45° in one frame, a
  near-plane change, a magnification (tan(fov/2)) that changes by more than twice either
  way (`LENS_CUT_RATIO`), a new loading screen, and a change of play state. A smooth zoom,
  such as aiming down the sights, keeps the history ("Aim zoom").
- **After FSR**, for the passes that still read the scene on the display-size target:
  - The depth prepass is expanded to display size by nearest texels, with the first-person
    squeeze put back, for SSAO, the light shafts and the image-space blur.
  - FSR writes alpha 1, so each pixel's alpha is copied back from its nearest render texel
    (`fsr_fallback.wgsl` `alpha`). That way the Pip-Boy screen's mark still keeps it out of
    the image-space grade (`imagespace.wesl` `SCREEN_MARK`). [observed 2026-11-06, the
    Pip-Boy open at Goodsprings at noon, clear weather, Native AA, 1280x720] Without the
    copy, the screen was graded with the scene: its mean luma was 0.208, against 0.158
    with FSR off. With the copy it was 0.151.
- **Screen-space lookups in the main pass.**
  - Refraction, the additive shapes' backdrop and water map the render viewport into the
    display-size transmission texture (`transmission_uv`).
  - Water looks its mirror reflection up with the unjittered projection, as the reflection
    camera draws without jitter.
  - Two `bevy_pbr` patches (engine/vendor/README.md) make the transmission pass and the
    clustered point and spot lights follow the override.
- **Fallbacks.** A view FSR can't use (multisampled, not HDR, or offset) logs a warning once
  and is drawn natively. A failed dispatch logs `FSR dispatch failed` and shows a bilinear
  upscale (`fsr_fallback.wgsl`).

## The reactive mask

FSR keeps less history where the reactive mask is high. The port has no `GenerateReactive`
step, so `fsr.rs` makes the mask itself:

- The main texture is copied after the opaque pass (`capture_opaque`).
- `fsr_reactive.wgsl` compares that copy with the finished frame. One R8 texture serves as
  both the reactive and the transparency/composition input.
- So whatever the transparent phase draws over the copy (particles, glass, water, blended
  meshes) reads as reactive.

Two bands of the transparent phase draw surfaces that are solid in effect.
`reference_sky_and_terrain_layers` draws them again over the copy before the mask is made,
with the same pipelines and depth, so they count as opaque:

- **The terrain's texture layers.** The terrain draws each extra layer as a blended copy of
  the quadrant (`terrain.rs`), sorted as one band at the start of the transparent phase
  (`nif_material::sort_first`). [observed 2026-11-05, owner report and FSR input captures
  at Goodsprings, heading 0, noon, Native AA, 1280x720] Left reactive, the mask was:
  - above 0.05 on 53% of the image and above 0.3 on 25%, nearly all of it textured ground;
  - so FSR kept little history there, and its dilation of the mask spread that to the
    twigs, blades and leaf edges standing on the ground, which shimmered with the jitter.

  With the band in the reference:
  - those shares fell to 11% and 3.3%;
  - with a still camera, the mean frame-to-frame change of tonemapped luma fell from
    0.0024 to 0.0007 over the image, and from 0.0099 to 0.0013 over a bush patch;
  - the bush patch now changes about the same at every quality. Before, its change grew
    from 0.0099 at Native AA to 0.0139 at Ultra Performance.
- **The sky's blended parts**: the cloud layers, the night's starfield, the stars and the
  moon (`sky::sorts_with_sky`, the band before the terrain layers). The sky dome itself is
  opaque and in the prepass (depth about 1.8e-5, 350,000 units away), so the sky has motion
  vectors. The parts drawn over it lie far beyond the world, so a turn of the camera moves
  them with the dome. [observed 2026-11-06, a still view at Goodsprings, FSR Quality,
  1280x720]
  - By day, the mask over the sky fell from a mean of 0.214 to 0.
  - The frame-to-frame change along the skyline fell from 0.00052 to 0.00044.
  - Turning at 6° a second, the motion-compensated change along the skyline fell from
    0.00441 to 0.00369, with no smeared clouds or silhouettes in the frames.

  The clouds' own drift is not in the motion vectors [hypothesis: slow enough for FSR's
  history clamping].

Water, refraction and every other blended mesh stay reactive. The copy is display-size to
share the depth attachment; only the render rectangle is read. The log gives each band's
draw count (`FSR reactive reference: …`), which smoke `fsr-foliage` checks.

## Blended cut-outs

A blended mesh is in neither the depth prepass nor the motion prepass, and it draws after the
opaque copy. So FSR sees what is behind it, under a reactive mask. That is right for glass,
but not for shapes whose alpha is a cut-out mask.

[observed 2026-11-05] Goodsprings' corn rows (`scol\scolmaizerow1b.nif`, `2b`,
`maizewitheredgroup2b.nif`) flickered under FSR. Their NiAlphaProperty is 0x12ED with
threshold 0 (blend SRC_ALPHA / INV_SRC_ALPHA, test GREATER 0), so they are drawn blended
(`nif_spawn::alpha_mode`), and their reactive value reached 0.9. The single corn plants cut
the same `maize_d.dds` at 124 and were fine.

Where alpha is 0 or 1, blending and alpha testing give the same pixels.
`nif_spawn::blended_cutout` marks a shape (`NifSurface::blended_cutout`) when all of these
hold:

- it blends SRC_ALPHA / INV_SRC_ALPHA and tests alpha below `BLEND_TEST_CUT` (64);
- nothing else sets its alpha: no vertex-alpha cut, material alpha 1, no alpha animation,
  falloff, light, decal, refraction or hair shading;
- its base texture's alpha is a cut-out mask (`texture::AlphaShares::is_mask`: at least 2%
  clear and at most 13% in between, in the top mip).

While FSR is on, `nif_material::BlendedCutouts` draws those materials alpha-tested at 0.5,
with depth and motion. With FSR off, they are blended as before. Copies that a fade or an
effect drives are not switched, and the shapes cast no shadow either way.

[observed, a one-off survey of the base game's and DLC's meshes] There are 144 such shapes,
in 115 files: the corn, 10 plant models, the tumbleweed, grave flowers, the quarry's
lattices, broken-wall stones, signs and glass shards. Dirty or clear glass stays blended:
`nvposter01.nif` is 40% in between, the glasses 26% and the Robobrain dome 68%. At the corn,
the reactive mean fell from 0.082 to 0.005, and the mean frame-to-frame change from 0.0040
to 0.0008. Smoke `fsr-corn-cutouts` turns FSR on and off there and checks both switches.

## Aim zoom

[observed 2026-11-06, FSR input captures, a 9mm pistol aimed in and out at Goodsprings]
When every change of the field of view counted as a cut, all 18 frames of the aim zoom
(`nvf_play::aim_zoom`, 0.25 s each way) reset the history. Each aim showed the raw jittered
frame for a quarter of a second.

Bevy's motion vectors use the previous frame's unjittered projection, so a zoom reprojects
like any other camera move. Only a jump past twice the magnification (the ratio of
tan(fov/2), `LENS_CUT_RATIO`), such as the console's `fov`, is now a cut. A 20° sight's zoom
keeps the history at 15 frames a second and up (unit test
`an_aim_zoom_keeps_the_history_and_a_lens_jump_resets_it`).

## Ambient occlusion

`ssao.wesl` runs after FSR on the expanded depth, where one render texel covers 1.5 to 3
pixels. It took each normal from the neighbouring pixels, and there it found the same texel
again, with no depth step. So the normal faced the camera, and every surface seen at an
angle occluded itself.

[observed 2026-11-06, AO-on and AO-off screenshots of one still view at Goodsprings, FSR
Performance, 1280x720] This laid a grey veil over the scene: a mean AO factor of 0.958,
against 0.993 without FSR.

The normals now come from the neighbouring texels as drawn, at their centres (`normal_at`).
`fsr.rs` sets `AmbientOcclusion::render_scale` while it upscales. At 1:1 the pass is
unchanged.

## Limits

- **Backend and speed.** Vulkan only, with full-precision shader variants (no FP16). The
  gain depends on how busy the GPU is; a CPU-bound scene gains little.
- **Surfaces whose picture changes by itself** have no mask: animated textures and
  render-to-texture screens (the Pip-Boy's, terminals, a scope's lens) may trail
  [hypothesis: not measured].
- **Jittered depth after FSR.** SSAO, the light shafts and the image-space blur read a depth
  drawn with the frame's jitter, so their edges move by up to half a render texel from frame
  to frame. Computing the occlusion before FSR would fix that, but would change the look:
  it is multiplied in after tonemapping now.
- **Thin and fine detail.** FSR replaces MSAA, so thin alpha-tested strands lose
  multisampled coverage, and FSR resolves their edges over time instead. Fine textures at
  grazing angles shimmer at Performance (minification under the mip bias).
