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
