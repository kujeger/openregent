//! Front end for GPL (Game Play Language), the scripting language for the AI
//! and rules of Majesty: The Fantasy Kingdom Sim (Cyberlore, 2000).
//!
//! Lexer, parser, `.dat` reader, back end, loader and [`vm`]. [`emit`]
//! compiles a project to all three `.bcd` sections; five of the SDK's seven
//! images match `Gplbcc.exe`'s byte for byte. See
//! `a_whole_bcd_is_compiled_from_its_project`.
//!
//! Knows nothing of the world: a builtin that touches it goes to the VM's
//! [`vm::Host`], which [`crate::sim`] provides. Uses only [`crate::base`],
//! [`crate::data`], [`crate::content`] and [`crate::save`].
//!
//! The token set comes from a lexical survey of the SDK's 306 `.gpl` files, not
//! the language reference, whose keyword list omits `is`, `in` and `do`. See
//! `docs/gpl/language.md`.
//!
//! ```
//! let src = r#"if (palace's "gold" >= #Limit) begin $Pay(palace, 100); end"#;
//! let lexed = regent_core::gpl::lex(src);
//!
//! assert!(lexed.is_ok());
//! assert_eq!(lexed.tokens[0].node, regent_core::gpl::Token::Kw(regent_core::gpl::Keyword::If));
//! ```

pub mod ast;
pub mod bcd;
pub mod builtins;
pub mod dat;
pub mod diag;
pub mod emit;
pub mod exec;
pub(crate) mod lexer;
pub mod load;
pub mod opcode;
pub mod ops;
pub(crate) mod parser;
pub mod proj;
pub mod resolve;
pub mod runtime;
pub mod startup;
pub(crate) mod token;
pub mod vm;

pub use lexer::{Span, lex};
pub use parser::{ParseError, parse};
pub use token::{Keyword, Token};

#[cfg(test)]
mod tests {
    use super::*;

