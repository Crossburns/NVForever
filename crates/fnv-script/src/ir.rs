//! The decoded form of a compiled script (docs/design/layer5.md §3.2, the contract between
//! the reader and the interpreter). Byte layouts behind every node are described in
//! docs/spec-notes/scripts.md §2; the tags there say what is observed and what is not.
//!
//! The tree keeps everything the bytes say, in order: flattening it statement by
//! statement gives back the compiled statement sequence. Nothing is dropped. A
//! parameter or token the decoder cannot explain is kept as `Raw` and counted.

use serde::{Deserialize, Serialize};

/// A 1-based index into the unit's reference list (`ScriptUnit::refs`), as stored in the
/// bytecode after `r`, `Z`, `G` and the `0x001C` prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RefIndex(pub u16);

/// Type letter of a variable slot as the bytecode spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VarLetter {
    /// `s` (0x73): a variable declared `short`/`int`/`long` (SLSD type Integer).
    Short,
    /// `f` (0x66): a variable declared `float` or `ref` (SLSD type FloatOrRef).
    Float,
}

impl VarLetter {
    pub fn from_byte(b: u8) -> Option<VarLetter> {
        match b {
            b's' => Some(VarLetter::Short),
            b'f' => Some(VarLetter::Float),
            _ => None,
        }
    }
    pub fn byte(self) -> u8 {
        match self {
            VarLetter::Short => b's',
            VarLetter::Float => b'f',
        }
    }
}

/// Where a variable lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "slot", rename_all = "snake_case")]
pub enum VarSlot {
    /// A local of the script instance (`s`/`f` + u16 index); with `VarRef::on`, a local of
    /// the script attached to that reference or quest.
    Local { index: u16, letter: VarLetter },
    /// A global variable (`G` + u16): the reference list entry names the GLOB record.
    Global { global: RefIndex },
}

/// A variable read or written by a statement or expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VarRef {
    /// `r` + u16 before the slot: the variable belongs to the script of that reference
    /// (`Quest.var`, `SomeREF.var`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub on: Option<RefIndex>,
    pub var: VarSlot,
}

/// A function call: a statement (opcode ≥ 0x1000) or an `X` token in an expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    /// The reference the call runs on (`Ref.Func`): the `0x001C` prefix of a statement,
    /// or `r` + u16 before `X` in an expression.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub on: Option<RefIndex>,
    pub opcode: u16,
    /// `false` when the call has no parameter block at all (body length 0); `true` when it
    /// has a u16 parameter count, even if that count is 0.
    pub has_params: bool,
    pub args: Vec<Arg>,
}

/// One function or event parameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Arg {
    /// `n` + i32.
    Int(i32),
    /// `z` + f64.
    Float(f64),
    /// `r` + u16: a reference list entry (a form, or a local `ref` variable via SCRV).
    Ref(RefIndex),
    /// `s`/`f`/`G` + u16, or `r` + u16 + `s`/`f` + u16.
    Var(VarRef),
    /// A bare u16 code (actor value, animation group, sex, misc stat, critical stage).
    Enum(u16),
    /// One ASCII byte `X`, `Y` or `Z`.
    Axis(char),
    /// u16 length + that many bytes (Windows-1252), no terminator.
    String(String),
    /// The variables of a formatted message: u16 count + that many values
    /// (`ShowMessage`, `ShowWarning`).
    FormatVars(Vec<Arg>),
    /// An untagged i32 that follows a message's format variables (`ShowMessage` duration).
    Int32(i32),
    /// Bytes the decoder could not explain (everything from that point to the end of the
    /// parameter block). Counted in the decode report, never dropped.
    Raw(#[serde(with = "hex_bytes")] Vec<u8>),
}

/// An expression operator, stored as ASCII text in the bytecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    /// `~`: unary minus (the compiler writes `-1` as `1 ~`).
    Neg,
}

