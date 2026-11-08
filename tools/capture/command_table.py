#!/usr/bin/env python3
"""Script extender commands from the original game's own command table (xNVSE DumpDocs).

  command_table.py parse <nvse.log> -o <table.json>
  command_table.py merge <table.json> [--functions schemas/script/functions.json] [--check]

`DumpDocs` (an xNVSE console command) writes every registered command into `nvse.log` as
HTML: its name, aliases, parameter types (optional ones in italics), return type, opcode,
the plugin that registered it ("Origin"), whether it is a condition function and whether
it needs a calling reference. Running it in the original game with a mod list's plugins
loaded lists what that game has [observed]; nothing is read from any plugin's code.

`parse` turns the log into a private table (it describes the owner's installed plugins;
keep it with the goldens, outside the repo). `merge` adds to the catalogue every command
the table has that the catalogue lacks by name or alias, as an extender function called by
name (no `opcodes`: NVForever compiles extender calls by name), with the opcode the game
reported kept in `observed_opcode` and `opcode_evidence` "observed: DumpDocs". Entries
already in the catalogue are left alone. `--check` only reports what would be added.

Standard library only.
"""
import argparse
import html
import json
import re
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from script_functions import render_json  # noqa: E402  (the catalogue's own layout)

TABLE_FORMAT = "nvf.ext.command_table/1"

# The registering plugin's name as DumpDocs gives it -> the catalogue's origin code.
ORIGINS = {
    "NVSE": "NVSE",
    "JIP LN NVSE": "JIP",
    "JohnnyGuitarNVSE": "JohnnyGuitar",
    "SUP NVSE Plugin": "SUP",
    "ShowOffNVSE Plugin": "ShowOff",
    "itr-nvse": "ITR",
    "kNVSE": "kNVSE",
    "AnhNVSE": "Anh",
    "MCM Extensions": "MCM",
    "lStewieAl's Tweaks": "Tweaks",
    "Overcharge": "Overcharge",
    "yUI": "yUI",
    "hNVSE": "hNVSE",
    "ClimateControl": "ClimateControl",
    "Book Menu Restored": "BookMenu",
}


def parse(text):
    """The DumpDocs entries of an nvse.log text, in order."""
    out = []
    for block in re.split(r'(?=<p><a name=")', text):
        m = re.match(r'<p><a name="[^"]+"></a><b>([^<]+)</b>', block)
        if not m:
            continue
        body = block.split("</p>", 1)[0]

        def field(k):
            f = re.search(r"<b>" + re.escape(k) + r":</b>\s*([^<]*)", body)
            return f.group(1).strip() if f else ""

        params, count = [], None
        pm = re.search(r"<b>Parameters:</b>\s*(\d+)(.*?)<br><b>Return Type", body, re.S)
        if pm:
            count = int(pm.group(1))
            # One line per parameter; an optional one is in italics (<i>Integer</i>).
            for p in re.findall(r"<br>(?:&nbsp;)*\s*(<i>[^<]*</i>|[^<]+)", pm.group(2)):
                t = html.unescape(re.sub(r"</?i>", "", p)).strip()
                if t:
                    params.append({"type": t, "optional": p.startswith("<i>")})
        op = re.match(r"0x([0-9a-fA-F]+)", field("Opcode"))
        alias = field("Alias")
        out.append({
            "name": html.unescape(m.group(1).strip()),
            "aliases": [] if alias in ("", "none") else [a.strip() for a in alias.split(",") if a.strip()],
            "params": params,
            "param_count": count,
            "returns": field("Return Type"),
            "opcode": f"0x{int(op.group(1), 16):04X}" if op else None,
            "origin": field("Origin"),
            "condition": field("Condition Function") == "Yes",
            "needs_ref": field("Requires calling reference") == "Yes",
        })
    return out


def kind_of(t):
    """The catalogue's parameter kind for a DumpDocs parameter type (`label:Type` or `Type`)."""
    t = t.split(":")[-1].strip().lower()
    if t in ("integer", "int", "short", "long"):
        return "int"
    if t in ("float", "double"):
        return "float"
    if t == "string":
        return "string"
    if t in ("objectref", "mapmarker", "actor"):
        return "reference"
    if t in ("scriptvar", "variable"):
        return "variable"
    if t == "array":
        return "array"
    if t == "axis":
        return "axis"
    if t == "actorvalue":
        return "actor_value"
    if t in ("anytype", "pair", "anything"):
        return "unknown"
    # Everything else names a form type (AnyForm, ObjectID, ActorBase, Quest, FormList …).
    return "form"


def entry(c):
    params = []
    for p in c["params"]:
        name, _, ty = p["type"].rpartition(":")
        q = {"name": name.strip() or None, "type": ty.strip(), "kind": kind_of(p["type"])}
        if p["optional"]:
            q["optional"] = True
        params.append(q)
    e = {
        "name": c["name"],
        "origin": ORIGINS.get(c["origin"], c["origin"] or None),
        "params": params,
        "family": "extender",
        "observed_opcode": c["opcode"],
        "opcode_evidence": "observed: DumpDocs",
    }
    if c["aliases"]:
        e["aliases"] = c["aliases"]
    if c["needs_ref"]:
        e["calling_reference"] = "reference"
    if c["returns"]:
        e["returns"] = {"type": c["returns"].lower(), "name": None}
    return e


def merge(table, catalogue):
    known = set()
    for f in catalogue["functions"]:
        known.add(f["name"].lower())
        known.update(a.lower() for a in f.get("aliases") or [])
    added = []
    for c in table["commands"]:
        names = [c["name"], *c["aliases"]]
        if any(n.lower() in known for n in names):
            continue
        e = entry(c)
        catalogue["functions"].append(e)
        known.update(n.lower() for n in names)
        added.append(e)
    if added:
        catalogue.setdefault("sources", {})["dumpdocs"] = (
            "xNVSE DumpDocs in the original game: names, aliases, parameter types, calling "
            "reference and opcode of commands the wiki export lacks (tools/capture/command_table.py)")
    return added


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("parse")
    p.add_argument("log")
    p.add_argument("-o", "--output", required=True)
    m = sub.add_parser("merge")
    m.add_argument("table")
    m.add_argument("--functions", default="schemas/script/functions.json")
    m.add_argument("--check", action="store_true")
    a = ap.parse_args(argv)
    if a.cmd == "parse":
        with open(a.log, encoding="utf-8", errors="replace") as f:
            commands = parse(f.read())
        with open(a.output, "w", encoding="utf-8") as f:
            json.dump({"format": TABLE_FORMAT, "commands": commands}, f, indent=1)
        print(f"{len(commands)} commands")
        return 0
    with open(a.table, encoding="utf-8") as f:
        table = json.load(f)
    if table.get("format") != TABLE_FORMAT:
        print(f"{a.table}: not a {TABLE_FORMAT} table", file=sys.stderr)
        return 1
    with open(a.functions, encoding="utf-8") as f:
        catalogue = json.load(f)
    added = merge(table, catalogue)
    by = {}
    for e in added:
        by[e["origin"]] = by.get(e["origin"], 0) + 1
    print(f"{len(added)} commands added: " + ", ".join(f"{k} {n}" for k, n in sorted(by.items(), key=lambda x: -x[1])))
    if not a.check:
        with open(a.functions, "w", encoding="utf-8", newline="\n") as f:
            f.write(render_json(catalogue))
    return 0


if __name__ == "__main__":
    sys.exit(main())
