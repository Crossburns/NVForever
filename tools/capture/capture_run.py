#!/usr/bin/env python3
"""Unattended in-game captures: start the original game, let nvf_capture run the jobs,
wait for the results and report.

  capture_run.py --game <FNV folder> --save NAME [options] JOB.steps.json [...]

The plugin (tools/capture/nvf_capture, docs/spec-notes/xnvse-capture.md section 4) runs
jobs by itself when the capture directory holds a fresh `auto.json`: it loads the save
from the main menu with a console line, starts each pending job without waiting for End,
and quits the game with a console line when none is left. This script writes that file,
copies the jobs into `jobs/`, starts `nvse_loader.exe` from the game folder, and watches
`out/` and `nvf_capture.log` until every job has its end line, the game exits or the time
runs out. The game folder is only read (the loader is started there); everything this
script writes is in the capture directory or the paths given with -o / --trace-dir.

The default console lines, `load <save>` and `qqq`, loaded the save and closed the game
on the owner's install [observed 2026-10-08, xnvse-capture.md section 6]; both can be
changed with --menu-console / --quit-console (no plugin rebuild needed). Manual steps
(Wait menu, doors, container menu) still wait for End; jobs that have them are refused
unless --attended is given.

Exit codes: 0 every job ended, 1 bad input or the game could not be started,
2 some job did not end (crash, abort, timeout; see the report).
Standard library only.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import time

CAPTURE_DIR_HERE = os.path.dirname(os.path.abspath(__file__))
if CAPTURE_DIR_HERE not in sys.path:
    sys.path.insert(0, CAPTURE_DIR_HERE)

import capture_steps as cs  # noqa: E402

DEFAULT_CAPTURE_DIR = r"C:\fnv-private\capture"
AUTO_FORMAT = "fnv.capture.auto/1"
REPORT_FORMAT = "fnv.capture.run/1"
GAME_EXE = "FalloutNV.exe"
LOADER_EXE = "nvse_loader.exe"
# Time allowed between starting the game and the plugin renaming auto.json; it is read
# while plugins load, before the main menu.
PLUGIN_READ_SECONDS = 120
# Time the game may stay "Not Responding" (Windows' window status) before the run is
# given up; loading screens can show it briefly.
HUNG_SECONDS = 120
# Time allowed for the quit lines to close the game once every job has ended.
QUIT_SECONDS = 90


class InputError(Exception):
    pass


# --------------------------------------------------------------------------- jobs

def load_job(path):
    with open(path, encoding="utf-8") as f:
        job = json.load(f)
    if job.get("format") != cs.STEPS_FORMAT:
        raise InputError(f"{path}: not a {cs.STEPS_FORMAT} job")
    name = job.get("name") or ""
    if not name or not all(c.isascii() and (c.isalnum() or c in "_.-") for c in name):
        raise InputError(f"{path}: job name {name!r} must be non-empty [A-Za-z0-9_.-]")
    expected = f"{name}.steps.json"
    if os.path.basename(path) != expected:
        raise InputError(f"{path}: the plugin moves jobs by name, so the file must be called {expected}")
    return job


def manual_steps(job):
    return [i for i, s in enumerate(job.get("steps", [])) if "manual" in s]


def pending_jobs(capture_dir):
    jobs = os.path.join(capture_dir, "jobs")
    if not os.path.isdir(jobs):
        return []
    return sorted(f[:-len(".steps.json")] for f in os.listdir(jobs) if f.endswith(".steps.json"))


def plan(paths, capture_dir, attended, include_pending):
    """Validated jobs ({name: (path, job)}) and the names the plugin will run."""
    jobs = {}
    for p in paths:
        job = load_job(p)
        if job["name"] in jobs:
            raise InputError(f"job {job['name']} given twice")
        if not attended and manual_steps(job):
            raise InputError(f"{p}: steps {manual_steps(job)} are manual; run it with --attended "
                             "(someone at the game presses End after each)")
        jobs[job["name"]] = (p, job)
    others = [n for n in pending_jobs(capture_dir) if n not in jobs]
    if others and not include_pending:
        raise InputError(f"other jobs are pending in {os.path.join(capture_dir, 'jobs')}: {others}; "
                         "they would run too (move them away, or pass --include-pending)")
    return jobs, sorted(set(jobs) | set(others))


def auto_file(save, expires, menu_console=None, quit_console=None):
    return {"format": AUTO_FORMAT, "expires": int(expires),
            "menu_console": menu_console if menu_console is not None else [f"load {save}"],
            "quit_console": quit_console if quit_console is not None else ["qqq"]}


# --------------------------------------------------------------------------- results

def output_files(capture_dir, name, since):
    """The job's output files written at or after `since` (Unix seconds), oldest first."""
    out = os.path.join(capture_dir, "out")
    if not os.path.isdir(out):
        return []
    found = []
    prefix = name + "."
    for f in os.listdir(out):
        if f.startswith(prefix) and f.endswith(".jsonl"):
            stamp = f[len(prefix):-len(".jsonl")]
            if stamp.isdigit() and int(stamp) >= since:
                found.append((int(stamp), os.path.join(out, f)))
    return [p for _, p in sorted(found)]