impl Op {
    pub fn parse(s: &str) -> Option<Op> {
        Some(match s {
            "==" => Op::Eq,
            "!=" => Op::Ne,
            "<" => Op::Lt,
            "<=" => Op::Le,
            ">" => Op::Gt,
            ">=" => Op::Ge,
            "&&" => Op::And,
            "||" => Op::Or,
            "+" => Op::Add,
            "-" => Op::Sub,
            "*" => Op::Mul,
            "/" => Op::Div,
            "%" => Op::Mod,
            "~" => Op::Neg,
            _ => return None,
        })
    }
    pub fn symbol(self) -> &'static str {
        match self {
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::And => "&&",
            Op::Or => "||",
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Mod => "%",
            Op::Neg => "~",
        }
    }
    /// Number of operands taken from the stack.
    pub fn arity(self) -> usize {
        if self == Op::Neg {
            1
        } else {
            2
        }
    }
}

/// One postfix token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Token {
    /// A number literal, stored as the ASCII text of the source (`19.50`, `.5`, `3`).
    Num { value: f64, text: String },
    Var { var: VarRef },
    /// `Z` + u16: a reference list entry used as a value (`set rDoor to SomeDoorREF`).
    Ref { r: RefIndex },
    Call { call: Call },
    Op { op: Op },
    /// An extended expression (`if eval …`), NVForever's own form
    /// (docs/spec-notes/script-compiler.md §8).
    X { e: XExpr },
    /// A token the decoder could not explain; the rest of the expression is kept here.
    Raw {
        #[serde(with = "hex_bytes")]
        bytes: Vec<u8>,
    },
}

/// A postfix (reverse Polish) token list, evaluated left to right with a stack.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Expr(pub Vec<Token>);

/// One branch of an `If` chain: `If`/`ElseIf` condition and its statements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arm {
    pub cond: Expr,
    pub body: Vec<Stmt>,
}

/// A branch marker that has no matching `If` (or an `EndIf` with nothing open). The
/// compiler accepts these (docs/spec-notes/scripts.md §2.6); they are kept in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Marker {
    Else,
    ElseIf,
    EndIf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "stmt", rename_all = "snake_case")]
pub enum Stmt {
    Set {
        target: VarRef,
        expr: Expr,
    },
    /// `If` … (`ElseIf` …)* (`Else` …)? `EndIf`, nesting rebuilt from the markers and
    /// checked against their skip counts.
    If {
        arms: Vec<Arm>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        otherwise: Option<Vec<Stmt>>,
    },
    Return,
    Call {
        call: Call,
    },
    /// A stray branch marker (see [`Marker`]), with its stored skip count and, for
    /// `ElseIf`, its condition.
    Unmatched {
        marker: Marker,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        skip: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        cond: Option<Expr>,
    },
    /// `let <assignment>` and the Let macro (`x = 1`, `x += 1`): an extended expression
    /// whose top is an assignment (docs/spec-notes/script-compiler.md §8).
    Let { expr: XExpr },
    /// An extended expression run for what it does: a call with inline expressions as
    /// arguments (`SetAV Health (GetAV Health + 50)`), an extender function as a statement
    /// [documented: Inline Expression].
    Eval { expr: XExpr },
    /// `while <condition>` … `loop` [documented: While].
    While { cond: XExpr, body: Vec<Stmt> },
    /// `foreach <var> <- <source>` … `loop` [documented: ForEach].
    ForEach { head: XForEach, body: Vec<Stmt> },
    /// `Label n` and `GoTo n` [documented: Label; GoTo]: a GoTo goes on after the label of
    /// that number in its statement list or one around it.
    Label { n: i32 },
    GoTo { n: i32 },
    /// `break`: leave the innermost loop [documented: While; ForEach].
    Break,
    /// `continue`: the innermost loop's next round [documented: While; ForEach].
    Continue,
    /// A statement whose opcode or body the decoder could not explain.
    Raw {
        opcode: u16,
        #[serde(with = "hex_bytes")]
        body: Vec<u8>,
    },
}

/// A `Begin <event> [params]` … `End` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// Event code (docs/spec-notes/script-functions.md §3.3).
    pub event: u16,
    /// Event parameters, encoded like function parameters; `None` when the Begin
    /// statement has no parameter block.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<Vec<Arg>>,
    pub body: Vec<Stmt>,
}

/// A decoded unit.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Program {
    /// The unit starts with a `ScriptName` statement (`SCPT` scripts do; result scripts
    /// don't).
    pub script_name: bool,
    pub blocks: Vec<Block>,
    /// Statements outside any block: the whole body of a result script (quest stage,
    /// dialogue, package, terminal, perk, patrol).
    pub top: Vec<Stmt>,
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(b: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&fnv_util::sha256::to_hex(b))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() % 2 != 0 {
            return Err(serde::de::Error::custom("odd hex length"));
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(serde::de::Error::custom))
            .collect()
    }
}

