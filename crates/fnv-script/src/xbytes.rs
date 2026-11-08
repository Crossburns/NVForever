//! The script extender's bytecode as the GECK writes it (docs/spec-notes/ext-bytecode.md):
//! read into the same tree NVForever's compiler makes ([`XExpr`]), so a unit whose bytes
//! call extender functions runs as compiled, without its source.
//!
//! What every layout and code here rests on [observed]: the compile cases of
//! ext-bytecode.md, and High and Dry's 28,018 compiled units lined up with their source
//! text (2026-11-08): a reading uses every byte of an expression and leaves one value;
//! operator bytes named by the symbols on the source lines their statements came from.
//! Operator bytes no compiled script shows are not read (the statement stays raw).

use crate::decode::Rd;
use crate::ir::{Arg, FuncId, RefIndex, VarLetter, VarRef, VarSlot, XAssign, XCall, XExpr, XForEach, XOp, XUnary};
use crate::table::{FunctionTable, ParamEnc};

/// Statement opcodes of the GECK's extender statements (ext-bytecode.md §1).
pub mod op {
    pub const LET: u16 = 0x1539;
    pub const EVAL: u16 = 0x153A;
    pub const WHILE: u16 = 0x153B;
    pub const LOOP: u16 = 0x153C;
    pub const FOREACH: u16 = 0x153D;
    pub const CONTINUE: u16 = 0x153E;
    pub const BREAK: u16 = 0x153F;
    /// Written right after `Begin _GameMode` and right before its `End`.
    pub const MARK_BEGIN: u16 = 0x154D;
    pub const MARK_END: u16 = 0x154E;
    /// `Begin Function {…}` (ext-bytecode.md §4).
    pub const FUNCTION_EVENT: u16 = 0x000D;
    /// The extenders' opcodes start here [documented: NVSE Opcode Base].
    pub const FIRST_EXTENDER: u16 = 0x1400;
}