def job_status(path):
    """(status, errors) of one output file: status ended / aborted / running."""
    status, errors = "running", 0
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError:
                continue  # a line being written
            if not isinstance(rec, dict):
                continue  # a cut line that still parses (e.g. a bare string)
            kind = rec.get("kind")
            if kind == "end":
                status = "ended"
            elif kind == "aborted":
                status = "aborted"
            elif kind == "error" or (kind == "result" and "error" in rec):
                errors += 1
    return status, errors


def jobs_state(capture_dir, names, since):
    """{name: {"status", "output", "errors"}}; status missing when no output yet."""
    state = {}
    for n in names:
        files = output_files(capture_dir, n, since)
        if not files:
            state[n] = {"status": "missing", "output": None, "errors": 0}
            continue
        status, errors = job_status(files[-1])
        state[n] = {"status": status, "output": files[-1], "errors": errors}
    return state


def log_since(capture_dir, since):
    """Lines of nvf_capture.log written at or after `since`."""
    try:
        with open(os.path.join(capture_dir, "nvf_capture.log"), encoding="utf-8", errors="replace") as f:
            lines = f.read().splitlines()
    except OSError:
        return []
    keep = []
    for line in lines:
        stamp = line.split(" ", 1)[0]
        if stamp.isdigit() and int(stamp) >= since:
            keep.append(line)
    return keep


# --------------------------------------------------------------------------- the game

class Game:
    """The original game's process (Windows). Replaced by a fake in the tests.

    Only the game this run starts is watched and closed: a copy already running (the
    owner playing) makes the run refuse to start, since the game allows one copy and a
    second exits at once, and closing by name would close the player's game too."""

    def __init__(self, game_dir):
        self.game_dir = game_dir
        self.before = set()

    @staticmethod
    def pids():
        r = subprocess.run(["tasklist", "/FI", f"IMAGENAME eq {GAME_EXE}", "/FO", "CSV", "/NH"],
                           capture_output=True, text=True, check=False)
        out = set()
        for line in r.stdout.splitlines():
            cells = [c.strip('"') for c in line.split('","')]
            if len(cells) > 1 and cells[0].lower() == GAME_EXE.lower() and cells[1].isdigit():
                out.add(int(cells[1]))
        return out

    def check(self):
        """A problem text if the game cannot be started from this folder, else None."""
        if not os.path.isfile(os.path.join(self.game_dir, LOADER_EXE)):
            return f"no {LOADER_EXE} in {self.game_dir} (install xNVSE first)"
        if self.pids():
            return f"{GAME_EXE} is already running (close it first: the game runs one copy at a time)"
        return None

    def ours(self):
        return self.pids() - self.before

    def running(self):
        return bool(self.ours())

    def responding(self):
        """False if Windows reports this run's game window as not responding."""
        for pid in self.ours():
            r = subprocess.run(["tasklist", "/V", "/FI", f"PID eq {pid}", "/FO", "CSV", "/NH"],
                               capture_output=True, text=True, check=False)
            if "not responding" in r.stdout.lower():
                return False
        return True

    def launch(self):
        self.before = self.pids()
        loader = os.path.join(self.game_dir, LOADER_EXE)
        # The loader starts the game and injects xNVSE; it may exit straight away, so
        # the game is watched by its process id, not through this handle.
        subprocess.Popen([loader], cwd=self.game_dir)

    def kill(self):
        for pid in self.ours():
            subprocess.run(["taskkill", "/PID", str(pid), "/F"], capture_output=True, check=False)


