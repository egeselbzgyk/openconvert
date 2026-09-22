//! A minimal parser for llama.cpp's GBNF dialect, and a matcher over what it parses.
//!
//! **Why a parser of our own.** A grammar is the one artifact whose error is invisible: the server
//! refuses it, every call fails, and every failure falls back to the deterministic answer — so a
//! broken grammar looks exactly like a model that is never needed. Test 8.14 parses all four here
//! instead, with the dialect's actual rules. The rule that matters is the one that surprises: **a
//! rule ends at the end of its line unless it is inside parentheses.** llama.cpp's
//! `parse_sequence` skips newlines only when nested, so a choice written as continuation lines
//! beginning with `|` is a syntax error at the `|`. IMPLEMENTATION_PLAN Appendix A.2 prints
//! `heading_roles` in exactly that form.
//!
//! The parser follows `llama-grammar.cpp`: rule names are `[a-zA-Z0-9-]`, a literal is a
//! double-quoted string, a class is `[...]` or `[^...]` with ranges, the escapes are `\x`, `\u`,
//! `\U` (with exactly 2, 4 and 8 hex digits), `\t`, `\r`, `\n`, `\\`, `\"`, `\[` and `\]`, and
//! the repetitions are `*`, `+`, `?`, `{m}`, `{m,}` and `{m,n}`. It is stricter than the server in
//! two places, both deliberate: a rule defined twice is an error (the server silently keeps the
//! second), and so is a bound whose maximum is below its minimum. A grammar that passes here
//! passes there.
//!
//! **The matcher** answers whether a grammar admits a string. It simulates the grammar over
//! *sets* of input positions rather than backtracking, so an ambiguous grammar costs a larger set
//! instead of an exponential search; left recursion, which would make the simulation loop, is
//! rejected at parse time as the server rejects it.

use std::collections::{BTreeMap, BTreeSet};

/// One element of a rule's right-hand side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// A string literal: these characters, in order.
    Literal(Vec<char>),
    /// A character class. `ranges` holds inclusive `(low, high)` pairs; a single character is a
    /// range with `low == high`.
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
    /// `.` — any one character.
    Any,
    /// A reference to a named rule.
    Rule(String),
    /// A parenthesised choice.
    Group(Alternatives),
    /// `x*`, `x+`, `x?`, `x{m}`, `x{m,}`, `x{m,n}`. `max` is `None` when unbounded.
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
    },
}

/// A sequence of nodes, matched one after another.
pub type Sequence = Vec<Node>;

/// A choice between sequences.
pub type Alternatives = Vec<Sequence>;

/// Why a grammar was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GbnfError {
    /// The text is not GBNF. `line` is 1-based.
    #[error("line {line}: {message}")]
    Syntax { line: usize, message: String },
    #[error("rule `{0}` is referenced and never defined")]
    Undefined(String),
    #[error("rule `{0}` is defined twice")]
    Redefined(String),
    #[error("the grammar has no `root` rule")]
    NoRoot,
    /// A rule can reach itself without consuming a character, which the server refuses and the
    /// matcher could not terminate on.
    #[error("rule `{0}` is left-recursive")]
    LeftRecursive(String),
}

/// A parsed grammar: every rule by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grammar {
    rules: BTreeMap<String, Alternatives>,
}

/// The rule every grammar starts from.
const ROOT: &str = "root";

impl Grammar {
    /// Parse and validate GBNF text.
    pub fn parse(text: &str) -> Result<Grammar, GbnfError> {
        let source: Vec<char> = text.chars().collect();
        let mut parser = Parser {
            source: &source,
            pos: 0,
        };
        let mut rules = BTreeMap::new();
        parser.skip_space(true);
        while parser.peek().is_some() {
            let (name, alternatives) = parser.rule()?;
            if rules.insert(name.clone(), alternatives).is_some() {
                return Err(GbnfError::Redefined(name));
            }
        }

        let grammar = Grammar { rules };
        grammar.validate()?;
        Ok(grammar)
    }

    /// A rule's definition, or `None` when the grammar has no rule of that name.
    pub fn rule(&self, name: &str) -> Option<&Alternatives> {
        self.rules.get(name)
    }

