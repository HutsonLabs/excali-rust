//! Soft wrapping: `packages/element/src/textWrapping.ts`, the whole module.
//!
//! Upstream splits each hard line into breakable tokens with one Unicode
//! regex (`getLineBreakRegexAdvanced`, `textWrapping.ts:154-181`) passed to
//! `String.prototype.split`, then fills lines token by token, breaking a
//! token that alone is too wide character by character and trimming
//! whitespace at soft breaks the way the browser does.
//!
//! The port evaluates that regex directly: [`break_at`] is the sticky match
//! `split` attempts at each code point, its alternatives in upstream's
//! order (the emoji sequence first, then the zero-width break rules), and
//! [`parse_tokens`] is ECMA-262's `RegExp.prototype[@@split]` loop over it.
//! Character classes are upstream's: JS `\s` (ECMA-262 WhiteSpace and
//! LineTerminator, which is not Unicode `White_Space`: U+FEFF is in, U+0085
//! is out), the literal CJK and bracket sets, and `\p{...}` properties from
//! ICU (`icu_properties`), where V8 takes them from too. The regex
//! fallback for engines without lookbehind (`getLineBreakRegexSimple`,
//! `:141-145`) is only used when building the advanced regex throws; the
//! port always has the advanced rules.
//!
//! Widths come from a [`TextMetricsProvider`] and a [`CharWidthCache`]
//! passed by the caller, where upstream uses the module globals
//! `getLineWidth` and `charWidth` (see [`crate::text_measurements`]).
//! Offsets in [`WrappedTextLine`] are UTF-16 code units, as upstream's are.

use icu_normalizer::ComposingNormalizerBorrowed;
use icu_properties::props::{
    Emoji, EmojiModifier, EmojiPresentation, ExtendedPictographic, RegionalIndicator, Script,
};
use icu_properties::{CodePointMapData, CodePointSetData};

use crate::text_measurements::{get_line_width, CharWidthCache, TextMetricsProvider};

/// `containsCJK(text)` (`textWrapping.ts:30-36`), exported here as upstream
/// does; the table lives with the font code that also needs it.
pub use crate::font_assets::contains_cjk;

/// JS `\s`: ECMA-262 WhiteSpace (TAB, VT, FF, ZWNBSP, and `Zs`: U+0020,
/// U+00A0, U+1680, U+2000-U+200A, U+202F, U+205F, U+3000) and
/// LineTerminator (LF, CR, U+2028, U+2029). Also what `trimEnd` trims.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{D}'
            | ' '
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

/// What `.` does not match without the `s` flag: LineTerminator.
fn is_js_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `COMMON.HYPHEN` (`textWrapping.ts:70`).
fn is_hyphen(c: char) -> bool {
    c == '-'
}

/// `COMMON.OPENING` (`textWrapping.ts:81`).
fn is_common_opening(c: char) -> bool {
    matches!(c, '<' | '(' | '[' | '{')
}

/// `COMMON.CLOSING` (`textWrapping.ts:82`).
fn is_common_closing(c: char) -> bool {
    matches!(
        c,
        '>' | ')' | ']' | '}' | '.' | ',' | ':' | ';' | '!' | '?' | '\u{2026}' | '/'
    )
}

/// `CJK.CHAR` (`textWrapping.ts:95`): Han, Hiragana, Katakana and Hangul
/// script (`\p{Script=...}`, not Script_Extensions) and the listed symbols.
fn is_cjk_char(c: char) -> bool {
    let script = CodePointMapData::<Script>::new().get(c);
    script == Script::Han
        || script == Script::Hiragana
        || script == Script::Katakana
        || script == Script::Hangul
        || matches!(
            c,
            '\u{FF40}'
                | '\u{FF07}'
                | '\u{FF3E}'
                | '\u{3003}'
                | '\u{3030}'
                | '\u{3006}'
                | '\u{FF03}'
                | '\u{FF06}'
                | '\u{FF0A}'
                | '\u{FF0B}'
                | '\u{FF0D}'
                | '\u{30FC}'
                | '\u{FF0F}'
                | '\u{FF3C}'
                | '\u{FF1D}'
                | '\u{FF5C}'
                | '\u{FFE4}'
                | '\u{3012}'
                | '\u{FFE2}'
                | '\u{FFE3}'
        )
}

