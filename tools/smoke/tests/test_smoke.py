"""Self-tests for the smoke runner: parsing, selection, launch merging, expectation
matching and locks. Sample logs only, no game data.

Run:  python -m unittest discover -s tools/smoke/tests     (from the repo root)
"""
import io
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
from contextlib import redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))

from smoke import choose, cli  # noqa: E402
from smoke.expect import Expect, ScenarioError, check_forbid, check_one, check_order  # noqa: E402
from smoke.plan import merge, plan, shift  # noqa: E402
from smoke.runner import FROM_SCRIPTS, Locks, Queue, aid_free_environ, game_env, judge  # noqa: E402
from smoke.scenarios import Timed, load, load_text  # noqa: E402

LOG = [
    "2026-10-11T10:00:00.000000Z  INFO nvforever: world index built in 1.20s",
    '2026-10-11T10:00:03.000000Z  INFO nvf_play::corner_messages: corner message: "A" for 2 s, icon x.dds (0 waiting)',
    '2026-10-11T10:00:05.250000Z  INFO nvf_play::corner_messages: corner message: "B" for 10 s, icon default (0 waiting)',
    "2026-10-11T10:00:06.000000Z  INFO nvforever::alert: alert: 1EFiendViolet 000F570B Alert → Combat: detects the player, 2,651 units away",
    "2026-10-11T10:00:07.000000Z  INFO nvforever::speech_look: speech look: line 0010A1E6 over: SunnyRef has the player +2° off its front",
    "2026-10-11T10:00:08.000000Z  INFO nvforever::speech_look: speech look: line 0010A1E7 over: SunnyRef has CheyenneREF -3° off its front",
    "2026-10-11T10:00:09.000000Z  INFO nvforever::music: music summary: 1 started, 0 changes of layers, 0 files not played",
    "2026-10-11T10:00:10.000000Z  INFO nvforever: run finished after 40.0s: 9 cells loaded",
]

GAME = """
[[scenario]]
name = "{name}"
paths = ["engine/"]
args = {args}
run_seconds = {run}
env = {env}
{extra}
"""


def game(name, args='["--cell", "X"]', run=20, env="{}", extra=""):
    return GAME.format(name=name, args=args, run=run, env=env, extra=extra)


TIMED = """
untimed = ["NVF_PLAY_TEST_SCRIPT_MENUS"]
[timed.NVF_PLAY_TEST_SCRIPT]
sep = ","
[timed.NVF_PLAY_TEST_TALK]
sep = ";"
[timed.NVF_PLAY_TEST_PAUSE]
sep = ","
shift = "all"
"""


