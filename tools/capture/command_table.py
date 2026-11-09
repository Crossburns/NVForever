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
        # The compiler counts named parameters; an unlabelled one is named by its type.
        q = {"name": name.strip() or ty.strip().lower(), "type": ty.strip(), "kind": kind_of(p["type"])}
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


def extend(table, catalogue):
    """Catalogue rows with fewer parameters than the game reports get the rest appended
    (optional, as the game types them). Left alone: functions taking any number of
    values (a `...` or format parameter, or more than 4 missing), and rows this tool
    added."""
    by = {c["name"].lower(): c for c in table["commands"]}
    out = []
    for f in catalogue["functions"]:
        c = by.get(f["name"].lower())
        have = f.get("params") or []
        if not c or f.get("family") != "extender" or f.get("opcode_evidence") == "observed: DumpDocs":
            continue
        # Statements the compiler parses itself, not calls.
        if f["name"].lower() in ("break", "continue", "foreach", "while", "loop", "let", "eval", "call"):
            continue
        extra = c["params"][len(have):]
        if not extra or len(extra) > 4:
            continue
        if any(p.get("name", "").startswith("...") or "format" in (p.get("name", "") + (p.get("type") or "")).lower() for p in have):
            continue
        for p in entry({**c, "params": extra})["params"]:
            p["optional"] = True
            have.append(p)
        f["params"] = have
        f["params_evidence"] = "observed: DumpDocs lists %d parameters" % len(c["params"])
        out.append(f["name"])
    return out


def add_aliases(table, catalogue):
    """Aliases the game registers that a catalogue row lacks (GPCDEx for
    GetPlayerControlsDisabledAltEx), when no other row has that name; not one-letter ones."""
    known = set()
    for f in catalogue["functions"]:
        known.add(f["name"].lower())
        known.update(a.lower() for a in f.get("aliases") or [])
    by = {c["name"].lower(): c for c in table["commands"]}
    added = []
    for f in catalogue["functions"]:
        c = by.get(f["name"].lower())
        if not c:
            continue
        for a in c["aliases"]:
            # One-letter aliases (Rand's "r") are left out: scripts name variables so.
            if len(a) > 1 and a.lower() not in known:
                f.setdefault("aliases", []).append(a)
                known.add(a.lower())
                added.append(a)
    return added


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


WIKIDOCS_FORMAT = "nvf.ext.wikidocs/1"

# Base-game commands kept without their opcode: NVForever runs them from its console
# (functions::console), not as script functions.
KEEP_UNNUMBERED = {"killallactors"}

# A base-game parameter type as DumpCommandWikiDocs names it -> its bytecode encoding
# (crates/fnv-script/data/encodings.json): the types whose values are not tagged.
VANILLA_ENCODINGS = {
    "String": "string",
    "ActorValue": "enum:actor_value",
    "Axis": "axis",
    "AnimationGroup": "enum:anim_group",
    "Sex": "enum:sex",
    "MiscStat": "enum:misc_stat",
    "CriticalStage": "enum:critical_stage",
}


def parse_wikidocs(text, first):
    """`DumpCommandWikiDocs <first> <last>` output: one `{{Function …}}` block per
    command, in opcode order from `first` [observed: 485 catalogue opcodes agree with the
    position, none disagree]."""
    out = []
    for block in re.findall(r"\{\{Function\n(.*?)\n\}\}\n\[\[Category", text, re.S):
        def field(k):
            m = re.search(r"^ \|" + re.escape(k) + r" = (.*)$", block, re.M)
            return m.group(1).strip() if m else None
        args = []
        for a in re.findall(r"\{\{FunctionArgument\n(.*?)\n  \}\}", block, re.S):
            args.append({k: v.strip() for k, v in re.findall(r"\|(\w+) = (.*)", a)})
        out.append({"name": field("name"), "alias": field("alias"), "returnType": field("returnType"),
                    "referenceType": field("referenceType"), "conditionFunc": field("conditionFunc"), "args": args})
    for i, c in enumerate(out):
        c["opcode"] = f"0x{first + i:04X}"
    return out


