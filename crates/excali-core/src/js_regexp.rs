//! `new RegExp(source).test(string)` (ECMA-262 section 22.2, with the web
//! compatibility syntax of Annex B.1.2, as V8 implements it) for the patterns
//! `validateLibraryUrl` builds (`packages/excalidraw/data/library.ts:516-521`):
//! `(^|\.)<hostname>$` and `^<pathname>(/+|$)`, where hostname and pathname
//! are the serializations of an allow-list entry parsed as a URL.
//!
//! Upstream interpolates them unescaped, so they are regular expression
//! source: the dots of `excalidraw.com` match any character, and an entry
//! such as `example.com/a+b` or `exa(mple.com` quantifies or fails to
//! compile. [`RegExp::new`] reports V8's `SyntaxError` message for those
//! (`Invalid regular expression: /<source>/: <reason>`).
//!
//! The syntax is the part of the grammar such sources can contain: a URL
//! serialization never holds `\` (hosts forbid it, special URL paths turn it
//! into `/`) or `?` (it starts the query), so the only escape is the
//! template's `\.`, read as the character after the backslash, and `(?`
//! never occurs. Supported: literals, `.`, `^`, `$`, `|`, groups, classes
//! with ranges and negation (`[]` matches nothing, `[^]` anything), and the
//! quantifiers `*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}` (lazy or not). As in
//! Annex B a `{` that does not start a quantifier, a `}` and a `]` are
//! literals, and V8 caps quantifier bounds at 2^31 - 1, a bound that large
//! meaning no bound.
//!
//! [`RegExp::test`] only answers whether a match exists anywhere, so
//! greediness and captures do not matter: the matcher computes, for each
//! node, the set of positions a match can end at from a set of starts.

use std::collections::HashMap;
use std::fmt;

/// V8's `RegExpTree::kInfinity`: quantifier bounds are capped here, and a
/// maximum this large is unbounded.
const INFINITY: u32 = i32::MAX as u32;

/// A pattern `new RegExp` did not accept, with V8's message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SyntaxError(pub String);

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug)]
enum Node {
    Char(char),
    /// `.`: anything but a line terminator.
    Any,
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
    Start,
    End,
    Seq(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        /// `None` is unbounded.
        max: Option<u32>,
    },
}

/// A compiled regular expression without flags.
#[derive(Debug)]
pub(crate) struct RegExp {
    root: Node,
}

impl RegExp {
    /// `new RegExp(source)`.
    pub(crate) fn new(source: &str) -> Result<Self, SyntaxError> {
        let mut parser = Parser {
            chars: source.chars().collect(),
            pos: 0,
        };
        let root = parser
            .disjunction()
            .and_then(|root| match parser.peek() {
                None => Ok(root),
                // disjunction() stops only at the end or at `)`.
                Some(_) => Err("Unmatched ')'"),
            })
            .map_err(|reason| {
                SyntaxError(format!("Invalid regular expression: /{source}/: {reason}"))
            })?;
        Ok(Self { root })
    }

    /// `regexp.test(string)`: a match starts somewhere in `string`.
    pub(crate) fn test(&self, string: &str) -> bool {
        let text: Vec<char> = string.chars().collect();
        let starts = Positions::all(text.len());
        !ends(&self.root, &text, &starts).is_empty()
    }
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

type Parse<T> = Result<T, &'static str>;

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn disjunction(&mut self) -> Parse<Node> {
        let mut alternatives = vec![self.alternative()?];
        while self.peek() == Some('|') {
            self.pos += 1;
            alternatives.push(self.alternative()?);
        }
        Ok(if alternatives.len() == 1 {
            alternatives.remove(0)
        } else {
            Node::Alt(alternatives)
        })
    }

