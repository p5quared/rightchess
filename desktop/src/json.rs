//! The inspector's JSON highlighting, plus the tags used for the list.

use rightchess::{Date, Game};

/// The tags worth showing beside a game in the list.
pub struct Meta {
    pub white: Option<String>,
    pub black: Option<String>,
    pub result: Option<String>,
    pub date: Option<Date>,
}

impl Meta {
    /// A one-line description built from whichever tags exist, e.g.
    /// `2024.03.15 · White vs Black · 1-0`.
    #[must_use]
    pub fn caption(&self) -> String {
        let players = match (&self.white, &self.black) {
            (Some(white), Some(black)) => Some(format!("{white} vs {black}")),
            _ => None,
        };
        let date = self.date.as_ref().map(ToString::to_string);
        [date, players, self.result.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// Reads the display tags from a game's PGN. Empty tags count as absent.
#[must_use]
pub fn meta(game: &Game) -> Meta {
    let tag = |name: &str| game.pgn.tag(name).filter(|value| !value.is_empty());
    Meta {
        white: tag("White"),
        black: tag("Black"),
        result: tag("Result"),
        date: game.pgn.date(),
    }
}

/// Highlights pretty-printed JSON as HTML spans so the inspector can colour it. The input is
/// escaped, so the result is safe to inject.
#[must_use]
pub fn highlight(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut html = String::with_capacity(text.len() * 2);
    let mut i = 0;
    while i < chars.len() {
        let current = chars[i];
        if current == '"' {
            let end = string_end(&chars, i);
            let token: String = chars[i..end].iter().collect();
            // A string followed by a colon is an object key, not a value.
            let mut next = end;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            let class = if next < chars.len() && chars[next] == ':' {
                "tok-key"
            } else {
                "tok-string"
            };
            push_span(&mut html, class, &token);
            i = end;
        } else if current.is_ascii_digit() || current == '-' {
            let mut end = i;
            while end < chars.len() && matches!(chars[end], '0'..='9' | '.' | 'e' | 'E' | '+' | '-')
            {
                end += 1;
            }
            let token: String = chars[i..end].iter().collect();
            push_span(&mut html, "tok-number", &token);
            i = end;
        } else if current.is_ascii_alphabetic() {
            let mut end = i;
            while end < chars.len() && chars[end].is_ascii_alphabetic() {
                end += 1;
            }
            let token: String = chars[i..end].iter().collect();
            push_span(&mut html, "tok-literal", &token);
            i = end;
        } else {
            escape_into(&mut html, current);
            i += 1;
        }
    }
    html
}

/// Copies text to the system clipboard. Best effort: access can fail when the window isn't
/// focused, and there's nothing useful to show the user if it does.
pub fn copy(text: &str) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        drop(clipboard.set_text(text));
    }
}

/// The index just past the closing quote of the string starting at `start`, or the end of input.
fn string_end(chars: &[char], start: usize) -> usize {
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' => return i + 1,
            _ => i += 1,
        }
    }
    chars.len()
}

fn push_span(out: &mut String, class: &str, text: &str) {
    out.push_str("<span class=\"");
    out.push_str(class);
    out.push_str("\">");
    for c in text.chars() {
        escape_into(out, c);
    }
    out.push_str("</span>");
}

fn escape_into(out: &mut String, c: char) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        _ => out.push(c),
    }
}

#[cfg(test)]
mod tests {
    use rightchess::{GameId, SourceKind};

    use super::*;

    fn game(tags: &[(&str, &str)]) -> Game {
        let pgn = tags
            .iter()
            .map(|(name, value)| format!("[{name} \"{value}\"]"))
            .collect::<Vec<_>>()
            .join("\n");
        Game {
            id: GameId {
                source: SourceKind::ChessCom,
                external_id: "id".to_string(),
            },
            pgn: pgn.parse().unwrap(),
        }
    }

    #[track_caller]
    fn check(tags: &[(&str, &str)], expected: &str) {
        assert_eq!(meta(&game(tags)).caption(), expected, "{tags:?}");
    }

    #[test]
    fn caption_joins_the_tags_that_exist() {
        check(
            &[
                ("White", "Alice"),
                ("Black", "Bob"),
                ("Result", "1-0"),
                ("UTCDate", "2024.03.15"),
            ],
            "2024.03.15 · Alice vs Bob · 1-0",
        );
        check(&[("White", "Alice"), ("Result", "1-0")], "1-0");
        check(&[], "");
    }

    #[test]
    fn caption_shows_the_day_the_game_started() {
        check(
            &[("Date", "2024.06.01"), ("UTCDate", "2024.??.??")],
            "2024.06.01",
        );
        check(&[("Date", "2024.??.??")], "");
    }
}
