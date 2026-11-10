//! GPL back end: `.dat` and `.gpl` sources to a `.bcd` image.
//!
//! `crate::data::bcd` owns the byte layout; this module the compiler's choices:
//! which values survive, their order and spelling. **Corpus**: shipped `.bcd`
//! files decoded against their sources.
//!
//! | Section | Built by |
//! |---|---|
//! | 1, the code | [`code_section`] |
//! | 2, the scope table | [`scope_section`] |
//! | 3, the data | [`data_section`] |
//!
//! [`compile_project`] joins them. Five of the SDK's seven projects compile
//! byte-identical to the shipped images. The other two, neither a format gap:
//! see `a_whole_bcd_is_compiled_from_its_project`.

use std::borrow::Cow;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use super::ast::{
    AssignOp, BinOp, ConstantDef, Decl, Expr, ExprKind, FunctionDef, Item, PostOp, PrototypeDef,
    Script, Stmt, StmtKind, Type, UnOp,
};
use super::bcd::{
    COUNT_RECORD, Compiled, DataSection, Definition, Kind, Mark, NO_LINE, Proto, RESULT_FLAG,
    RETURN_RECORD, Record, Scope, ScopeSection, Slot,
};
use super::dat::{DataFile, Value};
use super::opcode::Op;
use crate::base::gfix::Gfix;
use crate::data::gpltype::TypeCode;

/// Empty section 1: zero flag, zero count (see `super::bcd::symbol_count`).
const EMPTY_SYMBOLS: [u8; 8] = [0; 8];

/// Compile a data-only project's parsed `.dat` files into a `.bcd`.
///
/// Pass files in project order; it decides which duplicate key comes first.
pub fn compile(files: &[DataFile<'_>]) -> Vec<u8> {
    let section = data_section(files);

    crate::data::bcd::write_program(
        &EMPTY_SYMBOLS,
        &ScopeSection::default().encode(),
        &section.encode(),
    )
}

/// Build the data section with the compiler's normalisation.
///
/// **Corpus**: `bytecode.bcd`, `MX_Data.bcd` and their twelve `.dat` files.
///
/// - Prototypes sort by name, case-insensitively, across the project.
/// - Within a prototype, the first of two case-folded equal keys wins:
///   `Gnome_Hovel` has `Max_Members` then `max_members`, output holds one.
///   Five cases, three with differing values; the expansion's six files hold
///   three and two.
/// - Keys then sort, case-insensitively.
///
/// Both sorts are stable, keeping source order on ties. **Inferred**: nothing
/// shipped exercises the tie-break.
pub fn data_section<'a>(files: &[DataFile<'a>]) -> DataSection<'a> {
    let mut prototypes: Vec<Proto<'a>> = files
        .iter()
        .flat_map(|f| &f.prototypes)
        .map(|p| Proto {
            name: Cow::Borrowed(p.name),
            shape: Cow::Borrowed(p.shape),
            values: values(&p.values),
        })
        .collect();

    prototypes.sort_by_key(|p| p.name.to_ascii_lowercase());

    DataSection { prototypes }
}

/// Drop repeated keys, keeping the first, then sort.
fn values<'a>(source: &[(&'a str, Value<'a>)]) -> Vec<(Cow<'a, str>, Cow<'a, str>)> {
    let mut seen: Vec<String> = Vec::with_capacity(source.len());
    let mut out = Vec::with_capacity(source.len());

    for (key, value) in source {
        let folded = key.to_ascii_lowercase();

        if !seen.contains(&folded) {
            seen.push(folded);
            out.push((Cow::Borrowed(*key), render(value)));
        }
    }

    out.sort_by_key(|(key, _)| key.to_ascii_lowercase());

    out
}

/// Spell a value as stored: text, booleans lowercased, the rest verbatim.
///
/// [`Value::Int`] holds a number, so `007` or `+5` renders `7` or `5`. Neither
/// corpus spells one so; all 21,458 values render exactly. See
/// `integer_spelling_is_normalised`.
fn render<'a>(value: &Value<'a>) -> Cow<'a, str> {
    match value {
        Value::Bool(true) => Cow::Borrowed("true"),
        Value::Bool(false) => Cow::Borrowed("false"),
        Value::Int(n) => Cow::Owned(n.to_string()),
        Value::Float(s) | Value::Word(s) | Value::Str(s) => Cow::Borrowed(s),
    }
}

/// Why a project or source did not compile.
#[derive(Debug)]
pub enum Error {
    /// A listed file in no spelling on disk.
    Missing(PathBuf),
    /// A file that will not read.
    Read(PathBuf, std::io::Error),
    /// A source or `.dat` with parse errors, each `line:column: message`.
    Parse { file: String, errors: String },
    /// A construct without a specimen; see [`function`].
    Unwritable(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Missing(path) => write!(f, "{} is not on disk", path.display()),
            Error::Read(path, e) => write!(f, "{}: {e}", path.display()),
            Error::Parse { file, errors } => write!(f, "{file}:{errors}"),
            Error::Unwritable(file) => write!(f, "{file}: a definition the emitter cannot write"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Read(_, e) => Some(e),
            _ => None,
        }
    }
}

/// Read a `.gplproj` and compile it into a `.bcd` image.
///
/// The only reader of files in the crate: parsed files borrow their source, so
/// callers would otherwise repeat the read-then-parse.
///
/// Section 1 records source paths as the project spells them. An image built
/// elsewhere holds that build's path (the SDK example: `C:\UGServer\…`), not
/// recoverable from the sources.
///
/// # Errors
///
/// A listed file unreadable or unparsable, or a construct without a specimen;
/// see [`function`].
pub fn compile_project(path: &Path) -> Result<Vec<u8>, Error> {
    compile_listed(&read_project(path)?, path)
}