class ExpectTests(unittest.TestCase):
    def test_text_with_ellipsis(self):
        e = Expect.parse("music summary: 1 started, … 0 files not played")
        self.assertTrue(check_one(e, LOG).ok)
        self.assertFalse(check_one(Expect.parse("music summary: 2 started"), LOG).ok)

    def test_text_is_literal(self):
        self.assertFalse(check_one(Expect.parse("index built in 1x20s"), LOG).ok)
        self.assertTrue(check_one(Expect.parse("index built in 1.20s"), LOG).ok)

    def test_regex_and_counts(self):
        self.assertTrue(check_one(Expect.parse({"re": "corner message:", "count": 2}), LOG).ok)
        o = check_one(Expect.parse({"re": "corner message:", "count": 3}), LOG)
        self.assertFalse(o.ok)
        self.assertIn("2 lines", o.detail)
        self.assertTrue(check_one(Expect.parse({"re": "speech look:", "min": 1, "max": 2}), LOG).ok)
        self.assertFalse(check_one(Expect.parse({"re": "speech look:", "max": 1}), LOG).ok)
        self.assertTrue(check_one(Expect.parse({"re": "no such", "count": 0}), LOG).ok)

    def test_range_any_and_all(self):
        e = Expect.parse({"re": r"Combat: detects the player.*?(?P<n>[\d,]+) units", "range": [1987, 3313]})
        self.assertTrue(check_one(e, LOG).ok)
        e = Expect.parse({"re": r"Combat: detects the player.*?(?P<n>[\d,]+) units", "range": [1, 100]})
        o = check_one(e, LOG)
        self.assertFalse(o.ok)
        self.assertIn("2651", o.detail)
        each = {"re": r"over: SunnyRef has .* (?P<n>[+-]\d+)° off its front", "range": [-2, 2]}
        self.assertTrue(check_one(Expect.parse(each), LOG).ok)  # one line is in range
        self.assertFalse(check_one(Expect.parse(dict(each, all=True)), LOG).ok)  # not every line
        self.assertTrue(check_one(Expect.parse(dict(each, range=[-5, 5], all=True)), LOG).ok)

    def test_gap(self):
        e = Expect.parse({"gap": ['corner message: "A"', 'corner message: "B"'], "range": [1.7, 2.8]})
        o = check_one(e, LOG)
        self.assertTrue(o.ok, o.detail)
        e = Expect.parse({"gap": ['corner message: "A"', 'corner message: "B"'], "range": [3, 4]})
        self.assertFalse(check_one(e, LOG).ok)

    def test_order(self):
        seq = [Expect.parse('corner message: "A"'), Expect.parse('corner message: "B"'), Expect.parse("run finished")]
        self.assertTrue(all(o.ok for o in check_order(seq, LOG)))
        bad = [Expect.parse('corner message: "B"'), Expect.parse('corner message: "A"')]
        out = check_order(bad, LOG)
        self.assertFalse(out[-1].ok)
        self.assertEqual(out[-1].what, "order")

    def test_forbid(self):
        self.assertTrue(check_forbid(Expect.parse("Lost → Combat"), LOG).ok)
        o = check_forbid(Expect.parse("Alert → Combat"), LOG)
        self.assertFalse(o.ok)
        self.assertIn("Alert → Combat", o.context[0])

    def test_missing_shows_similar_lines(self):
        o = check_one(Expect.parse("music summary: 5 started"), LOG)
        self.assertEqual(o.what, "missing")
        self.assertTrue(o.context and "music summary" in o.context[0])
        self.assertNotIn("2026-10-11T", o.context[0])  # timestamps cut

    def test_bad_entries(self):
        with self.assertRaises(ScenarioError):
            Expect.parse({"re": "x", "range": [1, 2]})  # no group
        with self.assertRaises(ScenarioError):
            Expect.parse({"re": "("})
        with self.assertRaises(ScenarioError):
            Expect.parse({"txt": "x"})
        with self.assertRaises(ScenarioError):
            Expect.parse(3)


class JsonExpectTests(unittest.TestCase):
    OUT = [
        'warning: something on stderr {not json',
        '{"schema":"fnv.nav.walk/1","summary":{"outcome":"arrived","seconds":11.7}}',
        '{"schema":"fnv.nav.walk/1","summary":{"outcome":"stuck","seconds":3.0}}',
        '{"schema":"fnv.actor.detect/1","situations":[{"situation":"sneaking, lit","detected_out_to":1840.0},'
        '{"situation":"sneaking, dark","detected_out_to":null}],"actors":[{"actor":"c","factions":[1,2,3,4]}]}',
        '{"format":"fnv.script.trace/1","observations":[{"label":"final: Q 80","stage":80,"done.80":true,"var.a.b":2}]}',
    ]

    def ok(self, item):
        return check_one(Expect.parse(item), self.OUT).ok

    def test_paths_filters_and_counts(self):
        self.assertTrue(self.ok({"json": "summary.outcome", "equals": "arrived"}))
        self.assertFalse(self.ok({"json": "summary.outcome", "equals": "arrived", "all": True}))
        self.assertFalse(self.ok({"json": "summary.outcome", "equals": "arrived", "count": 2}))
        self.assertTrue(self.ok({"json": "summary.outcome", "schema": "fnv.nav.walk/1", "count": 2}))
        self.assertTrue(self.ok({"json": "summary.seconds", "range": [11, 12]}))
        self.assertTrue(self.ok({"json": "situations.?situation=sneaking, lit.detected_out_to", "equals": 1840.0}))
        self.assertTrue(self.ok({"json": "situations.?situation=sneaking, dark.detected_out_to", "null": True}))
        self.assertFalse(self.ok({"json": "situations.?situation=sneaking, lit.detected_out_to", "null": True}))
        self.assertTrue(self.ok({"json": "situations.*.situation", "count": 2}))
        self.assertTrue(self.ok({"json": "actors.?actor=c.factions.#", "equals": 4}))
        self.assertTrue(self.ok({"json": "observations.?label=final: Q 80.done.80", "equals": True}))
        self.assertTrue(self.ok({"json": "observations.?label=final: Q 80.var.a.b", "equals": 2}))
        self.assertTrue(self.ok({"json": "observations.?label=final: Q 80.stage", "equals": 80}))
        o = check_one(Expect.parse({"json": "nope.x", "equals": 1}), self.OUT)
        self.assertEqual((o.ok, o.what), (False, "missing"))
        o = check_one(Expect.parse({"json": "summary.outcome", "equals": "lost"}), self.OUT)
        self.assertIn('"arrived"', o.detail)

    def test_json_only_in_expect(self):
        with self.assertRaises(ScenarioError):
            load_text('[[scenario]]\nname = "t"\nkind = "tool"\npaths = ["x"]\ncommands = [["a"]]\n'
                      'forbid = [{ json = "a", equals = 1 }]\n')


