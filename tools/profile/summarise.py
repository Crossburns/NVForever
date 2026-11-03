"""Summarise a tracy-tour.ps1 output folder: one block per cell with the frame split
(main app, extract, render thread, GPU), the GPU passes and the heaviest CPU spans.

    python tools/profile/summarise.py F:/nvf-target/tracy-tour [--top 12] > summary.md

Times are milliseconds a frame: a zone's total over the capture divided by the frame count
(the number of `schedule{name=Main}` spans). Spans that run in parallel add up beyond the
frame time; the elapsed spans include waits and the instrumentation itself.
"""
import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path

# Wrappers that only contain other spans; left out of the heaviest-span list.
WRAPPERS = re.compile(r"^(schedule\{|sub app\{|main_render_schedule|camera_schedule\{|RenderGraph$)")


def read(path, tail):
    """Rows as dicts. Zone names hold unquoted commas (generic types), so the last `tail`
    columns are split off the right and the rest is the name."""
    with open(path, encoding="utf-8-sig") as f:
        head = f.readline().rstrip(chr(10)).split(",")
        rows = []
        for line in f:
            parts = line.rstrip(chr(10)).split(",")
            rows.append(dict(zip(head, [",".join(parts[:-tail])] + parts[-tail:])))
        return rows


def cell(stem, folder, top):
    cpu = read(folder / f"{stem}-cpu.csv", 9)
    by = defaultdict(lambda: [0, 0])  # name -> [total_ns, counts]
    for r in cpu:
        by[r["name"]][0] += int(r["total_ns"])
        by[r["name"]][1] += int(r["counts"])
    frames = by["schedule{name=Main}"][1]
    if not frames:
        return f"## {stem}\n\nno frames recorded\n"
    ms = lambda name: by[name][0] / frames / 1e6 if name in by else 0.0
    gpu = defaultdict(int)
    for r in read(folder / f"{stem}-gpu.csv", 3):
        gpu[r["name"]] += int(r["GPU execution time"])
    gpu_ms = {k: v / frames / 1e6 for k, v in gpu.items()}
    out = [f"## {stem}", ""]
    scene = ""
    log = folder / f"{stem}.log"
    if log.exists():
        for line in log.read_text(encoding="utf-8", errors="replace").splitlines():
            if "profile scene:" in line:
                scene = line.split("profile scene:", 1)[1].strip()
    out.append(f"frames {frames}; main {ms('schedule{name=Main}'):.2f} ms "
               f"(Update {ms('schedule{name=Update}'):.2f}, PostUpdate {ms('schedule{name=PostUpdate}'):.2f}); "
               f"extract {ms('schedule{name=ExtractSchedule}'):.2f}; render thread {ms('sub app{name=RenderApp}'):.2f} "
               f"(RenderGraph {ms('RenderGraph'):.2f}); GPU scopes {sum(gpu_ms.values()):.2f}")
    if scene:
        out.append(f"scene: {scene}")
    out += ["", "GPU: " + ", ".join(f"{k} {v:.2f}" for k, v in sorted(gpu_ms.items(), key=lambda kv: -kv[1])[:top] if v >= 0.01)]
    heavy = sorted(((n, t / frames / 1e6) for n, (t, _) in by.items() if not WRAPPERS.match(n)), key=lambda kv: -kv[1])[:top]
    out += ["", "CPU spans (elapsed, may overlap):"] + [f"- {v:.2f} {n[:140]}" for n, v in heavy] + [""]
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("folder", type=Path)
    ap.add_argument("--top", type=int, default=12)
    a = ap.parse_args()
    stems = sorted(p.name[: -len("-cpu.csv")] for p in a.folder.glob("*-cpu.csv"))
    if not stems:
        sys.exit(f"no *-cpu.csv in {a.folder}")
    print("\n".join(cell(s, a.folder, a.top) for s in stems))


if __name__ == "__main__":
    main()