/// Compile `project`'s files into a `.bcd` image, as [`compile_project`]
/// does a read one: a mod's `<GPL><Source>` list is one with no file.
/// `label` names it in an error.
///
/// # Errors
///
/// As [`compile_project`].
pub fn compile_listed(project: &super::proj::Project, label: &Path) -> Result<Vec<u8>, Error> {
    let data = section_of(&project.data)?;

    if project.sources.is_empty() {
        return Ok(crate::data::bcd::write_program(
            &EMPTY_SYMBOLS,
            &ScopeSection::default().encode(),
            &data,
        ));
    }

    // Project paths use Windows case; `proj::on_disk` resolves them.
    let texts = project
        .sources
        .iter()
        .map(|listed| {
            let found =
                crate::content::on_disk(listed).ok_or_else(|| Error::Missing(listed.clone()))?;

            fs::read(&found)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .map_err(|e| Error::Read(found, e))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    // A listed file that does not parse fails the project.
    let scripts: Vec<Script<'_>> = texts
        .iter()
        .zip(&project.spelled)
        .map(|(t, file)| {
            let parsed = super::parse(t);

            if parsed.errors.is_empty() {
                Ok(parsed.script)
            } else {
                Err(Error::Parse {
                    file: file.clone(),
                    errors: located(t, &parsed.errors),
                })
            }
        })
        .collect::<Result<_, _>>()?;

    let units: Vec<Unit<'_>> = scripts
        .iter()
        .zip(&texts)
        .zip(&project.spelled)
        .map(|((script, src), file)| Unit { script, src, file })
        .collect();

    let code =
        code_section(&units).ok_or_else(|| Error::Unwritable(label.display().to_string()))?;

    Ok(crate::data::bcd::write_program(
        &super::bcd::encode(&code),
        &scope_section(&units).encode(),
        &data,
    ))
}

/// Compile one GPL source text into a `.bcd` image with no data section.
///
/// # Errors
///
/// A parse error, or a construct without a specimen; see [`function`].
pub fn compile_source(src: &str, file: &str) -> Result<Vec<u8>, Error> {
    let parsed = super::parse(src);

    if !parsed.errors.is_empty() {
        return Err(Error::Parse {
            file: file.to_owned(),
            errors: located(src, &parsed.errors),
        });
    }

    let units = [Unit {
        script: &parsed.script,
        src,
        file,
    }];

    let code = code_section(&units).ok_or_else(|| Error::Unwritable(file.to_owned()))?;

    Ok(crate::data::bcd::write_program(
        &super::bcd::encode(&code),
        &scope_section(&units).encode(),
        &data_section(&[]).encode(),
    ))
}

/// Compile only a project's data section, ignoring `source=` lines. Lets every
/// mixed project test the normalisation, not just the one data-only mod.
///
/// # Errors
///
/// A listed `.dat` file unreadable or unparsable.
pub fn project_data_section(path: &Path) -> Result<Vec<u8>, Error> {
    let project = read_project(path)?;

    section_of(&project.data)
}

/// Each of `errors` as `line:column: message`, `; ` between: the file
/// goes before it, so `x.gpl:3:7: …`.
fn located(src: &str, errors: &[super::parser::ParseError]) -> String {
    errors
        .iter()
        .map(|e| {
            let (line, column) = super::diag::location(src, e.span.start);

            format!("{line}:{column}: {e}")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn read_project(path: &Path) -> Result<super::proj::Project, Error> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let text = fs::read_to_string(path).map_err(|e| Error::Read(path.to_path_buf(), e))?;

    Ok(super::proj::parse(&text, dir))
}

/// Read, parse and compile `.dat` files into a data section body.
fn section_of(paths: &[PathBuf]) -> Result<Vec<u8>, Error> {
    let sources = paths
        .iter()
        .map(|p| {
            fs::read(p)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .map_err(|e| Error::Read(p.clone(), e))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let files = sources
        .iter()
        .zip(paths)
        .map(|(src, p)| {
            let parsed = super::dat::parse(src);

            if parsed.is_ok() {
                Ok(parsed)
            } else {
                Err(Error::Parse {
                    file: p.display().to_string(),
                    errors: located(src, &parsed.errors),
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(data_section(&files).encode())
}

// --------------------------------------------------------------------------
// Section 1
// --------------------------------------------------------------------------

/// Name of every function's return slot.
const RETURN_VALUE: &str = "__Return_value__";

/// The engine's code of a source type.
const fn type_code(ty: Type) -> TypeCode {
    match ty {
        Type::Integer => TypeCode::Integer,
        Type::Float => TypeCode::Float,
        Type::String => TypeCode::String,
        Type::List => TypeCode::List,
        Type::Agent => TypeCode::Agent,
        Type::Boolean => TypeCode::Boolean,
        Type::Coordinate => TypeCode::Coord,
        Type::Function => TypeCode::Function,
    }
}

/// A source text and where its lines start, so a record's line is a
/// binary search rather than a scan from the start.
#[derive(Clone, Copy)]
struct Text<'a, 'l> {
    src: &'a str,
    /// Byte offset of each line's first byte; the first is 0.
    starts: &'l [usize],
}

/// Where each line of `src` starts.
fn line_starts(src: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(src.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

impl Text<'_, '_> {
    /// Line of a byte offset, from 1, as [`super::diag::location`] counts
    /// it; [`NO_LINE`] past `u16`.
    fn line(self, offset: usize) -> u16 {
        u16::try_from(self.starts.partition_point(|start| *start <= offset)).unwrap_or(NO_LINE)
    }
}

const fn unused() -> Slot<'static> {
    Slot::UNUSED
}

/// A slot of type `kind`, other fields unused.
const fn typed(kind: TypeCode) -> Slot<'static> {
    Slot {
        kind: kind.code(),
        ..unused()
    }
}

/// A record, other fields defaulted.
const fn blank(op: Op, text: &str, line: u16) -> Record<'_> {
    Record {
        mark: Mark::NONE,
        values: [unused(), unused()],
        head: 0,
        text,
        line,
        slots: Vec::new(),
        opcode: op.code(),
    }
}

/// A declaration record: `opLocalize`, `opDeclare` or `opReference`.
///
/// First value slot holds the declared type, 0 for none. `opLocalize` and
/// `opDeclare` make a variable, so their mark yields a place; `opReference`'s
/// never does.
fn declaration(op: Op, name: &str, line: u16, ty: Option<Type>) -> Record<'_> {
    let mut out = blank(op, name, line);

    out.values[0] = typed(ty.map_or(TypeCode::Null, type_code));

    if op != Op::Reference {
        out.mark = Mark::PLACE;
    }

    out
}

/// Emit the records before a function body: all the signature decides.
///
/// ```text
/// [0] opAdvance   head 4              the source file
/// [1] opNop                           the definition's own name
/// [2] opExit
/// [3] opAdvance   head = records      line 65535
/// [4] opReference                     __Return_value__
///     opLocalize  head = 1, 2, ...    one per parameter, in order
///     opDeclare                       one per declare, flattened
///     (39)                            the body starts
/// ```
///
/// `file` is [`Unit::file`]: relative in the SDK's image, absolute in the
/// shipped one. `records` is the total record count, known only after the
/// body, so the caller supplies it.
///
/// The body-start record takes the last declared name's line, else the
/// `declare` keyword's, else the function's.
///
/// # Panics
///
/// Parameter count over `u32`.
#[must_use]
pub fn prologue<'a>(
    f: &'a FunctionDef<'a>,
    src: &str,
    file: &'a str,
    records: u32,
) -> Vec<Record<'a>> {
    let starts = line_starts(src);

    prologue_in(
        f,
        Text {
            src,
            starts: &starts,
        },
        file,
        records,
    )
}

/// [`prologue`] on a text whose lines are known.
fn prologue_in<'a>(
    f: &'a FunctionDef<'a>,
    text: Text<'_, '_>,
    file: &'a str,
    records: u32,
) -> Vec<Record<'a>> {
    let line = text.line(f.span.start);

    // The record count goes in record `COUNT_RECORD`; `__Return_value__`
    // is record `RETURN_RECORD`.
    let mut out = vec![
        Record {
            head: RETURN_RECORD,
            ..blank(Op::Advance, file, line)
        },
        blank(Op::Nop, f.name, line),
        blank(Op::Exit, "", line),
        Record {
            head: records,
            ..blank(Op::Advance, "", NO_LINE)
        },
        // Return slot type: the `is <type>` clause, else 0.
        declaration(Op::Reference, RETURN_VALUE, line, f.ret),
    ];

    for (nth, param) in f.params.iter().enumerate() {
        out.push(Record {
            head: u32::try_from(nth + 1).expect("a sane parameter count"),
            ..declaration(Op::Localize, param.name, line, Some(param.ty))
        });
    }

    let mut last = None;

    for decl in &f.decls {
        for (name, span) in &decl.names {
            let at = text.line(span.start);
            last = Some(at);

            out.push(declaration(Op::Declare, name, at, Some(decl.ty)));
        }
    }

    let boundary = f.declare_at.as_ref().map(|at| text.line(at.start));

    out.push(blank(Op::Body, "", last.or(boundary).unwrap_or(line)));
    out
}

/// How an operand reaches the record using it.
///
/// A call's arguments are bare slot numbers, so each needs a record: a leaf
/// gets an `opNop`; a computed value has one.
enum Operand<'a> {
    /// Index (and slot) of the emitted record.
    Computed(u32),
    /// A variable, by its declaring record's index.
    Variable(u32),
    /// A literal, held in the value slot itself.
    Literal(Slot<'a>),
}

/// The value slot of a leaf expression; `None` if computed.
///
/// Payload: integer or boolean in `word` (with [`RESULT_FLAG`]), float in `extra`,
/// string or name in `text`.
///
/// Also used by [`constant`], which folds a leading `-`; a body emits
/// `opNegate` instead.
fn literal<'a>(e: &Expr<'a>, src: &'a str) -> Option<Slot<'a>> {
    let integer = |n: i64| Slot {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        word: n as u32,
        flag: RESULT_FLAG,
        ..typed(TypeCode::Integer)
    };

    let slot = match &e.kind {
        ExprKind::Int(n) => integer(*n),
        ExprKind::Bool(b) => Slot {
            word: u32::from(*b),
            flag: RESULT_FLAG,
            ..typed(TypeCode::Boolean)
        },
        ExprKind::Str(text) => Slot {
            text,
            ..typed(TypeCode::String)
        },
        ExprKind::Float(text) => Slot {
            extra: fixed_point(text),
            ..typed(TypeCode::Float)
        },
        ExprKind::Func(name) => Slot {
            text: name,
            ..typed(TypeCode::Function)
        },
        // Keeps the `#` the AST drops: text taken from the source, past any
        // brackets in the span.
        ExprKind::Const(name) => Slot {
            text: sigil(&src[e.span.start..e.span.end], name),
            ..typed(TypeCode::Reference)
        },
        _ => return None,
    };

    Some(slot)
}

const fn reference(slot: u32) -> Slot<'static> {
    Slot {
        word: slot,
        flag: RESULT_FLAG,
        ..typed(TypeCode::Undefined)
    }
}

impl<'a> Operand<'a> {
    /// As an operator operand: a reference or a literal.
    const fn slot(self) -> Slot<'a> {
        match self {
            Operand::Computed(at) | Operand::Variable(at) => reference(at),
            Operand::Literal(value) => value,
        }
    }
}

/// Opcode of each operator.
const fn binary_op(op: BinOp) -> Op {
    match op {
        BinOp::Or => Op::Or,
        BinOp::And => Op::And,
        BinOp::Eq => Op::Equal,
        BinOp::Ne => Op::NotEqual,
        BinOp::Lt => Op::LessThan,
        BinOp::Gt => Op::GreaterThan,
        BinOp::Le => Op::LessThanOrEqualTo,
        BinOp::Ge => Op::GreaterThanOrEqualTo,
        BinOp::Add => Op::Plus,
        BinOp::Sub => Op::Minus,
        BinOp::Mul => Op::Star,
        BinOp::Div => Op::Divide,
        BinOp::Rem => Op::Mod,
    }
}

const fn assign_op(op: AssignOp) -> Op {
    match op {
        AssignOp::Assign => Op::AssignEqual,
        AssignOp::Add => Op::PlusEqual,
        AssignOp::Sub => Op::MinusEqual,
        AssignOp::Mul => Op::TimesEqual,
        AssignOp::Div => Op::DivideEqual,
        AssignOp::Rem => Op::ModEqual,
        AssignOp::Append => Op::Inserter,
    }
}

/// Declaring record of each parameter and local, by lower-cased name. Section
/// 1 only; unrelated to section 2's [`Scope`].
type Slots = std::collections::BTreeMap<String, u32>;

struct Body<'a, 'l> {
    out: Vec<Record<'a>>,
    scope: Slots,
    text: Text<'a, 'l>,
    /// Each open `foreach`, innermost last.
    loops: Vec<Loop<'a>>,
}

/// An open `foreach`.
struct Loop<'a> {
    /// Its `opForeach` record. `break` and `continue` name their loop by it.
    at: u32,
    /// The `opReturnValue` of a `return` in its body, written after the loop.
    ret: Option<Record<'a>>,
}

impl<'a> Body<'a, '_> {
    /// Append `record`; returns its slot.
    fn push(&mut self, record: Record<'a>) -> u32 {
        let at = u32::try_from(self.out.len()).expect("a sane record count");
        self.out.push(record);

        at
    }

    /// A record computing a value from one or two operands.
    fn operation(&mut self, op: Op, line: u16, operands: [Slot<'a>; 2]) -> u32 {
        let mut record = blank(op, "", line);
        record.values = operands;
        record.mark = Mark::VALUE;

        self.push(record)
    }

    /// A record yielding a place: assignment and kin.
    fn place(&mut self, op: Op, line: u16, operands: [Slot<'a>; 2]) -> u32 {
        let mut record = blank(op, "", line);
        record.values = operands;
        record.mark = Mark::PLACE;

        self.push(record)
    }

    /// Give an operand its own slot, as call arguments need.
    fn materialise(&mut self, operand: Operand<'a>, line: u16) -> u32 {
        match operand {
            Operand::Computed(at) => at,
            other => {
                let mut record = blank(Op::Nop, "", line);
                record.values[0] = other.slot();
                record.mark = Mark::PLACE;

                self.push(record)
            }
        }
    }
}

impl<'a> Body<'a, '_> {
    /// Emit one expression, post-order; returns how to reach its result.
    fn expr(&mut self, e: &'a Expr<'a>) -> Operand<'a> {
        // An operator sits on its right operand's line: `a &&\n b` puts
        // `opAnd` on b's line.
        let line = match &e.kind {
            ExprKind::Binary { rhs, .. } | ExprKind::Assign { rhs, .. } => {
                self.text.line(rhs.span.start)
            }
            ExprKind::Attr { attr, .. } => self.text.line(attr.span.start),
            ExprKind::Unary { operand, .. } | ExprKind::Postfix { operand, .. } => {
                self.text.line(operand.span.start)
            }
            _ => self.text.line(e.span.start),
        };

        let got = match &e.kind {
            ExprKind::Str(_)
            | ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Func(_)
            | ExprKind::Const(_) => {
                Operand::Literal(literal(e, self.text.src).expect("a leaf value is a slot"))
            }
            ExprKind::Ident(name) => self.scope.get(&name.to_ascii_lowercase()).map_or_else(
                || {
                    Operand::Literal(Slot {
                        text: name,
                        ..typed(TypeCode::Undefined)
                    })
                },
                |at| Operand::Variable(*at),
            ),
            ExprKind::Binary { op, lhs, rhs } => {
                let (a, b) = (self.expr(lhs), self.expr(rhs));
                let line = self.operand_line(&b, line);
                let operands = [a.slot(), b.slot()];

                Operand::Computed(self.operation(binary_op(*op), line, operands))
            }
            ExprKind::Assign { op, lhs, rhs } => {
                let (a, b) = (self.expr(lhs), self.expr(rhs));
                let line = self.operand_line(&b, line);
                let operands = [a.slot(), b.slot()];

                Operand::Computed(self.place(assign_op(*op), line, operands))
            }
            ExprKind::Attr { obj, attr } => {
                let (a, b) = (self.expr(obj), self.expr(attr));
                let line = self.operand_line(&b, line);
                let operands = [a.slot(), b.slot()];

                Operand::Computed(self.operation(Op::Apostrophe, line, operands))
            }
            ExprKind::Unary { op, operand } => {
                let a = self.expr(operand);

                match op {
                    // Unary plus emits nothing; the operand is the result.
                    UnOp::Pos => a,
                    UnOp::Neg => {
                        let line = self.operand_line(&a, line);
                        let operands = [a.slot(), unused()];

                        Operand::Computed(self.operation(Op::Negate, line, operands))
                    }
                }
            }
            ExprKind::Postfix { op, operand } => {
                let a = self.expr(operand);
                let line = self.operand_line(&a, line);
                let operands = [a.slot(), unused()];
                let op = match op {
                    PostOp::Inc => Op::Inc,
                    PostOp::Dec => Op::Dec,
                };

                Operand::Computed(self.place(op, line, operands))
            }
            ExprKind::Call { callee, args } => Operand::Computed(self.call(callee, args, line)),
        };

        // Brackets round a leaf give it its own slot; the parser keeps them in
        // the span for this.
        if self.text.src[e.span.start..].starts_with('(') && !matches!(got, Operand::Computed(_)) {
            return Operand::Computed(self.materialise(got, line));
        }

        got
    }

    /// A call: callee, then each argument in its own slot.
    fn call(&mut self, callee: &'a Expr<'a>, args: &'a [Expr<'a>], line: u16) -> u32 {
        let before = self.here();

        // `$Name(...)` puts the callee in the value slot, no record.
        // `($Name)(...)` has a slot from the brackets and is referenced.
        let target = match self.expr(callee) {
            Operand::Literal(value) => value,
            other => reference(self.materialise(other, self.text.line(callee.span.start))),
        };

        let mut slots = Vec::with_capacity(args.len() + 1);

        for arg in args {
            // Each `opNop` takes its argument's line.
            let at = self.text.line(arg.span.start);
            let operand = self.expr(arg);
            let at = self.materialise(operand, at);

            slots.push(u16::try_from(at).expect("a sane slot number"));
        }

        // A multi-line call sits on its last argument, not its `)`.
        let line = self.fed_line(before, line);

        let mut record = blank(Op::Call, "", line);
        record.values[0] = target;

        let at = self.push(record);
        let result = u16::try_from(at).expect("a sane slot number");

        self.out[at as usize].slots = std::iter::once(result).chain(slots).collect();
        at
    }
}