class ScenarioFileTests(unittest.TestCase):
    def test_game_scenario_expects_run_finished(self):
        s = load_text(game("a")).scenarios[0]
        self.assertEqual(s.expect[0].label, "run finished")

    def test_game_scenario_can_end_another_way(self):
        s = load_text(game("a", extra='ends = "the new run is up"\n')).scenarios[0]
        self.assertEqual(s.expect[0].label, "the new run is up")
        self.assertFalse(any(e.label == "run finished" for e in s.expect))

    def test_errors(self):
        with self.assertRaises(ScenarioError):
            load_text(game("a", run=0))
        with self.assertRaises(ScenarioError):
            load_text(game("a", extra='colour = "red"'))
        with self.assertRaises(ScenarioError):
            load_text(game("a") + game("a"))
        with self.assertRaises(ScenarioError):
            load_text(game("a", args='["--run-seconds", "3"]'))
        with self.assertRaises(ScenarioError):
            load_text('[[scenario]]\nname = "t"\nkind = "tool"\npaths = ["x"]\n')
        with self.assertRaises(ScenarioError):
            load_text('[[scenario]]\nname = "t"\nrun_seconds = 3\n')  # cannot be selected
        with self.assertRaises(ScenarioError):
            load_text('bogus = 1\n' + game("a"))

    def test_launch_groups_are_validated_at_load(self):
        text = TIMED + game("a", extra='launch = "L"') + game("b", args='["--cell", "Y"]', extra='launch = "L"')
        with self.assertRaises(ScenarioError):
            load_text(text)

    def test_repo_scenario_file(self):
        suite = load(HERE / "scenarios.toml")
        self.assertGreater(len(suite.scenarios), 40)
        for s in suite.scenarios:
            self.assertTrue(s.source, s.name)
            self.assertTrue(s.expect or s.order or s.manual, s.name)

    def test_repo_globs_match_files(self):
        """Every trigger glob names something that exists (catches typos and renames)."""
        def ls(*extra):
            r = subprocess.run(["git", "-C", str(HERE), "ls-files", "--full-name", *extra, ":/"],
                               capture_output=True, text=True)
            if r.returncode != 0:
                self.skipTest("no git checkout")
            return r.stdout.split()

        files = ls() + ls("--others", "--exclude-standard")
        suite = load(HERE / "scenarios.toml")
        dead = [(s.name, g) for s in suite.scenarios for g in s.paths if not any(choose.matches(f, [g]) for f in files)]
        dead += [(c.name, g) for c in suite.checks for g in c.paths if not any(choose.matches(f, [g]) for f in files)]
        self.assertEqual(dead, [])


