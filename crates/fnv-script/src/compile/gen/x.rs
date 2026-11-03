//! The extender's expressions (task A7; docs/spec-notes/ext-syntax.md, script-compiler.md
//! §9): tokens → a resolved [`XExpr`] tree, by precedence climbing over the operator table
//! of NVSE Expressions [documented: NVSE Expressions].
//!
//! Precedence, loosest first (the page's numbers): `:=` (0, right to left) · `||` (1) ·
//! `&&` and the compound assignments (2) · `:` `::` (3) · `==` `!=` (4) · `<` `>` `<=` `>=`
//! (5) · `|` (6) · `&` (7) · `<<` `>>` (8) · `+` `-` (9) · `*` `/` `%` (10) · `^` (11) ·
//! unary `-` `$` `#` `*` `&` (12) · `!` (13) · `[]` `->` (15). `:=` groups right to left
//! and `&&` binds tighter than `||` [documented; observed in the bytes the GECK with xNVSE
//! wrote, docs/spec-notes/ext-bytecode.md]; the other binary operators group left to right
//! and the compound assignments right to left [hypothesis, probes in
//! tools/capture/ext-probes/expressions].
//!
//! A function inside an expression takes up to as many arguments as it has parameters. An
//! argument is an operand: `Player.GetActorValue Health + 50` is `(GetAV Health) + 50`
//! [documented: Inline Expression; Let]. The arguments of an NVSE-aware function (the
//! wiki's category: Call, Print, SetFunctionValue, the ar_* family…) are whole
//! expressions [documented: Script Compiler Override, "Let, if eval, While, Print and any
//! other NVSE-Aware Function"]. Inside a compiler-override block (`Begin _GameMode`) the
//! wiki says `Sin 14 == 1` is `Sin (14 == 1)`, but xNVSE 6.4.9 gives `Sin 90 > 0.5` 1 there,
//! but `Abs -3 + 1` 2 [observed: probes expressions/08, 09 in the original game, which beat
//! the page]: there an argument takes arithmetic and stops at a comparison. Arguments are separated by a comma or by two
//! operands meeting without an operator (`Rand 0, 10`, `Rand 0 10`), and a `-` starts the
//! first argument (`Abs -3`) [hypothesis, probes in tools/capture/ext-probes/expressions].

use super::{Gen, Val};
use crate::compile::lex::{XKind, XTok};
use crate::ir::{FuncId, RefIndex, VarRef, VarSlot, XAssign, XCall, XExpr, XOp, XUnary};
use crate::table::ParamEnc;

pub(in crate::compile) struct P<'t> {
    t: &'t [XTok],
    i: usize,
}

impl<'t> P<'t> {
    pub(in crate::compile) fn new(t: &'t [XTok]) -> P<'t> {
        P { t, i: 0 }
    }
    fn peek(&self) -> Option<&'t XTok> {
        self.t.get(self.i)
    }
    fn peek2(&self) -> Option<&'t XTok> {
        self.t.get(self.i + 1)
    }
    fn next(&mut self) -> Option<&'t XTok> {
        let t = self.t.get(self.i);
        self.i += 1;
        t
    }
    pub(in crate::compile) fn done(&self) -> bool {
        self.i >= self.t.len()
    }
    pub(in crate::compile) fn rest(&self) -> String {
        self.t[self.i.min(self.t.len())..].iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ")
    }
}

enum Infix {
    Bin(XOp),
    Assign(XAssign),
}

