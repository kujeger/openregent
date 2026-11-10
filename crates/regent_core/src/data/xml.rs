//! A small XML element tree, for the files mods add: `<Descriptions>`
//! and `<Strings>` ([`super::desc::xml`], [`super::text`]).
//!
//! Elements, attributes and text; comments, the `<?xml ?>` line, a
//! `<!DOCTYPE>`, CDATA, a UTF-8 byte order mark and the five named
//! entities with numeric ones. No namespaces, no DTD. Names are compared
//! as written (**Inferred**: HD's `CyXml` is case-sensitive; every
//! Workshop file spells its names one way).
//!
//! A file that will not parse is refused whole, as HD skips it
//! (the parse failing in desc set load xml and the strings
//! loader strings file load).

use std::borrow::Cow;

use super::error::{Error, Result};

/// One element: its name, attributes in order, child elements and text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element<'a> {
    pub name: &'a str,
    pub attributes: Vec<(&'a str, Cow<'a, str>)>,
    pub children: Vec<Element<'a>>,
    /// Every text run directly inside, joined; entities decoded.
    pub text: String,
}

impl<'a> Element<'a> {
    /// Attribute `name`'s value.
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_ref())
    }

    /// The first child element named `name`.
    #[must_use]
    pub fn child(&self, name: &str) -> Option<&Element<'a>> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Every child element named `name`, in order.
    pub fn named<'s>(&'s self, name: &'s str) -> impl Iterator<Item = &'s Element<'a>> {
        self.children.iter().filter(move |c| c.name == name)
    }
}

/// Read a document: its root element.
///
/// # Errors
///
/// Anything not well-formed: a tag never closed or closed by another
/// name, an attribute without a quoted value, an unknown entity, text
/// after the root. The value is the byte offset.
pub fn parse(src: &str) -> Result<Element<'_>> {
    let mut r = Reader {
        src,
        at: usize::from(src.starts_with('\u{feff}')) * '\u{feff}'.len_utf8(),
    };

    r.misc()?;

    let root = r.element()?;

    r.misc()?;

    if r.at == src.len() {
        Ok(root)
    } else {
        Err(r.error("text after the root element"))
    }
}

/// A cursor over the document.
struct Reader<'a> {
    src: &'a str,
    at: usize,
}

impl<'a> Reader<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.at..]
    }

    const fn error(&self, what: &'static str) -> Error {
        Error::BadValue {
            what,
            value: self.at as u64,
        }
    }

    fn skip_space(&mut self) {
        let rest = self.rest();

        self.at += rest.len() - rest.trim_start().len();
    }

    /// Past `end`, which must come.
    fn skip_past(&mut self, end: &str, what: &'static str) -> Result<&'a str> {
        let rest = self.rest();
        let found = rest.find(end).ok_or_else(|| self.error(what))?;

        self.at += found + end.len();
        Ok(&rest[..found])
    }

    /// Space, comments, `<?…?>` and `<!DOCTYPE …>` between elements.
    fn misc(&mut self) -> Result<()> {
        loop {
            self.skip_space();

            let rest = self.rest();

            if rest.starts_with("<!--") {
                self.skip_past("-->", "an unclosed comment")?;
            } else if rest.starts_with("<?") {
                self.skip_past("?>", "an unclosed declaration")?;
            } else if rest.starts_with("<!DOCTYPE") {
                self.skip_past(">", "an unclosed doctype")?;
            } else {
                return Ok(());
            }
        }
    }

    /// A name: up to space, `/`, `>`, `=` or the end.
    fn name(&mut self) -> Result<&'a str> {
        let rest = self.rest();
        let len = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '/' | '>' | '=' | '<'))
            .unwrap_or(rest.len());

        if len == 0 {
            Err(self.error("a missing name"))
        } else {
            self.at += len;
            Ok(&rest[..len])
        }
    }

    /// The element starting at `<`.
    fn element(&mut self) -> Result<Element<'a>> {
        if !self.rest().starts_with('<') {
            return Err(self.error("a missing element"));
        }

        self.at += 1;

        let name = self.name()?;
        let mut out = Element {
            name,
            attributes: Vec::new(),
            children: Vec::new(),
            text: String::new(),
        };

        // Attributes, up to `>` or `/>`.
        loop {
            self.skip_space();

            let rest = self.rest();

            if let Some(after) = rest.strip_prefix("/>") {
                self.at = self.src.len() - after.len();
                return Ok(out);
            }

            if rest.starts_with('>') {
                self.at += 1;
                break;
            }

            let key = self.name()?;

            self.skip_space();

            if !self.rest().starts_with('=') {
                return Err(self.error("an attribute without a value"));
            }

            self.at += 1;
            self.skip_space();

            let quote = self
                .rest()
                .chars()
                .next()
                .filter(|c| matches!(c, '"' | '\''))
                .ok_or_else(|| self.error("an unquoted attribute value"))?;

            self.at += 1;

            let raw = self.skip_past(
                if quote == '"' { "\"" } else { "'" },
                "an unclosed attribute value",
            )?;

            out.attributes.push((key, decode(raw, self.at)?));
        }

        self.content(&mut out)?;
        Ok(out)
    }

    /// An element's content, through its closing tag.
    fn content(&mut self, out: &mut Element<'a>) -> Result<()> {
        loop {
            let rest = self.rest();
            let lt = rest
                .find('<')
                .ok_or_else(|| self.error("an unclosed element"))?;

            out.text.push_str(&decode(&rest[..lt], self.at)?);
            self.at += lt;

            let rest = self.rest();

            if let Some(after) = rest.strip_prefix("</") {
                self.at = self.src.len() - after.len();

                let name = self.name()?;

                self.skip_space();

                if name != out.name || !self.rest().starts_with('>') {
                    return Err(self.error("a closing tag of another name"));
                }

                self.at += 1;
                return Ok(());
            }

            if rest.starts_with("<!--") {
                self.skip_past("-->", "an unclosed comment")?;
            } else if rest.starts_with("<![CDATA[") {
                self.at += "<![CDATA[".len();
                out.text
                    .push_str(self.skip_past("]]>", "an unclosed CDATA section")?);
            } else if rest.starts_with("<?") {
                self.skip_past("?>", "an unclosed declaration")?;
            } else {
                out.children.push(self.element()?);
            }
        }
    }
}