/// Span of the innermost last statement in `s`. An empty `begin end` counts,
/// so this can differ from the last record.
fn last_statement<'a>(s: &'a Stmt<'a>) -> &'a super::lexer::Span {
    match &s.kind {
        StmtKind::Block(body) => body.last().map_or(&s.span, last_statement),
        StmtKind::If { then, els, .. } => els
            .as_deref()
            .map_or_else(|| last_statement(then), last_statement),
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. } => last_statement(body),
        StmtKind::Expr(_) | StmtKind::Return(_) | StmtKind::Break | StmtKind::Continue => &s.span,
    }
}

/// `#Name` as written, from a span that may hold brackets.
fn sigil<'a>(span: &'a str, name: &str) -> &'a str {
    span.find('#')
        .and_then(|at| span.get(at..at + 1 + name.len()))
        .unwrap_or(span)
}

/// Float literal to stored fixed point, truncated.
fn fixed_point(text: &str) -> u32 {
    let value: f64 = text.parse().unwrap_or_default();

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fixed = (value * f64::from(Gfix::ONE.0)) as u32;

    fixed
}

/// Emit a function: prologue, then body.
///
/// `None` where no specimen shows the output:
///
/// - `do while`, used nowhere;
/// - `break` or `continue` outside a `foreach`, which the compiler rejects;
/// - `return` inside a `foreach`, beyond the shape shipped images show:
///   see `inside`.
///
/// `file` is [`Unit::file`].
///
/// # Panics
///
/// Record count over `u32`.
#[must_use]
pub fn function<'a>(
    f: &'a FunctionDef<'a>,
    src: &'a str,
    file: &'a str,
) -> Option<Vec<Record<'a>>> {
    let starts = line_starts(src);

    function_in(
        f,
        Text {
            src,
            starts: &starts,
        },
        file,
    )
}

