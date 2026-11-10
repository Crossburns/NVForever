# RTX Remix as NVForever's renderer

`nvforever --remix <Remix install>\trex\d3d9.dll` (or `NVF_REMIX`, or `[Render]
Remix=<that path>` in NVForever.ini) renders the world
with NVIDIA RTX Remix's path tracer. The runtime is loaded into NVForever's own process
and driven through its C API (`remix_c.h`); there is no Direct3D 9, no bridge process and
no game executable. Crate `engine/nvf-remix` holds the API bindings and a safe wrapper;
`engine/nvforever/src/remix.rs` feeds it the scene.

## How the scene reaches Remix

- **Window.** Remix presents into a disabled child window over NVForever's window (one
  presenter per window: Bevy keeps its own swapchain on the parent). Input reaches the
  parent.
- **Meshes and textures.** Copied in the frame Bevy adds them (Bevy keeps only GPU
  copies after that): vertices into Remix's vertex layout with the triangle winding
  reversed (Remix's front faces are Direct3D's), joint weights and indices for skinned
  meshes; images uploaded as Remix textures at once, block formats included (BC1 with
  alpha, BC2 and BC4 through the host-import extension's raw `VkFormat`).
- **Materials.** From the NIF material's base: albedo, normal and emissive textures (by
  their texture hash, `0x%016llX`), colours, roughness, metallic, alpha test or blend.
  A material is made once its textures have arrived (or after about 5 s without).
  Textures repeat (Remix's wrap mode 1; 0 is clamp). Normal maps go with Remix's
  tangent-space encoding: a linear texture named `…_n` in the texture cache (game
  textures have no asset path); LOD terrain's model-space normal maps are left out.
- **Each frame.** The player's camera (`PlayerCamera`), every shape the camera sees
  (Bevy's view culling, so an interior hides the exterior as it does in Bevy) as an
  instance, skinned ones posed by their joints in the instance's space; point lights as
  sphere lights, directional lights as distant lights, in the game's light scale
  (Bevy's values over `sky::light_unit`); then Present. Blended materials go with
  Remix's blend types [documented: the runtime's surface_shared.h, BlendType]: alpha
  blend as alpha, additive (glows, lit windows, fire) as alpha-emissive, multiply as
  multiplicative. Alpha-tested and alpha-blended materials take their alpha state from
  each instance (`useDrawCallAlphaState`, `InstanceInfoBlendEXT`): the texture's alpha
  times the vertex colour's, as Bevy draws them. NIF decals go as static decals; LOD
  terrain (depth bias -50 in Bevy, behind the loaded land) is sunk 200 units instead.
- **Interiors.** Bevy lights a room largely by the cell's ambient term and directional
  light, which do not reach inside closed walls under path tracing; a fill light (a
  sphere of 16 units, 24 above the eye) in the ambient colour stands in
  (`NVF_REMIX_FILL_SCALE`, default 20000) [design: tuned by eye in the Prospector
  Saloon].
- **Changing meshes.** A mesh changed after Remix got it (particles, lip shapes,
  dismembered bodies) is made again under a new generation's hash; about 23 a frame in
  Goodsprings [observed 2026-11-09].
- **Sky and night.** Remix's atmosphere places its sun by its own settings (fixed in
  the install's rtx.conf), not by the lights it is given; NVForever sets
  `rtx.atmosphere.sunElevation` and `sunRotation` from the game's sun
  (`fnv_world::sky::sun_direction`) whenever it moves, with the sky's own day cycle
  off, so the night has a night sky. The first-person arms' shade light is not sent
  (it lights only the arms, at a constant strength).
- **Terrain.** A land quadrant's base mesh and its blended layer meshes (same vertex
  grid, the layer's opacity in the vertex alpha) go as one mesh with Remix Plus's
  terrain-layers material: the base and the top seven layers, weighted per vertex as the
  layers composite over each other, the weights in two RGBA textures over the quadrant.
- **Trees and grass.** Every material type built on Bevy's standard material (leaf,
  bark, billboard, grass) goes through the same path as the NIF material.
- **HUD and menus.** A 2D camera, the default UI camera, draws Bevy's interface into
  an RGBA image with a transparent background; a GPU readback of it each frame goes to
  Remix's screen overlay (`DrawScreenOverlay`), over the traced image; the image follows
  the window's size. Menus with their own cameras draw into images instead (below).
- **Water.** A translucent Remix material (refractive index 1.33) in the water type's
  shallow colour's hue, absorbing over about 3000 units, drawn as animated water, with
  the water's noise texture as its normal map (the waves).
- **Screens.** An image Bevy renders into (the Pip-Boy's screen, a terminal's) is read
  back from the GPU each frame and replaced in place in Remix (the host-import
  extension's REPLACE flag); an unlit material (a screen) is sent as emissive with its
  own picture, a material's UV transform baked into the mesh's texture coordinates.
- **The player.** In first person the third-person body is not sent (Remix's
  player-model masking follows Portal's camera and still showed the head); shapes only
  on the shadow layer go as Remix's third-person player model.

## Files

The runtime works in `Documents\My Games\NVForever\remix`: copies of the install's
`rtx.conf` and `dxvk.conf` (NVForever's own settings appended: no welcome window),
Remix's caches, `logs` and `captures`. The install's replacement assets are read where
they are (`DEFAULT_MODS_DIR`). NVForever writes nothing into the install or the game.

## Parity with New Vegas RTX

New Vegas RTX (the install whose runtime this loads) feeds the game to the same Remix API
through its wrapper and NVSE plugin (MIT-licensed, read for this). NVForever follows it
[documented: the wrapper's comp.cpp, game.cpp, api_material.cpp, api_rtxconf.cpp,
weather.cpp; the plugin's lights.cpp; the runtime's rtx_light_utils.cpp]:

- Settings: the install's rtx.conf, and the player's user.conf (the install's, else
  NVForever.ini `[Render] RemixUserConf`, or NVF_REMIX_USER_CONF); copied when newer.
- rtx.conf texture lists by D3D9 image hash (XXH3-64 of a block-compressed texture's top
  level): instance categories and `ignoreAlphaOnTextures`; glow at
  `rtx.fnvProtocol.glowEmissiveIntensity`.
- Normal maps as a one-layer layered material (RGB tangent space, alpha as gloss).
- Lights: point lights only, each as the D3D9 light the wrapper sends (diffuse colour x
  fade, attenuation (0, 0, a2), range its radius) through the runtime's legacy conversion;
  no directional light (`NVF_REMIX_LIGHT_A2`, default 0.001 [hypothesis: the wrapper's
  note of the placed lights' values]).
- Sky: the atmosphere's sun and moon from the game's clock (moon opposite, the climate's
  phase), the camera as the clouds' anchor, and the weather as `<FormID>, fnv.<class>`
  with the outgoing weather and blend; the install's weather presets copied with their
  class bindings from remix-comp.ini [Weather].

Second pass, 2026-11-09 (remix.rs, read against the wrapper and the runtime again):

- Sun and moon bearing: the sky builds its sun as (cos e sin r, sin e, cos e cos r) in the
  scene's space [documented: the runtime's numos renderer.cpp]; Bevy's +Z is the game's
  south, so r is atan2(x, -y) of the game's sun direction. With the original's atan2(x, y)
  the sun stood mirrored north to south and shadows fell the wrong way [observed against
  the original's capture].
- Scene membership: everything the game shows within 8192 units of the eye is in the
  traced scene, in the camera's view or not (`NVF_REMIX_MEMBER_RADIUS`), built at most
  150,000 vertices a frame [documented: the wrapper's docs/fnv-api-only.md, "scene
  membership, not draws"]. Before, only what Bevy's view culling kept was sent.
- Every instance carries the texture stage state (colour texture x vertex colour, as
  baked lighting on lit shapes) [documented: the wrapper's vertex_color_blend]; the alpha
  blend factors are the colour's, as D3D9's.
- Glow only through a glow map, tinted by the emissive colour held to 1 a channel; a
  shape with an emissive colour and no glow map does not glow [documented: the wrapper's
  scene_statics.cpp].
- Water's medium from the shallow and deep colours [documented: api_water.cpp medium_of;
  WaterMeasurementDistance 256, WaterClarity 0.8], in place of the shallow hue over 3000
  units.
- Land: the vertex colours as the terrain material's tint texture, white vertices,
  roughness 0.85 [documented: api_terrain.cpp; remix-comp.ini TerrainRoughness].

Effects, third pass (2026-11-09):

- Light cards (unlit, additive, External Emittance or under a NiBillboardNode:
  `NifSurface::light_card`) are left out, as the wrapper leaves them out; so are
  refracting shells, which only bend the picture behind them. About 100 shapes a frame in
  Goodsprings.
- Unlit alpha-over cards that are not decals (smoke, dust, steam) carry PARTICLE.
- Additive and multiplied shapes carry their instance state: texture x vertex colour, with
  their own blend factors (SRC_ALPHA/ONE, ONE/ONE for `add_whole`, DST_COLOR/ZERO); a
  blended shape with an alpha cut is tested and blended.
- View-angle falloff (BSShaderNoLighting): each vertex's opacity from |N.V| as
  nif_material.wesl's falloff_opacity, in its alpha (and colour for ONE/ONE), refreshed in
  place when the eye has moved 32 units; about 190 shapes in Goodsprings. The original
  fades them in its vertex shader, which Remix captures on the D3D9 route.

Reference rules and identities, fourth pass (2026-11-10):

- Game fog cards left out: unlit, alpha-over, UV-animated (`NifSurface::uv_animated`, from
  the model's texture-transform controllers), not a particle system [documented: the
  wrapper's plugin fog.cpp]. Light cards only on non-skinned shapes. Remaining unlit
  alpha-over cards carry PARTICLE and IGNORE_OPACITY_MICROMAP (api_unlit.cpp).
- Texture lists resolved over rtx.conf then user.conf: each list its positives minus its
  own negatives; categories and the alpha drop from the final membership.
- `rtx.fnvProtocol.apiGlowTint` on (the emissive colour tints a glow map only with it);
  `rtx.scene.assetKeys` on.
- Addressing per material: the albedo's sampler U and V (texture.rs loads a clamped copy
  for a clamping shape) as Remix wrap modes; land and water repeat.
- Late and changed textures: a material made without a texture is made again when it
  arrives; a changed image replaces its Remix texture in place; only the adapter's own
  shrink event is skipped.
- Terrain layers composited in rank order (the layer material's depth bias).
- Interior fill light off by default (`NVF_REMIX_FILL_SCALE` turns it on).
- Replacement identities: meshes carry AssetKeyEXT with the wrapper's geometry key
  (positions used, indices in stored order, descriptor; scoped by the diffuse when a mod's
  shared_mesh_keys.txt names it); materials carry the albedo's D3D9 hash.
- Remix mods from the game folder's rtx-remix\mods (MO2's merged view: the HND install has
  three, RTX-FNV, RTX-FNV-MAIN, RTXFNV, which the original loads too), opt-in
  (`[Render] RemixMods=1`, NVF_REMIX_MODS=1): with them the card ran out of memory
  [observed 2026-11-10].
- Memory: NVForever with the HND load order reaches about 15 GB of private memory in the
  first 25 s at Goodsprings with or without Remix [observed 2026-11-10, NVF_REMIX_MEMLOG
  and process samples]; with about 12 GB of commit free (other programs 25 GB) it stops
  on a failed allocation.

Memory and frame cost, same pass [observed, Goodsprings, 1280x720, High preset]:

- A world texture's pixels go to Remix and Bevy keeps one 4x4 level of it
  (`NVF_REMIX_KEEP_BEVY_TEXTURES` keeps Bevy's): the driver's memory in use 14.4 GB
  (98 % of the card, the runtime's budget at "high") -> 12.1 GB.
- Textures are uploaded at most 48 MB a frame.
- A changed mesh with the same vertex and index counts is refreshed in place
  (MeshInfoRefreshGeometryEXT); others are made again (still about 14 a frame).
- Still open: the process's private memory stands near 19 GB in Goodsprings and passes
  15 GB within five seconds of the runtime's start (so did the build before this pass).
  With under about 20 GB of commit free on the machine an allocation fails and the game
  stops or hangs [observed three times, 10 GB free]. Where it goes is not measured yet.

Compared at three Goodsprings cameras against the original with New Vegas RTX
(tools/capture/capture_frames.py, 2026-11-09): sky, sun and shadows, night, road and rock
shading match closely. Still different after the second pass (one camera compared
again, gs_town): the water tower's texture is brighter, windows glow a little by day, no
smoke plume, denser grass. Not compared with the original: interiors (the fill light is
NVForever's own stand-in, the wrapper has none), night after the bearing fix, actors,
the Pip-Boy.

## Cost

[observed: 2026-11-09, Goodsprings at 1280x800 window, HND load order, Remix Medium
preset with DLSS-RR] about 26-30 ms a frame with Remix against 16 ms without (where
NVForever is held by the CPU, the GPU 5 ms busy); of that, 3-5 ms building Remix's
scene and under 1 ms in Present, Remix reporting its GPU 60-64 % busy. Bevy's own 3D
pass under the Remix window costs 0-4 ms (0 with the sun's shadows off). The stats line
every 600 frames gives the build and Present times.

[observed: 2026-11-10, same place, 1280x800 window, High preset, DLSS-RR, frame
generation on; docs/handoff/memory-runs.md] 29.1 -> 40.7 traced frames a second (p50
34.1 -> 25.1 ms) with the picture unchanged (mean difference 0.8 of 255, path-tracing
noise):
- Static shapes are retained instances (`rtx.scene.directRetained`), made once and
  only moved, shown or hidden; skinned, changing and view-faded shapes and water still
  go as a DrawInstance a frame (`NVF_REMIX_RETAINED=0`: all of them, as before). The
  runtime's render thread 17.8 -> 6.3 ms a frame, 4360 -> 223 draws.
- The UI overlay's readback buffer is taken and recycled (vendor/README.md, bevy_render).
- Bevy's own main pass under the Remix window is drawn at 64x36 (`NVF_REMIX_BEVY_FULL=1`
  keeps it whole); its visibility, which picks Remix's shapes, is untouched.
- Test aids: `NVF_REMIX_SET=key=value;…` sets runtime options for a run (the profiler:
  `rtx.profile.detailedLogging=True`), `NVF_GPU_REPORT=<s>` logs wgpu's allocations by
  label, `NVF_GPU_MEMLOG=1` every device-memory block, `--remix off` renders without Remix.
- Memory: the process's commit includes its VRAM (Windows backs device memory with
  commit), so Remix's 7.3 GB and Bevy's 3.6 GB count against the machine's commit limit as
  well as the card's.

## Not yet

- NVForever's sky meshes (Remix draws its own sky),
  particle systems as Remix particles (they go as meshes, made again as they change).
- Replacement assets: New Vegas RTX names API meshes for Remix mods by an asset key
  (AssetKeyEXT: XXH3 of the D3D9 buffers' positions and indices, shared_mesh_keys.txt)
  and lights by a key of their reference. NVForever sends neither, so a Remix mod's
  mesh, material or light replacements do not apply. The install's rtx-remix\mods is
  empty today [observed 2026-11-09].
- The wrapper's hair layer and FaceGen skin materials, height maps (displacement),
  particles as its unlit route, the Pip-Boy light's placement, the game-fog and decal
  rules of its NVSE plugin.
- The player's shadow in first person.


Test aids: `NVF_REMIX_SHOT=<seconds>:<file.bmp>` saves the Remix window's pixels;
`NVF_REMIX_TRACE=<text>` logs the shapes whose albedo path, ` bias=<n>`, ` land=<bool>`
or ` faded` holds the text, `NVF_REMIX_TRACE_HIDE` leaves them out and
`NVF_REMIX_HIDE_LAND` leaves the land out;
`NVF_REMIX_SUN_SCALE`, `NVF_REMIX_LIGHT_SCALE` scale the lights.