class SelectionTests(unittest.TestCase):
    def test_globs(self):
        m = choose.matches
        self.assertTrue(m("engine/nvf-play/src/wait.rs", ["engine/nvf-play/src/wait*"]))
        self.assertTrue(m("engine/nvf-play/src/wait/model.rs", ["engine/nvf-play/src/wait*/**"]))
        self.assertFalse(m("engine/nvf-play/src/wait/model.rs", ["engine/nvf-play/src/wait*"]))
        self.assertTrue(m("crates/fnv-ai/src/x/y.rs", ["crates/fnv-ai/"]))
        self.assertTrue(m("a/b/c.rs", ["**/c.rs"]))
        self.assertTrue(m("c.rs", ["**/c.rs"]))
        self.assertFalse(m("crates/fnv-aix/src/a.rs", ["crates/fnv-ai/"]))
        self.assertTrue(m("engine\\nvforever\\src\\sky.rs", ["engine/nvforever/src/sky*"]))
        self.assertFalse(m("engine/nvforever/src/skyline/x.rs", ["engine/nvforever/src/sky?rs"]))

    def test_changed_selection_and_checks(self):
        suite = load_text(TIMED + game("a") + game("b", extra='paths = ["docs/x.md"]').replace(
            'paths = ["engine/"]\n', "", 1) + """
[[check]]
name = "engine clippy"
paths = ["engine/", "crates/"]
cmd = ["cargo", "clippy"]
[[check]]
name = "lab"
paths = ["parity-lab/"]
cmd = ["python"]
""")
        hits = choose.by_changes(suite.scenarios, ["engine/nvforever/src/main.rs", "README.md"])
        self.assertEqual([s.name for s, _ in hits], ["a"])
        self.assertEqual([c.name for c in choose.checks_for(suite.checks, ["crates/fnv-esm/src/lib.rs"])],
                         ["engine clippy"])
        self.assertEqual(choose.checks_for(suite.checks, ["docs/x.md"]), [])

    def test_changed_files_from_git(self):
        with tempfile.TemporaryDirectory() as d:
            def git(*a):
                subprocess.run(["git", "-C", d, *a], check=True, capture_output=True)
            git("init", "-q", "-b", "main")
            git("config", "user.email", "t@example.invalid")
            git("config", "user.name", "t")
            (Path(d) / "a.txt").write_text("1")
            git("add", "a.txt")
            git("commit", "-qm", "one")
            git("branch", "base")
            (Path(d) / "b.txt").write_text("2")
            git("add", "b.txt")
            git("commit", "-qm", "two")
            (Path(d) / "a.txt").write_text("changed")
            (Path(d) / "new.txt").write_text("untracked")
            self.assertEqual(choose.changed_files(Path(d), "base"), ["a.txt", "b.txt", "new.txt"])

    def test_area(self):
        suite = load_text(game("a", extra='areas = ["Audio"]') + game("b"))
        self.assertEqual([s.name for s in choose.by_area(suite.scenarios, ["audio"])], ["a"])