/// [`function`] on a text whose lines are known.
fn function_in<'a>(
    f: &'a FunctionDef<'a>,
    text: Text<'a, '_>,
    file: &'a str,
) -> Option<Vec<Record<'a>>> {
    if f.body.iter().any(unspecified) {
        return None;
    }

    // Record count placeholder, filled in after the body.
    let head = prologue_in(f, text, file, 0);
    let mut body = Body {
        scope: scope_of(f, &head),
        out: head,
        text,
        loops: Vec::new(),
    };

    for stmt in &f.body {
        body.stmt(stmt);
    }

    let mut out = body.out;
    out[COUNT_RECORD].head = u32::try_from(out.len()).expect("a sane record count");

    Some(out)
}

/// Emit a named constant: three records, no body.
///
/// ```text
/// [0] opNop   the source file
/// [1] opNop   #Name, spelled as it was written
/// [2] opNop   the value, in the first slot, marked as a place: CONSTANT_RECORD;
///     for `-literal`, an opNegate of it, marked as a value
/// ```
///
/// All three carry the `expression` keyword's line. Unlike a function: no
/// `opAdvance`, no record count; nothing to jump over.
///
/// `None` where a slot cannot hold the value. The grammar allows any
/// expression; no corpus uses more than an optionally negated literal.
#[must_use]
pub fn constant<'a>(
    c: &'a ConstantDef<'a>,
    src: &'a str,
    file: &'a str,
) -> Option<Vec<Record<'a>>> {
    let starts = line_starts(src);

    constant_in(
        c,
        Text {
            src,
            starts: &starts,
        },
        file,
    )
}