def vanilla(table, catalogue, encodings):
    """The base game's commands (a parsed DumpCommandWikiDocs table): opcodes for rows
    that lack them, the game's alias where no row has it, new rows for commands the
    catalogue lacks, and the encodings of their non-tagged parameters. Returns counts."""
    rows = catalogue["functions"]
    idx = {}
    for f in rows:
        idx.setdefault(f["name"].lower(), f)
        for a in f.get("aliases") or []:
            idx.setdefault(a.lower(), f)
    taken = {o.upper() for f in rows for o in f.get("opcodes") or []}
    n = {"opcodes": 0, "aliases": 0, "rows": 0, "encodings": 0}
    for c in table["commands"]:
        name, op = c["name"], c["opcode"]
        if not name or name.lower().startswith("unusedfunction") or op.upper() in taken and not idx.get(name.lower()):
            continue
        f = idx.get(name.lower()) or (c["alias"] and idx.get(c["alias"].lower()))
        evidence = "observed: DumpCommandWikiDocs"
        if f is None:
            params = []
            for a in c["args"]:
                pname = a.get("Name", "")
                optional = "(optional)" in pname.lower()
                pname = re.sub(r"\s*\((?i:optional)\)", "", pname).strip()
                if pname.startswith("[Enter"):
                    pname = ""
                ty = a.get("Type", "")
                q = {"name": pname or ty.lower(), "type": ty, "kind": kind_of(ty)}
                if optional:
                    q["optional"] = True
                params.append(q)
            f = {"name": name, "origin": "GECK1", "params": params, "family": "vanilla",
                 "opcodes": [op], "opcode_evidence": evidence}
            if c["referenceType"] and c["referenceType"] != "none":
                f["calling_reference"] = "reference"
            if c["alias"]:
                f["aliases"] = [c["alias"]]
            rows.append(f)
            idx[name.lower()] = f
            n["rows"] += 1
        elif not f.get("opcodes") and "Console Functions" in (f.get("categories") or []):
            # A console command NVForever runs from its console by name (functions::console);
            # so is KillAllActors, kept without its opcode by hand.
            continue
        elif not f.get("opcodes") and (f.get("family") != "vanilla" or f["name"].lower() in KEEP_UNNUMBERED):
            # An extender's command of the same name wins in the original (Sin, Cos, …
            # degrees, not the base game's) [observed: the compile tests' probes]; and
            # the commands NVForever runs from its console by name.
            continue
        elif not f.get("opcodes") and op.upper() in taken:
            # Another row has the opcode (SwapTexture for SwapTextureOnRef).
            continue
        elif not f.get("opcodes"):
            f["opcodes"] = [op]
            f["opcode_evidence"] = evidence
            n["opcodes"] += 1
        if c["alias"] and c["alias"].lower() not in idx and len(c["alias"]) > 1:
            f.setdefault("aliases", []).append(c["alias"])
            idx[c["alias"].lower()] = f
            n["aliases"] += 1
        taken.add(op.upper())
        encs = [VANILLA_ENCODINGS.get(a.get("Type", ""), "value") for a in c["args"]]
        key = op
        if any(e != "value" for e in encs) and key not in encodings["functions"] and key.lower() not in {k.lower() for k in encodings["functions"]}:
            encodings["functions"][key] = {"name": name, "params": encs, "evidence": evidence}
            n["encodings"] += 1
    if any(n.values()):
        catalogue.setdefault("sources", {})["dumpcommandwikidocs"] = (
            "xNVSE DumpCommandWikiDocs 4096 5119 in the original game: the base game's commands in "
            "opcode order (name, alias, parameter types); opcodes of rows that had none and rows "
            "the wiki lacks (tools/capture/command_table.py vanilla)")
    return n


def write_encodings(path, encodings):
    """encodings.json with its `functions` block rewritten one function a line in opcode
    order, the rest of the file as it was."""
    with open(path, encoding="utf-8", newline="") as f:
        text = f.read()
    nl = "\r\n" if "\r\n" in text else "\n"
    start = text.index('"functions": {')
    end = text.index(nl + "  }", start)
    lines = [f'    "{k}": ' + json.dumps(v, ensure_ascii=False) for k, v in sorted(encodings["functions"].items(), key=lambda kv: int(kv[0], 16))]
    text = text[:start] + '"functions": {' + nl + ("," + nl).join(lines) + text[end:]
    json.loads(text)
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(text)


def opcodes(table):
    """Opcode -> command name, every command the table has (fnv.script.opcode_names/1)."""
    names = {}
    for c in table["commands"]:
        if c["opcode"]:
            names[c["opcode"]] = c["name"]
    return {
        "format": "fnv.script.opcode_names/1",
        "source": "observed: xNVSE DumpDocs in the original game with High and Dry's plugins "
                  "(xNVSE 6.4.9 and 54 plugins), 2026-11-08 (tools/capture/command_table.py); "
                  "used to name the extender opcodes in compiled scripts",
        "names": dict(sorted(names.items())),
    }


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
    v = sub.add_parser("vanilla", help="merge the base game's commands from DumpCommandWikiDocs output")
    v.add_argument("log", help="nvse.log holding DumpCommandWikiDocs output")
    v.add_argument("--first", default="0x1000", help="the first opcode the dump was asked for")
    v.add_argument("--functions", default="schemas/script/functions.json")
    v.add_argument("--encodings", default="crates/fnv-script/data/encodings.json")
    v.add_argument("--check", action="store_true")
    o = sub.add_parser("opcodes")
    o.add_argument("table")
    o.add_argument("-o", "--output", default="schemas/script/opcode_names.json")
    a = ap.parse_args(argv)
    if a.cmd == "vanilla":
        with open(a.log, encoding="utf-8", errors="replace") as f:
            table = {"commands": parse_wikidocs(f.read(), int(a.first, 0))}
        with open(a.functions, encoding="utf-8") as f:
            catalogue = json.load(f)
        with open(a.encodings, encoding="utf-8") as f:
            encodings = json.load(f)
        n = vanilla(table, catalogue, encodings)
        print(f"{len(table['commands'])} commands: " + ", ".join(f"{k} {v}" for k, v in n.items()))
        if not a.check:
            with open(a.functions, "w", encoding="utf-8", newline="\n") as f:
                f.write(render_json(catalogue))
            write_encodings(a.encodings, encodings)
        return 0
    if a.cmd == "opcodes":
        with open(a.table, encoding="utf-8") as f:
            doc = opcodes(json.load(f))
        with open(a.output, "w", encoding="utf-8", newline="\n") as f:
            json.dump(doc, f, indent=0, ensure_ascii=False)
            f.write("\n")
        print(f"{len(doc['names'])} opcodes")
        return 0
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
    extended = extend(table, catalogue)
    aliases = add_aliases(table, catalogue)
    print(f"{len(aliases)} aliases added: " + ", ".join(aliases[:20]) + (" …" if len(aliases) > 20 else ""))
    print(f"{len(extended)} rows given the game's further parameters: " + ", ".join(extended[:20]) + (" …" if len(extended) > 20 else ""))
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
