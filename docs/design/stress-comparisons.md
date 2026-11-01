# Reproducible stress comparisons

[documented] Stage A of [#1277](https://github.com/LootJunkieAzza/NVForever/issues/1277) implements Python comparison and validation
(`tools/stress/stress/comparison.py:202`, `tools/stress/stress/telemetry.py:193`). Scene commands remain stage B work; “add now”
means the agreed stage B scope. [documented] The maintainer's [#1253
comment](https://github.com/LootJunkieAzza/NVForever/issues/1253) lists historical leads concerning towns, REPCONN, memory, combat,
terrain generation and physics catch-up. These are reproduction targets, not confirmed current defects
([#1277](https://github.com/LootJunkieAzza/NVForever/issues/1277)).

## 1. Existing foundation and comparison workflow

[documented] Commit `77a508df` supplies provenance and serial repetitions for `tour`, `battle` and `soak`
(`tools/stress/stress/metadata.py:145`, `tools/stress/stress/cli.py:445`). Each repetition has a new `run-NNN` folder with
`results.json` and `report.md`. The parent comparison index is checkpointed after each child, remains incomplete during execution,
and retains excluded rows (`cli.py:364,387,477`). [documented] Ordinary `run` still accepts one executable and a caller's
`--build-label`; the separate `compare` command adds two-build ordering and frame captures (`tools/stress/stress/cli.py:60,445`).

[hypothesis] Extend `compare` to the stage B `scene` test, retaining its two executable paths, two labels and compatibility with
ordinary `run --exe ... --repetitions ... --build-label ...`. The following PowerShell sequence is the stage B validation interface.
The supervisor supplies already built executables, one settings file and a
repository scene selected from `tools/stress/scenes.toml` (stage B).

```powershell
$baseExe = '<base executable>'
$candidateExe = '<candidate executable>'
$dataPath = '<read-only Data directory>'
$settingsPath = '<comparison NVForever.ini>'
$sceneId = 'goodsprings'
python tools/stress compare scene --exe-base $baseExe --exe-candidate $candidateExe --base-label base --candidate-label candidate --scenario $sceneId --ini $settingsPath --data $dataPath --name scene-abba --repetitions 4 --order ABBA --warmup 1 --settle-seconds 10 --measure-seconds 30 --telemetry-seconds 60
Get-Content target/stress/scene/scene-abba/comparison.md
```

[hypothesis] The scene description records a stable opaque ID, start cell, pose, hour, weather request, expected log checks and any
scheduled aids. Scenes live in the repository's `tools/stress/scenes.toml`, like smoke scenarios; saves and captures remain private.
`compare tour`, `compare battle` and
`compare soak` accept their existing scenario options and an explicit fnvtool path when needed; materialize and hash the same route
or event schedule for both sides. Stage A checks the materialized route and event hashes across launches and rejects a mismatch.

[hypothesis] Repetitions count measured launches per side, with a minimum of three and a default of four. First launch A and B once
each as warm-ups, using the same scenario; retain them with `warmup=true`, but exclude them from verdicts and spread. Then repeat
ABBA blocks. Four repetitions give `A B B A A B B A`; three give `A B B A A B`, explicitly labelled as a partial final block. Prefer
complete blocks and six or more repetitions for a borderline result. Record actual order and times.

[hypothesis] Run one game at a time and hold both existing game locks throughout the sequence, including warm-ups. Do not use
`--no-locks` on a shared machine. Keep builds and other games off the machine, record console or remote-session and display state,
and announce the sequence before launching it. An interrupted plan keeps its completed rows and unattempted slots; never silently
retry only a poor row. An additional balanced block is an explicit new attempt with fresh folders and the earlier rows retained.

[documented] `lib.run` supplies an isolated home, captured output, command record, hang watch and crash collection, plus god mode,
unseen mode, script-menu closing and stress mode (`tools/stress/stress/lib.py:33,276`). Tour adds route stepping and samples; battle
adds placement and samples; soak overrides stress mode and adds script events (`tour.py:349`, `battle.py:143`, `soak.py:148`).
[hypothesis] Reuse those launches and preserve their aids. A stationary scene uses the common aids, explicit start arguments,
`NVF_PLAY_TEST_SAMPLE=1`, and no tour or battle aid. Record every override; its gameplay runtime is settle plus measurement plus
five seconds.

[hypothesis] For a stationary scene, select 30 real seconds after ten seconds of uninterrupted gameplay. After every startup or
loading transition, exclude the first gameplay record and restart the settle timer. Count a duration only when its frame and
preceding frame are in the selected gameplay window. Keep the selection rule and excluded counts in the report; never remove a frame
merely because it is slow. An interrupted or short window is insufficient, rather than a shorter successful measurement.

[observed] In the supervisor's runs on 2026-11-01, the first frame after loading naturally took about 100–180 ms or about 240 ms, in
actor placement (`residents::drive`, `ai::draw_and_place`) or render submission. Including it made a whole-run “1% low” switch
between modes across launches. [hypothesis] Keep these transition costs visible separately; steady-state verdicts use the window
above. Traversal and combat retain separate event-inclusive gameplay results, so streaming or fighter creation is not hidden by
stationary settling. A battle also reports a named post-placement window, if available.

[hypothesis] Extend the existing layout as follows; the index maps folder ordinals to side and purpose.

```text
target/stress/scene/scene-abba/
  comparison.json                 # The versioned plan, rows and aggregates live here.
  comparison.md                   # The readable comparison lives here.
  run-001/                        # This folder retains warm-up A.
  run-002/                        # This folder retains warm-up B.
  run-003/                        # This folder retains measured A1.
    results.json, report.md        # These keep the ordinary per-run results.
    scene.log, scene.cmd.txt, scene.exit.json, scene.memory.csv
    telemetry-validation.json      # This records matching, validation and window selection.
    scene.home/logs/frame-telemetry-<capture_id>.jsonl
```

## 2. Capture discovery, validation and eligibility

[documented] Schema 1 captures begin at process startup, include startup, and request at most 60 real seconds; they cannot restart later
in the same process (`docs/design/frame-telemetry.md:5`, `engine/nvforever/src/frame_telemetry/mod.rs:50`). A long tour or soak
therefore has only an early capture, reported alongside the unchanged coarse metrics. [hypothesis] Fixed scenes should fit about eight seconds of
loading, ten seconds of settling and thirty seconds of measurement. Delayed captures are a follow-up engine issue; #1277 stays Python-only.
[hypothesis] Every launch, including warm-ups, receives `NVF_PLAY_TEST_FRAME_TELEMETRY=60` and
unique `NVF_PLAY_TEST_FRAME_TELEMETRY_RUN_ID` and `NVF_PLAY_TEST_FRAME_TELEMETRY_SCENARIO_ID` tokens of at most 64 ASCII characters,
mapping the launch-specific scenario token to the stable scenario identity in the manifest. Add
`NVF_PLAY_TEST_FRAME_TELEMETRY_GPU=1` only for a separate, equally instrumented diagnostic comparison. Record capture duration
independently of game runtime.

[hypothesis] After process exit, inspect only that launch's fresh home `logs/`. Require exactly one file whose header IDs match the
assigned tokens, whose filename matches `capture_id`, and whose path matches the capture start and terminal summary lines. Preserve
all candidate paths and ambiguity errors. Never pick the newest file or reuse a sibling launch's capture. Store a hash of the
selected file.

[hypothesis] Implement the schema-1 consumer described in `docs/design/frame-telemetry.md:289` with these checks.

| Check | Required validation |
|---|---|
| The envelope is valid. | Require UTF-8 JSONL without a BOM, complete lines of at most 16 KiB, required fields, schema version 1, declared capture limits, bounded strings and integers, finite numbers, and valid null reasons. Ignore unknown fields, but reject unsupported versions or record types. |
| Records are ordered. | Require one header first with seq 0 and one trailer last, strictly increasing seq and nondecreasing collection `t_ns`. Validate marker bounds and phase entries. Allow `main_frame` wrap and diagnostic import times before collection. |
| Counts agree. | Recount written frames by phase, non-null samples and available GPU observations. Require attempted frames to equal written plus dropped frames, phase counts to sum to written frames, and payload seq gaps to match dropped frame and diagnostic attempts. |
| Byte cuts are respected. | For non-null `cut_seq`, validate only payloads through that cut and exclude the trailer's later seq gap. Require the cut to name the last wholly written payload or header 0; later attempts are outside the capture interval. |
| Statistics agree. | Recompute every phase's mean, worst, nearest-rank p50, p95 and p99, slowest-1% mean and strict counts above 33, 50, 100 and 250 ms from integer nanoseconds. Require exact counts and allow at most 0.000001 ms numeric rounding error. Empty populations have null measures. |
| Completion is confirmed. | Require trailer `status=complete`, zero dropped frames, diagnostics and markers, and a matching terminal summary confirming the final result. A `flush unconfirmed` suffix, uncertainty message or absent summary is insufficient, even with a complete trailer. |
| The scenario window exists. | Require the planned window, scene checks and scenario completion evidence. A lossless byte or duration limit may be complete yet end too early for the requested comparison. |

[hypothesis] Retain `missing`, `ambiguous`, `invalid`, `truncated`, `flush_unconfirmed` and `insufficient_window` outcomes with
reasons, paths, raw files and ordinary launch results. Exclude these from distribution verdicts; never substitute zeros or
reconstruct percentiles from sample-window means. Old runs that never requested captures remain readable with coarse metrics only
and explicitly unavailable percentiles. A requested missing capture is an error, distinct from that legacy case.

[documented] Current aggregates exclude failed launches and unknown background CPU, and mark a run busy when other programs exceed
25% of all cores in more than 10% of samples (`lib.py:152`, `cli.py:344`). [hypothesis] Preserve these gates and the
`below_busy_threshold` wording, which does not prove quietness. Keep coarse eligibility and distribution eligibility separate.
Require three eligible runs per side per metric, matching scenario conditions and a finished plan; otherwise the comparison is
inconclusive. Report excluded counts prominently. A crash, hang, failed scene check or release-budget failure also prevents an
overall “no change” result, even when remaining timing rows are usable.

## 3. Provenance and conditions

[hypothesis] Each value carries its source and an explicit unavailable reason. Caller labels are claims, not measured provenance.
Keep requested settings beside effective settings rather than overwriting them.

| Value | Source and limitations |
|---|---|
| Engine identity and features are recorded. | Read `engine_revision` and `build_features` from the telemetry header (`docs/design/frame-telemetry.md:106`). The revision is embedded at build time; engine dirty state is unavailable. Never infer it from runner Git, paths, executable names or build labels. |
| Renderer and presentation are recorded. | Read adapter, backend, driver, driver_info, physical width and height, display_sync, cap_fps, refresh_millihz and window_present_mode from the header (`frame-telemetry.md:108`). An Auto present mode is not the driver's concrete mode, and refresh does not prove cadence. |
| Runner and supplied artifacts are recorded. | Retain runner commit and dirty state, executable and fnvtool SHA-256, requested INI SHA-256, platform and CPU facts (`tools/stress/stress/metadata.py:45,83,140`). Recheck artifact hashes after the sequence and mark changes invalid. |
| Settings beyond presentation are recorded. | Keep the input INI and arguments, and hash initial and final home INIs. Shadow, MSAA and other effective values need explicit engine log evidence or remain unknown; the telemetry header does not export all graphics settings. |
| Scenario identity is recorded. | Hash the scene description, materialized route, event schedule and any supplied save. Record pose, requested hour and weather, aid clock, expected checks, actual duration and selection window. Unknown effective weather or time remains unknown. |
| Load order identity is recorded. | Retain the requested masters list from the runner (`lib.py:42`). When available, record engine-reported resolved order and a private digest of ordered plugin identities and hashes. A requested list alone does not prove effective plugin or asset identity. |
| Machine conditions are recorded. | Record OS, CPU, memory capacity and GPU memory capacity when available, session and display state, power policy and background-CPU samples. Extend the current platform manifest or accept a sourced caller note for facts it cannot collect. |
| Cache conditions are recorded. | Record process freshness, prior warm-up, route lap, home and generated-cache preparation, and separately OS file-cache and driver shader-cache state. A fresh home or process does not establish a globally cold machine. |

[hypothesis] Default to a new process and home for every launch. Warm-ups prime only caches shared across launches; they do not
preserve the prior process's asset caches. “Warm traversal” means a later lap of the same process over the same route, with the
first lap retained as warm-up context. “Cold traversal” means the first visit in a fresh process with explicitly prepared engine
caches; call OS and driver caches unknown unless a repeatable preparation procedure is recorded. Do not purge shared caches
automatically. For a generated-cache experiment, seed identical private cache contents per launch and hash the seed, or use empty
private caches per launch, after verifying where the engine actually stores those caches. Compare cold and warm strata separately;
do not let ABBA order serve as evidence of cache control.

## 4. Statistics and verdicts

[hypothesis] Per launch, report raw gameplay-phase statistics over the complete capture and separately the selected steady-state
window: sample count, measured seconds, mean, p50, p95, p99, worst and all four hitch counts. Nearest-rank quantiles use sorted
durations at `ceil(q*N)`; thresholds use strict `>`. Keep startup, loading, transition frames and placement costs in their own rows,
with no steady-state gate. Capture statistics describe only the bounded capture, never the whole tour or soak.

[documented] Existing `stress frames` statistics omit startup and hidden loading frames, but retain later visible frames; their “1%
low” averages the largest `max(1,floor(N/100))` durations and is not p99 (`engine/nvforever/src/stress_sample.rs:209,251,336`).
Existing comparison summaries use mean, min and max of those per-run metrics (`tools/stress/stress/cli.py:380`). Tour and soak
retain only ten slowest stress hitches in their main lists; their length is not a total hitch count (`tour.py:289`, `cli.py:377`).
[hypothesis] Preserve those legacy fields, populations, budget checks and summaries under their existing names, alongside the new
tables. Never replace their population with the settled telemetry window.

[hypothesis] For each new metric and side, list every eligible per-run value, its median, min, max and spread `max-min`, plus
excluded counts. The headline p99 is the median of per-run p99 values. Stage A does not compute pooled percentiles; optionally
concatenate the selected raw durations from eligible complete captures of the same scenario and population, then recompute a
separately labelled “pooled captured-frame distribution”; publish run and frame counts and acknowledge its frame weighting. Do not
pool different settings, phases, cache conditions or unequal scenario windows into that distribution.

[hypothesis] Let B and C be the medians for base and candidate, and S the larger of the two sides' absolute spreads. A
lower-is-better metric changes only when `abs(C-B) > max(T*abs(B), S, A)`, where T is the fractional threshold and A the absolute
floor below. Equality means no change beyond noise. This includes the base's own spread and guards against increased candidate
variation. Report both spreads, the resulting limit and the signed delta; report a percentage only when B is nonzero. Zero baselines
use the absolute rule without infinity. This is a conservative descriptive rule, not a significance test.

| Metric | Accepted relative threshold T | Accepted absolute floor A |
|---|---:|---:|
| Gameplay mean and p50 use milliseconds. | 3% | 0.05 ms |
| Gameplay p95 and p99 use milliseconds. | 10% | 0.10 ms |
| Process CPU per frame uses milliseconds. | 5% | 0.20 ms |
| An explicitly selected span uses milliseconds per frame. | 10% | 0.02 ms |
| Each hitch threshold uses events per 1,000 eligible frames. | 10% | 1 event per 1,000 frames |

[hypothesis] Publish hitch counts and counts per measured minute beside normalized rates; compare raw counts only with equivalent
exposure. Label each eligible metric `regression`, `improvement` or `no change beyond noise`; use `unavailable` otherwise. Mixed
changes give a mixed overall result, any regression remains prominent, and unavailable required metrics make the overall verdict
inconclusive. Worst frames, memory slopes and legacy metrics remain contextual here rather than acquiring new gates. Store
thresholds in the report so reanalysis cannot silently change the original verdict.

[documented] The profile aid logs process CPU time per frame and the costliest spans; CPU is available without the profile feature,
while Bevy span coverage depends on it (`engine/nvforever/src/profile.rs:320,376`; `docs/design/frame-rate-audit.md:18`).
[hypothesis] Request these with `--aid NVF_PLAY_TEST_PROFILE=5` on both sides. Combine only complete profile windows wholly inside
the selected interval, weighting CPU and each consistently present span by window frame counts. Align log times to the capture
clock and omit ambiguous boundary windows; stage A uses a conservative 0.1-second boundary guard. Missing or top-list-omitted spans are unavailable, not zero. Keep
overlapping spans separate and record coverage. [documented] GPU diagnostics lack source-frame identity and result age
(`docs/design/frame-telemetry.md:48`). [hypothesis] Treat them as asynchronous observations, not per-frame attribution or additive
GPU totals.

[observed] On the supervisor's machine, frame times can remain near the display's 60 Hz even with vsync off. [hypothesis] Report CPU
and relevant spans alongside cadence; verify achieved pacing before calling a run uncapped throughput. Keep throughput, capped play
and vsync play in separate comparisons and keep instrumentation identical within each. Profile and GPU diagnostic runs need their
own overhead context. [documented] Existing release limits are defined in `docs/design/stress-tests.md:311`. [hypothesis] Keep them
unchanged. New timing gates require an agreed hardware tier and frame budget; shared CI validates parsing and functionality rather
than performance on uncontrolled machines.

## 5. Scenario coverage and agreed implementation scope

[hypothesis] The statuses below describe runner reuse and agreed scope, not completed measurements. The five stage B scenes live in
`tools/stress/scenes.toml`, including their stable ID, cell, position, heading, pitch, hour, weather, aids and expected log checks.
Saves and game-derived captures stay private and outside version control.

| Scenario | Existing coverage and evidence | Proposed disposition and remaining requirement |
|---|---|---|
| Stationary REPCONN interior targets render-thread cost. | `cells` tests loading, not a fixed view (`tools/stress/stress/cells.py:173`). | Add now: a `scene` wrapper uses existing cell, pose, heading, pitch and weather arguments (`docs/development.md:86,97`) with the sample and optional profile aids. The supervisor must select and verify the facility-floor pose. |
| Stationary town and settings cases target GPU cost. | `tour --path towns` visits towns, but does not hold views (`tour.py:124,349`). | Add Goodsprings now as the fixed-view control. Document other towns and settings sweeps for a follow-up issue to keep this PR lean. |
| Continuous traversal complements stepped and teleport tours. | `tour` places the player at steps and travels beyond loaded ground (`engine/nvf-play/src/stress.rs:470`); `markers` covers fast travel (`markers.py:118`). | Add now: a short, verified collision-aware segment uses `NVF_PLAY_TEST_MOVE` and `NVF_PLAY_TEST_LOOK`, without the tour aid (`engine/nvf-play/src/test_aid.rs:31,41`). Check sampled progress and reject blockage; a robust long walking route with recovery is for later. |
| Water-facing, away and occluded views isolate reflection work. | Existing start arguments and LOOK control a view, but provide no visibility assertion (`test_aid.rs:31`). | Add water-facing and water-away scenes now, with verified poses and headings. Document occluded water, automatic pass assertions and native GPU attribution for a follow-up issue. |
| Dusk and night expose time-dependent costs. | `tour --hour 19 --laps 2` uses existing options (`tools/stress/stress/cli.py:130,142`). | Reuse now: retain two laps and record actual conditions. Longer night captures need a later delayed-capture facility; an early capture does not establish later-lap performance. A night soak also needs an exposed hour option because it currently starts at noon (`cli.py:321`). |
| Cold and warm traversal distinguish cache effects. | `tour --laps 2` retains samples by lap and waypoint (`tour.py:212,236`). | Reuse now with the cache definitions in §3 and first-versus-later-lap memory context. Later work must control verified cache locations and capture later laps before claiming full cold-versus-warm frame distributions. |
| Standard and 25-a-side combat target combat and the unrepeated slowdown. | `battle --per-side 10` and `--per-side 25` already use the placement aid (`battle.py:60,143`). | Reuse now with repeated ABBA launches. Validate placed counts, keep spawning separate, and show event-inclusive early gameplay plus any comparable post-placement window; do not assume seeded combat or equal survivor counts. |
| A long soak targets slow process and GPU memory growth. | `soak` reuses tour samples, lap checks and events (`soak.py:148`; `tour.py:236,265`). Windows process memory and optional allocator totals exist (`lib.py:95`; `stress_sample.rs:329`). | Reuse the existing coarse memory and leak reports now. Document fitted memory slopes for a follow-up issue; allocator totals are not total device VRAM. |
| Startup and loading need independent phase markers. | Telemetry emits `phase_enter`; `cells` and `markers` parse load durations (`engine/nvforever/src/frame_telemetry/plugin.rs:323`; `cells.py:96`; `markers.py:74`). | Reuse now: report these phases separately and correlate ordinary load logs with stated clock uncertainty. Runner window boundaries are analysis metadata; no existing aid emits arbitrary telemetry scenario or waypoint markers. |
| Terrain generation and physics catch-up need causal evidence. | The export has frame and diagnostic records but no terrain-job or fixed-step counters (`docs/design/frame-telemetry.md:94,140`; `plugin.rs:323`). | Document for later: add bounded terrain-generation begin/end markers and fixed-step count plus elapsed-time observations in a focused engine change. Correlate through capture epoch and frame identity when available; until then, retain profile evidence without attributing a hitch to those subsystems. |

## 6. Synthetic fixtures and supervisor validation

[hypothesis] Extend `tools/stress/tests` with generated, game-data-free fixtures covering these cases.

| Fixture group | Required cases |
|---|---|
| Capture parsing validates the contract. | Include valid files, unsupported schemas, malformed fields, non-finite numbers, partial lines, missing or duplicate headers and trailers, bad IDs, missing and ambiguous matching files, seq gaps, counter mismatches, FrameCount wrap, unavailable diagnostics and forged statistics. |
| Completeness controls inclusion. | Include all drop types, truncated status, settings changes, missing summaries, `flush unconfirmed`, late trailer uncertainty, lossless byte cuts, losses before the cut, and a trailer seq beyond the cut. A complete short capture must still fail window coverage. |
| Statistics preserve populations. | Include known distributions, N=99, 100, 199 and 200, strict threshold equality, first null and later zero durations, loading transitions with a 240 ms reveal frame, settle resets, and a slow gameplay frame that must remain. Distinguish pooled quantiles from medians of run quantiles. |
| Ordering and provenance survive interruption. | Include ABBA for three and four repetitions, discarded warm-ups, stable unique IDs, private homes, existing-name refusal, incomplete checkpoints, changed executable or INI hashes, missing Git and metadata-source conflicts. |
| Verdicts respect noise and missing evidence. | Include threshold and floor equality, base and candidate spread, zero baselines, mixed metrics, fewer than three eligible runs, busy and unknown CPU, crash retention, missing spans, unequal exposures and old-format runs with unavailable percentiles. Preserve existing release-budget cases. |

[hypothesis] The supervisor's small real-run validation uses the #1276 executable and a byte-identical copy at a different path,
matching SHA-256, with the proposed §1 command and one verified stationary Goodsprings scene. Four measured launches per side plus
one discarded warm-up each require ten launches. Use matching settings, no competing work and frames-only telemetry; expect `no
change beyond noise`. Check every capture and summary, source identities, ABBA order, selected windows and exclusions. A noisy or
invalid result is evidence to investigate, not a reason to loosen thresholds until the example passes. This example has not been run
by this documentation task and validates neither the full scenario matrix nor long-session percentiles. Run the synthetic suite with
`python -m unittest discover -s tools/stress/tests`.

## 7. Accepted decisions and remaining validation

[documented] The user accepted four measured launches per side, ABBA, one discarded warm-up per side, ten seconds of settling,
thirty seconds of measurement and the §4 thresholds (task instructions, 2026-11-01). Stage A implements comparison and validation;
stage B adds Goodsprings, REPCONN, water-facing, water-away and one short MOVE/LOOK segment, with no tour aid in that segment.
Other towns, settings cases, occluded water, soak slopes and later-lap or night captures belong in follow-up issues. Terrain markers,
physics counters and delayed capture require separate engine changes. No scope question remains open; stage B still needs verified
scene poses and log checks, and the supervisor still owns all real-run validation.