/// [`constant`] on a text whose lines are known.
fn constant_in<'a>(
    c: &'a ConstantDef<'a>,
    text: Text<'a, '_>,
    file: &'a str,
) -> Option<Vec<Record<'a>>> {
    let src = text.src;
    let line = text.line(c.span.start);

    // One record for the value. A leading `-` is not folded: an
    // `opNegate` of the literal, as in a body (Monster Kingdom's
    // `#WYRM_HEAL_MOD_TIER1 -2`); the engine evaluates the records.
    let value = match &c.value.kind {
        ExprKind::Unary {
            op: UnOp::Neg,
            operand,
        } => Record {
            mark: Mark::VALUE,
            values: [negatable(literal(operand, src)?)?, unused()],
            ..blank(Op::Negate, "", line)
        },
        _ => Record {
            mark: Mark::PLACE,
            values: [literal(&c.value, src)?, unused()],
            ..blank(Op::Nop, "", line)
        },
    };

    Some(vec![
        blank(Op::Nop, file, line),
        blank(Op::Nop, sigil(&src[c.span.start..c.span.end], c.name), line),
        value,
    ])
}

/// `slot` if `-` applies to it: an integer or float literal.
fn negatable(slot: Slot<'_>) -> Option<Slot<'_>> {
    matches!(
        TypeCode::try_from(slot.kind),
        Ok(TypeCode::Integer | TypeCode::Float)
    )
    .then_some(slot)
}

/// Emit a prototype: two header records, then one `opDeclare` per member,
/// shaped as a function local, on its own line.
///
/// No `__Return_value__`, record count or body start. No corpus prototype
/// takes a parameter; one of 135 has a statement in its body, which compiles
/// to nothing.
///
/// `ends_at` is the header records' line, from [`script`].
fn prototype_in<'a>(
    p: &'a PrototypeDef<'a>,
    text: Text<'_, '_>,
    file: &'a str,
    ends_at: u16,
) -> Vec<Record<'a>> {
    let mut out = vec![
        blank(Op::Nop, file, ends_at),
        blank(Op::Nop, p.name, ends_at),
    ];

    for decl in &p.decls {
        for (name, span) in &decl.names {
            let at = text.line(span.start);

            out.push(declaration(Op::Declare, name, at, Some(decl.ty)));
        }
    }

    out
}

