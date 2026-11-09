# Spec note: extender bytecode, as the GECK writes it

What the GECK with xNVSE 6.4.9 loaded writes into `SCDA` for extender syntax, read from
our own test scripts: the 22 compile cases of tools/capture/ext-compile/, compiled by the
owner into a private test plugin (2026-10-25) and lined up statement by statement with
their source (`SCTX`), the same method that named the base game's opcodes
(script-functions.md §3). This is the G2 evidence of the extender plan
([extender-compat.md](../design/extender-compat.md) §4.1 item 3: bytes the editor wrote
for our own scripts). Every fact is tagged `[observed: case NN]`; a reading of what a byte
*means* beyond what the cases show is `[hypothesis]`. The plugin and full hex dumps stay
private (`C:\fnv-private\ext\`); the excerpts here are from our own scripts.

Statement framing is the base game's: opcode u16, body length u16, body; `0x001C` +
u16 is a calling-reference prefix (script-functions.md §3.2).

## 1. Statement opcodes

| Opcode | Statement | Body | Evidence |
|---|---|---|---|
| 0x1539 | `let …` and the let macro (`x = 10`, `x += 1`) | `01`, u16 length, extender expression (§3) | [observed: cases 01–05, 07, 12–14, 16, 18, 19, 21, 23] |
| 0x153B | `while <expr>` | u32 jump, then `01`, u16 length, expression | [observed: cases 07–09] |
| 0x153C | `loop` | empty | [observed: cases 07–11] |
| 0x153D | `foreach <var> <- <expr>` | u32 jump, then `01`, u16 length, expression ending in the `<-` operator | [observed: cases 10, 11] |
| 0x153E | `continue` | empty | [observed: case 09] |
| 0x153F | `break` | empty | [observed: case 09] |
| 0x1541 | `Print <expr>` | `01`, u16 length, expression | [observed: cases 07–09] |
| 0x1545 | `Call …` as a statement | `01`, u16 length, expression (the callee), then the arguments | [observed: cases 18, 19] |
| 0x1546 | `SetFunctionValue <expr>` | `01`, u16 length, expression | [observed: cases 15, 17] |
| 0x154D / 0x154E | emitted right after `Begin _GameMode` / right before its `End` | empty | [observed: case 23] |

- The u16 after `01` counts the bytes that follow **including itself** (case 01: `01 0b
  00` then 9 more bytes) [observed: cases 01, 07]. `01` is [hypothesis: an argument
  count].
- `while`'s and `foreach`'s u32 is a jump size (case 07: 0x54) [hypothesis: bytes to
  skip to after `loop`, as Begin's block length].
- The let macro compiles exactly like `let`: `iA = 10` and `let iA := 10` give the same
  body [observed: cases 01, 03]; `iA += 10` likewise [observed: cases 04, 05].
- **`if eval <expr>`** is the base game's `If` (0x0016) whose ASCII condition holds a call
  of **0x153A** with the extender expression as its argument: ` X 0x153A …` [observed:
  cases 06, 16, 21]. So `eval` is a function (0x153A), not a statement.
- `Begin _GameMode` keeps event code 0 (GameMode); the underscore only adds the
  0x154D/0x154E pair [observed: case 23].
- An inline declaration (`let int iCount := 10`, `int iOther = 10`) adds the variable to
  the script's variable list; the statement is an ordinary 0x1539 [observed: case 14].

## 2. Functions seen as calls

| Opcode | Function | Evidence |
|---|---|---|
| 0x153A | eval | [observed: cases 06, 16, 21] |
| 0x1545 | Call | [observed: case 16, inside an expression] |
| 0x1550 | ar_Size | [observed: case 13] |
| 0x1567 | Ar_List | [observed: cases 10, 13] |
| 0x156E | sv_Length | [observed: case 12] |

All lie in xNVSE's opcode range 0x1400–0x1FFF [documented: GECK wiki "NVSE Opcode
Base"].

## 3. The extender expression

A binary postfix (reverse Polish) stream, unlike the base game's ASCII expressions
[observed: every case above]. Tokens:

| Byte | Token | Layout | Evidence |
|---|---|---|---|
| 0x56 `V` | variable | type byte, u16 0, u16 variable index | [observed: all] |
| 0x42 `B` | small integer literal | one byte | [observed: cases 01–09, 13, 16, 17] |
| 0x52 `R` | reference-list entry | u16 index into the script's references | [observed: cases 10, 11, 13, 16] |
| 0x53 `S` | string literal | u16 length, the characters | [observed: cases 10, 12] |
| 0x58 `X` | function call | u16 calling reference (0 none), u16 opcode, u16 parameter bytes, parameters | [observed: cases 06, 10, 12, 13, 16, 20] |
| 0x46 `F` | lambda | u32 length, then a complete compiled script (§4) | [observed: cases 18, 19] |

The variable token's type byte: **01 number, 02 string, 03 array, 04 ref** [observed:
cases 12 (string), 10 and 13 (array), 11 (ref), the int cases]. Floats: not covered.

Operators, one byte each [observed]:

| Byte | Operator | Case | Byte | Operator | Case |
|---|---|---|---|---|---|
| 00 | `:=` | 01 | 15 | `!` | 06 |
| 01 | `\|\|` | 21 | 18 | subscript `[ ]` | 10, 13 |
| 02 | `&&` | 21 | 1a | `<-` | 10, 11 |
| 06 | `>` | 08 | 1b | `$` | 07 |
| 07 | `<` | 07 | 1c | `+=` | 04, 05 |
| 0e | `+` | 15, 20 | 1d | `*=` | 04 |
|  |  |  | 1e | `/=` | 04 |
|  |  |  | 20 | `-=` | 04, 05 |

What the order shows:
- `iA || iB && iC` → `V V V 02 01`: **`&&` binds tighter than `||`** [observed: case 21].
- `iA := iB := 2` → `V V B 00 00`: **`:=` is right-associative** [observed: case 02].
- `while 5 > (iCount += 1)` → `B5 V B1 1c 06` [observed: case 08].
- `Entry["value"]` → `V S 18` [observed: case 10].

## 4. Lambdas and user functions

- `Begin Function {…}` is event code **0x000D** [observed: cases 15, 17, and inside the
  lambdas of 18 and 19]. Its parameter bytes: `01 02 01 00 01 02 00 01 00` for `{a, b}`
  and `01 00 00` for `{ }` [observed: cases 15, 17]; [hypothesis: `01`, a parameter
  count, then per parameter a u16 variable index and a type byte].
- **A lambda is a whole script inside the expression**: `F`, u32 length, then
  ScriptName, a Begin Function block (event 0x0D) with the body, and End, all in the
  ordinary statement framing [observed: case 18, 37 bytes]. `let rCallback := (begin
  function {} … end)` is then `V(ref) F… 00` [observed: case 18]; the one-line form
  `{p} => expr` compiles the same way, its expression becoming a `let` inside the
  embedded script [observed: case 19].

## 5. Inline expressions as base-game arguments

`Player.SetActorValue Health (Player.GetActorValue Health + 50)` keeps the base game's
call (0x100F, its actor-value code 0x0010), and the bracketed argument becomes **`FF FF`,
u16 length, an extender expression** (here a call of 0x100E on reference 1, `B 50`, `+`)
[observed: case 20].

A variable where a base-game parameter takes a code (`set iSkill to player.GetAV iS`,
iS a short holding an actor-value code) makes the whole parameter list the extender's:
the parameter count is **`FF FF`**, then a u8 count and that many (u16 length counting
itself, extender expression), here `01`, `08 00`, `V` of local 32 [observed: Vanilla UI Plus.esp,
its compiled quest script; GECK with xNVSE].

## 6. Variable types in SLSD

| Declared as | SLSD type byte | Evidence |
|---|---|---|
| `int` | 1 (integer) | [observed: all int cases] |
| `ref` | 0 | [observed: cases 10, 11, 18, 19] |
| **`string_var`** | **1**, as an integer | [observed: case 12] |
| **`array_var`** | **0**, as a ref/float | [observed: cases 10, 13] |

So the variable list does not tell a string_var from an int, nor an array_var from a
ref; only the source or the expressions' variable tokens (type 02 / 03) do.

## 7. What the GECK refused (compile messages)

- `let := 5`: "Missing required parameter 'expression' for parameter #'1'." and "Could not
  parse this line." [observed: case 22].
- `$(!-1)`: "Too many operators." [observed: expressions probe 10].
- `int r`: "Invalid variable name 'r'." [observed: an earlier version of the arrays
  probes].
- A `;` inside a string literal starts a comment: "Mismatched quotes." [observed: strings
  probe 03].
- An unknown bare word is taken as a string, with a warning ("Unquoted argument … will
  be treated as string by default") [observed: loops probes 05, 06 before the form list
  existed].

## 8. Open

Float literals (`B` only covers small integers here); the meaning of the `01` prefix and
of `while`'s jump; other operators (`-`, `*`, `/`, `%`, `^`, `==`, `!=`, `<=`, `>=`, bit
operators, `::`, `->`, `#`, unary `-`, box/unbox); more user-function parameter lists.
The expression probes' compiled scripts in the same plugin cover most of these and can be
read the same way.