/// `CJK.OPENING` (`textWrapping.ts:110`).
fn is_cjk_opening(c: char) -> bool {
    matches!(
        c,
        '\u{FF08}'
            | '\u{FF3B}'
            | '\u{FF5B}'
            | '\u{3008}'
            | '\u{300A}'
            | '\u{FF5F}'
            | '\u{FF62}'
            | '\u{300C}'
            | '\u{300E}'
            | '\u{3010}'
            | '\u{3016}'
            | '\u{3014}'
            | '\u{3018}'
            | '\u{301A}'
            | '\u{FF1C}'
            | '\u{301D}'
    )
}

/// `CJK.CLOSING` (`textWrapping.ts:111`).
fn is_cjk_closing(c: char) -> bool {
    matches!(
        c,
        '\u{FF09}'
            | '\u{FF3D}'
            | '\u{FF5D}'
            | '\u{3009}'
            | '\u{300B}'
            | '\u{FF60}'
            | '\u{FF63}'
            | '\u{300D}'
            | '\u{300F}'
            | '\u{3011}'
            | '\u{3017}'
            | '\u{3015}'
            | '\u{3019}'
            | '\u{301B}'
            | '\u{FF1E}'
            | '\u{3002}'
            | '\u{FF0E}'
            | '\u{FF0C}'
            | '\u{3001}'
            | '\u{301F}'
            | '\u{2025}'
            | '\u{FF1F}'
            | '\u{FF01}'
            | '\u{FF1A}'
            | '\u{FF1B}'
            | '\u{30FB}'
            | '\u{301C}'
            | '\u{301E}'
    )
}

/// `CJK.CURRENCY` (`textWrapping.ts:118`).
fn is_cjk_currency(c: char) -> bool {
    matches!(
        c,
        '\u{FFE5}' | '\u{FFE6}' | '\u{FFE1}' | '\u{FFE0}' | '\u{FF04}'
    )
}

/// `\p{RI}`.
fn is_regional_indicator(c: char) -> bool {
    CodePointSetData::new::<RegionalIndicator>().contains(c)
}

/// `EMOJI.MOST`: `[\p{Extended_Pictographic}\p{Emoji_Presentation}]`.
fn is_emoji_most(c: char) -> bool {
    CodePointSetData::new::<ExtendedPictographic>().contains(c)
        || CodePointSetData::new::<EmojiPresentation>().contains(c)
}

/// `EMOJI.ANY`: `[\p{Emoji}]`.
fn is_emoji_any(c: char) -> bool {
    CodePointSetData::new::<Emoji>().contains(c)
}

/// `EMOJI.FLAG` at `i`: `\p{RI}\p{RI}`.
fn match_flag(chars: &[char], i: usize) -> Option<usize> {
    match chars.get(i..i + 2) {
        Some(&[a, b]) if is_regional_indicator(a) && is_regional_indicator(b) => Some(i + 2),
        _ => None,
    }
}

/// `EMOJI.JOINER` at `i` (`textWrapping.ts:124-125`):
/// `(?:\p{Emoji_Modifier}|\uFE0F\u20E3?|[\u{E0020}-\u{E007E}]+\u{E007F})?`,
/// greedy, alternatives in order; always matches (possibly empty).
fn match_joiner(chars: &[char], i: usize) -> usize {
    match chars.get(i) {
        Some(&c) if CodePointSetData::new::<EmojiModifier>().contains(c) => i + 1,
        Some('\u{FE0F}') => {
            if chars.get(i + 1) == Some(&'\u{20E3}') {
                i + 2
            } else {
                i + 1
            }
        }
        Some(&c) if ('\u{E0020}'..='\u{E007E}').contains(&c) => {
            let mut j = i;
            while chars
                .get(j)
                .is_some_and(|c| ('\u{E0020}'..='\u{E007E}').contains(c))
            {
                j += 1;
            }
            // The tag run cannot give back a U+E007F, so no backtracking
            // finds a match the greedy run misses.
            if chars.get(j) == Some(&'\u{E007F}') {
                j + 1
            } else {
                i
            }
        }
        _ => i,
    }
}