    fn alternative(&mut self) -> Parse<Node> {
        let mut terms = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            terms.push(self.term()?);
        }
        Ok(Node::Seq(terms))
    }

    fn term(&mut self) -> Parse<Node> {
        let c = self.bump().ok_or("Unexpected end")?;
        let atom = match c {
            // Assertions take no quantifier: one after them is read as an
            // atom below, which is "Nothing to repeat".
            '^' => return Ok(Node::Start),
            '$' => return Ok(Node::End),
            '*' | '+' | '?' => return Err("Nothing to repeat"),
            '{' => {
                if self.braced_quantifier_at(self.pos - 1).is_some() {
                    return Err("Nothing to repeat");
                }
                Node::Char('{')
            }
            '.' => Node::Any,
            '(' => {
                if self.peek() == Some('?') {
                    self.pos += 1;
                    if self.bump() != Some(':') {
                        return Err("Invalid group");
                    }
                }
                let inner = self.disjunction()?;
                if self.bump() != Some(')') {
                    return Err("Unterminated group");
                }
                inner
            }
            '[' => self.class()?,
            '\\' => Node::Char(self.bump().ok_or("\\ at end of pattern")?),
            c => Node::Char(c),
        };
        self.quantified(atom)
    }

    fn quantified(&mut self, atom: Node) -> Parse<Node> {
        let (min, max) = match self.peek() {
            Some('*') => {
                self.pos += 1;
                (0, INFINITY)
            }
            Some('+') => {
                self.pos += 1;
                (1, INFINITY)
            }
            Some('?') => {
                self.pos += 1;
                (0, 1)
            }
            Some('{') => match self.braced_quantifier_at(self.pos) {
                Some((min, max, end)) => {
                    if max < min {
                        return Err("numbers out of order in {} quantifier");
                    }
                    self.pos = end;
                    (min, max)
                }
                None => return Ok(atom),
            },
            _ => return Ok(atom),
        };
        // A lazy quantifier matches the same strings.
        if self.peek() == Some('?') {
            self.pos += 1;
        }
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max: (max != INFINITY).then_some(max),
        })
    }

    /// `{n}`, `{n,}` or `{n,m}` starting at `at` (the `{`): the bounds, capped
    /// at [`INFINITY`] as V8 does (`{n,}` is unbounded), and the position
    /// after the `}`.
    fn braced_quantifier_at(&self, at: usize) -> Option<(u32, u32, usize)> {
        let mut i = at + 1;
        let number = |i: &mut usize| {
            let start = *i;
            let mut value: u32 = 0;
            while let Some(d) = self.chars.get(*i).and_then(|c| c.to_digit(10)) {
                value = value.saturating_mul(10).saturating_add(d).min(INFINITY);
                *i += 1;
            }
            (*i > start).then_some(value)
        };
        let min = number(&mut i)?;
        let max = match self.chars.get(i) {
            Some('}') => min,
            Some(',') => {
                i += 1;
                if self.chars.get(i) == Some(&'}') {
                    INFINITY
                } else {
                    let max = number(&mut i)?;
                    if self.chars.get(i) != Some(&'}') {
                        return None;
                    }
                    max
                }
            }
            _ => return None,
        };
        Some((min, max, i + 1))
    }

    /// After `[`: a class up to its `]`.
    fn class(&mut self) -> Parse<Node> {
        let negated = self.peek() == Some('^');
        if negated {
            self.pos += 1;
        }
        let mut ranges = Vec::new();
        loop {
            let from = match self.bump() {
                None => return Err("Unterminated character class"),
                Some(']') => break,
                Some('\\') => self.bump().ok_or("\\ at end of pattern")?,
                Some(c) => c,
            };
            let dash = self.peek() == Some('-');
            let after = self.chars.get(self.pos + 1).copied();
            if dash && after.is_some() && after != Some(']') {
                self.pos += 1;
                let to = match self.bump() {
                    Some('\\') => self.bump().ok_or("\\ at end of pattern")?,
                    Some(c) => c,
                    None => return Err("Unterminated character class"),
                };
                if to < from {
                    return Err("Range out of order in character class");
                }
                ranges.push((from, to));
            } else {
                ranges.push((from, from));
            }
        }
        Ok(Node::Class { negated, ranges })
    }
}

