# Memory and frame-rate runs (High and Dry load order, RTX Remix)

One entry per measured run, newest last. Each says when, which build, the command, and
what was measured, so later sessions compare against numbers rather than memory.
All runs are on the owner's machine: Ryzen 7 7800X3D, 31 GB RAM (45 GB commit limit),
RTX 5070 Ti 16 GB, Windows 10 19045. The game runs inside MO2's virtual file system
(profile NVF-Probe) through `nvf_hnd\mo2run.ps1`; scripts are in `nvf_hnd\run`.

Tools:
- `memwatch.ps1`: private bytes, working set, system commit free, nvidia-smi VRAM (whole
  GPU, desktop about 1.0 GB idle), GPU shared usage, every 4 s.
- `vmwalk.ps1`: VirtualQueryEx walk of committed memory by type, allocation size and
  protection (0x4 read/write, 0x404 read/write + write-combined).
- `NVF_GPU_MEMLOG=1` with `RUST_LOG=gpu_allocator=info`: every wgpu device-memory block
  made or freed, with the live total per Vulkan memory type (vendored gpu-allocator).
- `NVF_REMIX_MEMLOG=1`: NVForever's Remix queue and mesh totals; Remix's own
  `[AccelMem]`/`[VramBudget]` lines are in `Documents\My Games\NVForever\remix\logs`.
- `NVF_REMIX=off` (or `--remix off`): render without Remix even when NVForever.ini's
  `[Render] Remix=` names a runtime. Before this switch (2026-11-10) every run with that
  INI line loaded Remix, including runs labelled "no Remix" in older notes.

Scene unless said otherwise: `--play --cell Goodsprings --hour 14 --size 1280x800`,
plugins `@C:\HND\profiles\NVF-Probe\plugins.txt`, data `C:\HND\stock game\Data`.

## 2026-11-10 08:04-08:14, build bd687e55c + uncommitted memlog (Remix loaded)

`noremix.cmd` (despite its name Remix loaded, see above), sampled at 75 s.
- private 18.5-19.1 GB, working set 7.8 GB, VRAM 12.7 GB, GPU shared 1.48 GB.
- vmwalk: private committed 18.9 GB; 337 write-combined blocks of 16-64 MB = 10.3 GB;
  766 read/write blocks of 1-16 MB = 4.8 GB; largest read/write blocks 850 and 366 MB.
- Remix `[VramBudget]` frame 1500: demand 10.8 GB of 14.8 GB budget; DXVK allocated
  7.3 GB, of which material textures 4.2 GB, render targets 1.0 GB, accel 0.4 GB,
  scene geometry 0.4 GB.
- Remix `[Session]` at 60 s: 1302 frames, 21.6 fps, frame ms avg 46.1, worst 5143.9,
  4354 instances.

## 2026-11-10 08:20-08:24, same build, `gpumem.cmd` with NVF_REMIX=off

- memwatch at 63-78 s: private 9.74 GB, working set 3.93 GB, commit free 18.3 GB,
  VRAM 7.7 GB (whole GPU), GPU shared 0.15 GB.
- gpu-allocator live: about 6.2 GB at steady state (device-local type 1 about 5.5 GB,
  host-visible types 3/4 about 0.3-1.0 GB), matching the VRAM reading; peak 7.0 GB.
- vmwalk at 55 s: private committed 9.4 GB; 177 write-combined blocks of 16-64 MB =
  5.5 GB (not gpu-allocator blocks, which are 128/256 MB or dedicated); 259 read/write
  blocks of 1-16 MB = 2.6 GB; 7 write-combined 128 MB blocks = 0.8 GB (gpu-allocator's
  host-visible blocks).

