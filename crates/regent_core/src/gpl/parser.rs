//! Recursive-descent parser for GPL.
//!
//! One function per rule of the grammar recovered from `Gplbcc.exe`, named to
//! match the EBNF in `docs/gpl/language.md`. Not Pratt: a loop per precedence
//! tier can be checked against the spec by eye.
//!
//! Errors are collected, recovered from and capped, as in the lexer.

use super::ast::*;
use super::lexer::{Span, Spanned, lex};
use super::token::{Keyword, Token};

/// A syntax error, located in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseError {}

/// The result of parsing one file.
#[derive(Debug, Clone)]
pub struct Parsed<'a> {
    pub script: Script<'a>,
    /// Lexical and syntactic errors together, at most `super::lexer::MAX_ERRORS`.
    pub errors: Vec<ParseError>,
    pub suppressed: usize,
}

impl Parsed<'_> {
    pub const fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub const fn error_count(&self) -> usize {
        self.errors.len() + self.suppressed
    }
}

/// A rule gave up. The caller unwinds to a recovery point.
struct Bail;

type Rule<T> = Result<T, Bail>;

/// Lex and parse GPL source.
pub fn parse(src: &str) -> Parsed<'_> {
    let lexed = lex(src);

    let mut p = Parser {
        tokens: lexed.tokens,
        pos: 0,
        end: src.len(),
        errors: Vec::new(),
        suppressed: lexed.suppressed,
    };

    // Lexical errors come first so the report reads in source order.
    for e in lexed.errors {
        p.errors.push(ParseError {
            span: e.span.clone(),
            message: e.to_string(),
        });
    }

    let script = p.script();

    Parsed {
        script,
        errors: p.errors,
        suppressed: p.suppressed,
    }
}

struct Parser<'a> {
    tokens: Vec<Spanned<Token<'a>>>,
    pos: usize,
    end: usize,
    errors: Vec<ParseError>,
    suppressed: usize,
}

// --------------------------------------------------------------------------
// Token access
// --------------------------------------------------------------------------

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.pos).map(|t| t.node)
    }

    /// Span of the current token, or a zero-width span at end of file.
    fn span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span.clone())
            .unwrap_or(self.end..self.end)
    }

    fn prev_span(&self) -> Span {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span.clone())
            .unwrap_or(self.end..self.end)
    }

    fn at(&self, tok: Token<'_>) -> bool {
        self.peek() == Some(tok)
    }

    fn at_kw(&self, kw: Keyword) -> bool {
        self.at(Token::Kw(kw))
    }

    fn eat(&mut self, tok: Token<'_>) -> bool {
        let hit = self.at(tok);

        if hit {
            self.pos += 1;
        }

        hit
    }

    fn eat_kw(&mut self, kw: Keyword) -> bool {
        self.eat(Token::Kw(kw))
    }

    fn describe(&self) -> String {
        match self.peek() {
            Some(t) => format!("`{}`", t.kind()),
            None => "end of file".to_string(),
        }
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        super::diag::capped(&mut self.errors, &mut self.suppressed, || ParseError {
            span,
            message: message.into(),
        });
    }

    /// Consume `tok` or record an error and bail.
    fn expect(&mut self, tok: Token<'_>, ctx: &str) -> Rule<Span> {
        if self.at(tok) {
            let span = self.span();
            self.pos += 1;
            return Ok(span);
        }

        let msg = format!("expected `{}` {ctx}, found {}", tok.kind(), self.describe());
        let span = self.span();
        self.error(span, msg);

        Err(Bail)
    }

    fn expect_kw(&mut self, kw: Keyword, ctx: &str) -> Rule<Span> {
        self.expect(Token::Kw(kw), ctx)
    }

    fn expect_ident(&mut self, ctx: &str) -> Rule<&'a str> {
        if let Some(Token::Ident(name)) = self.peek() {
            self.pos += 1;
            return Ok(name);
        }

        let msg = format!("expected a name {ctx}, found {}", self.describe());
        let span = self.span();
        self.error(span, msg);

        Err(Bail)
    }
}

// --------------------------------------------------------------------------
// Top level
// --------------------------------------------------------------------------

impl<'a> Parser<'a> {
    fn script(&mut self) -> Script<'a> {
        let mut items = Vec::new();

        while self.peek().is_some() {
            let start = self.pos;

            match self.item() {
                Ok(item) => items.push(item),
                Err(Bail) => self.recover_to_item(),
            }

            // Force progress even where a rule bailed immediately.
            if self.pos == start {
                self.pos += 1;
            }
        }

