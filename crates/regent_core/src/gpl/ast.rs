//! The GPL syntax tree.
//!
//! Node shapes match the grammar recovered from `Gplbcc.exe` one for one; EBNF
//! in `docs/gpl/language.md`.
//!
//! Nodes borrow `&'a str` from the source and carry a byte [`Span`].
//! Parentheses leave no node: nothing here reproduces the source text.

use super::lexer::Span;

/// One parsed file.
#[derive(Debug, Clone, PartialEq)]
pub struct Script<'a> {
    pub items: Vec<Item<'a>>,
}

/// A top-level definition. GPL has three.
#[derive(Debug, Clone, PartialEq)]
pub enum Item<'a> {
    Function(FunctionDef<'a>),
    Prototype(PrototypeDef<'a>),
    /// `expression #Name <expr>` — a named constant, resolved at load time.
    Constant(ConstantDef<'a>),
}

impl<'a> Item<'a> {
    pub const fn name(&self) -> &'a str {
        match self {
            Item::Function(f) => f.name,
            Item::Prototype(p) => p.name,
            Item::Constant(c) => c.name,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Item::Function(f) => f.span.clone(),
            Item::Prototype(p) => p.span.clone(),
            Item::Constant(c) => c.span.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef<'a> {
    pub name: &'a str,
    pub params: Vec<Param<'a>>,
    /// The `is <type>` clause. Absent on 1,583 of 1,967 corpus functions.
    pub ret: Option<Type>,
    /// Contents of the `declare` block. Optional per the grammar; present on
    /// every corpus definition, empty in 641.
    pub decls: Vec<Decl<'a>>,
    /// The `declare` keyword, if present.
    ///
    /// A compiled body-boundary record carries the last declared name's line,
    /// or this line if nothing is declared.
    pub declare_at: Option<Span>,
    pub body: Vec<Stmt<'a>>,
    pub span: Span,
}

/// `prototype Name declare ... begin end` — a record shape that `.dat`
/// files instantiate. The grammar permits a parameter list and a body; the
/// corpus leaves both empty.
#[derive(Debug, Clone, PartialEq)]
pub struct PrototypeDef<'a> {
    pub name: &'a str,
    pub params: Vec<Param<'a>>,
    pub decls: Vec<Decl<'a>>,
    /// The `declare` keyword itself; see [`FunctionDef::declare_at`].
    pub declare_at: Option<Span>,
    pub body: Vec<Stmt<'a>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstantDef<'a> {
    /// Without the leading `#`.
    pub name: &'a str,
    /// The grammar allows any expression; the corpus uses only literals.
    pub value: Expr<'a>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param<'a> {
    pub ty: Type,
    pub name: &'a str,
    pub span: Span,
}

/// One `declare` line: a single type with one or more names.
///
/// 898 of the corpus's 4,172 member lines declare several names at once. The
/// grammar permits no initialisers.
#[derive(Debug, Clone, PartialEq)]
pub struct Decl<'a> {
    pub ty: Type,
    pub names: Vec<(&'a str, Span)>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Agent,
    Boolean,
    Coordinate,
    Float,
    Function,
    Integer,
    List,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt<'a> {
    pub kind: StmtKind<'a>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind<'a> {
    /// `expr ;`. Calls, assignments and appends are all expressions in GPL.
    Expr(Expr<'a>),
    Block(Vec<Stmt<'a>>),
    If {
        cond: Expr<'a>,
        then: Box<Stmt<'a>>,
        /// Bound to the nearest `if`. 616 corpus sites chain via `else if`.
        els: Option<Box<Stmt<'a>>>,
    },
    While {
        cond: Expr<'a>,
        body: Box<Stmt<'a>>,
    },
    /// Permitted by the grammar; no source uses it, shipped or modded.
    DoWhile {
        body: Box<Stmt<'a>>,
        cond: Expr<'a>,
    },
    Foreach {
        /// The grammar makes this a full expression; the corpus always writes
        /// a bare identifier.
        var: Expr<'a>,
        iter: Expr<'a>,
        body: Box<Stmt<'a>>,
    },
    /// No shipped source uses either; mods use `break` twice and `continue`
    /// once. The compiler tracks only `foreach` depth, so it rejects both
    /// inside a bare `while`.
    Break,
    Continue,
    Return(Option<Expr<'a>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr<'a> {
    pub kind: ExprKind<'a>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind<'a> {
    Str(&'a str),
    Int(i64),
    /// As written: GPL floats are fixed point, so `f64` would round.
    Float(&'a str),
    Bool(bool),
    Ident(&'a str),
    /// `$Name` — a function, called or referenced. 1,232 corpus sites use one
    /// as a bare value, with no argument list.
    Func(&'a str),
    /// `#Name` — a named constant, without the `#`.
    Const(&'a str),
    Unary {
        op: UnOp,
        operand: Box<Expr<'a>>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr<'a>>,
        rhs: Box<Expr<'a>>,
    },
    /// Right-associative. `<<` belongs to this tier.
    Assign {
        op: AssignOp,
        lhs: Box<Expr<'a>>,
        rhs: Box<Expr<'a>>,
    },
    /// `obj's attr` — a full expression per the grammar, and a string literal
    /// at all 7,001 shipped sites.
    Attr {
        obj: Box<Expr<'a>>,
        attr: Box<Expr<'a>>,
    },
    /// The callee is whatever the postfix rule produced: `$name(...)`,
    /// `(expr)(...)`, `id's "attr"(...)`.
    Call {
        callee: Box<Expr<'a>>,
        args: Vec<Expr<'a>>,
    },
    Postfix {
        op: PostOp,
        operand: Box<Expr<'a>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    /// `<<`, list append.
    Append,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostOp {
    Inc,
    Dec,
}

/// Walk a statement and everything nested in it, depth first.
fn walk_stmt(s: &Stmt<'_>, f: &mut impl FnMut(&Stmt<'_>)) {
    f(s);

    match &s.kind {
        StmtKind::Block(body) => body.iter().for_each(|s| walk_stmt(s, f)),
        StmtKind::If { then, els, .. } => {
            walk_stmt(then, f);
            if let Some(e) = els {
                walk_stmt(e, f);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::Foreach { body, .. } => walk_stmt(body, f),
        _ => {}
    }
}

/// Walk an expression and everything nested in it, depth first.
fn walk_expr(e: &Expr<'_>, f: &mut impl FnMut(&Expr<'_>)) {
    f(e);

    match &e.kind {
        ExprKind::Unary { operand, .. } | ExprKind::Postfix { operand, .. } => {
            walk_expr(operand, f);
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Assign { lhs, rhs, .. } => {
            walk_expr(lhs, f);
            walk_expr(rhs, f);
        }
        ExprKind::Attr { obj, attr } => {
            walk_expr(obj, f);
            walk_expr(attr, f);
        }
        ExprKind::Call { callee, args } => {
            walk_expr(callee, f);
            for a in args {
                walk_expr(a, f);
            }
        }
        _ => {}
    }
}

impl<'a> Item<'a> {
    /// The `declare` block's lines. Empty for a constant.
    pub fn decls(&self) -> &[Decl<'a>] {
        match self {
            Item::Function(d) => &d.decls,
            Item::Prototype(d) => &d.decls,
            Item::Constant(_) => &[],
        }
    }

    /// The parameter list. Empty for a constant.
    pub fn params(&self) -> &[Param<'a>] {
        match self {
            Item::Function(d) => &d.params,
            Item::Prototype(d) => &d.params,
            Item::Constant(_) => &[],
        }
    }

    /// How many names this definition introduces: parameters plus every name
    /// in the `declare` block.
    pub fn declared_names(&self) -> usize {
        self.params().len() + self.decls().iter().map(|d| d.names.len()).sum::<usize>()
    }

    /// Every statement this definition holds, depth first. None for a constant.
    pub fn visit_stmts(&self, f: &mut impl FnMut(&Stmt<'_>)) {
        let body: &[Stmt<'a>] = match self {
            Item::Function(d) => &d.body,
            Item::Prototype(d) => &d.body,
            Item::Constant(_) => &[],
        };

        for s in body {
            walk_stmt(s, f);
        }
    }

    /// Every expression, nested ones included. For a constant, its value.
    pub fn visit_exprs(&self, f: &mut impl FnMut(&Expr<'_>)) {
        if let Item::Constant(c) = self {
            walk_expr(&c.value, f);
        }

        self.visit_stmts(&mut |s| match &s.kind {
            StmtKind::Expr(e) | StmtKind::Return(Some(e)) => walk_expr(e, f),
            StmtKind::If { cond, .. }
            | StmtKind::While { cond, .. }
            | StmtKind::DoWhile { cond, .. } => walk_expr(cond, f),
            StmtKind::Foreach { var, iter, .. } => {
                walk_expr(var, f);
                walk_expr(iter, f);
            }
            _ => {}
        });
    }
}

impl Script<'_> {
    /// Walk every statement in the script, depth first.
    pub fn visit_stmts(&self, f: &mut impl FnMut(&Stmt<'_>)) {
        for item in &self.items {
            item.visit_stmts(f);
        }
    }

    /// Walk every expression in the script, nested ones included, depth first.
    pub fn visit_exprs(&self, f: &mut impl FnMut(&Expr<'_>)) {
        for item in &self.items {
            item.visit_exprs(f);
        }
    }
}