/// Left and right binding power of an infix operator (twice the page's precedence, plus
/// one; a right-to-left operator's right side binds as loosely as its left).
fn infix(t: &XTok) -> Option<(u8, u8, Infix)> {
    if t.kind != XKind::Op {
        return None;
    }
    let l = |p: u8| (p * 2 + 1, p * 2 + 2);
    let r = |p: u8| (p * 2 + 1, p * 2 + 1);
    let (lb, rb, op) = match t.text.as_str() {
        ":=" | "=" => (r(0).0, r(0).1, Infix::Assign(XAssign::Set)),
        "||" => (l(1).0, l(1).1, Infix::Bin(XOp::Or)),
        "&&" => (l(2).0, l(2).1, Infix::Bin(XOp::And)),
        "+=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Add)),
        "-=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Sub)),
        "*=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Mul)),
        "/=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Div)),
        "^=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Pow)),
        "|=" => (r(2).0, r(2).1, Infix::Assign(XAssign::BitOr)),
        "&=" => (r(2).0, r(2).1, Infix::Assign(XAssign::BitAnd)),
        "%=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Mod)),
        ":" => (l(3).0, l(3).1, Infix::Bin(XOp::Slice)),
        "::" => (l(3).0, l(3).1, Infix::Bin(XOp::Pair)),
        "==" => (l(4).0, l(4).1, Infix::Bin(XOp::Eq)),
        "!=" => (l(4).0, l(4).1, Infix::Bin(XOp::Ne)),
        "<" => (l(5).0, l(5).1, Infix::Bin(XOp::Lt)),
        ">" => (l(5).0, l(5).1, Infix::Bin(XOp::Gt)),
        "<=" => (l(5).0, l(5).1, Infix::Bin(XOp::Le)),
        ">=" => (l(5).0, l(5).1, Infix::Bin(XOp::Ge)),
        "|" => (l(6).0, l(6).1, Infix::Bin(XOp::BitOr)),
        "&" => (l(7).0, l(7).1, Infix::Bin(XOp::BitAnd)),
        "<<" => (l(8).0, l(8).1, Infix::Bin(XOp::Shl)),
        ">>" => (l(8).0, l(8).1, Infix::Bin(XOp::Shr)),
        "+" => (l(9).0, l(9).1, Infix::Bin(XOp::Add)),
        "-" => (l(9).0, l(9).1, Infix::Bin(XOp::Sub)),
        "*" => (l(10).0, l(10).1, Infix::Bin(XOp::Mul)),
        "/" => (l(10).0, l(10).1, Infix::Bin(XOp::Div)),
        "%" => (l(10).0, l(10).1, Infix::Bin(XOp::Mod)),
        "^" => (l(11).0, l(11).1, Infix::Bin(XOp::Pow)),
        _ => return None,
    };
    Some((lb, rb, op))
}

/// A prefix operator and the binding power of its operand.
fn prefix(t: &XTok) -> Option<(XUnary, u8)> {
    if t.kind != XKind::Op {
        return None;
    }
    Some(match t.text.as_str() {
        "-" => (XUnary::Neg, 25),
        "$" => (XUnary::Stringize, 25),
        "#" => (XUnary::Numericize, 25),
        "*" => (XUnary::Unbox, 25),
        "&" => (XUnary::Box, 25),
        "!" => (XUnary::Not, 27),
        _ => return None,
    })
}

/// Binding power of `[]` and `->` (precedence 15).
const POSTFIX: u8 = 31;
/// An argument of a function: an operand (prefix operators, `[]`, `->` and dot calls on
/// it, no binary operator).
const ARG: u8 = 25;
/// An argument of an NVSE-aware function: everything but the assignments.
const ARG_OVERRIDE: u8 = 2;
/// An argument inside a compiler-override block: arithmetic and the bitwise operators,
/// not the comparisons (`Abs -3 + 1` is Abs(-3 + 1), `Sin 90 > 0.5` is (Sin 90) > 0.5)
/// [observed: probes expressions/08, 09 in the original game; bitwise operators
/// hypothesis].
const ARG_IN_OVERRIDE: u8 = 13;

fn starts_operand(t: Option<&XTok>) -> bool {
    match t {
        Some(t) => match t.kind {
            XKind::Word | XKind::Num | XKind::Str | XKind::Open => true,
            XKind::Op => matches!(t.text.as_str(), "$" | "#" | "!"),
            _ => false,
        },
        None => false,
    }
}