    /// Lex `src`, assert no errors, return the tokens.
    fn toks(src: &str) -> Vec<Token<'_>> {
        let lexed = lex(src);
        assert!(lexed.is_ok(), "unexpected errors: {:?}", lexed.errors);
        lexed.tokens.into_iter().map(|t| t.node).collect()
    }

    #[test]
    fn keywords_are_case_insensitive() {
        for spelling in ["begin", "Begin", "BEGIN", "BeGiN"] {
            assert_eq!(toks(spelling), [Token::Kw(Keyword::Begin)], "{spelling}");
        }

        // All three spellings occur in the corpus.
        for spelling in ["true", "True", "TRUE"] {
            assert_eq!(toks(spelling), [Token::Kw(Keyword::True)], "{spelling}");
        }
    }

    #[test]
    fn longer_word_beats_keyword_prefix() {
        assert_eq!(toks("beginning"), [Token::Ident("beginning")]);
        assert_eq!(toks("ends"), [Token::Ident("ends")]);
        assert_eq!(toks("information"), [Token::Ident("information")]);
    }

    #[test]
    fn undocumented_keywords_are_recognised() {
        // Absent from the manual and editor template; 384, 458 and 569 corpus
        // sites.
        assert_eq!(toks("is"), [Token::Kw(Keyword::Is)]);
        assert_eq!(toks("in"), [Token::Kw(Keyword::In)]);
        assert_eq!(toks("do"), [Token::Kw(Keyword::Do)]);
    }

    #[test]
    fn possessive_follows_an_identifier() {
        assert_eq!(
            toks(r#"palace's "gold""#),
            [Token::Ident("palace"), Token::Poss, Token::Str("gold")]
        );
    }

    #[test]
    fn possessive_after_a_digit() {
        // 18 corpus sites.
        assert_eq!(
            toks(r#"EventAgent1's "x""#),
            [Token::Ident("EventAgent1"), Token::Poss, Token::Str("x")]
        );
    }

    #[test]
    fn all_three_float_forms() {
        assert_eq!(toks("100.0"), [Token::Float("100.0")]);
        // Trailing dot, no fraction digits: `$Foo(100.);`
        assert_eq!(
            toks("(100.)"),
            [Token::LParen, Token::Float("100."), Token::RParen]
        );
        // Leading dot with no integer part.
        assert_eq!(toks(".75"), [Token::Float(".75")]);
    }

    #[test]
    fn integers_are_parsed() {
        assert_eq!(
            toks("0 42 10000000"),
            [Token::Int(0), Token::Int(42), Token::Int(10_000_000)]
        );
    }

    #[test]
    fn negative_numbers_are_operator_plus_literal() {
        // GPL has no negative literals; `-` is always an operator.
        assert_eq!(toks("-1"), [Token::Minus, Token::Int(1)]);
    }

    #[test]
    fn append_is_one_token() {
        assert_eq!(
            toks("names << x"),
            [Token::Ident("names"), Token::Append, Token::Ident("x")]
        );
    }

    #[test]
    fn multi_character_operators_lex_whole() {
        assert_eq!(toks("=="), [Token::Eq]);
        assert_eq!(toks("!="), [Token::Ne]);
        assert_eq!(toks("<="), [Token::Le]);
        assert_eq!(toks(">="), [Token::Ge]);
        assert_eq!(toks("&&"), [Token::AndAnd]);
        assert_eq!(toks("||"), [Token::OrOr]);
        assert_eq!(toks("++"), [Token::PlusPlus]);
        assert_eq!(toks("--"), [Token::MinusMinus]);
        assert_eq!(toks("+="), [Token::PlusEq]);
        assert_eq!(toks("-="), [Token::MinusEq]);
        assert_eq!(toks("*="), [Token::StarEq]);
        assert_eq!(toks("/="), [Token::SlashEq]);
        assert_eq!(toks("%"), [Token::Percent]);
    }

    #[test]
    fn sigils_strip_their_prefix() {
        assert_eq!(toks("$GetAttribute"), [Token::Call("GetAttribute")]);
        assert_eq!(toks("#ATTRIB_Strength"), [Token::Expr("ATTRIB_Strength")]);
    }

    #[test]
    fn strings_have_no_escapes_and_keep_spaces() {
        // No backslash anywhere in the corpus.
        assert_eq!(toks(r#""a\b""#), [Token::Str(r"a\b")]);
        assert_eq!(toks(r#"" pad ""#), [Token::Str(" pad ")]);
        assert_eq!(toks(r#""""#), [Token::Str("")]);
    }

    #[test]
    fn comments_are_skipped_and_swallow_quotes() {
        // A `"` inside a comment must not open a string literal.
        assert_eq!(
            toks("a // don't \"quote\" me\nb"),
            [Token::Ident("a"), Token::Ident("b")]
        );
    }

    #[test]
    fn comment_running_to_eof_terminates() {
        // 295 of the 306 shipped files have no final newline.
        assert_eq!(toks("a // trailing"), [Token::Ident("a")]);
    }

    #[test]
    fn crlf_is_whitespace() {
        assert_eq!(
            toks("a\r\n\tb // c\r\nd"),
            [Token::Ident("a"), Token::Ident("b"), Token::Ident("d")]
        );
    }

    #[test]
    fn unterminated_string_is_a_located_error() {
        let lexed = lex("x = \"oops\ny");

        assert!(!lexed.is_ok());
        assert_eq!(lexed.errors.len(), 1);
        // The error points at the stray quote, not the whole file.
        assert_eq!(lexed.errors[0].span.start, 4);
    }

    #[test]
    fn errors_are_capped_but_still_counted() {
        // The cap bounds the recorded list; `error_count` stays exact.
        let junk = "@".repeat(500);
        let lexed = lex(&junk);

        assert_eq!(lexed.errors.len(), lexer::MAX_ERRORS);
        assert_eq!(lexed.error_count(), 500);
        assert_eq!(lexed.suppressed, 500 - lexer::MAX_ERRORS);
        assert!(lexed.is_flooded());
    }

    #[test]
    fn realistic_mistakes_stay_under_the_cap() {
        // The likeliest real error must not trip the flood warning.
        let lexed = lex("x = \"oops\ny = 1;\nz = \"also;\n");

        assert!(!lexed.is_flooded(), "{:?}", lexed.errors);
        assert!(lexed.errors.len() < lexer::MAX_ERRORS);
    }

    #[test]
    fn unknown_characters_are_collected_not_fatal() {
        // `[` and `@` never appear in real GPL: flag both, and lex on past them.
        let lexed = lex("a [ b @ c");

        assert_eq!(lexed.errors.len(), 2);
        assert_eq!(
            lexed.tokens.iter().map(|t| t.node).collect::<Vec<_>>(),
            [Token::Ident("a"), Token::Ident("b"), Token::Ident("c")]
        );
    }

    #[test]
    fn spans_point_at_the_source_text() {
        let src = "alpha beta";
        let lexed = lex(src);

        assert_eq!(&src[lexed.tokens[0].span.clone()], "alpha");
        assert_eq!(&src[lexed.tokens[1].span.clone()], "beta");
    }

    #[test]
    fn function_signature_shape() {
        assert_eq!(
            toks("function Check(agent a) is boolean"),
            [
                Token::Kw(Keyword::Function),
                Token::Ident("Check"),
                Token::LParen,
                Token::Kw(Keyword::Agent),
                Token::Ident("a"),
                Token::RParen,
                Token::Kw(Keyword::Is),
                Token::Kw(Keyword::Boolean),
            ]
        );
    }

    #[test]
    fn foreach_shape() {
        assert_eq!(
            toks("foreach m in monsters do"),
            [
                Token::Kw(Keyword::Foreach),
                Token::Ident("m"),
                Token::Kw(Keyword::In),
                Token::Ident("monsters"),
                Token::Kw(Keyword::Do),
            ]
        );
    }

    #[test]
    fn keyword_helpers() {
        assert!(Keyword::Agent.is_type());
        assert!(
            Keyword::Function.is_type(),
            "function is both a keyword and a type"
        );
        assert!(!Keyword::Begin.is_type());
        assert_eq!(Keyword::Coordinate.as_str(), "coordinate");
    }

    #[test]
    fn agentref_is_not_a_keyword() {
        // The reference lists `agentref` as a type; the compiler has no such
        // token.
        assert_eq!(toks("agentref"), [Token::Ident("agentref")]);
    }

    #[test]
    fn operator_words_are_ordinary_identifiers() {
        // The compiler's `AND`, `OR`, `MOD`, `NOT` name `&&`, `||`, `%`, `!`.
        // Several Workshop mods declare `agent mod;` and compile.
        for word in ["and", "or", "mod", "not"] {
            assert_eq!(toks(word), [Token::Ident(word)], "{word} should be a name");
        }
    }

    #[test]
    fn mod_assign_lexes() {
        assert_eq!(toks("%="), [Token::PercentEq]);
    }

    #[test]
    fn location_reports_line_and_column() {
        assert_eq!(diag::location("abc\ndef", 5), (2, 2));
        assert_eq!(diag::location("abc", 0), (1, 1));
    }
}