    /// Whether the grammar admits `text`, all of it, from `root`.
    pub fn accepts(&self, text: &str) -> bool {
        let input: Vec<char> = text.chars().collect();
        let mut matcher = Matcher {
            grammar: self,
            input: &input,
            memo: BTreeMap::new(),
        };
        matcher.rule(ROOT, 0).contains(&input.len())
    }

    fn validate(&self) -> Result<(), GbnfError> {
        if !self.rules.contains_key(ROOT) {
            return Err(GbnfError::NoRoot);
        }
        for alternatives in self.rules.values() {
            for node in alternatives.iter().flatten() {
                self.check_references(node)?;
            }
        }
        self.check_left_recursion()
    }

    fn check_references(&self, node: &Node) -> Result<(), GbnfError> {
        match node {
            Node::Rule(name) if !self.rules.contains_key(name) => {
                Err(GbnfError::Undefined(name.clone()))
            }
            Node::Group(alternatives) => alternatives
                .iter()
                .flatten()
                .try_for_each(|inner| self.check_references(inner)),
            Node::Repeat { node, .. } => self.check_references(node),
            _ => Ok(()),
        }
    }

    /// Refuse a rule that can reach itself before consuming anything, including through a prefix
    /// that can match the empty string — `a ::= b a "x"` with `b ::= "y"?` is left-recursive.
    fn check_left_recursion(&self) -> Result<(), GbnfError> {
        let nullable = self.nullable_rules();
        let leads: BTreeMap<&str, BTreeSet<&str>> = self
            .rules
            .iter()
            .map(|(name, alternatives)| {
                let mut first = BTreeSet::new();
                for sequence in alternatives {
                    leading_rules(sequence, &nullable, &mut first);
                }
                (name.as_str(), first)
            })
            .collect();

        for start in self.rules.keys() {
            let mut stack: Vec<&str> = leads
                .get(start.as_str())
                .map(|first| first.iter().copied().collect())
                .unwrap_or_default();
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            while let Some(name) = stack.pop() {
                if name == start.as_str() {
                    return Err(GbnfError::LeftRecursive(start.clone()));
                }
                if seen.insert(name) {
                    if let Some(first) = leads.get(name) {
                        stack.extend(first.iter().copied());
                    }
                }
            }
        }
        Ok(())
    }

    /// The rules that can match the empty string, by fixpoint.
    fn nullable_rules(&self) -> BTreeSet<&str> {
        let mut nullable: BTreeSet<&str> = BTreeSet::new();
        loop {
            let before = nullable.len();
            for (name, alternatives) in &self.rules {
                if !nullable.contains(name.as_str())
                    && alternatives
                        .iter()
                        .any(|sequence| sequence_nullable(sequence, &nullable))
                {
                    nullable.insert(name.as_str());
                }
            }
            if nullable.len() == before {
                return nullable;
            }
        }
    }
}

fn node_nullable(node: &Node, nullable: &BTreeSet<&str>) -> bool {
    match node {
        Node::Literal(chars) => chars.is_empty(),
        Node::Class { .. } | Node::Any => false,
        Node::Rule(name) => nullable.contains(name.as_str()),
        Node::Group(alternatives) => alternatives
            .iter()
            .any(|sequence| sequence_nullable(sequence, nullable)),
        Node::Repeat { node, min, .. } => *min == 0 || node_nullable(node, nullable),
    }
}

fn sequence_nullable(sequence: &[Node], nullable: &BTreeSet<&str>) -> bool {
    sequence.iter().all(|node| node_nullable(node, nullable))
}

/// Every rule a sequence can begin with: the first node's, and the next node's for as long as
/// every node before it can match nothing.
fn leading_rules<'a>(sequence: &'a [Node], nullable: &BTreeSet<&str>, out: &mut BTreeSet<&'a str>) {
    for node in sequence {
        node_leading_rules(node, nullable, out);
        if !node_nullable(node, nullable) {
            return;
        }
    }
}

fn node_leading_rules<'a>(node: &'a Node, nullable: &BTreeSet<&str>, out: &mut BTreeSet<&'a str>) {
    match node {
        Node::Rule(name) => {
            out.insert(name.as_str());
        }
        Node::Group(alternatives) => {
            for sequence in alternatives {
                leading_rules(sequence, nullable, out);
            }
        }
        Node::Repeat { node, .. } => node_leading_rules(node, nullable, out),
        Node::Literal(_) | Node::Class { .. } | Node::Any => {}
    }
}