        Script { items }
    }

    fn item(&mut self) -> Rule<Item<'a>> {
        match self.peek() {
            Some(Token::Kw(Keyword::Function)) => self.function_definition().map(Item::Function),
            Some(Token::Kw(Keyword::Prototype)) => self.prototype_definition().map(Item::Prototype),
            Some(Token::Kw(Keyword::Expression)) => self.constant_definition().map(Item::Constant),
            _ => {
                let msg = format!(
                    "expected `function`, `prototype` or `expression`, found {}",
                    self.describe()
                );
                let span = self.span();
                self.error(span, msg);
                Err(Bail)
            }
        }
    }

    /// Skip ahead to the next plausible top-level definition.
    fn recover_to_item(&mut self) {
        while let Some(t) = self.peek() {
            if matches!(
                t,
                Token::Kw(Keyword::Function | Keyword::Prototype | Keyword::Expression)
            ) {
                return;
            }

            self.pos += 1;
        }
    }

    fn function_definition(&mut self) -> Rule<FunctionDef<'a>> {
        let start = self.expect_kw(Keyword::Function, "to start a definition")?;
        let name = self.expect_ident("after `function`")?;
        let params = self.parameters()?;

        let ret = if self.eat_kw(Keyword::Is) {
            Some(self.type_specifier("after `is`")?)
        } else {
            None
        };

        let (decls, declare_at) = self.declaration_block()?;
        let body = self.compound_statement()?;

        Ok(FunctionDef {
            name,
            params,
            ret,
            decls,
            declare_at,
            body,
            span: start.start..self.prev_span().end,
        })
    }

    fn prototype_definition(&mut self) -> Rule<PrototypeDef<'a>> {
        let start = self.expect_kw(Keyword::Prototype, "to start a definition")?;
        let name = self.expect_ident("after `prototype`")?;
        let params = self.parameters()?;
        let (decls, declare_at) = self.declaration_block()?;
        let body = self.compound_statement()?;

        Ok(PrototypeDef {
            name,
            params,
            decls,
            declare_at,
            body,
            span: start.start..self.prev_span().end,
        })
    }

    /// `expression #Name <expr>` — no `=` and no terminator.
    fn constant_definition(&mut self) -> Rule<ConstantDef<'a>> {
        let start = self.expect_kw(Keyword::Expression, "to start a definition")?;

        let Some(Token::Expr(name)) = self.peek() else {
            let msg = format!(
                "expected `#name` after `expression`, found {}",
                self.describe()
            );
            let span = self.span();
            self.error(span, msg);
            return Err(Bail);
        };
        self.pos += 1;

        let value = self.expression()?;

        Ok(ConstantDef {
            name,
            value,
            span: start.start..self.prev_span().end,
        })
    }

    fn parameters(&mut self) -> Rule<Vec<Param<'a>>> {
        self.expect(Token::LParen, "to open a parameter list")?;

        let mut params = Vec::new();

        if !self.at(Token::RParen) {
            loop {
                let ty = self.type_specifier("in a parameter list")?;
                let span = self.span();
                let name = self.expect_ident("for a parameter")?;

                params.push(Param {
                    ty,
                    name,
                    span: span.start..self.prev_span().end,
                });

                if !self.eat(Token::Comma) {
                    break;
                }
            }
        }

        self.expect(Token::RParen, "to close a parameter list")?;

        Ok(params)
    }

    /// The `declare` block. Optional per the grammar; present on every corpus
    /// definition, empty in 641.
    ///
    /// Also returns the keyword's span: with nothing declared, the compiled
    /// body-boundary record carries its line. See `gpl::emit::prologue`.
    fn declaration_block(&mut self) -> Rule<(Vec<Decl<'a>>, Option<Span>)> {
        let at = self.span();

        if !self.eat_kw(Keyword::Declare) {
            return Ok((Vec::new(), None));
        }

        let mut decls = Vec::new();

        while self.peek().is_some_and(is_type_keyword) {
            decls.push(self.compound_declaration()?);
        }

        Ok((decls, Some(at)))
    }

    /// `type name ( ',' name )* ';'`
    fn compound_declaration(&mut self) -> Rule<Decl<'a>> {
        let start = self.span();
        let ty = self.type_specifier("to start a declaration")?;
        let mut names = Vec::new();

        loop {
            let span = self.span();
            let name = self.expect_ident("in a declaration")?;
            names.push((name, span));

            if !self.eat(Token::Comma) {
                break;
            }
        }

        self.expect(Token::Semi, "after a declaration")?;

        Ok(Decl {
            ty,
            names,
            span: start.start..self.prev_span().end,
        })
    }

    fn type_specifier(&mut self, ctx: &str) -> Rule<Type> {
        let ty = match self.peek() {
            Some(Token::Kw(Keyword::Agent)) => Type::Agent,
            Some(Token::Kw(Keyword::Boolean)) => Type::Boolean,
            Some(Token::Kw(Keyword::Coordinate)) => Type::Coordinate,
            Some(Token::Kw(Keyword::Float)) => Type::Float,
            Some(Token::Kw(Keyword::Function)) => Type::Function,
            Some(Token::Kw(Keyword::Integer)) => Type::Integer,
            Some(Token::Kw(Keyword::List)) => Type::List,
            Some(Token::Kw(Keyword::String)) => Type::String,
            _ => {
                let msg = format!("expected a type {ctx}, found {}", self.describe());
                let span = self.span();
                self.error(span, msg);
                return Err(Bail);
            }
        };

        self.pos += 1;

        Ok(ty)
    }
}

