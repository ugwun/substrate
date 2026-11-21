//! Style-preserving word wrap with hanging indents for label/value panels.
//!
//! Ratatui's built-in `Wrap` restarts continuation rows at column 0, so a wrapped value in a
//! `LABEL       value` row lands under the label. Pre-wrapping with [`hang_wrap`] keeps
//! continuation rows aligned under the value and lets callers count real rendered rows for
//! scrolling and overflow cues.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

/// Widest label column that is treated as a hanging-indent prefix.
const MAX_LABEL_INDENT: usize = 18;

/// Rendered width of a line in terminal cells (one cell per char; the UI uses narrow glyphs only).
#[must_use]
pub fn line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum()
}

/// Wrap every line to `width` cells, preserving span styles and alignment.
#[must_use]
pub fn hang_wrap(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        out.extend(hang_wrap_line(line, width));
    }
    out
}

/// Continuation indent for a line: the label column when the first span is a padded label
/// (`format!("{label:<12}")`), otherwise the line's own leading whitespace.
fn hanging_indent(line: &Line<'_>, width: usize) -> usize {
    let limit = (width / 2).min(MAX_LABEL_INDENT);
    if line.spans.len() >= 2 {
        let first = line.spans[0].content.as_ref();
        let first_width = first.chars().count();
        if first.ends_with(' ') && first_width <= limit && !first.trim().is_empty() {
            return first_width;
        }
    }
    let leading = line
        .spans
        .iter()
        .flat_map(|span| span.content.chars())
        .take_while(|c| *c == ' ')
        .count();
    leading.min(limit)
}

struct Token {
    text: String,
    style: Style,
    space: bool,
}

fn tokenize(line: &Line<'static>) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    for span in &line.spans {
        let style = line.style.patch(span.style);
        for ch in span.content.chars() {
            let space = ch == ' ';
            match tokens.last_mut() {
                Some(last) if last.space == space && last.style == style => last.text.push(ch),
                _ => tokens.push(Token {
                    text: ch.to_string(),
                    style,
                    space,
                }),
            }
        }
    }
    tokens
}

fn hang_wrap_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    if width < 8 || line_width(&line) <= width {
        return vec![line];
    }
    let alignment = line.alignment;
    let indent = hanging_indent(&line, width);
    let tokens = tokenize(&line);

    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
    let mut row: Vec<Span<'static>> = Vec::new();
    let mut row_width = 0usize;
    let mut row_floor = 0usize; // width of the indent that starts the current row
    let new_row = |rows: &mut Vec<Vec<Span<'static>>>, row: &mut Vec<Span<'static>>| {
        rows.push(std::mem::take(row));
        if indent > 0 {
            row.push(Span::raw(" ".repeat(indent)));
        }
    };

    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].space {
            let space_width = tokens[i].text.chars().count();
            // Width of the following word (adjacent non-space tokens, possibly across spans).
            let word_width: usize = tokens[i + 1..]
                .iter()
                .take_while(|t| !t.space)
                .map(|t| t.text.chars().count())
                .sum();
            if row_width > row_floor && row_width + space_width + word_width > width {
                new_row(&mut rows, &mut row);
                row_width = indent;
                row_floor = indent;
            } else if row_width > row_floor || rows.is_empty() {
                row.push(Span::styled(tokens[i].text.clone(), tokens[i].style));
                row_width += space_width;
            }
            i += 1;
            continue;
        }
        let mut j = i;
        while j < tokens.len() && !tokens[j].space {
            j += 1;
        }
        let word_width: usize = tokens[i..j].iter().map(|t| t.text.chars().count()).sum();
        if row_width > row_floor && row_width + word_width > width {
            new_row(&mut rows, &mut row);
            row_width = indent;
            row_floor = indent;
        }
        for token in &tokens[i..j] {
            // Hard-split a word that cannot fit on an empty row.
            let mut chunk = String::new();
            for ch in token.text.chars() {
                if row_width >= width {
                    if !chunk.is_empty() {
                        row.push(Span::styled(std::mem::take(&mut chunk), token.style));
                    }
                    new_row(&mut rows, &mut row);
                    row_width = indent;
                    row_floor = indent;
                }
                chunk.push(ch);
                row_width += 1;
            }
            if !chunk.is_empty() {
                row.push(Span::styled(chunk, token.style));
            }
        }
        i = j;
    }
    if row_width > row_floor || rows.is_empty() {
        rows.push(row);
    }
    rows.into_iter()
        .map(|spans| {
            let mut out = Line::from(spans);
            out.alignment = alignment;
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn label_value_rows_continue_under_the_value() {
        let line = Line::from(vec![
            Span::raw(format!("{:<12}", "POSITION")),
            Span::raw("C2 // row 3 of 4 // column 2 of 4"),
        ]);
        let rows = hang_wrap(vec![line], 30);
        assert_eq!(rows.len(), 2);
        assert_eq!(plain(&rows[0]), "POSITION    C2 // row 3 of 4");
        assert_eq!(plain(&rows[1]), "            // column 2 of 4");
        assert!(rows.iter().all(|row| line_width(row) <= 30));
    }

    #[test]
    fn short_lines_are_untouched_and_leading_indent_is_kept() {
        let short = Line::raw("fits");
        assert_eq!(hang_wrap(vec![short.clone()], 20), vec![short]);
        let rows = hang_wrap(vec![Line::raw("    one two three four five six seven")], 16);
        assert!(rows.len() > 1);
        assert!(rows.iter().all(|row| plain(row).starts_with("    ")));
        assert!(rows.iter().all(|row| line_width(row) <= 16));
    }

    #[test]
    fn overlong_words_hard_split_without_overflow() {
        let rows = hang_wrap(vec![Line::raw("abcdefghijklmnopqrstuvwxyz")], 10);
        assert!(rows.iter().all(|row| line_width(row) <= 10));
        let joined: String = rows.iter().map(plain).collect();
        assert_eq!(joined, "abcdefghijklmnopqrstuvwxyz");
    }
}