// ---------------------------------------------------------------------------
// The parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    source: &'a [char],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.source.get(self.pos + offset).copied()
    }

    fn error(&self, message: impl Into<String>) -> GbnfError {
        let line = 1 + self.source[..self.pos.min(self.source.len())]
            .iter()
            .filter(|&&c| c == '\n')
            .count();
        GbnfError::Syntax {
            line,
            message: message.into(),
        }
    }

    /// Skip spaces, tabs and comments, and newlines only when `newline_ok` — llama.cpp's
    /// `parse_space`, whose second argument is what makes a top-level rule end at a newline.
    fn skip_space(&mut self, newline_ok: bool) {
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' => self.pos += 1,
                '#' => {
                    while self.peek().is_some_and(|c| c != '\r' && c != '\n') {
                        self.pos += 1;
                    }
                }
                '\r' | '\n' if newline_ok => self.pos += 1,
                _ => return,
            }
        }
    }

    fn rule(&mut self) -> Result<(String, Alternatives), GbnfError> {
        let name = self.name()?;
        self.skip_space(false);
        if !(self.peek() == Some(':')
            && self.peek_at(1) == Some(':')
            && self.peek_at(2) == Some('='))
        {
            return Err(self.error(format!("expecting ::= after `{name}`")));
        }
        self.pos += 3;
        self.skip_space(true);
        let alternatives = self.alternatives(false)?;

        match self.peek() {
            Some('\r') => {
                self.pos += 1;
                if self.peek() == Some('\n') {
                    self.pos += 1;
                }
            }
            Some('\n') => self.pos += 1,
            None => {}
            Some(c) => {
                return Err(self.error(format!(
                    "expecting a newline or the end after rule `{name}`, found `{c}`"
                )))
            }
        }
        self.skip_space(true);
        Ok((name, alternatives))
    }

    fn name(&mut self) -> Result<String, GbnfError> {
        let start = self.pos;
        while self.peek().is_some_and(is_word_char) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(self.error("expecting a rule name"));
        }
        Ok(self.source[start..self.pos].iter().collect())
    }

    fn alternatives(&mut self, nested: bool) -> Result<Alternatives, GbnfError> {
        let mut alternatives = vec![self.sequence(nested)?];
        while self.peek() == Some('|') {
            self.pos += 1;
            self.skip_space(true);
            alternatives.push(self.sequence(nested)?);
        }
        Ok(alternatives)
    }

    fn sequence(&mut self, nested: bool) -> Result<Sequence, GbnfError> {
        let mut sequence: Sequence = Vec::new();
        while let Some(c) = self.peek() {
            match c {
                '"' => {
                    self.pos += 1;
                    let mut chars = Vec::new();
                    loop {
                        match self.peek() {
                            None => return Err(self.error("unterminated string literal")),
                            Some('"') => break,
                            Some(_) => chars.push(self.character()?),
                        }
                    }
                    self.pos += 1;
                    sequence.push(Node::Literal(chars));
                }
                '[' => {
                    self.pos += 1;
                    let negated = self.peek() == Some('^');
                    if negated {
                        self.pos += 1;
                    }
                    let mut ranges = Vec::new();
                    loop {
                        match self.peek() {
                            None => return Err(self.error("unterminated character class")),
                            Some(']') => break,
                            Some(_) => {
                                let low = self.character()?;
                                let high = if self.peek() == Some('-')
                                    && self.peek_at(1).is_some_and(|c| c != ']')
                                {
                                    self.pos += 1;
                                    self.character()?
                                } else {
                                    low
                                };
                                ranges.push((low, high));
                            }
                        }
                    }
                    self.pos += 1;
                    sequence.push(Node::Class { negated, ranges });
                }
                '(' => {
                    self.pos += 1;
                    self.skip_space(true);
                    let inner = self.alternatives(true)?;
                    if self.peek() != Some(')') {
                        return Err(self.error("expecting `)`"));
                    }
                    self.pos += 1;
                    sequence.push(Node::Group(inner));
                }
                '.' => {
                    self.pos += 1;
                    sequence.push(Node::Any);
                }
                '*' | '+' | '?' => {
                    self.pos += 1;
                    let (min, max) = match c {
                        '*' => (0, None),
                        '+' => (1, None),
                        _ => (0, Some(1)),
                    };
                    self.repeat_last(&mut sequence, c, min, max)?;
                }
                '{' => {
                    self.pos += 1;
                    self.skip_space(nested);
                    let min = self.integer()?;
                    self.skip_space(nested);
                    let max = match self.peek() {
                        Some('}') => Some(min),
                        Some(',') => {
                            self.pos += 1;
                            self.skip_space(nested);
                            let max = if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                                let max = self.integer()?;
                                self.skip_space(nested);
                                Some(max)
                            } else {
                                None
                            };
                            if self.peek() != Some('}') {
                                return Err(self.error("expecting `}`"));
                            }
                            max
                        }
                        _ => return Err(self.error("expecting `,` or `}` in a repetition")),
                    };
                    self.pos += 1;
                    if max.is_some_and(|max| max < min) {
                        return Err(self
                            .error(format!("a repetition's maximum is below its minimum {min}")));
                    }
                    self.repeat_last(&mut sequence, '{', min, max)?;
                }
                c if is_word_char(c) => {
                    let name = self.name()?;
                    sequence.push(Node::Rule(name));
                }
                _ => break,
            }
            self.skip_space(nested);
        }
        Ok(sequence)
    }

    fn repeat_last(
        &self,
        sequence: &mut Sequence,
        operator: char,
        min: u32,
        max: Option<u32>,
    ) -> Result<(), GbnfError> {
        let Some(node) = sequence.pop() else {
            return Err(self.error(format!("`{operator}` has nothing before it to repeat")));
        };
        sequence.push(Node::Repeat {
            node: Box::new(node),
            min,
            max,
        });
        Ok(())
    }

    fn integer(&mut self) -> Result<u32, GbnfError> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(self.error("expecting an integer"));
        }
        let digits: String = self.source[start..self.pos].iter().collect();
        digits
            .parse()
            .map_err(|_| self.error(format!("{digits} does not fit a repetition bound")))
    }

    /// One character of a literal or a class, with llama.cpp's escapes.
    fn character(&mut self) -> Result<char, GbnfError> {
        let Some(c) = self.peek() else {
            return Err(self.error("unexpected end of input"));
        };
        self.pos += 1;
        if c != '\\' {
            return Ok(c);
        }
        let Some(escape) = self.peek() else {
            return Err(self.error("unexpected end of input after `\\`"));
        };
        self.pos += 1;
        match escape {
            'x' => self.hex(2),
            'u' => self.hex(4),
            'U' => self.hex(8),
            't' => Ok('\t'),
            'r' => Ok('\r'),
            'n' => Ok('\n'),
            '\\' | '"' | '[' | ']' => Ok(escape),
            other => Err(self.error(format!("unknown escape `\\{other}`"))),
        }
    }

    fn hex(&mut self, digits: usize) -> Result<char, GbnfError> {
        let mut value: u32 = 0;
        for _ in 0..digits {
            let Some(digit) = self.peek().and_then(|c| c.to_digit(16)) else {
                return Err(self.error(format!("expecting {digits} hex digits")));
            };
            self.pos += 1;
            value = (value << 4) | digit;
        }
        char::from_u32(value)
            .ok_or_else(|| self.error(format!("U+{value:X} is not a Unicode scalar value")))
    }
}