const fn is_type_keyword(t: Token<'_>) -> bool {
    matches!(t, Token::Kw(kw) if kw.is_type())
}

// --------------------------------------------------------------------------
// Statements
// --------------------------------------------------------------------------

impl<'a> Parser<'a> {
    fn compound_statement(&mut self) -> Rule<Vec<Stmt<'a>>> {
        self.expect_kw(Keyword::Begin, "to open a block")?;

        let mut body = Vec::new();

        while !self.at_kw(Keyword::End) {
            if self.peek().is_none() {
                let span = self.span();
                self.error(span, "unclosed block: reached end of file expecting `end`");
                return Err(Bail);
            }

            let start = self.pos;

            match self.statement() {
                Ok(s) => body.push(s),
                Err(Bail) => self.recover_in_block(),
            }

            if self.pos == start {
                self.pos += 1;
            }
        }

        self.expect_kw(Keyword::End, "to close a block")?;

        Ok(body)
    }

    /// After a bad statement, skip to the next `;` or to a block boundary.
    fn recover_in_block(&mut self) {
        while let Some(t) = self.peek() {
            match t {
                Token::Semi => {
                    self.pos += 1;
                    return;
                }
                // Block or item boundary: end the statement, consume nothing.
                Token::Kw(
                    Keyword::End
                    | Keyword::Begin
                    | Keyword::Function
                    | Keyword::Prototype
                    | Keyword::Expression,
                ) => return,
                _ => self.pos += 1,
            }
        }
    }

    fn statement(&mut self) -> Rule<Stmt<'a>> {
        let start = self.span();

        let kind = match self.peek() {
            Some(Token::Kw(Keyword::Begin)) => StmtKind::Block(self.compound_statement()?),
            Some(Token::Kw(Keyword::If)) => return self.selection_statement(),
            Some(Token::Kw(Keyword::While | Keyword::Do | Keyword::Foreach)) => {
                return self.iteration_statement();
            }
            Some(Token::Kw(Keyword::Break)) => {
                self.pos += 1;
                self.expect(Token::Semi, "after `break`")?;
                StmtKind::Break
            }
            Some(Token::Kw(Keyword::Continue)) => {
                self.pos += 1;
                self.expect(Token::Semi, "after `continue`")?;
                StmtKind::Continue
            }
            Some(Token::Kw(Keyword::Return)) => return self.return_statement(),
            _ => {
                let e = self.expression()?;
                self.expect(Token::Semi, "after a statement")?;
                StmtKind::Expr(e)
            }
        };

        Ok(Stmt {
            kind,
            span: start.start..self.prev_span().end,
        })
    }

    fn selection_statement(&mut self) -> Rule<Stmt<'a>> {
        let start = self.expect_kw(Keyword::If, "to start a conditional")?;
        self.expect(Token::LParen, "after `if`")?;
        let cond = self.expression()?;
        self.expect(Token::RParen, "after an `if` condition")?;

        let then = Box::new(self.statement()?);

        // Falling through here binds `else` to the nearest unmatched `if`.
        let els = if self.eat_kw(Keyword::Else) {
            Some(Box::new(self.statement()?))
        } else {
            None
        };

        Ok(Stmt {
            kind: StmtKind::If { cond, then, els },
            span: start.start..self.prev_span().end,
        })
    }

    fn iteration_statement(&mut self) -> Rule<Stmt<'a>> {
        let start = self.span();

        let kind = match self.peek() {
            Some(Token::Kw(Keyword::While)) => {
                self.pos += 1;
                self.expect(Token::LParen, "after `while`")?;
                let cond = self.expression()?;
                self.expect(Token::RParen, "after a `while` condition")?;
                self.expect_kw(Keyword::Do, "after a `while` condition")?;

                StmtKind::While {
                    cond,
                    body: Box::new(self.statement()?),
                }
            }

            // Permitted by the grammar; no source uses it, shipped or modded.
            Some(Token::Kw(Keyword::Do)) => {
                self.pos += 1;
                let body = Box::new(self.statement()?);
                self.expect_kw(Keyword::While, "after a `do` body")?;
                self.expect(Token::LParen, "after `while`")?;
                let cond = self.expression()?;
                self.expect(Token::RParen, "after a `while` condition")?;
                self.expect(Token::Semi, "after `do ... while`")?;

                StmtKind::DoWhile { body, cond }
            }

            _ => {
                self.expect_kw(Keyword::Foreach, "to start a loop")?;
                let var = self.expression()?;
                self.expect_kw(Keyword::In, "after a `foreach` variable")?;
                let iter = self.expression()?;
                self.expect_kw(Keyword::Do, "after a `foreach` collection")?;

                StmtKind::Foreach {
                    var,
                    iter,
                    body: Box::new(self.statement()?),
                }
            }
        };

        Ok(Stmt {
            kind,
            span: start.start..self.prev_span().end,
        })
    }

    /// `return [expr] ';'`
    ///
    /// The recovered grammar requires the expression; 236 corpus statements
    /// write a bare `return;`, so it is optional here.
    fn return_statement(&mut self) -> Rule<Stmt<'a>> {
        let start = self.expect_kw(Keyword::Return, "to start a return")?;

        let value = if self.at(Token::Semi) {
            None
        } else {
            Some(self.expression()?)
        };

        self.expect(Token::Semi, "after `return`")?;

        Ok(Stmt {
            kind: StmtKind::Return(value),
            span: start.start..self.prev_span().end,
        })
    }
}