What owns the 5.5 GB of 32 MB write-combined blocks: answered at 09:41 below (the
commit Windows charges for the game's VRAM).

## 2026-11-10 09:41, same build, finding: write-combined commit is VRAM's backing

`gpumem.cmd` (NVF_REMIX=off, NVF_GPU_MEMLOG=1) with vmwalk at 75 s.
- vmwalk: write-combined private commit 0.03 + 5.91 + 0.48 = 6.42 GB (6574 MiB).
- gpu-allocator live at the same moment: t1 6198 + t3 64 + t4 256 = 6518 MiB.
- [observed] The two agree within 1%: Windows charges the process commit for the game's
  own device memory (its VRAM allocations), in about 32 MB pieces. It is not a leak; VRAM
  NVForever (and the Remix runtime, in the same process) holds counts twice against the
  machine's commit limit, once as VRAM and once as commit.
- A kernel VirtualAlloc trace (xperf, stack walk) saw no large VirtualAlloc for these
  blocks, consistent with the driver mapping them.

## 2026-11-10 09:45, same build, Bevy's GPU memory under Remix

`gpumem_remix.cmd` (Remix on, NVF_GPU_MEMLOG=1), memwatch to 93 s.
- private 18.5 GB, working set 7.8 GB, VRAM 12.6 GB, GPU shared 1.48 GB.
- gpu-allocator (Bevy/wgpu) live about 3.6 GB (device-local 2.9, host-visible 0.8).
- Remix `[Session]` 60 s: 21.9 fps, 4340 instances.
- `NVF_GPU_REPORT=60` (11:0x, later build): wgpu holds 2180 MiB in 3855 allocations inside
  3446 MiB of blocks: mesh slabs 1155 + 196 MiB (vertex, index), render targets about
  600 MiB (main textures, depth, prepass, transmission, shadow maps), dds textures 121 MiB
  (the 4x4 levels Remix leaves Bevy, interface and unshrunk), readback buffers 47 MiB.

## 2026-11-10 09:50-11:07, frame-rate work (Remix on, Goodsprings 14:00, 1280x800 window, Remix output 2560x1440 DLSS-RR, frame generation on)

Profiles: Superluminal attach (`run\slattach.ps1`) on the release build (PDB resolves
NVForever; the installed Remix d3d9.dll of 2026-11-03 has no matching PDB) and Remix's own
`rtx.profile.detailedLogging=True` through the new `NVF_REMIX_SET`. Each line: the third
30 s `[Perf]` window (steady state).

| run | change | fps (p50 ms) | GPU busy, ms/frame | Remix render thread ms/frame | API draws/frame |
|---|---|---|---|---|---|
| ab_ret0 | before (DrawInstance each shape) | 29.1 (34.1) | 67 %, 23.3 | 17.8 | 4360 |
| ab_ret1 | retained instances (rtx.scene.directRetained) | 31.0 (31.8) | 62 %, 20.1 | 6.9 | 223 |
| ab_ret2 | + foldhash maps, ref walk grouping, overlay buffer reuse | 33.6 (29.9) | 73 %, 22.0 | 7.3 | 223 |
| ab_ret3 | + no statement clone per UDF/lambda call, overlay readback taken not copied, nif_mirror per-frame cache | 38.2 (25.8) | 90 %, 23.7 | 6.1 | 223 |
| ab_ret4 | + Bevy's hidden main pass drawn at 64x36 under Remix | 40.7 (25.1) | 88 %, 21.8 | 6.3 | 223 |

Picture: `NVF_REMIX_SHOT=60` of each run against ab_ret0, mean absolute difference 0.67
(ab_ret1) and 0.79 (ab_ret4) of 255, pixels more than 48 apart 0.06 %; luminance 82.3 in
both (the path tracer's noise, the frame counter, one moving object). Scene sizes equal
(4326/4335/4434 instances in the three windows of each run).

Superluminal, main thread, before: `remix::frame` 5.2 s of 22 s (draw_instance_skinned
1.2 s with two memsets in the runtime per call, the UI overlay memcpy 0.7 s, NtFreeVirtualMemory
of per-frame buffers 1.1 s); workers: `run_scripts` 3.6 s (statement clones 0.8 s, ref
walks 0.5 s), `sync_readbacks` 2.3-2.5 s (the 14.7 MB UI overlay readback copied and freed
every frame), nif_mirror 1.3 s. Memory is unchanged by these (18.7 GB private at 107 s).

Not a clean measure: `--no-sun-shadows` changes what goes to Remix (3947 instances: the
shadow-only shapes are chosen by layer), so it is not a way to time Bevy's shadows.

## 2026-11-10 11:2x, mesh shells under Remix (run ab_ret5)

Under Remix a mesh that only the render world holds (usage RENDER_WORLD, not skinned) keeps
its bounds (`final_aabb`) and loses its vertices once NVForever's Remix adapter has its
copy, so Bevy never uploads it (`NVF_REMIX_KEEP_BEVY_MESHES=1` keeps them).
- private 18.7 -> 16.9 GB, VRAM (whole card) 12.77 -> 11.02 GB, commit free 8.8 -> 10.6 GB.
- wgpu: 2180 -> 841 MiB allocated, 3446 -> 1408 MiB in blocks (14 -> 8).
- 41.1 fps (p50 23.9 ms); Remix GPU 21.8 -> 17.5 ms a frame (busy 72 %): the GPU no longer
  draws Bevy's copy of the world.
- Same scene (4435 instances, 4451 visible shapes); screenshot mean difference 0.82/255
  against ab_ret0, the remaining spots a bighorner that walked and the sky.

## 2026-11-10 11:4x-12:0x, packed CPU vertices and shared texture contents (runs ab_ret6, ab_ret8)

- umdh (gflags +ust, `umdh -p`, resolved with the NVForever PDB) of a run at 16.3 GB:
  4.40 GB of heap with stacks; 1.32 GB the Remix adapter's CPU mesh copies (64-byte
  vertices, 28 of them padding), 1.62 GB decoded DDS data. [observed] +ust slowed the
  game to a crawl, so the snapshot caught the texture upload queue undrained: not the
  steady state. A census of the main world (NVF_GPU_REPORT) found Bevy's images holding
  no pixel data (world textures at their 4x4 level).
- vmwalk at 95 s (ab_ret6): private 16.34 GB = 10.02 GB write-combined (VRAM backing:
  Remix about 8.6, Bevy 1.4) + 6.08 GB read/write.
- CPU mesh copies keep a 36-byte vertex (widened when sent): private 16.89 -> 16.39 GB.
- Textures of the same contents (format, size, levels, pixels) share one Remix texture,
  each keeping its own addressing and D3D9 hash: 109 textures, 360 MB not uploaded;
  Remix material textures 4217 -> 3927 MiB. ab_ret8: private 16.06 GB, VRAM 10.66 GB,
  commit free 11.5 GB; 40.6 fps (p50 24.2 ms), GPU 17.5 ms; screenshot mean difference
  0.77/255, pixels more than 48 apart 0.018 %.
- A census bug of this session's own (counting vertices of extracted meshes) panicked at
  70 s in ab_ret6/ab_ret7; fixed before ab_ret8.

Totals since the morning (Remix on, same place): private 18.7 -> 16.1 GB, VRAM (card)
12.77 -> 10.66 GB, frames 29.1 -> 40.6 a second (p50 34.1 -> 24.2 ms).

## 2026-11-10 12:0x-14:00, screens read back only while drawn; Pip-Boy raise (runs ab_ret9-14, pipt*)

- Superluminal (nvf_remix_fix3): the screens' readback (`watch_live_textures`: the Pip-Boy's
  and terminals' render targets, 1280x960) was copied, page-faulted and freed every frame,
  2.3 ms a frame, with no screen showing. Now the readback's buffer is taken (no copy) and
  a screen is read back only while a camera drawing into it is on; Remix keeps its last
  picture otherwise. ab_ret9: 40.6 -> 50.6 fps (p50 24.2 -> 19.4 ms).
- Pip-Boy check (`NVF_PLAY_TEST_PIPBOY=45:open,52:page=items`, `NVF_REMIX_SHOT`): with the
  mesh shells (commit 6ae343113) the first raise waited its full 3.9 s cap, because
  pipboy_arm waits for Bevy to have drawn the rig and the rig's meshes are emptied for Bevy.
  [observed] A shot at 60 s of Remix's clock fell inside that wait and looked like a missing
  Pip-Boy; a shot at 90 s (pipt3) showed it. Fixed at the gate: under Remix (`RemixDraws`)
  the raise does not wait for Bevy's draws of the rig; first raise held 199 ms (pipt2),
  device and ITEMS screen shown 4 s after opening.
- Tried and dropped (logged for whoever comes next): emptying meshes only after Remix drew
  their shape (rigs left whole) keeps Bevy's upload, and freeing a mesh inside Bevy's slabs
  frees no slab: private 17.9 GB. Holding new meshes in the main world until a shape showed
  whose they were: 15.9 GB but a held rig never reached Bevy. A GPU buffer per mesh
  (large_threshold 1): 10 fps (9410 buffers) and no memory gain.
- ab_ret14 (this commit): 50.7 fps (p50 19.4 ms, GPU 85 %, 16.9 ms), private 16.10 GB,
  VRAM 10.72 GB, commit free 11.1 GB; wgpu 828 MiB in 1408 MiB of blocks; picture mean
  difference 0.86/255 against ab_ret0.

Totals since the morning (Remix on, Goodsprings 14:00): 29.1 -> 50.7 traced frames a second
(p50 34.1 -> 19.4 ms), private memory 18.7 -> 16.1 GB, VRAM 12.77 -> 10.72 GB.

## 2026-11-10 14:2x-14:50, the travel leak (runs tele0, tele1, ab_ret15)

Eight `player.setpos` jumps across WastelandNV, 20 s apart
(`NVF_PLAY_TEST_CONSOLE=50:player.setpos x -40000;70:…`), memwatch every 5 s.
- tele0, before: nothing NVForever gave Remix was ever destroyed. Private 14.5 -> 22.7 GB,
  VRAM 9.0 -> 14.4 GB, commit free 12.9 -> 5.3 GB; Remix meshes 1588 -> 5276, textures
  1664 -> 4523, materials 1347 -> 3026, still climbing at the end. This is the growth that
  ends in failed allocations in long sessions.
- Fix: a mesh, image or NIF material the game lets go (AssetEvent Removed/Unused) leaves
  Remix too (retained instances, then meshes, then materials, then textures; a texture
  other images of the same contents share stays until the last of them goes).
- tele1, after: Remix meshes peak 2334 and come back to 2018, textures 2360 at the end;
  up to 921 meshes, 598 materials and 1434 textures freed in one 600-frame window.
  Private ends at 18.7 GB and falling (peak 20.1), VRAM 12.1 GB. What stays is the
  engine's own cache keep times (nvf_assets release.rs), not Remix.
- ab_ret15 (Goodsprings, no travel): 50.7 fps, private 15.98 GB, VRAM 10.61 GB, picture
  mean difference 0.85/255.

## 2026-11-10 15:00, upload census and commit pressure (tele2)

- Census (`NVF_REMIX_MEMLOG=1`, Goodsprings): textures uploaded to Remix by folder:
  landscape 1302 MB/319, architecture 594/248, clutter 490/201, bale 372/124, unnamed
  (generated LOD and land weights) 255/326, sneeze 253/80, dungeons 182/44,
  spiffyskytrooper 165/13. No interface textures among them: what Remix holds is the world
  it draws, so its 3.9 GB of material textures is the picture, not waste.
- The caches' memory budget (memory_budget.rs) also presses on the machine's commit:
  over 85 % of the commit limit in use (on this machine under 6.8 GB free), as allocations
  fail when commit runs out whatever RAM is free. tele2: pressed at about 180 s (commit
  6785 of 46515 MB free, GPU 85.7 %); peak 20.3 GB, end 18.4 GB, as tele1 here, where the
  GPU signal pressed as well; on a machine with less commit to spare it presses sooner.

## 2026-11-10 15:1x, no multisampling on Bevy's hidden pass (ab_ret16)

Under Remix (`RemixDraws`) graphics.rs gives the main camera no MSAA (the setting is kept):
wgpu 1408 -> 1152 MiB in blocks (multisampled colour and depth gone), private 15.87 GB.
[observed] Both ab_ret16 runs had the view moved (third person, player walking; then a
shifted first-person view): input reached the game window during them, so their frame
rates (43.6-44.4) and picture differences are not comparable; to be measured again with
the machine idle.