/// `getEmojiRegexUnicode()` matched at `i` (`textWrapping.ts:209-222`):
/// `(FLAG|MOST JOINER (?:ZWJ(?:FLAG|ANY JOINER))*)`. The end of the match,
/// or `None`. Nothing follows the group in the split regex, so the greedy
/// path is the one the backtracking engine returns.
fn match_emoji(chars: &[char], i: usize) -> Option<usize> {
    if let Some(end) = match_flag(chars, i) {
        return Some(end);
    }
    let &first = chars.get(i)?;
    if !is_emoji_most(first) {
        return None;
    }
    let mut end = match_joiner(chars, i + 1);
    while chars.get(end) == Some(&'\u{200D}') {
        let next = end + 1;
        if let Some(after) = match_flag(chars, next) {
            end = after;
        } else if chars.get(next).is_some_and(|&c| is_emoji_any(c)) {
            end = match_joiner(chars, next + 1);
        } else {
            break;
        }
    }
    Some(end)
}

/// The zero-width alternatives of `getLineBreakRegexAdvanced`
/// (`textWrapping.ts:158-180`), tried between `before` and `after`.
fn is_break_between(before: Option<char>, after: char) -> bool {
    // Negative lookbehind succeeds at the start of the string.
    let after_is = |f: fn(char) -> bool| f(after);
    let before_is = |f: fn(char) -> bool| before.is_some_and(f);

    // Break.Before(COMMON.WHITESPACE)
    after_is(is_js_whitespace)
        // Break.After(COMMON.WHITESPACE, COMMON.HYPHEN)
        || before_is(is_js_whitespace)
        || before_is(is_hyphen)
        // Break.Before(CJK.CHAR, CJK.CURRENCY).NotPrecededBy(COMMON.OPENING, CJK.OPENING)
        || (!before_is(is_common_opening)
            && !before_is(is_cjk_opening)
            && (after_is(is_cjk_char) || after_is(is_cjk_currency)))
        // Break.After(CJK.CHAR).NotFollowedBy(COMMON.HYPHEN, COMMON.CLOSING, CJK.CLOSING)
        || (before_is(is_cjk_char)
            && !(after_is(is_hyphen) || after_is(is_common_closing) || after_is(is_cjk_closing)))
        // Break.BeforeMany(CJK.OPENING).NotPrecededBy(COMMON.OPENING)
        || (!before_is(is_common_opening) && !before_is(is_cjk_opening) && after_is(is_cjk_opening))
        // Break.AfterMany(CJK.CLOSING).NotFollowedBy(COMMON.CLOSING)
        || (before_is(is_cjk_closing) && !after_is(is_cjk_closing) && !after_is(is_common_closing))
        // Break.AfterMany(COMMON.CLOSING).FollowedBy(COMMON.OPENING)
        || (before_is(is_common_closing)
            && !after_is(is_common_closing)
            && after_is(is_common_opening))
}

/// The line-break regex matched (sticky) at code point `q`: `Some(e)` with
/// the match's end (`e == q` for a zero-width break), or `None`.
fn break_at(chars: &[char], q: usize) -> Option<usize> {
    if let Some(end) = match_emoji(chars, q) {
        return Some(end);
    }
    let before = q.checked_sub(1).map(|p| chars[p]);
    if is_break_between(before, chars[q]) {
        Some(q)
    } else {
        None
    }
}

/// `parseTokens(line)` (`textWrapping.ts:382-390`): the line in NFC, split
/// at every break opportunity (the emoji sequences the regex captures are
/// tokens of their own), empty strings dropped.
pub fn parse_tokens(line: &str) -> Vec<String> {
    let normalized = ComposingNormalizerBorrowed::new_nfc().normalize(line);
    let chars: Vec<char> = normalized.chars().collect();
    let size = chars.len();
    let piece = |a: usize, b: usize| chars[a..b].iter().collect::<String>();

    // RegExp.prototype[@@split] (ECMA-262 22.2.6.14) with the sticky
    // splitter, advancing by code point (the regex has the `u` flag).
    let mut tokens = Vec::new();
    let mut p = 0;
    let mut q = 0;
    while q < size {
        match break_at(&chars, q) {
            Some(e) if e != p => {
                tokens.push(piece(p, q));
                if e > q {
                    // The capture group: the emoji sequence.
                    tokens.push(piece(q, e));
                }
                p = e;
                q = p;
            }
            _ => q += 1,
        }
    }
    tokens.push(piece(p, size));
    tokens.retain(|t| !t.is_empty());
    tokens
}