/// `raw` with its entities replaced; `at` locates an error.
fn decode(raw: &str, at: usize) -> Result<Cow<'_, str>> {
    if !raw.contains('&') {
        return Ok(Cow::Borrowed(raw));
    }

    let bad = || Error::BadValue {
        what: "an unknown entity",
        value: at as u64,
    };
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;

    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);

        let after = &rest[amp + 1..];
        let semi = after.find(';').ok_or_else(bad)?;
        let entity = &after[..semi];
        let ch = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => entity
                .strip_prefix("#x")
                .map(|h| u32::from_str_radix(h, 16))
                .or_else(|| entity.strip_prefix('#').map(str::parse))
                .and_then(std::result::Result::ok)
                .and_then(char::from_u32)
                .ok_or_else(bad)?,
        };

        out.push(ch);
        rest = &after[semi + 1..];
    }

    out.push_str(rest);
    Ok(Cow::Owned(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_reads_with_attributes_and_text() {
        let doc = parse(
            "\u{feff}<?xml version=\"1.0\"?>\r\n<!-- a mod -->\r\n<Majesty>\r\n\
             <Description type=\"Unit\" ID='ABE1'><Cost value=\"600\"/></Description>\r\n\
             <Language id=\"en_US\"><Text id=\"x\">a &amp; b&#33;</Text></Language>\r\n\
             </Majesty>\r\n",
        )
        .expect("well-formed");

        assert_eq!(doc.name, "Majesty");
        assert_eq!(doc.children.len(), 2);

        let d = doc.child("Description").expect("a description");

        assert_eq!(d.attribute("ID"), Some("ABE1"));
        assert_eq!(d.attribute("type"), Some("Unit"));
        assert_eq!(
            d.child("Cost").and_then(|c| c.attribute("value")),
            Some("600")
        );

        let text = doc.child("Language").and_then(|l| l.child("Text"));

        assert_eq!(text.map(|t| t.text.as_str()), Some("a & b!"));
    }

    #[test]
    fn comments_and_cdata_sit_among_children() {
        let doc = parse("<a><!-- <b/> --><c/><![CDATA[<x>]]></a>").expect("well-formed");

        assert_eq!(doc.children.len(), 1);
        assert_eq!(doc.text, "<x>");
    }

    #[test]
    fn a_tag_closed_by_another_name_is_refused() {
        assert!(parse("<a><b></a></b>").is_err());
        assert!(parse("<a>").is_err());
        assert!(parse("<a x=1/>").is_err());
        assert!(parse("<a/><b/>").is_err(), "two roots");
        assert!(parse("<a>&nbsp;</a>").is_err());
    }
}