/// llama.cpp's `is_word_char`: no underscore.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

// ---------------------------------------------------------------------------
// The matcher
// ---------------------------------------------------------------------------

struct Matcher<'a> {
    grammar: &'a Grammar,
    input: &'a [char],
    /// The end positions a rule reaches from a start position.
    memo: BTreeMap<(&'a str, usize), BTreeSet<usize>>,
}

impl<'a> Matcher<'a> {
    fn rule(&mut self, name: &'a str, start: usize) -> BTreeSet<usize> {
        if let Some(ends) = self.memo.get(&(name, start)) {
            return ends.clone();
        }
        let Some(alternatives) = self.grammar.rules.get(name) else {
            return BTreeSet::new();
        };
        let ends = self.alternatives(alternatives, &BTreeSet::from([start]));
        self.memo.insert((name, start), ends.clone());
        ends
    }

    fn alternatives(
        &mut self,
        alternatives: &'a Alternatives,
        starts: &BTreeSet<usize>,
    ) -> BTreeSet<usize> {
        let mut ends = BTreeSet::new();
        for sequence in alternatives {
            ends.extend(self.sequence(sequence, starts));
        }
        ends
    }

    fn sequence(&mut self, sequence: &'a [Node], starts: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut current = starts.clone();
        for node in sequence {
            if current.is_empty() {
                break;
            }
            current = self.node(node, &current);
        }
        current
    }