/// A rendered visual line (`WrappedTextLine`, `textWrapping.ts:409-418`).
///
/// `start` and `end` are end-exclusive UTF-16 code-unit offsets into the
/// original text, not counting the soft line breaks wrapping inserts. When
/// trailing whitespace was trimmed at a wrap boundary, `end` is just past
/// the last rendered character. Tokenizing assumes NFC input: for other
/// text the offsets count the normalized characters, as upstream's do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrappedTextLine {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// `string.length`.
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// `wrapText(text, font, maxWidth)` (`textWrapping.ts:392-406`): the lines
/// of [`get_wrapped_text_lines`] joined by `"\n"`.
pub fn wrap_text(
    text: &str,
    font: &str,
    max_width: f64,
    provider: &dyn TextMetricsProvider,
    char_widths: &mut CharWidthCache,
) -> String {
    get_wrapped_text_lines(text, font, max_width, provider, char_widths)
        .into_iter()
        .map(|line| line.text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// `getHardLineBreaks(text)` (`textWrapping.ts:423-437`).
fn get_hard_line_breaks(text: &str) -> Vec<WrappedTextLine> {
    let mut offset = 0;
    text.split('\n')
        .map(|line| {
            let start = offset;
            let end = start + utf16_len(line);
            offset = end + 1;
            WrappedTextLine {
                text: line.to_owned(),
                start,
                end,
            }
        })
        .collect()
}

/// `getWrappedTextLines(text, font, maxWidth)` (`textWrapping.ts:439-477`):
/// every hard line that fits as it is, the others wrapped. A width that is
/// not finite or is negative keeps the hard lines (upstream guards against
/// the infinite loop such a width would cause).
pub fn get_wrapped_text_lines(
    text: &str,
    font: &str,
    max_width: f64,
    provider: &dyn TextMetricsProvider,
    char_widths: &mut CharWidthCache,
) -> Vec<WrappedTextLine> {
    if !max_width.is_finite() || max_width < 0.0 {
        return get_hard_line_breaks(text);
    }

    let mut wrapper = Wrapper {
        font,
        max_width,
        provider,
        char_widths,
    };
    let mut lines = Vec::new();
    let mut offset = 0;
    for original_line in text.split('\n') {
        let len = utf16_len(original_line);
        let original_line_width = get_line_width(original_line, font, provider);
        if original_line_width <= max_width {
            lines.push(WrappedTextLine {
                text: original_line.to_owned(),
                start: offset,
                end: offset + len,
            });
        } else {
            lines.extend(wrapper.wrap_line(original_line, offset));
        }
        offset += len + 1;
    }
    lines
}

/// The font, width and measurement state `wrapLine` and its helpers share.
struct Wrapper<'a> {
    font: &'a str,
    max_width: f64,
    provider: &'a dyn TextMetricsProvider,
    char_widths: &'a mut CharWidthCache,
}

impl Wrapper<'_> {
    fn line_width(&self, text: &str) -> f64 {
        get_line_width(text, self.font, self.provider)
    }

    fn char_width(&mut self, ch: &str) -> f64 {
        self.char_widths.calculate(ch, self.font, self.provider)
    }

    /// `wrapLine(line, font, maxWidth, lineStart)` (`textWrapping.ts:485-584`).
    fn wrap_line(&mut self, line: &str, line_start: usize) -> Vec<WrappedTextLine> {
        let mut lines = Vec::new();
        let tokens = parse_tokens(line);

        let mut current_line = String::new();
        let mut current_line_start = line_start;
        let mut current_line_end = line_start;
        let mut current_line_width = 0.0;
        // The next token's code-unit position in the original text.
        let mut token_offset = line_start;
        let mut token_index = 0;

        while token_index < tokens.len() {
            let token = &tokens[token_index];
            let token_start = token_offset;
            let token_end = token_start + utf16_len(token);
            let test_line = format!("{current_line}{token}");

            // cache single codepoint whitespace, CJK or emoji width calc. as kerning should not apply here
            let test_line_width = if is_single_character(token) {
                current_line_width + self.char_width(token)
            } else {
                self.line_width(&test_line)
            };

            // build up the current line, skipping length check for possibly trailing whitespaces
            if token.chars().any(is_js_whitespace) || test_line_width <= self.max_width {
                if current_line.is_empty() {
                    current_line_start = token_start;
                }
                current_line = test_line;
                current_line_end = token_end;
                current_line_width = test_line_width;
                token_offset = token_end;
                token_index += 1;
                continue;
            }

            if current_line.is_empty() {
                // just the token (word) is longer than `maxWidth` and needs to be wrapped
                let mut wrapped_word = self.wrap_word(token, token_start);
                let trailing_line = wrapped_word.pop().unwrap_or(WrappedTextLine {
                    text: String::new(),
                    start: token_start,
                    end: token_start,
                });
                lines.extend(wrapped_word);

                // trailing line of the wrapped word might still be joined with next token/s
                current_line_width = self.line_width(&trailing_line.text);
                current_line = trailing_line.text;
                current_line_start = trailing_line.start;
                current_line_end = trailing_line.end;
                token_offset = token_end;
                token_index += 1;
            } else {
                // push & reset, but don't iterate on the next token, as we didn't use it yet!
                lines.push(trim_line_end_at_soft_break(
                    &current_line,
                    current_line_start,
                    current_line_end,
                ));
                current_line = String::new();
                current_line_start = token_start;
                current_line_end = token_start;
                current_line_width = 0.0;
            }
        }

        // iterator done, push the trailing line if exists
        if !current_line.is_empty() {
            lines.push(self.trim_line(&current_line, current_line_start, current_line_end));
        }
        lines
    }

    /// `wrapWord(word, font, maxWidth, wordStart)` (`textWrapping.ts:589-656`):
    /// a word that does not fit on an empty line, broken by code point.
    fn wrap_word(&mut self, word: &str, word_start: usize) -> Vec<WrappedTextLine> {
        let chars: Vec<char> = word.chars().collect();
        // multi-codepoint emojis are already broken apart and shouldn't be broken further
        // (`getEmojiRegex().test(word)`: an emoji anywhere in the word)
        if (0..chars.len()).any(|i| match_emoji(&chars, i).is_some()) {
            return vec![WrappedTextLine {
                text: word.to_owned(),
                start: word_start,
                end: word_start + utf16_len(word),
            }];
        }

        // satisfiesWordInvariant (`textWrapping.ts:731-740`), checked in
        // test and dev builds only, as upstream does.
        debug_assert!(
            !word.chars().any(is_js_whitespace),
            "Word should not contain any whitespaces!"
        );

        let mut lines = Vec::new();
        let mut current_line = String::new();
        let mut current_line_start = word_start;
        let mut current_line_end = word_start;
        let mut current_line_width = 0.0;
        let mut offset = word_start;

        let mut buf = [0u8; 4];
        for ch in chars {
            let ch_str: &str = ch.encode_utf8(&mut buf);
            let char_start = offset;
            let char_end = char_start + ch.len_utf16();
            let char_width = self.char_width(ch_str);
            let test_line_width = current_line_width + char_width;

            if test_line_width <= self.max_width {
                if current_line.is_empty() {
                    current_line_start = char_start;
                }
                current_line.push(ch);
                current_line_end = char_end;
                current_line_width = test_line_width;
                offset = char_end;
                continue;
            }

            if !current_line.is_empty() {
                lines.push(WrappedTextLine {
                    text: std::mem::take(&mut current_line),
                    start: current_line_start,
                    end: current_line_end,
                });
            }

            current_line = ch.to_string();
            current_line_start = char_start;
            current_line_end = char_end;
            current_line_width = char_width;
            offset = char_end;
        }

        if !current_line.is_empty() {
            lines.push(WrappedTextLine {
                text: current_line,
                start: current_line_start,
                end: current_line_end,
            });
        }
        lines
    }

    /// `trimLine(line, start, end, font, maxWidth)` (`textWrapping.ts:664-705`):
    /// the trailing visual line of a hard line keeps as much of its trailing
    /// whitespace as fits.
    fn trim_line(&mut self, line: &str, start: usize, end: usize) -> WrappedTextLine {
        let should_trim_whitespaces = self.line_width(line) > self.max_width;
        if !should_trim_whitespaces {
            return WrappedTextLine {
                text: line.to_owned(),
                start,
                end,
            };
        }

        // `line.match(/^(.+?)(\s+)$/)`, defaulting to `trimEnd`.
        let (mut trimmed_line, whitespaces) = match split_trailing_whitespace(line) {
            Some((head, tail)) => (head.to_owned(), tail),
            None => (js_trim_end(line).to_owned(), ""),
        };

        let mut trimmed_line_width = self.line_width(&trimmed_line);
        let mut buf = [0u8; 4];
        for whitespace in whitespaces.chars() {
            let char_width = self.char_width(whitespace.encode_utf8(&mut buf));
            let test_line_width = trimmed_line_width + char_width;
            if test_line_width > self.max_width {
                break;
            }
            trimmed_line.push(whitespace);
            trimmed_line_width = test_line_width;
        }

        WrappedTextLine {
            end: end - (utf16_len(line) - utf16_len(&trimmed_line)),
            text: trimmed_line,
            start,
        }
    }
}