class PlanTests(unittest.TestCase):
    def test_shift(self):
        self.assertEqual(shift("3:menu:1074,4.5:raise", Timed(), 18), "21:menu:1074,22.5:raise")
        self.assertEqual(shift("20,28", Timed(), 2), "22,30")
        self.assertEqual(shift("66:69", Timed(shift="all"), 1), "67:70")
        self.assertEqual(shift("18", Timed(shift="value"), 2), "20")
        self.assertEqual(shift("10:GSChetRef=for sale|leave", Timed(sep=";"), 5), "15:GSChetRef=for sale|leave")

    def test_merge_joins_timed_and_keeps_shared(self):
        text = TIMED + game("a", env='{ NVF_PLAY_TEST_SCRIPT_MENUS = "0.3", NVF_PLAY_TEST_SCRIPT = "3:x" }',
                            extra='launch = "L"') \
            + game("b", run=30, env='{ NVF_PLAY_TEST_SCRIPT_MENUS = "0.3", NVF_PLAY_TEST_SCRIPT = "3:x,9:y" }',
                   extra='launch = "L"\noffset = 0') \
            + game("c", run=10, env='{ NVF_PLAY_TEST_SCRIPT = "1:z", NVF_PLAY_TEST_SCRIPT_MENUS = "0.3" }',
                   extra='launch = "L"\noffset = 25')
        suite = load_text(text)
        [x] = plan(suite, suite.scenarios)
        self.assertEqual(x.name, "L")
        self.assertEqual(x.env["NVF_PLAY_TEST_SCRIPT"], "3:x,9:y,26:z")  # same item once, c moved by 25
        self.assertEqual(x.run_seconds, 35)  # c: 10 + 25
        self.assertEqual([s.name for s in x.scenarios], ["a", "b", "c"])
        alone = plan(suite, suite.scenarios, no_merge=True)
        self.assertEqual([(y.name, y.run_seconds) for y in alone], [("a", 20), ("b", 30), ("c", 10)])
        self.assertEqual(alone[2].env["NVF_PLAY_TEST_SCRIPT"], "1:z")  # no offset when alone
        part = plan(suite, [suite.scenarios[2]])
        self.assertEqual((part[0].name, part[0].env["NVF_PLAY_TEST_SCRIPT"]), ("c", "1:z"))

    def test_conflicts(self):
        s = load_text(TIMED + game("a", env='{ NVF_PLAY_TEST_SCRIPT_MENUS = "1" }')
                      + game("b", env='{ NVF_PLAY_TEST_SCRIPT_MENUS = "0.3" }')).scenarios
        with self.assertRaises(ScenarioError):
            merge("L", s, {})
        s = load_text(game("a", env='{ NVF_PLAY_TEST_GOD = "1" }') + game("b")).scenarios
        x = merge("L", s, {})
        self.assertEqual(x.notes, ["NVF_PLAY_TEST_GOD=1 also applies to b"])
        s = load_text(game("a") + game("b", env='{ NVF_PLAY_TEST_NEAR = "1" }', extra="offset = 3")).scenarios
        with self.assertRaises(ScenarioError):
            merge("L", s, {}, untimed=set())  # an offset member with an aid of unknown kind


class JudgeTests(unittest.TestCase):
    def suite(self, extra):
        return load_text(TIMED + game("a", extra=extra)).scenarios[0]

    def test_pass_fail_known(self):
        s = self.suite('expect = ["music summary: 1 started"]')
        self.assertEqual(judge(s, LOG, 0).status, "PASS")
        self.assertEqual(judge(s, LOG, 3).status, "FAIL")
        self.assertEqual(judge(s, LOG, None, "timed out").status, "FAIL")
        s = self.suite('expect = [{ text = "music summary: 9 started", known_failing = "bug 1" }]')
        v = judge(s, LOG, 0)
        self.assertEqual(v.status, "KNOWN")
        s = self.suite('expect = [{ text = "music summary: 1 started", known_failing = "bug 1" }]')
        v = judge(s, LOG, 0)
        self.assertEqual((v.status, v.xpass), ("PASS", ["music summary: 1 started"]))
        s = self.suite('order = [["run finished", "music summary"]]')
        self.assertEqual(judge(s, LOG, 0).status, "FAIL")


class GameEnvTests(unittest.TestCase):
    def test_play_runs_count_the_world_aids_from_the_scripts(self):
        suite = load_text(game("p", args='["--cell", "X", "--play"]', env='{ NVF_PLAY_TEST_CONSOLE = "3:tgm" }')
                          + game("v", env='{ NVF_PLAY_TEST_SHOTS = "3" }')
                          + game("old", args='["--play"]', env=f'{{ {FROM_SCRIPTS} = "0" }}'))
        p, v, old = plan(suite, suite.scenarios)
        self.assertEqual(game_env(p), {"NVF_PLAY_TEST_CONSOLE": "3:tgm", FROM_SCRIPTS: "1"})
        self.assertNotIn(FROM_SCRIPTS, game_env(v))  # no --play: no scripts to wait for
        self.assertEqual(game_env(old)[FROM_SCRIPTS], "0")  # a scenario keeps the old clock
        self.assertNotIn(FROM_SCRIPTS, p.env)  # the plan itself is unchanged

    def test_the_callers_switch_does_not_leak_in(self):
        os.environ[FROM_SCRIPTS] = "1"
        try:
            self.assertNotIn(FROM_SCRIPTS, aid_free_environ({}))
        finally:
            del os.environ[FROM_SCRIPTS]