std::thread_local! {
    /// Lambdas met so far in the unit being read (their `index`), and how deep the reading
    /// is inside lambdas' own scripts.
    static LAMBDAS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Called by `decode::decode` around each unit: a unit's lambdas are numbered from 0.
pub(crate) fn enter() {
    DEPTH.with(|d| {
        if d.get() == 0 {
            LAMBDAS.with(|l| l.set(0));
        }
        d.set(d.get() + 1);
    });
}

pub(crate) fn leave() {
    DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
}

/// `Call`'s opcode: its bytes hold the function, then its arguments (ext-bytecode.md §1).
pub const CALL: u16 = 0x1545;

/// One operator byte [observed: the symbol of the source line, ext-bytecode.md §3a].
enum Oper {
    Bin(XOp),
    Assign(XAssign),
    Unary(XUnary),
    /// `<-`, the head of a `foreach`.
    Arrow,
    /// A call on the value below it (`a[0].GetAV`).
    CallOn,
}

fn oper(b: u8) -> Option<Oper> {
    use Oper::*;
    Some(match b {
        0x00 => Assign(XAssign::Set),
        0x01 => Bin(XOp::Or),
        0x02 => Bin(XOp::And),
        0x03 => Bin(XOp::Slice),
        0x04 => Bin(XOp::Eq),
        0x05 => Bin(XOp::Ne),
        0x06 => Bin(XOp::Gt),
        0x07 => Bin(XOp::Lt),
        0x08 => Bin(XOp::Ge),
        0x09 => Bin(XOp::Le),
        0x0B => Bin(XOp::BitAnd),
        0x0C => Bin(XOp::Shl),
        0x0E => Bin(XOp::Add),
        0x0F => Bin(XOp::Sub),
        0x10 => Bin(XOp::Mul),
        0x11 => Bin(XOp::Div),
        0x12 => Bin(XOp::Mod),
        0x13 => Bin(XOp::Pow),
        0x14 => Unary(XUnary::Neg),
        0x15 => Unary(XUnary::Not),
        0x18 => Bin(XOp::Subscript),
        0x1A => Arrow,
        0x1B => Unary(XUnary::Stringize),
        0x1C => Assign(XAssign::Add),
        0x1D => Assign(XAssign::Mul),
        0x1E => Assign(XAssign::Div),
        0x20 => Assign(XAssign::Sub),
        0x21 => Unary(XUnary::Numericize),
        0x22 => Unary(XUnary::Unbox),
        0x24 => Bin(XOp::Pair),
        0x28 => CallOn,
        _ => return None,
    })
}

/// A value on the reading stack: an expression, or the two sides of `<-`.
enum Item {
    E(XExpr),
    Arrow(XExpr, XExpr),
    /// A call waiting for the value it is called on (`x` token, then 0x28).
    Pending(XCall),
}

fn num(v: f64) -> XExpr {
    let text = if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") };
    XExpr::Num { value: v, text }
}

/// The function a compiled call names: the table's by opcode, else the extender's by the
/// name the original game gives that opcode.
fn func(table: &FunctionTable, opcode: u16) -> Option<FuncId> {
    if table.get(opcode).is_some() {
        return Some(FuncId::Opcode(opcode));
    }
    table.opcode_names.get(&opcode).map(|n| FuncId::Name(n.clone()))
}

/// An extender expression: a postfix token stream using all of `b`, one value.
pub(crate) fn expr(table: &FunctionTable, b: &[u8]) -> Option<XExpr> {
    match items(table, b)? {
        Item::E(e) => Some(e),
        _ => None,
    }
}

fn items(table: &FunctionTable, b: &[u8]) -> Option<Item> {
    let mut r = Rd::new(b);
    let mut st: Vec<Item> = Vec::new();
    let pop = |st: &mut Vec<Item>| match st.pop()? {
        Item::E(e) => Some(e),
        _ => None,
    };
    while r.left() > 0 {
        let t = r.u8()?;
        match t {
            // V: type, u16 reference (0: own), u16 index.
            0x56 => {
                let ty = r.u8()?;
                let on = r.u16()?;
                let index = r.u16()?;
                let var = VarRef { on: (on != 0).then_some(RefIndex(on)), var: VarSlot::Local { index, letter: VarLetter::Float } };
                st.push(Item::E(XExpr::Var { var, form: ty == 4 }));
            }
            0x42 => st.push(Item::E(num(f64::from(r.u8()?)))),
            0x49 => st.push(Item::E(num(f64::from(r.u16()? as i16)))),
            0x4C => st.push(Item::E(num(f64::from(r.i32()?)))),
            0x5A => st.push(Item::E(num(r.f64()?))),
            0x53 => {
                let n = r.u16()? as usize;
                st.push(Item::E(XExpr::Str { s: fnv_util::text::decode(r.take(n)?) }));
            }
            0x52 => st.push(Item::E(XExpr::Ref { r: RefIndex(r.u16()?) })),
            0x47 => {
                let g = RefIndex(r.u16()?);
                st.push(Item::E(XExpr::Var { var: VarRef { on: None, var: VarSlot::Global { global: g } }, form: false }));
            }
            // X / x: u16 calling reference, u16 opcode, u16 parameter bytes (counting
            // themselves), parameters.
            0x58 | 0x78 => {
                let on = r.u16()?;
                let opcode = r.u16()?;
                let n = r.u16()? as usize;
                let params = r.take(n.checked_sub(2)?)?;
                let call = XCall {
                    on: (on != 0).then(|| Box::new(XExpr::Ref { r: RefIndex(on) })),
                    func: func(table, opcode)?,
                    args: args(table, opcode, params)?,
                };
                st.push(if t == 0x78 { Item::Pending(call) } else { Item::E(XExpr::Call { call }) });
            }
            // F: a lambda: u32 length, then a whole compiled script whose `Begin Function`
            // block is the lambda; its variables are this script's [documented: Lambda;
            // observed: ext-bytecode.md §4].
            0x46 => {
                let n = r.u32()? as usize;
                let inner = r.take(n)?;
                let index = LAMBDAS.with(|l| {
                    let i = l.get();
                    l.set(i + 1);
                    i
                });
                let d = crate::decode::decode(inner, table);
                if d.report.raw_total() > 0 || !d.report.issues.is_empty() {
                    return None;
                }
                let block = d.program.blocks.into_iter().find(|b| b.event == crate::ir::FUNCTION_BLOCK)?;
                let params = block
                    .params
                    .unwrap_or_default()
                    .into_iter()
                    .map(|a| match a {
                        Arg::Var(v) => Some(v),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                st.push(Item::E(XExpr::Lambda { params, body: block.body, value: None, index }));
            }
            _ => match oper(t)? {
                Oper::Bin(op) => {
                    let r_ = pop(&mut st)?;
                    let l = pop(&mut st)?;
                    st.push(Item::E(XExpr::Binary { op, l: Box::new(l), r: Box::new(r_) }));
                }
                Oper::Assign(op) => {
                    let value = pop(&mut st)?;
                    let target = pop(&mut st)?;
                    st.push(Item::E(XExpr::Assign { op, target: Box::new(target), value: Box::new(value) }));
                }
                Oper::Unary(op) => {
                    let e = pop(&mut st)?;
                    st.push(Item::E(XExpr::Unary { op, e: Box::new(e) }));
                }
                Oper::Arrow => {
                    let source = pop(&mut st)?;
                    let var = pop(&mut st)?;
                    st.push(Item::Arrow(var, source));
                }
                Oper::CallOn => {
                    let Item::Pending(mut call) = st.pop()? else { return None };
                    call.on = Some(Box::new(pop(&mut st)?));
                    st.push(Item::E(XExpr::Call { call }));
                }
            },
        }
    }
    if st.len() == 1 { st.pop() } else { None }
}

/// The arguments of a compiled extender call: the expression form (u8 count, then each an
/// u16 length counting itself and an expression) when it uses the bytes exactly, else the
/// base game's parameter block (u16 count, tagged values) read by the function's
/// parameter kinds.
pub(crate) fn args(table: &FunctionTable, opcode: u16, b: &[u8]) -> Option<Vec<XExpr>> {
    if b.is_empty() {
        return Some(Vec::new());
    }
    if opcode == CALL {
        return call_args(table, b);
    }
    expr_args(table, b).or_else(|| typed_args(table, opcode, b))
}

/// `Call`'s bytes: `01`, u16 length, the function (an expression), then the arguments in
/// the expression form, or none [observed: ext-bytecode.md cases 18, 19; High and Dry].
fn call_args(table: &FunctionTable, b: &[u8]) -> Option<Vec<XExpr>> {
    let mut r = Rd::new(b);
    if r.u8()? != 1 {
        return None;
    }
    let len = r.u16()? as usize;
    let mut out = vec![expr(table, r.take(len.checked_sub(2)?)?)?];
    if r.left() > 0 {
        out.extend(expr_args(table, r.rest())?);
    }
    Some(out)
}

/// u8 count, then that many (u16 length counting itself, expression).
pub(crate) fn expr_args(table: &FunctionTable, b: &[u8]) -> Option<Vec<XExpr>> {
    let mut r = Rd::new(b);
    let n = r.u8()?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let len = r.u16()? as usize;
        out.push(expr(table, r.take(len.checked_sub(2)?)?)?);
    }
    (r.left() == 0).then_some(out)
}

/// How parameter `k` of `opcode` is stored, from the table (numbered functions) or the
/// catalogue's parameter kinds (the extenders' functions, named by opcode).
fn enc(table: &FunctionTable, opcode: u16, k: usize) -> ParamEnc {
    if table.get(opcode).is_some() {
        return table.param_enc(opcode, k);
    }
    let kind = table
        .opcode_names
        .get(&opcode)
        .and_then(|n| table.unnumbered.get(&n.to_ascii_lowercase()))
        .and_then(|u| u.params.get(k))
        .map(|p| p.kind.as_str());
    match kind {
        Some("string") => ParamEnc::String,
        Some("axis") => ParamEnc::Axis,
        Some("actor_value") => ParamEnc::Enum("actor_value".into()),
        _ => ParamEnc::Value,
    }
}

/// u16 count, then typed parameters, read so that they use the bytes exactly: each takes
/// its catalogue form, else a tagged value or a string (a string parameter often holds a
/// string variable, `s` + index; a format string takes values past the listed
/// parameters) [observed]. A parameter `FF FF` + u16 length (counting itself) is an
/// expression, or `FF FF 01` + u16 length (one function's form) [observed].
fn typed_args(table: &FunctionTable, opcode: u16, b: &[u8]) -> Option<Vec<XExpr>> {
    let mut r = Rd::new(b);
    let n = r.u16()? as usize;
    let mut out = Vec::with_capacity(n);
    let mut failed = std::collections::HashSet::new();
    typed_from(table, opcode, &mut r, 0, n, &mut out, &mut failed).then_some(out)
}

fn typed_from(
    table: &FunctionTable,
    opcode: u16,
    r: &mut Rd<'_>,
    k: usize,
    n: usize,
    out: &mut Vec<XExpr>,
    failed: &mut std::collections::HashSet<(usize, usize)>,
) -> bool {
    if k == n {
        return r.left() == 0;
    }
    if failed.contains(&(r.i, k)) {
        return false;
    }
    let at = r.i;
    let declared = enc(table, opcode, k);
    let mut tries: Vec<u8> = vec![0];
    tries.extend(match declared {
        ParamEnc::String => [1u8, 2, 3],
        _ => [2u8, 3, 1],
    });
    for t in tries {
        r.i = at;
        let a = match t {
            0 => inline_expr(table, r).or_else(|| {
                r.i = at;
                match &declared {
                    ParamEnc::Axis => r.u8().filter(|c| matches!(c, b'X' | b'Y' | b'Z')).map(|c| XExpr::Axis { c: c as char }),
                    ParamEnc::Enum(_) => r.u16().map(|code| XExpr::Enum { code }),
                    _ => None,
                }
            }),
            1 => (|| {
                let len = r.u16()? as usize;
                Some(XExpr::Str { s: fnv_util::text::decode(r.take(len)?) })
            })(),
            2 => crate::decode::tagged_value(r, true).and_then(from_arg),
            _ => crate::decode::tagged_value(r, false).and_then(from_arg),
        };
        if let Some(a) = a {
            out.push(a);
            if typed_from(table, opcode, r, k + 1, n, out, failed) {
                return true;
            }
            out.pop();
        }
    }
    r.i = at;
    failed.insert((at, k));
    false
}

/// `FF FF` + u16 length (counting itself) + expression, or `FF FF 01` + u16 length.
fn inline_expr(table: &FunctionTable, r: &mut Rd<'_>) -> Option<XExpr> {
    if r.peek() != Some(0xFF) || r.peek_at(1) != Some(0xFF) {
        return None;
    }
    let at = r.i;
    r.i += 2;
    let a = (|| {
        let len = r.u16()? as usize;
        expr(table, r.take(len.checked_sub(2)?)?)
    })();
    if a.is_some() {
        return a;
    }
    r.i = at + 2;
    if r.u8()? != 1 {
        r.i = at;
        return None;
    }
    let len = r.u16()? as usize;
    let e = r.take(len.checked_sub(2)?).and_then(|b| expr(table, b));
    if e.is_none() {
        r.i = at;
    }
    e
}

/// A base-game argument as a tree value.
pub(crate) fn from_arg(a: Arg) -> Option<XExpr> {
    Some(match a {
        Arg::Int(v) | Arg::Int32(v) => num(f64::from(v)),
        Arg::Float(v) => num(v),
        Arg::Ref(r) => XExpr::Ref { r },
        Arg::Var(var) => XExpr::Var { var, form: false },
        Arg::Enum(code) => XExpr::Enum { code },
        Arg::Axis(c) => XExpr::Axis { c },
        Arg::String(s) => XExpr::Str { s },
        Arg::FormatVars(_) | Arg::Raw(_) => return None,
    })
}

/// The body of a statement in the expression form: u8 count 1, u16 length, expression.
pub(crate) fn single(table: &FunctionTable, b: &[u8]) -> Option<XExpr> {
    let mut v = expr_args(table, b)?;
    (v.len() == 1).then(|| v.pop()).flatten()
}

/// `foreach`'s head: the expression ends in `<-` (ext-bytecode.md §1).
pub(crate) fn foreach_head(table: &FunctionTable, b: &[u8]) -> Option<XForEach> {
    let mut r = Rd::new(b);
    if r.u8()? != 1 {
        return None;
    }
    let len = r.u16()? as usize;
    let e = r.take(len.checked_sub(2)?)?;
    if r.left() != 0 {
        return None;
    }
    match items(table, e)? {
        Item::Arrow(var, source) => Some(XForEach { var, source }),
        _ => None,
    }
}

/// `Begin Function {…}`'s parameter block: `01`, count, then each a u16 variable index and
/// a type byte, then `00` [observed: ext-bytecode.md §4, `01 02 01 00 01 02 00 01 00` for
/// two parameters, `01 00 00` for none].
pub(crate) fn function_params(b: &[u8]) -> Option<Vec<Arg>> {
    let mut r = Rd::new(b);
    if r.u8()? != 1 {
        return None;
    }
    let n = r.u8()?;
    let mut out = Vec::new();
    for _ in 0..n {
        let index = r.u16()?;
        r.u8()?;
        out.push(Arg::Var(VarRef { on: None, var: VarSlot::Local { index, letter: VarLetter::Float } }));
    }
    if r.u8()? != 0 || r.left() != 0 {
        return None;
    }
    Some(out)
}