/// A function as the extended grammar sees it.
struct XFn {
    func: FuncId,
    /// Arguments taken; `None`: all that follow.
    params: Option<usize>,
    opcode: Option<u16>,
    /// An NVSE-aware function: whole expressions as arguments.
    aware: bool,
}

impl Gen<'_> {
    fn xerr(&mut self, kind: &'static str, detail: impl Into<String>) -> Option<XExpr> {
        self.diag(kind, true, detail);
        None
    }

    /// A whole expression (`min` = the loosest binding power accepted).
    pub(in crate::compile) fn xexpr(&mut self, p: &mut P<'_>, min: u8) -> Option<XExpr> {
        let mut left = match p.peek().and_then(prefix) {
            Some((op, bp)) => {
                p.next();
                // `!-1` is refused, `-!0` compiles [observed: the GECK with xNVSE,
                // docs/spec-notes/ext-bytecode.md].
                if op == XUnary::Not && p.peek().is_some_and(|t| t.is(XKind::Op, "-")) {
                    return self.xerr("syntax", "too many operators (! before -)");
                }
                let e = self.xexpr(p, bp)?;
                XExpr::Unary { op, e: Box::new(e) }
            }
            None => self.xprimary(p)?,
        };
        while let Some(t) = p.peek() {
            if t.kind == XKind::LBracket && POSTFIX >= min {
                p.next();
                let was = std::mem::replace(&mut self.in_args, false);
                let inner = self.xexpr(p, 0);
                self.in_args = was;
                let inner = inner?;
                if p.next().map(|t| t.kind) != Some(XKind::RBracket) {
                    return self.xerr("syntax", "missing ]");
                }
                left = XExpr::Binary { op: XOp::Subscript, l: Box::new(left), r: Box::new(inner) };
                continue;
            }
            // `expr.Function args`: a call on a result or an element [documented:
            // Chained dot syntax, NVSE Expressions `.`].
            if t.kind == XKind::Word && t.text.starts_with('.') && t.text.len() > 1 && POSTFIX >= min {
                let name = &t.text[1..];
                let Some(f) = self.xfunction(name) else { return self.xerr("unknown_function", name.to_string()) };
                p.next();
                let args = self.xargs(p, &f)?;
                left = XExpr::Call { call: XCall { on: Some(Box::new(left)), func: f.func, args } };
                continue;
            }
            if t.is(XKind::Op, "->") && POSTFIX >= min {
                p.next();
                let Some(k) = p.next().filter(|k| matches!(k.kind, XKind::Word | XKind::Str)) else {
                    return self.xerr("syntax", "-> without a key");
                };
                left = XExpr::Member { l: Box::new(left), key: k.text.clone() };
                continue;
            }
            if t.sign && self.in_args {
                break;
            }
            let Some((lb, rb, op)) = infix(t) else { break };
            if lb < min {
                break;
            }
            p.next();
            let right = self.xexpr(p, rb)?;
            left = match op {
                Infix::Bin(op) => XExpr::Binary { op, l: Box::new(left), r: Box::new(right) },
                Infix::Assign(op) => {
                    // `1 && iX += 5` compiles in the editor and fails when it runs, and the script
                    // goes on [observed: probe expressions/04]: a warning here, a run-time
                    // error in the VM; only a literal on the left is refused.
                    if matches!(left, XExpr::Num { .. } | XExpr::Str { .. }) {
                        return self.xerr("bad_assignment_target", t.text.clone());
                    }
                    if !matches!(left, XExpr::Var { .. } | XExpr::Binary { op: XOp::Subscript, .. } | XExpr::Member { .. }) {
                        self.diag("assignment_to_a_value", false, t.text.clone());
                    }
                    XExpr::Assign { op, target: Box::new(left), value: Box::new(right) }
                }
            };
        }
        Some(left)
    }

    fn xprimary(&mut self, p: &mut P<'_>) -> Option<XExpr> {
        let Some(t) = p.next() else { return self.xerr("syntax", "expression ends early") };
        match t.kind {
            XKind::Num => Some(XExpr::Num { value: t.text.parse().unwrap_or(0.0), text: t.text.clone() }),
            XKind::Str => Some(XExpr::Str { s: t.text.clone() }),
            XKind::Open => {
                let was = std::mem::replace(&mut self.in_args, false);
                let was_paren = std::mem::replace(&mut self.in_paren, true);
                let e = self.xexpr(p, 0);
                self.in_args = was;
                self.in_paren = was_paren;
                let e = e?;
                if p.next().map(|c| c.kind) != Some(XKind::Close) {
                    return self.xerr("syntax", "missing )");
                }
                Some(e)
            }
            XKind::Word if t.text.starts_with("__lambda") => match t.text["__lambda".len()..].parse::<usize>() {
                Ok(i) => self.lambda(i),
                Err(_) => self.xerr("unresolved_name", t.text.clone()),
            },
            XKind::Word => self.xword(p, &t.text),
            // `{params} => expr` [documented: Lambda].
            XKind::LBrace => {
                let mut words = Vec::new();
                while let Some(n) = p.next() {
                    match n.kind {
                        XKind::RBrace => break,
                        XKind::Comma => {}
                        XKind::Word => words.push(n.text.clone()),
                        _ => return self.xerr("syntax", format!("{} in a lambda's parameters", n.text)),
                    }
                }
                let mut params = Vec::new();
                for w in self.typed_params(&words) {
                    match self.var(&w) {
                        Some(v) => params.push(VarRef { on: None, var: VarSlot::Local { index: v.index as u16, letter: super::letter_of(&v.ty) } }),
                        None => return self.xerr("unresolved_name", w),
                    }
                }
                if !p.next().is_some_and(|a| a.is(XKind::Op, "=>")) {
                    return self.xerr("syntax", "a lambda's {params} without =>");
                }
                let value = self.xexpr(p, 2)?;
                Some(XExpr::Lambda { params, body: Vec::new(), value: Some(Box::new(value)), index: self.next_lambda() })
            }
            _ => self.xerr("syntax", format!("unexpected {}", t.text)),
        }
    }

    /// Whether `name` (with or without `Ref.`) is a function the catalogue lists without
    /// an opcode.
    pub(in crate::compile) fn xfunction_name(&self, name: &str) -> bool {
        let n = name.rsplit_once('.').map_or(name, |(_, n)| n);
        self.table.unnumbered.contains_key(&n.to_ascii_lowercase())
    }

    fn xfunction(&self, name: &str) -> Option<XFn> {
        if let Some(op) = self.table.by_name(name) {
            let f = self.table.get(op)?;
            let params = match f.param_count {
                Some(n) => Some(n),
                None if f.params.iter().any(|p| p.name.starts_with("...")) => None,
                None => Some(f.params.len().max(f.encodings.len())),
            };
            return Some(XFn { func: FuncId::Opcode(op), params, opcode: Some(op), aware: false });
        }
        let u = self.table.unnumbered.get(&name.to_ascii_lowercase())?;
        // `Call udf [up to 15 arguments]` [documented: Call; User Defined Function]; the
        // catalogue's rows for it come from example pages.
        let named = u.params.iter().filter(|p| !p.name.is_empty()).count();
        let params = if u.name.eq_ignore_ascii_case("call") {
            Some(16)
        // A format string takes up to 20 values [documented: String Formatting]; the
        // catalogue marks them as a parameter named or typed "Formatting"/"formatable".
        // SetNameEx takes format values too, though its page shows none [observed:
        // nvse-fifth, `SetNameEx "Gun %g" 5 rObj` compiles and names it "Gun 5"].
        } else if u.name.eq_ignore_ascii_case("setnameex")
            || u.params.iter().any(|p| {
            p.name.starts_with("...")
                || p.name.to_ascii_lowercase().contains("format")
                || p.wiki_type.as_deref().is_some_and(|t| t.to_ascii_lowercase().contains("format"))
        }) {
            None
        } else {
            Some(named)
        };
        Some(XFn { func: FuncId::Name(u.name.clone()), params, opcode: None, aware: u.aware })
    }

    /// A word: a call (`[Ref.]Function args`), `Quest.var`, a local, a form.
    fn xword(&mut self, p: &mut P<'_>, w: &str) -> Option<XExpr> {
        // `0x1F`, `0b101` [documented: Hexadecimal Notation; Binary Notation].
        let lw = w.to_ascii_lowercase();
        for (prefix, radix) in [("0x", 16), ("0b", 2)] {
            if let Some(d) = lw.strip_prefix(prefix) {
                if let Ok(v) = i64::from_str_radix(d, radix) {
                    return Some(XExpr::Num { value: v as f64, text: v.to_string() });
                }
            }
        }
        // `Quest.refVar.Function`: the function on the reference a quest variable holds
        // [observed: JustAssortedMods.esp, `JLM.rContainer.LNGetName`].
        if let Some((target, fname)) = w.rsplit_once('.').filter(|(t, _)| t.contains('.')) {
            if let (Some(f), Some((q, v))) = (self.xfunction(fname), target.split_once('.')) {
                let on = self.remote(q, v)?;
                let args = self.xargs(p, &f)?;
                return Some(XExpr::Call { call: XCall { on: Some(Box::new(val_expr(on))), func: f.func, args } });
            }
        }
        let (head, name) = match w.split_once('.') {
            Some((h, n)) if !h.is_empty() => (Some(h), n),
            Some((_, n)) => (None, n),
            None => (None, w),
        };
        let is_local = head.is_none() && self.var(name).is_some();
        if !is_local {
            if let Some(f) = self.xfunction(name) {
                let on = match head {
                    Some(h) => match self.reference(h) {
                        Some(r) => Some(Box::new(XExpr::Ref { r: RefIndex(r) })),
                        None => return self.xerr("unresolved_name", h.to_string()),
                    },
                    None => None,
                };
                let args = self.xargs(p, &f)?;
                return Some(XExpr::Call { call: XCall { on, func: f.func, args } });
            }
        }
        if let Some(h) = head {
            let v = self.remote(h, name)?;
            return Some(val_expr(v));
        }
        if let Some(v) = self.var(name) {
            return Some(XExpr::Var {
                var: VarRef { on: None, var: VarSlot::Local { index: v.index as u16, letter: super::letter_of(&v.ty) } },
                form: self.ref_vars.contains(&name.to_ascii_lowercase()),
            });
        }
        match self.form(name) {
            Some(f) => Some(val_expr(self.form_val(f, name))),
            // A bare X, Y or Z that names nothing is an axis, for extender functions whose
            // parameter types the table does not carry (`GetLocalGravity Z`) [observed:
            // jip-getters localgravity rows compile in the original].
            None if self.in_args && name.len() == 1 && matches!(name.to_ascii_uppercase().as_str(), "X" | "Y" | "Z") => {
                Some(XExpr::Axis { c: name.to_ascii_uppercase().chars().next().unwrap_or('Z') })
            }
            // Any other bare word that names nothing is its own text, with the editor's
            // "Unquoted argument" warning, parenthesised too [observed: nvse-fourth,
            // `SetFunctionValue (GetFOSEVersion)` returns the text "GetFOSEVersion" in xNVSE
            // 6.4.9].
            None if self.in_arg_any => {
                self.diag("unquoted_argument", false, name.to_string());
                Some(XExpr::Str { s: name.to_string() })
            }
            // A call line starting with an unknown name, in a mod's source compiled by the
            // engine: a call of that name, reporting "not built" when it runs
            // ([`super::super::Seed::unknown_calls`]) [observed: TaleOfTwoWastelands.esm
            // SCPT 001880 calls TTW_EnableRadioFix and InitRockItLauncher, plugin functions
            // the wiki does not document].
            None if self.unknown_calls && self.call_line && !self.in_args && head.is_none() => {
                self.diag("unknown_function", false, name.to_string());
                let f = XFn { func: FuncId::Name(name.to_string()), params: None, opcode: None, aware: false };
                let args = self.xargs(p, &f)?;
                Some(XExpr::Call { call: XCall { on: None, func: f.func, args } })
            }
            None => self.xerr("unresolved_name", name.to_string()),
        }
    }

    /// The function is xNVSE's (or FOSE's) and its parameter `k` is a plain string, not a
    /// format string.
    fn plain_string_param(&self, f: &XFn, k: usize) -> bool {
        let FuncId::Name(n) = &f.func else { return false };
        let Some(u) = self.table.unnumbered.get(&n.to_ascii_lowercase()) else { return false };
        let formatted = u.params.iter().any(|p| p.name.starts_with("...") || p.name.to_ascii_lowercase().contains("format"));
        // The wiki's type is "string", not "string_var" (Sv_Erase takes the variable).
        matches!(u.origin.as_str(), "NVSE" | "FOSE1") && !formatted && u.params.get(k).is_some_and(|p| p.kind == "string" && p.wiki_type.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("string")))
    }

    /// The function's parameter `k` takes a form, or is named for a global (the base
    /// game's GetGlobalValue included).
    fn form_param(&self, f: &XFn, k: usize) -> bool {
        let params = match (&f.func, f.opcode) {
            (_, Some(op)) => self.table.get(op).map(|d| &d.params),
            (FuncId::Name(n), None) => self.table.unnumbered.get(&n.to_ascii_lowercase()).map(|u| &u.params),
            _ => None,
        };
        params.and_then(|ps| ps.get(k)).is_some_and(|p| matches!(p.kind.as_str(), "form" | "reference") || p.name.to_ascii_lowercase().contains("global"))
    }

    fn xargs(&mut self, p: &mut P<'_>, f: &XFn) -> Option<Vec<XExpr>> {
        let mut args = Vec::new();
        loop {
            if f.params.is_some_and(|n| args.len() >= n) {
                break;
            }
            let mut comma = false;
            if !args.is_empty() && p.peek().is_some_and(|t| t.kind == XKind::Comma) {
                p.next();
                comma = true;
            }
            // A `-` starts the first argument, or one after a comma (`Rand -5, -1`), or a
            // signed number after a space (`GetMaxOf -1 -5`).
            let minus = p.peek().is_some_and(|t| t.is(XKind::Op, "-"));
            let minus_first = f.params != Some(0) && minus && (comma || args.is_empty() || p.peek().is_some_and(|t| t.sign));
            if !starts_operand(p.peek()) && !minus_first {
                break;
            }
            // A string variable given by name to an xNVSE function's plain string
            // parameter, on a line that is that call alone, is its name as text [observed: nvse-sixth,
            // `SetStringIniSetting "sLanguage:General" sText` set the text "sText";
            // documented: SetNameEx, "does not accept string variables directly … must be
            // converted", `$sv`; hypothesis: JIP's and the other plugins' functions differ].
            if self.call_line && !self.in_args && !f.aware && !self.in_paren && self.plain_string_param(f, args.len()) {
                if let Some(t) = p.peek().filter(|t| t.kind == XKind::Word && !t.text.contains('.')).cloned() {
                    let string_var = self.var(&t.text).is_some_and(|v| matches!(&v.ty, crate::unit::VarType::Other(s) if s == "string_var"));
                    let alone = !p.peek2().is_some_and(|n| matches!(n.kind, XKind::LBracket | XKind::Op));
                    if string_var && alone {
                        p.next();
                        args.push(XExpr::Str { s: t.text.clone() });
                        continue;
                    }
                }
            }
            // A global's name given alone to a plugin function's form parameter (or one
            // named for a global) is the global, not its value [observed: plugins3,
            // `GetGlobalValue TimeScale` reads 30 and `SetGlobalValue TimeScale 31` sets
            // it in the original].
            if self.form_param(f, args.len()) {
                if let Some(t) = p.peek().filter(|t| t.kind == XKind::Word && !t.text.contains('.')).cloned() {
                    let alone = !p.peek2().is_some_and(|n| matches!(n.kind, XKind::LBracket | XKind::Op));
                    let global = self.var(&t.text).is_none().then(|| self.form(&t.text)).flatten().filter(|fi| &fi.sig == b"GLOB");
                    if let (true, Some(fi)) = (alone, global) {
                        p.next();
                        let r = self.ref_index(crate::unit::RefSlot::Form(fi.form), &t.text);
                        args.push(XExpr::Ref { r: RefIndex(r) });
                        continue;
                    }
                }
            }
            let enc = f.opcode.map(|o| self.table.param_enc(o, args.len())).unwrap_or(ParamEnc::Value);
            let min = if f.aware {
                ARG_OVERRIDE
            } else if self.co {
                ARG_IN_OVERRIDE
            } else {
                ARG
            };
            let was = std::mem::replace(&mut self.in_args, true);
            let was_any = std::mem::replace(&mut self.in_arg_any, true);
            let a = self.xarg(p, &enc, min);
            self.in_args = was;
            self.in_arg_any = was_any;
            args.push(a?);
        }
        Some(args)
    }

    /// One argument. A name in a parameter that takes an enumeration, an axis or a string
    /// is that, when nothing follows it but the next argument.
    fn xarg(&mut self, p: &mut P<'_>, enc: &ParamEnc, min: u8) -> Option<XExpr> {
        // Outside an override block an argument is an operand, so what follows the name
        // (`Health + 50`) is not part of it.
        let single = p.peek().is_some_and(|t| matches!(t.kind, XKind::Word | XKind::Str))
            && !p.peek2().is_some_and(|n| n.kind == XKind::LBracket || n.is(XKind::Op, "->"));
        if single && *enc != ParamEnc::Value {
            let t = p.next()?;
            return match enc {
                ParamEnc::Enum(kind) => {
                    let code = t.text.parse::<u16>().ok().or_else(|| self.enums.get(kind).and_then(|m| m.get(&t.text.to_ascii_lowercase())).copied());
                    match code {
                        Some(code) => Some(XExpr::Enum { code }),
                        None => self.xerr("unknown_enum_name", format!("{kind}: {}", t.text)),
                    }
                }
                ParamEnc::Axis => match t.text.to_ascii_uppercase().as_str() {
                    a @ ("X" | "Y" | "Z") => Some(XExpr::Axis { c: a.chars().next().unwrap_or('X') }),
                    _ => self.xerr("bad_axis", t.text.clone()),
                },
                _ => Some(XExpr::Str { s: t.text.clone() }),
            };
        }
        self.xexpr(p, min)
    }
}

fn val_expr(v: Val) -> XExpr {
    match v {
        Val::Num(text) => XExpr::Num { value: text.parse().unwrap_or(0.0), text },
        Val::Local { index, letter } => XExpr::Var { var: VarRef { on: None, var: VarSlot::Local { index, letter } }, form: false },
        Val::Global(r) => XExpr::Var { var: VarRef { on: None, var: VarSlot::Global { global: RefIndex(r) } }, form: false },
        Val::Remote { on, index, letter } => XExpr::Var { var: VarRef { on: Some(RefIndex(on)), var: VarSlot::Local { index, letter } }, form: false },
        Val::Ref(r) => XExpr::Ref { r: RefIndex(r) },
    }
}