class LockTests(unittest.TestCase):
    def test_all_or_none_and_release(self):
        with tempfile.TemporaryDirectory() as d:
            a, b = Path(d) / "x" / "run.lock", Path(d) / "smoke.lock"
            b.mkdir()  # someone else holds the second
            locks = Locks([a, b], poll=0.01, wait_max=0.05, say=lambda s: None)
            self.assertEqual(locks.try_take(), b)
            self.assertFalse(a.exists())  # the first is given back
            with self.assertRaises(TimeoutError):
                locks.acquire()
            b.rmdir()
            with locks:
                self.assertTrue(a.is_dir() and b.is_dir())
            self.assertFalse(a.exists() or b.exists())

    def test_stale_runner_lock_is_cleared_but_not_others(self):
        with tempfile.TemporaryDirectory() as d:
            mine, theirs = Path(d) / "a.lock", Path(d) / "b.lock"
            mine.mkdir()
            (mine / "smoke-owner.txt").write_text("999999999 smoke runner\n")  # no such process
            locks = Locks([mine], poll=0.01, wait_max=0.05, say=lambda s: None)
            self.assertIsNone(locks.try_take())
            locks.release()
            theirs.mkdir()  # a plain mkdir lock of another agent: never cleared
            locks = Locks([theirs], poll=0.01, wait_max=0.05, say=lambda s: None)
            self.assertEqual(locks.try_take(), theirs)
            self.assertTrue(theirs.is_dir())


    def test_old_mkdir_lock_without_a_game_is_reported_not_removed(self):
        with tempfile.TemporaryDirectory() as d:
            lock = Path(d) / "run.lock"
            lock.mkdir()
            said = []
            locks = Locks([lock], poll=0.01, wait_max=0.05, say=said.append, stale_after=0.0, running=lambda: False)
            with self.assertRaises(TimeoutError):
                locks.acquire()
            self.assertTrue(lock.is_dir())
            self.assertEqual(sum("ABANDONED" in x for x in said), 1)  # once, not every poll
            said.clear()
            busy = Locks([lock], poll=0.01, wait_max=0.05, say=said.append, stale_after=0.0, running=lambda: True)
            with self.assertRaises(TimeoutError):
                busy.acquire()
            self.assertFalse(any("ABANDONED" in x for x in said))  # a game runs: just waiting
            self.assertTrue(any("mkdir lock, made at" in x for x in said))


class QueueTests(unittest.TestCase):
    def test_first_come_first_served(self):
        with tempfile.TemporaryDirectory() as d:
            lock = Path(d) / "run.lock"
            live = {101, 102}
            first = Queue(Path(d) / "smoke-queue", pid=101, alive=lambda p: p in live)
            second = Queue(Path(d) / "smoke-queue", pid=102, alive=lambda p: p in live)
            first.join()
            time.sleep(0.002)
            second.join()
            self.assertEqual(first.ahead(), [])
            self.assertEqual(len(second.ahead()), 1)
            # The second runner does not take the free lock while the first is queued.
            waiting = Locks([lock], poll=0.01, wait_max=0.05, say=lambda s: None, queue=second)
            with self.assertRaises(TimeoutError):
                waiting.acquire()
            self.assertFalse(lock.exists())
            # The first takes it, gives it back between launches and keeps its place.
            ahead = Locks([lock], poll=0.01, wait_max=0.05, say=lambda s: None, queue=first)
            ahead.acquire()
            ahead.release()
            self.assertEqual(len(second.ahead()), 1)
            ahead.finish()
            self.assertEqual(second.ahead(), [])
            waiting.acquire()
            self.assertTrue(lock.is_dir())
            waiting.finish()
            self.assertFalse(lock.exists())
            self.assertEqual(list((Path(d) / "smoke-queue").glob("*.ticket")), [])

    def test_a_dead_runners_ticket_is_cleared(self):
        with tempfile.TemporaryDirectory() as d:
            gone = Queue(Path(d) / "q", pid=7, alive=lambda p: False)
            gone.join()
            time.sleep(0.002)
            me = Queue(Path(d) / "q", pid=8, alive=lambda p: p == 8)
            me.join()
            self.assertEqual(me.ahead(), [])
            self.assertFalse(gone.ticket.exists())


