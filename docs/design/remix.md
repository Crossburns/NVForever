# RTX Remix as NVForever's renderer

`nvforever --remix <Remix install>\trex\d3d9.dll` (or `NVF_REMIX`) renders the world
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
- **Each frame.** The player's camera (`PlayerCamera`), every shape the camera sees
  (Bevy's view culling, so an interior hides the exterior as it does in Bevy) as an
  instance, skinned ones posed by their joints in the instance's space; point lights as
  sphere lights, directional lights as distant lights, in the game's light scale
  (Bevy's values over `sky::light_unit`); then Present. Additive shapes (light shafts,
  glows) are not sent.
- **The player.** In first person the third-person body is not sent (Remix's
  player-model masking follows Portal's camera and still showed the head); shapes only
  on the shadow layer go as Remix's third-person player model.

## Files

The runtime works in `Documents\My Games\NVForever\remix`: copies of the install's
`rtx.conf` and `dxvk.conf` (NVForever's own settings appended: no welcome window),
Remix's caches, `logs` and `captures`. The install's replacement assets are read where
they are (`DEFAULT_MODS_DIR`). NVForever writes nothing into the install or the game.

## Not yet

- The HUD and menus: Bevy draws them under the Remix window; `DrawScreenOverlay` is
  wired in the wrapper but not fed.
- Terrain layers (only each chunk's base texture), water, sky meshes, grass and tree
  materials (their own material types), particles.
- Replacement assets keyed to the original's texture hashes (New Vegas RTX's packs).
- The player's shadow in first person.

Test aids: `NVF_REMIX_SHOT=<seconds>:<file.bmp>` saves the Remix window's pixels;
`NVF_REMIX_SUN_SCALE`, `NVF_REMIX_LIGHT_SCALE` scale the lights.
