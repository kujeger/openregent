//! Reader for `.qdd` quest metadata — plain text, CRLF, with `//` comments.
//!
//! A flat run of key/value pairs:
//!
//! ```text
//! Name =
//! [QUEST_ID]
//!
//! Short Description =
//! [value, may span several lines]
//! ```
//!
//! A key line ends in `=`; the bracketed value may span lines. `Name` starts a
//! quest block.
//!
//! A shipped comment says the name must match `RGSMissionTemplate`. Ids match
//! `.q` filenames: `barren_waste.q` is `BARREN_WASTE`.

use super::error::{Error, Result};

const NAME_KEY: &str = "Name";

/// One key and its bracketed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: String,
}

/// One quest: the id from its `Name` key, plus every following entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    pub id: String,
    pub fields: Vec<Entry>,
}

impl Quest {
    /// The value of `key`, if the quest has one.
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.value.as_str())
    }
}

/// Parse a whole `.qdd` file into its quest blocks.
///
/// Blank lines and `//` comments are skipped; anything else must be a key
/// line, its bracketed value, or blank filler between the two.
///
/// # Errors
///
/// A line that is none of these, a value never opened or never closed, or
/// an entry before the first `Name`. The error's value is the line number,
/// from 1.
pub fn parse(text: &str) -> Result<Vec<Quest>> {
    let mut quests: Vec<Quest> = Vec::new();

    for (line, entry) in entries(text)? {
        if entry.key == NAME_KEY {
            quests.push(Quest {
                id: entry.value.trim().to_string(),
                fields: Vec::new(),
            });
        } else {
            let current = quests.last_mut().ok_or(Error::BadValue {
                what: "qdd entry before the first Name",
                value: line as u64,
            })?;

            current.fields.push(entry);
        }
    }

    Ok(quests)
}

/// Split the file into key/value entries, each with its key's line number.
fn entries(text: &str) -> Result<Vec<(usize, Entry)>> {
    let mut out = Vec::new();
    let mut lines = text
        .lines()
        .map(strip_comment)
        .enumerate()
        .map(|(n, l)| (n + 1, l));

    while let Some((at, line)) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }

        // Key lines have trailing space: "Name = ".
        let key = line
            .trim_end()
            .strip_suffix('=')
            .map(str::trim)
            .ok_or(Error::BadValue {
                what: "qdd line that is not a key",
                value: at as u64,
            })?;

        out.push((
            at,
            Entry {
                key: key.to_string(),
                value: value(&mut lines, at)?,
            },
        ));
    }

    Ok(out)
}

/// The bracketed value after the key on line `key`: it opens on a later
/// line with `[` and runs until a line ending in `]`.
fn value<'t>(lines: &mut impl Iterator<Item = (usize, &'t str)>, key: usize) -> Result<String> {
    let mut value = String::new();
    let mut open = false;

    for (at, body) in lines {
        let body = match body.trim_start().strip_prefix('[') {
            Some(rest) if !open => {
                open = true;
                rest
            }
            _ if open => body,
            // Blank filler between the key and its value.
            _ if body.trim().is_empty() => continue,
            _ => {
                return Err(Error::BadValue {
                    what: "qdd text before a value opens",
                    value: at as u64,
                });
            }
        };

        // Trim first: a stripped comment can leave trailing spaces after ']'.
        if let Some(last) = body.trim_end().strip_suffix(']') {
            value.push_str(last);

            return Ok(value.trim_end().to_string());
        }

        value.push_str(body);
        value.push('\n');
    }

    Err(Error::BadValue {
        what: if open {
            "qdd value never closed"
        } else {
            "qdd key without a value"
        },
        value: key as u64,
    })
}

/// Drop a trailing `//` comment and any carriage return.
fn strip_comment(line: &str) -> &str {
    let line = line.trim_end_matches('\r');

    match line.find("//") {
        Some(at) => &line[..at],
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_before_the_first_name_is_refused() {
        assert!(matches!(
            parse("Short Description =\r\n[orphan]\r\n"),
            Err(Error::BadValue { value: 1, .. })
        ));
    }

    #[test]
    fn an_unclosed_value_is_refused() {
        assert!(matches!(
            parse("Name =\r\n[QUEST\r\nand on\r\n"),
            Err(Error::BadValue { value: 1, .. })
        ));
    }

    #[test]
    fn stray_text_is_refused() {
        assert!(matches!(
            parse("Name =\r\n[QUEST]\r\nstray\r\n"),
            Err(Error::BadValue { value: 3, .. })
        ));
        assert!(matches!(
            parse("Name =\r\nstray\r\n[QUEST]\r\n"),
            Err(Error::BadValue { value: 2, .. })
        ));
    }
}