/// `/^(.+?)(\s+)$/` (no flags): the shortest non-empty head free of line
/// terminators (what `.` matches) followed by nothing but whitespace, or
/// `None`. The head always takes the first character, so a line of spaces
/// splits after its first one.
fn split_trailing_whitespace(line: &str) -> Option<(&str, &str)> {
    let first_len = line.chars().next()?.len_utf8();
    // Start of the maximal whitespace run at the end, but at least one
    // character in.
    let run_start = line
        .char_indices()
        .rev()
        .take_while(|&(_, c)| is_js_whitespace(c))
        .last()
        .map_or(line.len(), |(i, _)| i);
    let split = run_start.max(first_len);
    if split >= line.len() {
        return None;
    }
    let (head, tail) = line.split_at(split);
    if head.chars().any(is_js_line_terminator) {
        return None;
    }
    Some((head, tail))
}

/// `String.prototype.trimEnd`: JS whitespace and line terminators.
fn js_trim_end(s: &str) -> &str {
    s.trim_end_matches(is_js_whitespace)
}

/// `trimLineEndAtSoftBreak(line, start, end)` (`textWrapping.ts:712-725`).
fn trim_line_end_at_soft_break(line: &str, start: usize, end: usize) -> WrappedTextLine {
    let trimmed_line = js_trim_end(line);
    WrappedTextLine {
        text: trimmed_line.to_owned(),
        start,
        end: end - (utf16_len(line) - utf16_len(trimmed_line)),
    }
}