/// A set of positions in the text, `0..=len`.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Positions(Vec<bool>);

impl Positions {
    fn empty(len: usize) -> Self {
        Self(vec![false; len + 1])
    }

    fn all(len: usize) -> Self {
        Self(vec![true; len + 1])
    }

    fn is_empty(&self) -> bool {
        !self.0.contains(&true)
    }

    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0
            .iter()
            .enumerate()
            .filter_map(|(i, &set)| set.then_some(i))
    }

    fn union_with(&mut self, other: &Self) {
        for (a, &b) in self.0.iter_mut().zip(&other.0) {
            *a |= b;
        }
    }
}

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// The positions a match of `node` can end at, starting from any of `starts`.
fn ends(node: &Node, text: &[char], starts: &Positions) -> Positions {
    let len = text.len();
    let step = |accept: &dyn Fn(char) -> bool| {
        let mut out = Positions::empty(len);
        for p in starts.iter() {
            if p < len && accept(text[p]) {
                out.0[p + 1] = true;
            }
        }
        out
    };
    match node {
        Node::Char(c) => step(&|t| t == *c),
        Node::Any => step(&|t| !is_line_terminator(t)),
        Node::Class { negated, ranges } => {
            step(&|t| ranges.iter().any(|&(a, b)| a <= t && t <= b) != *negated)
        }
        Node::Start => {
            let mut out = Positions::empty(len);
            out.0[0] = starts.0[0];
            out
        }
        Node::End => {
            let mut out = Positions::empty(len);
            out.0[len] = starts.0[len];
            out
        }
        Node::Seq(nodes) => nodes.iter().fold(starts.clone(), |set, node| {
            if set.is_empty() {
                set
            } else {
                ends(node, text, &set)
            }
        }),
        Node::Alt(nodes) => {
            let mut out = Positions::empty(len);
            for node in nodes {
                out.union_with(&ends(node, text, starts));
            }
            out
        }
        Node::Repeat { node, min, max } => repeat(node, *min, *max, text, starts),
    }
}