impl Program {
    /// Every statement in compiled order (block bodies included, `Begin`/`End` not).
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Stmt)) {
        fn rec<'a>(s: &'a [Stmt], f: &mut impl FnMut(&'a Stmt)) {
            for st in s {
                f(st);
                match st {
                    Stmt::If { arms, otherwise } => {
                        for a in arms {
                            rec(&a.body, f);
                        }
                        if let Some(o) = otherwise {
                            rec(o, f);
                        }
                    }
                    Stmt::While { body, .. } | Stmt::ForEach { body, .. } => rec(body, f),
                    _ => {}
                }
            }
        }
        rec(&self.top, f);
        for b in &self.blocks {
            rec(&b.body, f);
        }
    }
}

// ---------------------------------------------------------------------------- extender

/// A function in an extended expression: by opcode when the function table gives it one,
/// else by catalogue name (the extender's functions have no opcodes here,
/// docs/design/extender-compat.md §5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", content = "id", rename_all = "snake_case")]
pub enum FuncId {
    Opcode(u16),
    Name(String),
}

/// A call inside an extended expression. Its arguments are expressions themselves
/// [documented: Inline Expression; Script Compiler Override].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XCall {
    /// The calling reference: a reference list entry, or any expression (`a[0].GetAV`).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub on: Option<Box<XExpr>>,
    pub func: FuncId,
    pub args: Vec<XExpr>,
}

/// Operators of the extender's expressions [documented: NVSE Expressions].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XOp {
    Or,
    And,
    Slice,
    Pair,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    BitOr,
    BitAnd,
    Shl,
    Shr,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Subscript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XUnary {
    Neg,
    /// `$`: to a string.
    Stringize,
    /// `#`: a string to a number.
    Numericize,
    /// `*`: an array's first element (or its `"value"` key).
    Unbox,
    /// `&`: a one-element array.
    Box,
    Not,
}

/// `:=` and the compound assignments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XAssign {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    BitOr,
    BitAnd,
    Mod,
}

/// The head of a `foreach` loop: the loop variable and what it walks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct XForEach {
    pub var: XExpr,
    pub source: XExpr,
}

/// An extended expression, as a tree (the vanilla postfix [`Expr`] cannot hold strings,
/// arrays or calls as arguments). docs/spec-notes/ext-syntax.md.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "x", rename_all = "snake_case")]
pub enum XExpr {
    /// A number, with its text as written.
    Num { value: f64, text: String },
    Str { s: String },
    /// `form`: the variable was declared `ref`, so its unset 0 reads as no form (the
    /// record's variable list cannot tell `ref` from `float`).
    Var {
        var: VarRef,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        form: bool,
    },
    /// A reference list entry used as a value.
    Ref { r: RefIndex },
    /// A name in a parameter that takes an enumeration (an actor value), as its code.
    Enum { code: u16 },
    /// An axis parameter (`X`, `Y`, `Z`).
    Axis { c: char },
    Call { call: XCall },
    Unary { op: XUnary, e: Box<XExpr> },
    Binary { op: XOp, l: Box<XExpr>, r: Box<XExpr> },
    /// `l -> key`: a string map's element.
    Member { l: Box<XExpr>, key: String },
    Assign { op: XAssign, target: Box<XExpr>, value: Box<XExpr> },
    /// A lambda [documented: Lambda]: `(begin function {params} … end)` (its statements in
    /// `body`) or `{params} => expr` (`value`, returned as SetFunctionValue would). Its
    /// parameters and every variable it names are the enclosing script's.
    Lambda {
        params: Vec<VarRef>,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        body: Vec<Stmt>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        value: Option<Box<XExpr>>,
        /// Its number in its unit, in the order the compiler met the lambdas: with the
        /// unit, what makes it the same lambda each time it is made (#1088).
        #[serde(default)]
        index: u32,
    },
}

/// The event code NVForever gives a `Begin Function {…}` block (a user-defined function,
/// [documented: User Defined Function]); the vanilla codes end far below it (the
/// catalogue's `begin_blocks`). Its parameter block lists the parameter variables.
pub const FUNCTION_BLOCK: u16 = 0x7F00;