/// `isSingleCharacter(s)` (`textWrapping.ts:727-734`): `codePointAt(0)`
/// defined and `codePointAt(1)` undefined, i.e. exactly one UTF-16 code
/// unit. An astral character (two code units) is not single.
fn is_single_character(s: &str) -> bool {
    utf16_len(s) == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_whitespace_split_follows_the_lazy_regex() {
        assert_eq!(split_trailing_whitespace("ab  "), Some(("ab", "  ")));
        assert_eq!(split_trailing_whitespace("   "), Some((" ", "  ")));
        assert_eq!(split_trailing_whitespace(" "), None);
        assert_eq!(split_trailing_whitespace("ab"), None);
        assert_eq!(split_trailing_whitespace(""), None);
        // `.` stops at a line terminator in the head, so there is no match.
        assert_eq!(split_trailing_whitespace("a\rb  "), None);
        // In the tail a terminator is whitespace like any other.
        assert_eq!(split_trailing_whitespace("ab \r "), Some(("ab", " \r ")));
        assert_eq!(
            split_trailing_whitespace("\u{3000}x \u{FEFF}"),
            Some(("\u{3000}x", " \u{FEFF}"))
        );
    }

    #[test]
    fn js_whitespace_is_ecma_262_not_unicode_white_space() {
        assert!(is_js_whitespace('\u{FEFF}'));
        assert!(!is_js_whitespace('\u{85}'));
        assert!(!is_js_whitespace('\u{200B}'));
        assert_eq!(js_trim_end("a \u{FEFF}\u{2028}"), "a");
        assert_eq!(js_trim_end("a\u{85}"), "a\u{85}");
    }

    #[test]
    fn single_character_is_one_utf16_code_unit() {
        assert!(is_single_character("a"));
        assert!(is_single_character("中"));
        assert!(!is_single_character("😀"));
        assert!(!is_single_character("ab"));
        assert!(!is_single_character(""));
    }

    #[test]
    fn emoji_sequences_match_greedily() {
        let chars: Vec<char> = "🏴\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}x"
            .chars()
            .collect();
        assert_eq!(match_emoji(&chars, 0), Some(7));
        // An unterminated tag run is not part of the sequence.
        let chars: Vec<char> = "🏴\u{E0067}\u{E0062}".chars().collect();
        assert_eq!(match_emoji(&chars, 0), Some(1));
        // A keycap base is \p{Emoji} but not MOST: no match on its own.
        let chars: Vec<char> = "1\u{FE0F}\u{20E3}".chars().collect();
        assert_eq!(match_emoji(&chars, 0), None);
        // Three regional indicators: a flag, then a lone one.
        let chars: Vec<char> = "🇨🇿🇩".chars().collect();
        assert_eq!(match_emoji(&chars, 0), Some(2));
        assert_eq!(match_emoji(&chars, 2), Some(3));
    }
}