/// `node{min,max}`. Matching is the image of a relation, so it distributes
/// over unions: the ends after exactly `min` iterations, then everything
/// reachable in up to `max - min` more. Both sequences are over the finite
/// sets of positions, so they settle (or cycle, for the exact count) long
/// before a large bound, and the loops stop there.
fn repeat(node: &Node, min: u32, max: Option<u32>, text: &[char], starts: &Positions) -> Positions {
    let mut current = starts.clone();
    let mut seen: HashMap<Positions, u32> = HashMap::new();
    let mut i = 0;
    while i < min {
        if current.is_empty() {
            return current;
        }
        if let Some(&first) = seen.get(&current) {
            // current recurs every (i - first) iterations from `first` on.
            let period = i - first;
            let remaining = (min - i) % period;
            for _ in 0..remaining {
                current = ends(node, text, &current);
            }
            break;
        }
        seen.insert(current.clone(), i);
        current = ends(node, text, &current);
        i += 1;
    }
    let mut reached = current.clone();
    let mut frontier = current;
    let mut extra = 0;
    while max.is_none_or(|max| extra < max - min) {
        let next = ends(node, text, &frontier);
        let mut grown = reached.clone();
        grown.union_with(&next);
        if grown == reached {
            break;
        }
        // Only positions not reached before can lead anywhere new.
        frontier = Positions(
            next.0
                .iter()
                .zip(&reached.0)
                .map(|(&n, &r)| n && !r)
                .collect(),
        );
        reached = grown;
        extra += 1;
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test(source: &str, text: &str) -> bool {
        RegExp::new(source).expect(source).test(text)
    }

    fn error(source: &str) -> String {
        RegExp::new(source).expect_err(source).0
    }

    #[test]
    fn syntax_errors_are_v8s() {
        // Recorded with `new RegExp(p)` in Node 26 (V8).
        for (source, reason) in [
            ("^*", "Nothing to repeat"),
            ("$*", "Nothing to repeat"),
            ("$+x", "Nothing to repeat"),
            ("x{2}{3}", "Nothing to repeat"),
            ("(*)", "Nothing to repeat"),
            ("|*", "Nothing to repeat"),
            ("a|*", "Nothing to repeat"),
            ("a**", "Nothing to repeat"),
            ("a{1}*", "Nothing to repeat"),
            ("({1})", "Nothing to repeat"),
            ("[b-a]", "Range out of order in character class"),
            ("[z-a", "Range out of order in character class"),
            ("(a", "Unterminated group"),
            ("(a)(", "Unterminated group"),
            ("a)", "Unmatched ')'"),
            (")(", "Unmatched ')'"),
            ("[a", "Unterminated character class"),
            ("a{2,1}", "numbers out of order in {} quantifier"),
            ("a{99999999999,2}", "numbers out of order in {} quantifier"),
        ] {
            assert_eq!(
                error(source),
                format!("Invalid regular expression: /{source}/: {reason}")
            );
        }
    }

    #[test]
    fn annex_b_literals_and_matches() {
        // Recorded with `new RegExp(p).test("a{")` and `.test("aa")` in Node 26.
        for (source, a_brace, aa) in [
            ("a{", true, false),
            ("a{1", false, false),
            ("a{1,", false, false),
            ("{", true, false),
            ("}", false, false),
            ("]", false, false),
            ("a{,5}", false, false),
            ("[a-]", true, true),
            ("[-a]", true, true),
            ("[]a]", false, false),
            ("()", true, true),
            ("a{99999999999}", false, false),
            ("a{1}?", true, true),
            ("[a-b-c]", true, true),
            ("a{2,99999999999}", false, true),
        ] {
            assert_eq!(test(source, "a{"), a_brace, "{source} a{{");
            assert_eq!(test(source, "aa"), aa, "{source} aa");
        }
    }

    #[test]
    fn matching() {
        assert!(test(r"(^|\.)excalidraw.com$", "excalidraw.com"));
        assert!(test(r"(^|\.)excalidraw.com$", "libraries.excalidraw.com"));
        assert!(test(r"(^|\.)excalidraw.com$", "excalidraw-com"));
        assert!(!test(r"(^|\.)excalidraw.com$", "notexcalidraw.com"));
        assert!(!test(r"(^|\.)excalidraw.com$", "excalidraw.com."));
        assert!(test("^/a(/+|$)", "/a"));
        assert!(test("^/a(/+|$)", "/a//b"));
        assert!(!test("^/a(/+|$)", "/ab"));
        assert!(test("^(/+|$)", "/"));
        assert!(test("[^]", "x"));
        assert!(!test("[]", "x"));
        assert!(!test(".", "\n\r\u{2028}\u{2029}"));
        assert!(test("^(ab)+x$", "ababx"));
        assert!(!test("^(ab)+x$", "abax"));
        assert!(test("^a{3}$", "aaa"));
        assert!(!test("^a{3}$", "aa"));
        assert!(!test("^a{3}$", "aaaa"));
        assert!(test("^a{2,3}$", "aaa"));
        assert!(!test("^a{2,3}$", "aaaa"));
        assert!(test("^(a|)*$", "aaa"));
        assert!(test("^(a|){5}$", "aaa"));
        assert!(test("^(^|a){1000000}$", "a"));
        assert!(!test("^(^|aa){1000001}$", "a"));
        assert!(test("^(?:x|y)z$", "yz"));
        assert!(test("", ""));
        assert!(!test("^$", "a"));
    }
}