/// Emit one source file's section 1 definitions.
///
/// A prototype's header records carry the next definition's start line, or
/// the file's last line: the compiler writes the entry after reading past
/// `end`.
///
/// `None` if any definition lacks a specimen; see [`function`] and
/// [`constant`].
#[must_use]
pub fn script<'a>(
    script: &'a Script<'a>,
    src: &'a str,
    file: &'a str,
) -> Option<Vec<Compiled<'a>>> {
    let starts = line_starts(src);
    let text = Text {
        src,
        starts: &starts,
    };
    let last = text.line(src.len());
    let mut out = Vec::with_capacity(script.items.len());

    for (nth, item) in script.items.iter().enumerate() {
        let records = match item {
            Item::Function(f) => function_in(f, text, file)?,
            Item::Constant(c) => constant_in(c, text, file)?,
            Item::Prototype(p) => {
                let ends_at = script.items[nth + 1..]
                    .first()
                    .map_or(last, |next| text.line(next.span().start));

                prototype_in(p, text, file, ends_at)
            }
        };

        out.push(Compiled {
            name: entry_name(item, src),
            records,
        });
    }

    Some(out)
}

/// Entry name; a constant keeps its `#`.
fn entry_name<'a>(item: &'a Item<'a>, src: &'a str) -> &'a str {
    match item {
        Item::Constant(c) => sigil(&src[c.span.start..c.span.end], c.name),
        _ => item.name(),
    }
}

/// Emit section 1 from a project's source files.
///
/// Case-insensitive name order, as in all 75 `.bcd` files. Stable sort, so a
/// name defined twice (one Workshop mod) keeps file order; the compiler's
/// order there is unknown.
///
/// `None` if any definition cannot be emitted.
#[must_use]
pub fn code_section<'a>(units: &[Unit<'a>]) -> Option<Vec<Compiled<'a>>> {
    let mut out: Vec<Compiled<'a>> = units
        .iter()
        .map(|u| script(u.script, u.src, u.file))
        .collect::<Option<Vec<_>>>()?
        .concat();

    out.sort_by_key(|d| d.name.to_ascii_lowercase());

    Some(out)
}

/// One project source file.
#[derive(Debug, Clone, Copy)]
pub struct Unit<'a> {
    pub script: &'a Script<'a>,
    /// File text; record lines count in it.
    pub src: &'a str,
    /// Path as given to the compiler; every section 1 entry records it.
    pub file: &'a str,
}

/// Emit section 2, the scope table.
///
/// One entry per definition in its kind's table, sorted by name in byte order
/// (section 1 sorts case-insensitively).
///
/// Contents follow source order: every `declare`d name and parameter so far in
/// the file, reset per file. See [`Scope`].
#[must_use]
pub fn scope_section<'a>(units: &[Unit<'a>]) -> ScopeSection<'a> {
    let mut out = ScopeSection::default();

    for unit in units {
        let mut scope = Scope::default();

        for item in &unit.script.items {
            let (kind, table) = match item {
                Item::Prototype(_) => (Kind::Prototype, &mut out.prototypes),
                Item::Function(_) => (Kind::Function, &mut out.functions),
                Item::Constant(_) => (Kind::Constant, &mut out.constants),
            };

            // Grow before writing, so a definition's own names are included.
            match item {
                Item::Function(f) => {
                    scope
                        .parameters
                        .extend(f.params.iter().map(|p| type_code(p.ty).code()));
                    scope.declares.extend(declared(&f.decls));
                }
                Item::Prototype(p) => {
                    scope
                        .parameters
                        .extend(p.params.iter().map(|p| type_code(p.ty).code()));
                    scope.declares.extend(declared(&p.decls));
                }
                Item::Constant(_) => {}
            }

            let returns = match item {
                Item::Function(f) => f.ret.map_or(TypeCode::Null, type_code),
                _ => TypeCode::Unknown,
            };

            table.push(Definition {
                name: entry_name(item, unit.src),
                body: Cow::Owned(
                    Scope {
                        returns: returns.code(),
                        kind,
                        ..scope.clone()
                    }
                    .encode(),
                ),
            });
        }
    }

    for table in [&mut out.prototypes, &mut out.functions, &mut out.constants] {
        table.sort_by(|a, b| a.name.cmp(b.name));
    }

    out
}

/// Type of every name `declare` blocks introduce, flattened.
fn declared<'a>(decls: &'a [Decl<'a>]) -> impl Iterator<Item = u8> + 'a {
    decls
        .iter()
        .flat_map(|d| d.names.iter().map(|_| type_code(d.ty).code()))
}

/// Whether a statement uses a construct with no shipped specimen.
fn unspecified(s: &Stmt<'_>) -> bool {
    inside(s, 0)
}

/// As [`unspecified`], tracking how many `foreach` enclose the statement.
///
/// `break` and `continue` name their `opForeach`, so outside one they have
/// nothing to name; the compiler rejects them too. A `return` with a value
/// inside one is [`Body::returns`]; two shipped sites show it, each the only
/// `return` of a loop not nested in another. Anything else is refused: no
/// valueless one, none inside a `while` or a nested loop, one per loop.
fn inside(s: &Stmt<'_>, loops: u8) -> bool {
    match &s.kind {
        StmtKind::DoWhile { .. } => true,
        StmtKind::Break | StmtKind::Continue => loops == 0,
        StmtKind::Return(value) => loops > 1 || (loops == 1 && value.is_none()),
        StmtKind::Block(body) => body.iter().any(|s| inside(s, loops)),
        StmtKind::If { then, els, .. } => {
            inside(then, loops) || els.as_deref().is_some_and(|s| inside(s, loops))
        }
        StmtKind::While { body, .. } => inside(body, loops) || (loops > 0 && returns(body) > 0),
        StmtKind::Foreach { body, .. } => {
            inside(body, loops.saturating_add(1)) || returns(body) > 1
        }
        StmtKind::Expr(_) => false,
    }
}

/// `return` statements in `s`, not counting those in a nested `foreach`.
fn returns(s: &Stmt<'_>) -> usize {
    match &s.kind {
        StmtKind::Return(_) => 1,
        StmtKind::Block(body) => body.iter().map(returns).sum(),
        StmtKind::If { then, els, .. } => returns(then) + els.as_deref().map_or(0, returns),
        StmtKind::While { body, .. } | StmtKind::DoWhile { body, .. } => returns(body),
        StmtKind::Foreach { .. } | StmtKind::Expr(_) | StmtKind::Break | StmtKind::Continue => 0,
    }
}

fn scope_of(f: &FunctionDef<'_>, head: &[Record<'_>]) -> Slots {
    let declared: Vec<&str> = f
        .params
        .iter()
        .map(|p| p.name)
        .chain(f.decls.iter().flat_map(|d| d.names.iter().map(|(n, _)| *n)))
        .collect();

    // Prologue order, after __Return_value__.
    let first = head
        .iter()
        .position(|r| r.text == RETURN_VALUE)
        .map_or(0, |at| at + 1);

    declared
        .iter()
        .enumerate()
        .map(|(nth, name)| {
            let at = u32::try_from(first + nth).expect("a sane record count");

            ((*name).to_ascii_lowercase(), at)
        })
        .collect()
}