def run(jobs, names, capture_dir, game, save, timeout, menu_console=None, quit_console=None,
        kill_on_timeout=False, clock=time.time, sleep=time.sleep, poll=2.0, hung_seconds=HUNG_SECONDS):
    """Start the game for an automatic run and wait. Returns the report (dict)."""
    jobs_dir = os.path.join(capture_dir, "jobs")
    os.makedirs(jobs_dir, exist_ok=True)
    for name, (path, _) in jobs.items():
        dest = os.path.join(jobs_dir, f"{name}.steps.json")
        if os.path.abspath(path) != os.path.abspath(dest):
            shutil.copyfile(path, dest)
    start = int(clock())
    auto_path = os.path.join(capture_dir, "auto.json")
    with open(auto_path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(auto_file(save, start + timeout + PLUGIN_READ_SECONDS, menu_console, quit_console), f, indent=1)
        f.write("\n")
    wait_for_exit = quit_console is None or bool(quit_console)
    report = {"format": REPORT_FORMAT, "save": save, "started": start, "jobs": {}, "problems": []}
    seen_game = False
    all_ended_at = None
    hung_since = None
    outcome = None
    try:
        game.launch()
        while True:
            now = clock()
            alive = game.running()
            seen_game = seen_game or alive
            state = jobs_state(capture_dir, names, start)
            ended = all(s["status"] == "ended" for s in state.values())
            if ended and all_ended_at is None:
                all_ended_at = now
            if not seen_game and now - start > PLUGIN_READ_SECONDS:
                outcome = "the game did not start (is Steam running? is nvse_loader.exe in the game folder?)"
                break
            if seen_game and os.path.exists(auto_path) and now - start > PLUGIN_READ_SECONDS:
                outcome = ("the plugin did not read auto.json: copy the rebuilt nvf_capture.dll "
                           "(plugin version 2 or later) into Data\\NVSE\\Plugins")
                break
            if seen_game and not alive:
                outcome = None if ended else "the game exited before every job ended (crash? see the log)"
                break
            if ended and not wait_for_exit:
                break
            if ended and now - all_ended_at > QUIT_SECONDS:
                outcome = "every job ended but the game is still running (the quit console line did not close it)"
                break
            if alive and not game.responding():
                hung_since = now if hung_since is None else hung_since
                if now - hung_since > hung_seconds:
                    outcome = f"the game stopped responding for {hung_seconds} s (hung; see the log for the last step)"
                    break
            else:
                hung_since = None
            if not ended and now - start > timeout:
                outcome = f"timed out after {timeout} s"
                break
            sleep(poll)
    finally:
        if os.path.exists(auto_path):
            os.remove(auto_path)  # never leave an unattended run armed
    if outcome:
        report["problems"].append(outcome)
    if outcome and outcome.startswith(("timed out", "the game stopped responding")) and kill_on_timeout and game.running():
        game.kill()
        report["problems"].append("the game was closed (--kill-on-timeout)")
    report["jobs"] = jobs_state(capture_dir, names, start)
    report["game_running"] = game.running()
    report["log"] = log_since(capture_dir, start)
    for line in report["log"]:
        if " skipping " in line:  # the plugin could not read a job (e.g. a step kind it predates)
            report["problems"].append("plugin: " + line.split(" ", 1)[1])
    return report


def check_save(save, saves_dir):
    """A problem text if the save is clearly missing, else None."""
    if not saves_dir or not os.path.isdir(saves_dir):
        return None  # unknown location (e.g. a redirected Documents folder): not checked
    if not os.path.isfile(os.path.join(saves_dir, save + ".fos")):
        return f"no save {save}.fos in {saves_dir} (make one in game with the console: save {save})"
    return None


# Files the game itself overwrites during play: a quest script's `Autosave` (VCG01 stage
# 55 does) or a quicksave replace them even when no job saves [observed 2026-10-09:
# `autosave.fos` was rewritten during a capture although bAllowScriptedAutosave was 0,
# docs/spec-notes/layer5-captures.md section 10].
ROLLING_SAVES = ("autosave", "quicksave")


def saves_listing(saves_dir):
    """{file name: [size, mtime_ns]} of the Saves folder, or None when it is unknown.
    Only reads."""
    if not saves_dir or not os.path.isdir(saves_dir):
        return None
    out = {}
    for name in sorted(os.listdir(saves_dir)):
        path = os.path.join(saves_dir, name)
        if os.path.isfile(path):
            st = os.stat(path)
            out[name] = [st.st_size, st.st_mtime_ns]
    return out


def backup_rolling_saves(saves_dir, listing, dest):
    """Copy the game's rolling saves (autosave, quicksave and their .bak/.nvse files) to
    `dest` before a run, so a save the game overwrites can be restored by hand. Reads
    the Saves folder only; never writes there. Returns the copied names."""
    names = [n for n in (listing or {}) if n.lower().startswith(ROLLING_SAVES)]
    if not names:
        return []
    os.makedirs(dest, exist_ok=True)
    for n in names:
        shutil.copy2(os.path.join(saves_dir, n), os.path.join(dest, n))
    return names


def saves_changes(before, after):
    """What changed in the Saves folder between two listings ("added x", "changed x",
    "removed x"), sorted."""
    if before is None or after is None:
        return []
    out = [f"added {n}" for n in after if n not in before]
    out += [f"changed {n}" for n in after if n in before and after[n] != before[n]]
    out += [f"removed {n}" for n in before if n not in after]
    return sorted(out, key=lambda x: x.split(" ", 1)[1])


def trace_goldens(report, trace_dir):
    """fnv.sim.trace/1 goldens for the ended jobs; names of the files written."""
    os.makedirs(trace_dir, exist_ok=True)
    written = []
    for name, s in report["jobs"].items():
        if s["status"] != "ended":
            continue
        with open(s["output"], encoding="utf-8") as f:
            mods, results = cs.read_output(f)
        path = os.path.join(trace_dir, f"{name}.trace.json")
        cs.write(cs.trace_golden(mods, results), path)
        written.append(path)
    return written


def main(argv=None, game=None, clock=time.time, sleep=time.sleep):
    ap = argparse.ArgumentParser(prog="capture_run.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("jobs", nargs="+", metavar="JOB.steps.json")
    ap.add_argument("--game", required=True, help="the game folder (holds nvse_loader.exe); only read")
    ap.add_argument("--save", required=True, help="save to load from the main menu (file name without .fos)")
    ap.add_argument("--capture-dir", default=DEFAULT_CAPTURE_DIR)
    ap.add_argument("--saves-dir", default=os.path.join(os.path.expanduser("~"), "Documents", "My Games", "FalloutNV", "Saves"),
                    help="where to check that the save exists, back up the rolling saves from and watch for changes (only read)")
    ap.add_argument("--timeout", type=int, default=1800, help="seconds for all jobs (default 1800)")
    ap.add_argument("--attended", action="store_true", help="allow jobs with manual steps (End continues them)")
    ap.add_argument("--include-pending", action="store_true", help="also run jobs already pending in jobs/")
    ap.add_argument("--menu-console", action="append", metavar="LINE",
                    help="console line run at the main menu (repeatable; default: load <save>)")
    ap.add_argument("--quit-console", action="append", metavar="LINE",
                    help="console line run when no job is left (repeatable; default: qqq)")
    ap.add_argument("--hung-seconds", type=int, default=HUNG_SECONDS,
                    help=f"seconds the game may stay Not Responding before the run is given up (default {HUNG_SECONDS}; "
                         "a large modded load order can take longer to load)")
    ap.add_argument("--no-quit", action="store_true", help="leave the game running at the end")
    ap.add_argument("--kill-on-timeout", action="store_true", help="close the game if the run times out or the game hangs")
    ap.add_argument("--trace-dir", help="also write <job>.trace.json goldens (fnv.sim.trace/1) for ended jobs here")
    ap.add_argument("-o", "--output", help="report file (default: stdout)")
    args = ap.parse_args(argv)
    game = game or Game(args.game)
    try:
        if args.timeout <= 0:
            raise InputError("--timeout must be positive")
        problem = game.check() or check_save(args.save, args.saves_dir)
        if problem:
            raise InputError(problem)
        jobs, names = plan(args.jobs, args.capture_dir, args.attended, args.include_pending)
        if game.running():
            raise InputError(f"{GAME_EXE} is already running; close it first")
    except (OSError, json.JSONDecodeError, InputError) as e:
        print(f"capture_run: {e}", file=sys.stderr)
        return 1
    quit_console = [] if args.no_quit else args.quit_console
    before = saves_listing(args.saves_dir)
    backup_dir = os.path.join(args.capture_dir, "saves-backup", str(int(clock())))
    try:
        backed_up = backup_rolling_saves(args.saves_dir, before, backup_dir)
    except OSError as e:
        print(f"capture_run: could not back up the rolling saves: {e}", file=sys.stderr)
        return 1
    report = run(jobs, names, args.capture_dir, game, args.save, args.timeout, args.menu_console,
                 quit_console, args.kill_on_timeout, clock=clock, sleep=sleep, hung_seconds=args.hung_seconds)
    changed = saves_changes(before, saves_listing(args.saves_dir))
    if changed:
        report["saves_changed"] = changed
        where = f"; the rolling saves from before the run are in {backup_dir}" if backed_up else ""
        report["problems"].append("the game wrote to the Saves folder (" + ", ".join(changed) + ")" + where
                                  + ". Nothing was restored: restoring is the owner's (the tools never write there).")
    if args.trace_dir:
        try:
            report["goldens"] = trace_goldens(report, args.trace_dir)
        except (OSError, cs.InputError) as e:
            report["problems"].append(f"trace golden: {e}")
    cs.write(report, args.output)
    if changed:
        return 3
    ok = all(s["status"] == "ended" for s in report["jobs"].values())
    return 0 if ok else 2


if __name__ == "__main__":
    sys.exit(main())