    fn node(&mut self, node: &'a Node, starts: &BTreeSet<usize>) -> BTreeSet<usize> {
        match node {
            Node::Literal(chars) => starts
                .iter()
                .filter(|&&start| self.input[start..].starts_with(chars))
                .map(|&start| start + chars.len())
                .collect(),
            Node::Class { negated, ranges } => starts
                .iter()
                .filter(|&&start| {
                    self.input.get(start).is_some_and(|c| {
                        ranges.iter().any(|(low, high)| (low..=high).contains(&c)) != *negated
                    })
                })
                .map(|&start| start + 1)
                .collect(),
            Node::Any => starts
                .iter()
                .filter(|&&start| start < self.input.len())
                .map(|&start| start + 1)
                .collect(),
            Node::Rule(name) => {
                let mut ends = BTreeSet::new();
                for &start in starts {
                    ends.extend(self.rule(name, start));
                }
                ends
            }
            Node::Group(alternatives) => self.alternatives(alternatives, starts),
            Node::Repeat { node, min, max } => {
                let mut reached = if *min == 0 {
                    starts.clone()
                } else {
                    BTreeSet::new()
                };
                let mut frontier = starts.clone();
                let mut count = 0u32;
                while max.is_none_or(|max| count < max) {
                    frontier = self.node(node, &frontier);
                    count += 1;
                    if frontier.is_empty() {
                        break;
                    }
                    if count >= *min {
                        // Nothing new: every position the next round could start from has already
                        // been expanded, so every position it could reach has been reached.
                        if frontier.is_subset(&reached) {
                            break;
                        }
                        reached.extend(frontier.iter().copied());
                    }
                }
                reached
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Not rows of the Phase 8 table: test 8.14 says
// the four grammars parse, and these say that "parses" means what llama.cpp means by it.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn syntax_error_line(text: &str) -> usize {
    match Grammar::parse(text) {
        Err(GbnfError::Syntax { line, .. }) => line,
        other => panic!("expected a syntax error, got {other:?}"),
    }
}

/// Appendix A.2's printed form, and the reason test 8.14 needed a parser of its own: a
/// continuation line beginning with `|` is a new rule to llama.cpp, and `|` is not a name. The
/// same choice inside parentheses is legal, because only a nested sequence may cross a newline.
#[test]
fn a_rule_ends_at_the_end_of_its_line_unless_it_is_parenthesised() {
    let printed = "root ::= role\nrole ::= \"a\"\n       | \"b\"\n";
    assert_eq!(syntax_error_line(printed), 3);

    let parenthesised = "root ::= role\nrole ::= (\n    \"a\"\n  | \"b\"\n)\n";
    let grammar = Grammar::parse(parenthesised).expect("legal GBNF");
    assert!(grammar.accepts("a") && grammar.accepts("b") && !grammar.accepts("ab"));

    let same_line = "root ::= \"a\" | \"b\"\n";
    assert!(Grammar::parse(same_line).is_ok());
}

#[test]
fn a_grammar_needs_a_root_and_every_rule_it_names() {
    assert_eq!(Grammar::parse("start ::= \"a\"\n"), Err(GbnfError::NoRoot));
    assert_eq!(
        Grammar::parse("root ::= missing\n"),
        Err(GbnfError::Undefined("missing".to_owned()))
    );
    assert_eq!(
        Grammar::parse("root ::= \"a\"\nroot ::= \"b\"\n"),
        Err(GbnfError::Redefined("root".to_owned()))
    );
}

/// The server refuses left recursion, and the matcher could not terminate on it. It hides behind
/// a prefix that can match nothing as easily as it shows at the front.
#[test]
fn left_recursion_is_refused_even_behind_a_prefix_that_can_be_empty() {
    assert_eq!(
        Grammar::parse("root ::= root \"a\" | \"b\"\n"),
        Err(GbnfError::LeftRecursive("root".to_owned()))
    );
    assert_eq!(
        Grammar::parse("root ::= maybe root \"x\" | \"y\"\nmaybe ::= \"m\"?\n"),
        Err(GbnfError::LeftRecursive("root".to_owned()))
    );
    // Right recursion is fine: it consumes before it recurses.
    let right = Grammar::parse("root ::= \"a\" root | \"b\"\n").expect("legal");
    assert!(right.accepts("aaab") && !right.accepts("aaa"));
}

/// Exactly llama.cpp's escapes, with its exact hex widths. A `\d` that meant "digit" to whoever
/// wrote it is an error there and must be one here.
#[test]
fn escapes_are_the_ones_llama_cpp_knows() {
    let grammar = Grammar::parse(r#"root ::= "\x41é\U0001F600\t\"\\" [\[\]]"#)
        .expect("every escape is legal");
    assert!(grammar.accepts("Aé😀\t\"\\["));
    assert!(grammar.accepts("Aé😀\t\"\\]"));

    assert!(Grammar::parse(r#"root ::= "\d""#).is_err());
    assert!(Grammar::parse(r#"root ::= "\x4""#).is_err());
    assert!(Grammar::parse(r#"root ::= "\uD800""#).is_err());
    assert!(Grammar::parse("root ::= \"unterminated\n").is_err());
    assert!(Grammar::parse("root ::= [abc\n").is_err());
}

#[test]
fn rule_names_are_llama_cpp_word_characters() {
    assert!(Grammar::parse("root ::= a-b\na-b ::= \"x\"\n").is_ok());
    // An underscore ends the name, and what follows is not `::=`.
    assert!(Grammar::parse("root ::= \"x\"\nsnake_case ::= \"y\"\n").is_err());
}

#[test]
fn repetitions_have_their_meaning_and_their_bounds_are_checked() {
    let grammar = Grammar::parse("root ::= \"a\"{2} \"b\"{1,} \"c\"{0,2} \"d\"? \"e\"* \"f\"+\n")
        .expect("legal");
    assert!(grammar.accepts("aabf"));
    assert!(grammar.accepts("aabbbccdeeeff"));
    assert!(!grammar.accepts("abf"), "{{2}} is exactly two");
    assert!(!grammar.accepts("aaf"), "{{1,}} is at least one");
    assert!(!grammar.accepts("aabcccf"), "{{0,2}} is at most two");
    assert!(!grammar.accepts("aab"), "+ is at least one");

    assert!(Grammar::parse("root ::= \"a\"{3,2}\n").is_err());
    assert!(Grammar::parse("root ::= *\n").is_err());
    assert!(Grammar::parse("root ::= \"a\"{x}\n").is_err());
}

/// A class matches one character; a negated class matches any one character outside it; `.`
/// matches any one character; a group is a choice; and the set simulation copes with a nullable
/// body under an unbounded repetition, which is where a naive loop never ends.
#[test]
fn the_matcher_follows_the_grammar() {
    let grammar =
        Grammar::parse("root ::= [a-c]+ [^0-9] . ( \"x\" | \"yz\" ) ws\nws ::= ( [ ]? )*\n")
            .expect("legal");
    assert!(grammar.accepts("abc-!x"));
    assert!(grammar.accepts("a%%yz   "));
    assert!(
        !grammar.accepts("abc5!x"),
        "the negated class excludes digits"
    );
    assert!(!grammar.accepts("d-!x"), "d is outside a-c");
    assert!(
        !grammar.accepts("a-!y"),
        "yz is one alternative, not y or z"
    );

    let comments = Grammar::parse("# a comment\nroot ::= \"a\" # trailing\n\n# another\n")
        .expect("comments are space");
    assert!(comments.accepts("a"));
}