class CheckCommandTests(unittest.TestCase):
    def test_check_rejudges_saved_logs(self):
        with tempfile.TemporaryDirectory() as d:
            sc = Path(d) / "s.toml"
            sc.write_text(game("a", extra='expect = ["music summary: 1 started"]')
                          + game("b", extra='expect = ["music summary: 7 started"]'), encoding="utf-8")
            out = Path(d) / "out"
            out.mkdir()
            for n in ("a", "b"):
                (out / f"{n}.log").write_text("\n".join(LOG), encoding="utf-8")
                (out / f"{n}.exit").write_text(json.dumps({"exit": 0, "error": "", "seconds": 41.0}))
            buf = io.StringIO()
            with redirect_stdout(buf):
                code = cli.main(["check", "--all", "--scenarios", str(sc), "--out", str(out)])
            text = buf.getvalue()
            self.assertEqual(code, 1)
            self.assertIn("PASS  a 41 s", text)
            self.assertIn("FAIL  b", text)
            self.assertIn("music summary: 1 started", text)  # the similar line is shown
            self.assertIn("1 passed, 1 failed", text)
            self.assertLessEqual(len(text.splitlines()), 6)  # compact


@unittest.skipUnless(sys.platform == "win32", "thread stacks are read on Windows only")
class StackTests(unittest.TestCase):
    def test_a_running_process_shows_every_thread_with_frames(self):
        from smoke.stacks import dump
        p = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
        try:
            time.sleep(0.5)
            text = dump(p.pid)
        finally:
            p.kill()
            p.wait()
        self.assertIn("=== thread", text)
        self.assertRegex(text, r"\n  [0-9a-f]{16} \S")

    def test_a_launch_past_its_time_leaves_its_stacks(self):
        from smoke.plan import Launch
        from smoke.runner import Env, run_launch
        from smoke.scenarios import GAME as GAME_KIND
        s = load_text(game("hangs", extra="timeout = 1")).scenarios[0]
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            exe = out / "hangs.cmd"
            exe.write_text(f'@"{sys.executable}" -c "import time; time.sleep(4)"\n')
            launch = Launch(name="hangs", kind=GAME_KIND, scenarios=[s], run_seconds=1)
            code, err, _ = run_launch(launch, Env(repo=out, out=out, exe=exe, data="D", fnvtool=None), None)
            self.assertIsNone(code)
            self.assertIn("thread stacks in", err)
            self.assertIn("=== thread", (out / "hangs.stacks.txt").read_text(encoding="utf-8"))
            time.sleep(4)  # the orphaned sleeper lets go of the log



@unittest.skipUnless(os.name == "nt", "Windows process ids")
class ChildProcessTests(unittest.TestCase):
    """A load's new run outlives the run the runner started (engine/nvforever/src/handoff.rs):
    the runner waits for it, and for nothing it did not start."""

    def test_only_the_launchs_own_children(self):
        from smoke.runner import child_pids, own_times, wait_for_children

        exe = Path(sys.executable).name
        starter = "import subprocess, sys; subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(2)'])"
        p = subprocess.Popen([sys.executable, "-c", starter])
        p.wait()
        born = own_times(p)
        kids = child_pids(p.pid, born, exe)
        self.assertEqual(len(kids), 1)
        # Another program's name, or a process made outside the parent's lifetime: not its own.
        self.assertEqual(child_pids(p.pid, born, "nvforever.exe"), [])
        self.assertEqual(child_pids(p.pid, (born[0], born[0] + 1), exe), [])
        t = time.monotonic()
        self.assertEqual(wait_for_children(p.pid, kids, t + 20), "")
        self.assertGreater(time.monotonic() - t, 0.5)


if __name__ == "__main__":
    unittest.main()