// --------------------------------------------------------------------------
// Expressions, one function per precedence tier
// --------------------------------------------------------------------------

/// Build a left-associative binary tier from a token-to-operator mapping.
macro_rules! binary_tier {
    ($name:ident, $next:ident, $( $pat:pat => $op:expr ),+ $(,)?) => {
        fn $name(&mut self) -> Rule<Expr<'a>> {
            let mut lhs = self.$next()?;

            loop {
                let op = match self.peek() {
                    $( Some($pat) => $op, )+
                    _ => break,
                };

                self.pos += 1;
                let rhs = self.$next()?;
                let span = lhs.span.start..rhs.span.end;

                lhs = Expr {
                    kind: ExprKind::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    span,
                };
            }

            Ok(lhs)
        }
    };
}

impl<'a> Parser<'a> {
    fn expression(&mut self) -> Rule<Expr<'a>> {
        self.assignment_expression()
    }

    /// Right-associative. `<<`, list append, belongs to this tier.
    fn assignment_expression(&mut self) -> Rule<Expr<'a>> {
        let lhs = self.logical_or_expression()?;

        let op = match self.peek() {
            Some(Token::Assign) => AssignOp::Assign,
            Some(Token::PlusEq) => AssignOp::Add,
            Some(Token::MinusEq) => AssignOp::Sub,
            Some(Token::StarEq) => AssignOp::Mul,
            Some(Token::SlashEq) => AssignOp::Div,
            Some(Token::PercentEq) => AssignOp::Rem,
            Some(Token::Append) => AssignOp::Append,
            _ => return Ok(lhs),
        };

        self.pos += 1;
        let rhs = self.assignment_expression()?;
        let span = lhs.span.start..rhs.span.end;

        Ok(Expr {
            kind: ExprKind::Assign {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        })
    }

    binary_tier!(logical_or_expression, logical_and_expression,
        Token::OrOr => BinOp::Or);

    binary_tier!(logical_and_expression, equality_expression,
        Token::AndAnd => BinOp::And);

    binary_tier!(equality_expression, relational_expression,
        Token::Eq => BinOp::Eq, Token::Ne => BinOp::Ne);

    binary_tier!(relational_expression, additive_expression,
        Token::Lt => BinOp::Lt, Token::Gt => BinOp::Gt,
        Token::Le => BinOp::Le, Token::Ge => BinOp::Ge);

    binary_tier!(additive_expression, multiplicative_expression,
        Token::Plus => BinOp::Add, Token::Minus => BinOp::Sub);

    binary_tier!(multiplicative_expression, prefix_expression,
        Token::Star => BinOp::Mul, Token::Slash => BinOp::Div,
        Token::Percent => BinOp::Rem);

    /// Unary `-` and `+` only. GPL has no logical-not and no prefix `++`/`--`.
    ///
    /// `!` is a compiler token (`NOT`) no rule matches: a syntax error there, a
    /// lexical error here. Rejected either way.
    fn prefix_expression(&mut self) -> Rule<Expr<'a>> {
        let op = match self.peek() {
            Some(Token::Minus) => UnOp::Neg,
            Some(Token::Plus) => UnOp::Pos,
            _ => return self.postfix_expression(),
        };

        let start = self.span();
        self.pos += 1;
        let operand = self.prefix_expression()?;

        Ok(Expr {
            kind: ExprKind::Unary {
                op,
                operand: Box::new(operand),
            },
            span: start.start..self.prev_span().end,
        })
    }

    /// `accessor [ '(' args ')' ] [ '++' | '--' ]`
    ///
    /// The call is handled here, not in `primary`, so all three callee forms
    /// work alike: `$name(...)`, `(expr)(...)`, and `agent's "attr"(...)` (five
    /// corpus sites).
    fn postfix_expression(&mut self) -> Rule<Expr<'a>> {
        let mut e = self.accessor_expression()?;

        if self.at(Token::LParen) {
            self.pos += 1;
            let args = self.argument_expression_list()?;
            self.expect(Token::RParen, "to close an argument list")?;

            let span = e.span.start..self.prev_span().end;
            e = Expr {
                kind: ExprKind::Call {
                    callee: Box::new(e),
                    args,
                },
                span,
            };
        }

        let op = match self.peek() {
            Some(Token::PlusPlus) => PostOp::Inc,
            Some(Token::MinusMinus) => PostOp::Dec,
            _ => return Ok(e),
        };

        self.pos += 1;
        let span = e.span.start..self.prev_span().end;

        Ok(Expr {
            kind: ExprKind::Postfix {
                op,
                operand: Box::new(e),
            },
            span,
        })
    }

    /// `primary ( "'s" primary )*` — the attribute accessor.
    ///
    /// `IDENT "'s" STRING` covers the shipped corpus. Workshop mods chain
    /// accessors (`a's "b"'s "c"`, 157 sites) and compute the attribute
    /// (`x's funcName`, `x's ("prefix" + n)`, 646 sites), hence the loop and a
    /// full `primary` on the right.
    fn accessor_expression(&mut self) -> Rule<Expr<'a>> {
        let mut obj = self.primary_expression()?;

        while self.eat(Token::Poss) {
            let attr = self.primary_expression()?;
            let span = obj.span.start..attr.span.end;

            obj = Expr {
                kind: ExprKind::Attr {
                    obj: Box::new(obj),
                    attr: Box::new(attr),
                },
                span,
            };
        }

        Ok(obj)
    }

    fn primary_expression(&mut self) -> Rule<Expr<'a>> {
        let span = self.span();

        let kind = match self.peek() {
            Some(Token::Str(s)) => ExprKind::Str(s),
            Some(Token::Int(n)) => ExprKind::Int(n),
            Some(Token::Float(f)) => ExprKind::Float(f),
            Some(Token::Kw(Keyword::True)) => ExprKind::Bool(true),
            Some(Token::Kw(Keyword::False)) => ExprKind::Bool(false),
            Some(Token::Ident(name)) => ExprKind::Ident(name),

            // A `$name` with no argument list is a function reference: 1,232
            // corpus sites. `postfix_expression` applies any call.
            Some(Token::Call(name)) => ExprKind::Func(name),
            Some(Token::Expr(name)) => ExprKind::Const(name),

            // Parentheses create no node.
            Some(Token::LParen) => {
                let open = self.span();
                self.pos += 1;

                let mut inner = self.expression()?;
                let close = self.expect(Token::RParen, "to close a parenthesised expression")?;

                // The brackets join the span: the compiler gives a
                // parenthesised leaf its own slot. See `gpl::emit`.
                inner.span = open.start..close.end;

                return Ok(inner);
            }

            _ => {
                let msg = format!("expected an expression, found {}", self.describe());
                self.error(span, msg);
                return Err(Bail);
            }
        };

        self.pos += 1;

        Ok(Expr { kind, span })
    }

    fn argument_expression_list(&mut self) -> Rule<Vec<Expr<'a>>> {
        let mut args = Vec::new();

        if self.at(Token::RParen) {
            return Ok(args);
        }

        loop {
            args.push(self.expression()?);

            if !self.eat(Token::Comma) {
                return Ok(args);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a whole file, asserting it is clean.
    fn ok(src: &str) -> Script<'_> {
        let p = parse(src);
        assert!(p.is_ok(), "unexpected errors: {:?}", p.errors);
        p.script
    }

    /// Wrap `body` in a minimal function and return its statements.
    ///
    /// Leaks the source so the tree can borrow from it.
    fn stmts(body: &str) -> Vec<Stmt<'static>> {
        let src: &'static str =
            Box::leak(format!("function F()\ndeclare\nbegin\n{body}\nend").into_boxed_str());

        match ok(src).items.into_iter().next() {
            Some(Item::Function(f)) => f.body,
            other => panic!("expected a function, got {other:?}"),
        }
    }

    /// The single expression of a one-statement body.
    fn expr(body: &str) -> Expr<'static> {
        let mut s = stmts(body);
        assert_eq!(s.len(), 1, "expected exactly one statement");

        match s.remove(0).kind {
            StmtKind::Expr(e) => e,
            other => panic!("expected an expression statement, got {other:?}"),
        }
    }

    #[test]
    fn three_top_level_forms() {
        let s = ok("expression #N 1\nprototype P()\ndeclare\nbegin\nend\n\
                    function F()\ndeclare\nbegin\nend");

        assert_eq!(s.items.len(), 3);
        assert!(matches!(s.items[0], Item::Constant(_)));
        assert!(matches!(s.items[1], Item::Prototype(_)));
        assert!(matches!(s.items[2], Item::Function(_)));
    }

    #[test]
    fn constant_definition_has_no_terminator() {
        // `expression #Name <value>` takes neither `=` nor `;`.
        let s = ok("expression #A 1\nexpression #B 2");
        assert_eq!(s.items.len(), 2);

        match &s.items[0] {
            Item::Constant(c) => {
                assert_eq!(c.name, "A");
                assert_eq!(c.value.kind, ExprKind::Int(1));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn function_signature_and_declares() {
        let s = ok("function F(agent a, integer n) is boolean\n\
                    declare\n integer i, j, k;\n list xs;\nbegin\nend");

        let Item::Function(f) = &s.items[0] else {
            panic!()
        };

        assert_eq!(f.name, "F");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].ty, Type::Agent);
        assert_eq!(f.ret, Some(Type::Boolean));
        assert_eq!(f.decls.len(), 2);
        // One line may declare several names. 898 corpus lines do.
        assert_eq!(f.decls[0].names.len(), 3);
    }

    #[test]
    fn empty_declare_block_is_allowed() {
        // 641 corpus functions have `declare` immediately followed by `begin`.
        let s = ok("function F()\ndeclare\nbegin\nend");
        let Item::Function(f) = &s.items[0] else {
            panic!()
        };

        assert_eq!(f.decls.len(), 0);
        assert_eq!(f.params.len(), 0);
        assert_eq!(f.ret, None);
    }

    #[test]
    fn dollar_name_without_parens_is_a_reference() {
        // 1,232 corpus sites pass a function as a value rather than calling it.
        let ExprKind::Assign { rhs, .. } = expr("x = $Some_Script;").kind else {
            panic!("expected an assignment")
        };

        assert_eq!(rhs.kind, ExprKind::Func("Some_Script"));

        // With parentheses it becomes a call instead.
        assert!(matches!(
            expr("$Some_Script();").kind,
            ExprKind::Call { .. }
        ));
    }

    #[test]
    fn all_three_callee_forms() {
        // $name(...)
        let ExprKind::Call { callee, args } = expr("$Foo(1, 2);").kind else {
            panic!()
        };
        assert_eq!(callee.kind, ExprKind::Func("Foo"));
        assert_eq!(args.len(), 2);

        // (expr)(...) — 57 corpus sites
        let ExprKind::Call { callee, .. } = expr(r#"( u's "IGDeathScript" )( u );"#).kind else {
            panic!()
        };
        assert!(matches!(callee.kind, ExprKind::Attr { .. }));

        // id's "attr"(...) — 5 corpus sites; the form most likely to break.
        let ExprKind::Call { callee, args } = expr(r#"x's "ActiveScript"( x );"#).kind else {
            panic!()
        };
        assert!(matches!(callee.kind, ExprKind::Attr { .. }));
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn accessor_attribute_need_not_be_a_literal() {
        // No shipped source; 646 workshop mod sites. The name is evaluated at
        // run time.
        let ExprKind::Attr { attr, .. } = rhs_of("x = ThisAgent's funcName;").kind else {
            panic!()
        };
        assert_eq!(attr.kind, ExprKind::Ident("funcName"));

        let ExprKind::Attr { attr, .. } =
            rhs_of(r#"x = a's ( "TaskPackageDestination" + n );"#).kind
        else {
            panic!()
        };
        assert!(matches!(attr.kind, ExprKind::Binary { .. }));
    }

    #[test]
    fn accessors_chain() {
        // 157 mod sites walk from one agent to another; one omits the
        // whitespace: `thisagent's"cusa5"'s"extraint11"--;`.
        let ExprKind::Attr { obj, attr } = rhs_of(r#"x = a's"b"'s"c";"#).kind else {
            panic!()
        };

        assert_eq!(attr.kind, ExprKind::Str("c"));
        assert!(matches!(obj.kind, ExprKind::Attr { .. }));
    }

    #[test]
    fn postfix_on_parenthesised_lvalue() {
        // 41 corpus sites; misparsed if a leading `(` is taken as an indirect
        // call.
        let e = expr(r#"( a's "Counter" )++;"#);

        let ExprKind::Postfix { op, operand } = e.kind else {
            panic!()
        };
        assert_eq!(op, PostOp::Inc);
        assert!(matches!(operand.kind, ExprKind::Attr { .. }));
    }

    #[test]
    fn append_is_an_assignment_not_a_shift() {
        let ExprKind::Assign { op, .. } = expr("xs << y;").kind else {
            panic!()
        };
        assert_eq!(op, AssignOp::Append);
    }

    #[test]
    fn assignment_is_right_associative() {
        let ExprKind::Assign { rhs, .. } = expr("a = b = c;").kind else {
            panic!()
        };
        assert!(matches!(rhs.kind, ExprKind::Assign { .. }));
    }

    #[test]
    fn assignment_inside_a_condition_is_legal() {
        // Two corpus sites write `if (x's "flag" = True)`. Assignment is an
        // expression, so the grammar permits it.
        let s = stmts(r#"if (a's "f" = true) begin end"#);
        let StmtKind::If { cond, .. } = &s[0].kind else {
            panic!()
        };

        assert!(matches!(cond.kind, ExprKind::Assign { .. }));
    }

    #[test]
    fn bare_expression_statement_is_legal() {
        // Two corpus sites contain a valueless binary expression.
        let e = expr("a - b;");
        assert!(matches!(e.kind, ExprKind::Binary { op: BinOp::Sub, .. }));
    }

    /// The right-hand side of `x = <e>;`, for testing expression shapes.
    fn rhs_of(body: &str) -> Expr<'static> {
        let ExprKind::Assign { rhs, .. } = expr(body).kind else {
            panic!("expected an assignment")
        };

        *rhs
    }

    #[test]
    fn multiplicative_binds_tighter_than_additive() {
        // a + b * c  =>  a + (b * c)
        let ExprKind::Binary { op, rhs, .. } = rhs_of("x = a + b * c;").kind else {
            panic!()
        };

        assert_eq!(op, BinOp::Add);
        assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::Mul, .. }));
    }

    #[test]
    fn logical_binds_looser_than_comparison() {
        // a == b && c == d  =>  (a == b) && (c == d)
        let ExprKind::Binary { op, lhs, rhs } = rhs_of("q = a == b && c == d;").kind else {
            panic!()
        };

        assert_eq!(op, BinOp::And);
        assert!(matches!(lhs.kind, ExprKind::Binary { op: BinOp::Eq, .. }));
        assert!(matches!(rhs.kind, ExprKind::Binary { op: BinOp::Eq, .. }));
    }

    #[test]
    fn or_binds_looser_than_and() {
        // a && b || c  =>  (a && b) || c
        let ExprKind::Binary { op, lhs, .. } = rhs_of("q = a && b || c;").kind else {
            panic!()
        };

        assert_eq!(op, BinOp::Or);
        assert!(matches!(lhs.kind, ExprKind::Binary { op: BinOp::And, .. }));
    }

    #[test]
    fn unary_minus_takes_a_full_operand() {
        // 7 corpus sites negate a parenthesised expression, not just a literal.
        let ExprKind::Unary { op, operand } = rhs_of("x = -( a + b );").kind else {
            panic!()
        };

        assert_eq!(op, UnOp::Neg);
        assert!(matches!(operand.kind, ExprKind::Binary { .. }));
    }

    #[test]
    fn dangling_else_binds_to_the_nearest_if() {
        // 801 corpus sites nest an unbraced `if`; 3 are ambiguous.
        let s = stmts("if (a) if (b) x = 1; else x = 2;");

        let StmtKind::If { then, els, .. } = &s[0].kind else {
            panic!()
        };
        assert!(els.is_none(), "outer `if` must not take the `else`");

        let StmtKind::If { els: inner, .. } = &then.kind else {
            panic!()
        };
        assert!(inner.is_some(), "inner `if` should own the `else`");
    }

    #[test]
    fn else_if_chains() {
        // 616 corpus sites; there is no `elseif` keyword.
        let s = stmts("if (a) begin end else if (b) begin end else begin end");
        let StmtKind::If { els, .. } = &s[0].kind else {
            panic!()
        };
        assert!(matches!(
            els.as_deref().map(|s| &s.kind),
            Some(StmtKind::If { .. })
        ));
    }

    #[test]
    fn loops_require_do() {
        let s = stmts("foreach m in xs do begin end while (a) do begin end");

        assert!(matches!(s[0].kind, StmtKind::Foreach { .. }));
        assert!(matches!(s[1].kind, StmtKind::While { .. }));
    }

    #[test]
    fn foreach_over_an_attribute() {
        // 37 corpus sites iterate `home's "some_list"`.
        let s = stmts(r#"foreach m in home's "members" do begin end"#);
        let StmtKind::Foreach { iter, .. } = &s[0].kind else {
            panic!()
        };

        assert!(matches!(iter.kind, ExprKind::Attr { .. }));
    }

    #[test]
    fn return_with_and_without_a_value() {
        let s = stmts("return 1; return;");

        assert!(matches!(s[0].kind, StmtKind::Return(Some(_))));
        assert!(matches!(s[1].kind, StmtKind::Return(None)));
    }

    #[test]
    fn grammar_forms_absent_from_the_corpus() {
        // The compiler supports them. Mods use `break` twice, `continue` once;
        // no source uses `do ... while`.
        let s = stmts("break; continue; do begin end while (a);");

        assert!(matches!(s[0].kind, StmtKind::Break));
        assert!(matches!(s[1].kind, StmtKind::Continue));
        assert!(matches!(s[2].kind, StmtKind::DoWhile { .. }));

        assert!(matches!(expr("x %= 2;").kind, ExprKind::Assign { .. }));
    }

    #[test]
    fn operator_words_are_names_not_operators() {
        // `mod` is a local in several compiling workshop mods. Same for `and`,
        // `or`, `not`: the compiler tokens of those names spell `&&`, `||`, `!`.
        let s = ok("function F(agent mod)\ndeclare\n agent and, or, not;\nbegin\nend");

        let Item::Function(f) = &s.items[0] else {
            panic!()
        };
        assert_eq!(f.params[0].name, "mod");
        assert_eq!(f.decls[0].names.len(), 3);

        // And as an ordinary operand, including on the left of `'s`.
        let ExprKind::Assign { rhs, .. } = expr(r#"x = mod's "hp";"#).kind else {
            panic!()
        };
        assert!(matches!(rhs.kind, ExprKind::Attr { .. }));
    }

    #[test]
    fn a_bare_bang_is_rejected() {
        // The compiler lexes `!` as `NOT`, which no rule matches. Here the
        // lexer rejects it.
        let p = parse("function F()\ndeclare\nbegin\n x = !a;\nend");

        assert!(!p.is_ok());
    }

    #[test]
    fn errors_are_collected_and_recovered_from() {
        // A bad statement must not swallow the rest of the file.
        let p = parse(
            "function A()\ndeclare\nbegin\n x = ;\nend\n\
             function B()\ndeclare\nbegin\n y = 1;\nend",
        );

        assert!(!p.is_ok());
        assert_eq!(p.script.items.len(), 2, "both functions should survive");
    }

    #[test]
    fn unclosed_block_reports_rather_than_hangs() {
        let p = parse("function F()\ndeclare\nbegin\n x = 1;");

        assert!(!p.is_ok());
        assert!(p.errors.iter().any(|e| e.message.contains("end")));
    }

    #[test]
    fn spans_cover_the_definition() {
        let src = "function Foo()\ndeclare\nbegin\nend";
        let s = ok(src);
        let Item::Function(f) = &s.items[0] else {
            panic!()
        };

        assert_eq!(&src[f.span.clone()], src);
    }
}