impl<'a> Body<'a, '_> {
    /// Next slot: where a forward jump lands.
    fn here(&self) -> u32 {
        u32::try_from(self.out.len()).expect("a sane record count")
    }

    fn last_line(&self) -> Option<u16> {
        self.out.last().map(|r| r.line)
    }

    /// A record's line: that of the last record emitted since `before`, else
    /// `fallback` (the rightmost operand's position).
    fn fed_line(&self, before: u32, fallback: u16) -> u16 {
        if self.here() > before {
            self.last_line().unwrap_or(fallback)
        } else {
            fallback
        }
    }

    /// An operator's line: its right operand's.
    ///
    /// A computed operand gives its record's line, so `x =\n $f(\n a)` puts
    /// the assignment on the call's line, not `$f`'s. A literal or variable
    /// gives `fallback`.
    fn operand_line(&self, right: &Operand<'a>, fallback: u16) -> u16 {
        match right {
            Operand::Computed(at) => self.out[*at as usize].line,
            Operand::Variable(_) | Operand::Literal(_) => fallback,
        }
    }

    /// A test for `if` or `while`. Unlike an operator operand, always
    /// materialised: a leaf gets an `opNop`.
    fn condition(&mut self, cond: &'a Expr<'a>, line: u16) -> Slot<'a> {
        let got = self.expr(cond);

        reference(self.materialise(got, line))
    }

    /// A control record: at most one operand; head filled in later.
    fn control(&mut self, op: Op, line: u16, operand: Slot<'a>) -> u32 {
        let mut record = blank(op, "", line);
        record.values[0] = operand;

        self.push(record)
    }

    fn stmt(&mut self, s: &'a Stmt<'a>) {
        let line = self.text.line(s.span.start);

        match &s.kind {
            StmtKind::Expr(e) => {
                self.expr(e);
            }
            StmtKind::Block(body) => {
                for inner in body {
                    self.stmt(inner);
                }
            }
            StmtKind::If { cond, then, els } => self.conditional(cond, then, els.as_deref(), line),
            StmtKind::While { cond, body } => {
                // Jump back to the condition's first record, not the `opWhile`.
                let top = self.here();
                let test = self.condition(cond, line);
                let at = self.control(Op::While, line, test);

                self.stmt(body);

                let mut back = blank(Op::Rewind, "", self.text.line(last_statement(body).start));
                back.head = top;
                self.push(back);

                self.out[at as usize].head = self.here();
            }
            StmtKind::Foreach { var, iter, body } => self.foreach(var, iter, body, line),
            StmtKind::Return(value) => {
                let ret = reference(RETURN_RECORD);

                match value {
                    Some(e) if !self.loops.is_empty() => self.returns(e, line),
                    Some(e) => {
                        // The value gets its own slot, as a call argument.
                        let got = self.expr(e);
                        let got = reference(self.materialise(got, line));

                        let mut record = blank(Op::ReturnValue, "", line);
                        record.values = [ret, got];
                        self.push(record);
                    }
                    None => {
                        self.control(Op::Return, line, ret);
                    }
                }
            }
            // Name their `opForeach`; no source line. `unspecified` refused
            // any outside a `foreach`.
            StmtKind::Break | StmtKind::Continue => {
                let op = if matches!(s.kind, StmtKind::Break) {
                    Op::Break
                } else {
                    Op::Continue
                };
                let mut record = blank(op, "", NO_LINE);
                record.head = self.loops.last().expect("inside a loop").at;

                self.push(record);
            }
            // Refused by `unspecified`.
            StmtKind::DoWhile { .. } => unreachable!("refused before emitting"),
        }
    }

    /// `if cond then [else]`.
    ///
    /// The test jumps to the `else`, or past the `then`. A `then` followed by an
    /// `else` ends with an `opAdvance` over it.
    fn conditional(
        &mut self,
        cond: &'a Expr<'a>,
        then: &'a Stmt<'a>,
        els: Option<&'a Stmt<'a>>,
        line: u16,
    ) {
        let test = self.condition(cond, line);
        let at = self.control(Op::If, line, test);

        self.stmt(then);

        match els {
            None => self.out[at as usize].head = self.here(),
            Some(els) => {
                let over = self.push(blank(Op::Advance, "", NO_LINE));

                let before = self.here();
                self.out[at as usize].head = before;
                self.stmt(els);

                // The jump takes the greatest line the `else` touches: records
                // cover multi-line statements, `last_statement` an empty
                // `begin end`. On an `else if` chain, all jumps share the
                // chain's last line.
                let deepest = self.text.line(last_statement(els).start);
                let emitted = self.out[before as usize..]
                    .iter()
                    .map(|r| r.line)
                    .filter(|line| *line != NO_LINE)
                    .max();

                self.out[over as usize].line = emitted.map_or(deepest, |at| at.max(deepest));
                self.out[over as usize].head = self.here();
            }
        }
    }

    /// `foreach var in iter body`.
    ///
    /// Both operands materialised. The record after `opForeach` jumps out; the
    /// body ends with `opRewind` back to `opForeach`.
    fn foreach(&mut self, var: &'a Expr<'a>, iter: &'a Expr<'a>, body: &'a Stmt<'a>, line: u16) {
        let each = self.expr(var);
        let each = self.materialise(each, line);
        let over = self.expr(iter);
        let over = self.materialise(over, line);

        let mut record = blank(Op::Foreach, "", line);
        record.values = [reference(each), reference(over)];

        let at = self.push(record);
        let exit = self.push(blank(Op::Advance, "", line));

        self.loops.push(Loop { at, ret: None });
        self.stmt(body);
        let ret = self.loops.pop().and_then(|l| l.ret);

        // Last statement, not last record: `break` has no line.
        let mut back = blank(Op::Rewind, "", self.text.line(last_statement(body).start));
        back.head = at;
        self.push(back);

        // The jump out lands on a `return`'s `opReturnValue`, if any, so the
        // loop running out returns too: see `returns`.
        self.out[exit as usize].head = self.here();

        if let Some(mut ret) = ret {
            ret.line = line;
            self.push(ret);
        }
    }

    /// `return value` inside a `foreach`, as `Gplbcc.exe` writes it: an
    /// `opBreak` to free the cursor, then the value, and `opReturnValue` after
    /// the loop on the `foreach` line, where the jump out lands.
    ///
    /// ```text
    /// 19 opForeach
    /// 20 opAdvance  -> 34        loop runs out
    /// ...
    /// 31 opBreak    (19)         return True:
    /// 32 opNop      True         never runs
    /// 33 opRewind   -> 19
    /// 34 opReturnValue  32
    /// ```
    ///
    /// The value's records follow the `opBreak`, so they never run: the
    /// function returns whatever the value's slot last held, and the loop
    /// running out returns it too, skipping what follows the loop. **Game**:
    /// shipped `Phantom_Priestess_Assigned_To` and Majestic Majesty's
    /// `defend_building_packet`.
    fn returns(&mut self, value: &'a Expr<'a>, line: u16) {
        let open = self.loops.last().expect("inside a loop").at;
        let mut stop = blank(Op::Break, "", NO_LINE);
        stop.head = open;
        self.push(stop);

        let got = self.expr(value);
        let got = reference(self.materialise(got, line));

        let mut record = blank(Op::ReturnValue, "", line);
        record.values = [reference(RETURN_RECORD), got];
        self.loops.last_mut().expect("inside a loop").ret = Some(record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpl::dat;

    /// Borrows from `src`, not the parse tree, so the `DataFile` can drop.
    fn section(src: &str) -> DataSection<'_> {
        let parsed = dat::parse(src);
        assert!(parsed.is_ok(), "{:?}", parsed.errors);

        data_section(std::slice::from_ref(&parsed))
    }

    fn keys<'a>(s: &'a DataSection<'_>, n: usize) -> Vec<&'a str> {
        s.prototypes[n]
            .values
            .iter()
            .map(|(k, _)| k.as_ref())
            .collect()
    }

    #[test]
    fn keys_sort_case_insensitively() {
        let s = section("[A] {S (zebra 1) (Apple 2) (mango 3)} [end]");

        assert_eq!(keys(&s, 0), ["Apple", "mango", "zebra"]);
    }

    #[test]
    fn prototypes_sort_case_insensitively() {
        let s = section("[zulu] {S (a 1)} [end] [Alpha] {S (a 1)} [end]");

        // Not discriminating: bytewise order is the same here.
        assert_eq!(s.prototypes[0].name, "Alpha");
        assert_eq!(s.prototypes[1].name, "zulu");
    }

    #[test]
    fn first_duplicate_key_wins() {
        // Hooligan's `castingrange`: 250 then 200 in source, 250 shipped.
        let s = section("[H] {S (castingrange 250) (castingrange 200)} [end]");

        assert_eq!(s.prototypes[0].values.len(), 1);
        assert_eq!(s.prototypes[0].values[0].1, "250");
    }

    #[test]
    fn duplicate_keys_match_case_insensitively() {
        let s = section("[H] {S (Range 1) (range 2)} [end]");

        assert_eq!(keys(&s, 0), ["Range"]);
    }

    #[test]
    fn booleans_are_lowercased_and_the_rest_is_verbatim() {
        let s = section("[A] {S (a False) (b TRUE) (c Monster) (d 9) (e 1.5)} [end]");
        let vals: Vec<&str> = s.prototypes[0]
            .values
            .iter()
            .map(|(_, v)| v.as_ref())
            .collect();

        assert_eq!(vals, ["false", "true", "Monster", "9", "1.5"]);
    }

    /// Accepted divergence: no corpus writes integers so.
    #[test]
    fn integer_spelling_is_normalised() {
        let s = section("[A] {S (a 007) (b +5)} [end]");
        let vals: Vec<&str> = s.prototypes[0]
            .values
            .iter()
            .map(|(_, v)| v.as_ref())
            .collect();

        assert_eq!(vals, ["7", "5"]);
    }

    #[test]
    fn a_line_from_the_starts_is_the_scanned_line() {
        let src = "a\n\nbc\r\nd";
        let starts = line_starts(src);
        let text = Text {
            src,
            starts: &starts,
        };

        for offset in 0..=src.len() + 2 {
            let scanned = crate::gpl::diag::location(src, offset).0;

            assert_eq!(usize::from(text.line(offset)), scanned, "{offset}");
        }
    }

    /// A file's last prototype carries the line of its end: past a final
    /// newline, the line after it (Monster Kingdom's `AIRoot9`).
    #[test]
    fn a_last_prototype_takes_the_line_past_the_end() {
        for (src, line) in [
            ("prototype P()\nbegin\nend", 3),
            ("prototype P()\nbegin\nend\r\n", 4),
        ] {
            let parsed = crate::gpl::parse(src).script;
            let out = script(&parsed, src, "f.gpl").expect("emits");

            assert_eq!(out[0].records[0].line, line, "{src:?}");
        }
    }

    #[test]
    fn empty_project_still_produces_a_valid_image() {
        let bytes = compile(&[]);
        let program = crate::data::bcd::Container::parse(&bytes).expect("header");
        let sections = program.sections().expect("three sections");

        assert_eq!(
            DataSection::decode(sections.data.body)
                .expect("data section")
                .prototypes
                .len(),
            0
        );
    }
}
