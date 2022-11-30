use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display};
use std::mem;
use std::str::from_utf8;

use nom::branch::alt;
use nom::bytes::complete::take_until;
use nom::Parser;

mod code;
mod properties;
mod section;

pub use code::code;
use code::*;
use nom::error::ParseError;
use properties::*;
pub use properties::{betwixt, TangleMode};
pub use section::section;
use section::*;

pub const BETWIXT_TOKEN: &'static str = "<?btxt";
pub const BETWIXT_COM_TOKEN: &'static str = "<!--btxt";
pub const CLOSE_TOKEN: &'static str = "?>";
pub const CLOSE_COM_TOKEN: &'static str = "-->";

pub struct Document<'a> {
    pub code_blocks: Vec<Code<'a>>,
    pub root: Section<'a>,
}

impl<'a> Document<'a> {
    pub fn from_contents<P1, P2, P3>(
        contents: &'a [u8],
        parsers: MarkdownParsers<P1, P2, P3>,
    ) -> Result<Self, InvalidMatchDetails>
    where
        P1: LineParser<'a>,
        P2: LineParser<'a>,
        P3: LineParser<'a>,
    {
        let mut parser = alt((parsers.code, parsers.section, parsers.betwixt));
        let mut scanner = LineScanner::new(contents, parsers.strict);
        let mut next = scanner.scan(&mut parser);
        let properties = PropertiesCollection {
            global: Properties {
                ..Default::default()
            },
            languages: HashMap::new(),
        };
        let mut blocks = Vec::new();
        let mut section = Section {
            part: SectionPart {
                heading: None,
                level: 0,
            },
            code_block_indexes: Vec::new(),
            properties,
            children: Vec::new(),
        };
        // a given index in the stack is the current parent of that level.
        let mut section_frame: [Option<Section>; 10] =
            [None, None, None, None, None, None, None, None, None, None]; // support 9 + root levels of headings
        loop {
            match next {
                Ok(item) => {
                    match item {
                        ScanResult::Section(new) => {
                            if new.level == section.part.level {
                                // parent section isn't changing, just the active section is.
                                let props = section_frame[section.part.level]
                                    .as_ref()
                                    .unwrap()
                                    .properties
                                    .clone();
                                section_frame[section.part.level]
                                    .as_mut()
                                    .unwrap()
                                    .children
                                    .push(section);
                                section = Section::new(new, props);
                            } else if new.level < section.part.level {
                                // we're going back to a higher level heading. This means append the section
                                // to the current level's parent. Then find the appropriate parent for the new
                                // level and set the new current parent.
                                section_frame[section.part.level]
                                    .as_mut()
                                    .unwrap()
                                    .children
                                    .push(section);
                                if section_frame[new.level].is_none() {
                                    // find the next highest index with a parent
                                    for idx in new.level + 1..10 {
                                        if section_frame[idx].is_some() {
                                            section_frame.swap(new.level, idx);
                                            break;
                                        }
                                    }
                                }
                                // all children lower (numerically higher) than the new section
                                // will never get a chance to be reconciled. We need to do so now.
                                for idx in (new.level + 1..10).rev() {
                                    if section_frame[idx].is_some() {
                                        let mut child = None;
                                        mem::swap(&mut section_frame[idx], &mut child);
                                        let child = child.unwrap();
                                        section_frame[child.part.level]
                                            .as_mut()
                                            .unwrap()
                                            .children
                                            .push(child);
                                    }
                                }
                                let idx = new.level;
                                section = Section::new(
                                    new,
                                    section_frame[idx].as_ref().unwrap().properties.clone(),
                                );
                            } else {
                                // going to a child section
                                let props = section.properties.clone();
                                section_frame[new.level] = Some(section);
                                section = Section::new(new, props);
                            }
                        }
                        ScanResult::Code(code) => {
                            let props = section.properties.get_code_props(code.lang);
                            if !props.ignore.unwrap_or(false) {
                                section.code_block_indexes.push(blocks.len());
                                blocks.push(Code {
                                    properties: props,
                                    part: code,
                                });
                            }
                        }
                        ScanResult::Properties(props) => {
                            if let Some(code) = props.1.code {
                                section.code_block_indexes.push(blocks.len());
                                let lang = props.0;
                                section.properties.update(props.0, props.1);
                                let props = section.properties.get_code_props(lang);
                                blocks.push(Code {
                                    part: CodePart {
                                        lang,
                                        contents: code,
                                    },
                                    properties: props,
                                })
                            } else {
                                section.properties.update(props.0, props.1);
                            }
                        }
                        ScanResult::End => {
                            break;
                        }
                    }
                    next = scanner.scan(&mut parser);
                }
                Err(err) => return Err(err),
            }
        }
        section_frame[section.part.level]
            .as_mut()
            .unwrap()
            .children
            .push(section);
        for idx in (0..10).rev() {
            if section_frame[idx].is_some() {
                let mut child = None;
                mem::swap(&mut section_frame[idx], &mut child);
                let child = child.unwrap();
                match section_frame[child.part.level].as_mut() {
                    Some(parent) => parent.children.push(child),
                    None => {
                        return Ok(Document {
                            code_blocks: blocks,
                            root: child,
                        })
                    }
                }
            }
        }
        panic!("unreachable");
    }
}

pub struct MarkdownParsers<P1, P2, P3> {
    pub betwixt: P1,
    pub section: P2,
    pub code: P3,
    pub strict: bool,
}

#[derive(Debug, PartialEq)]
pub enum ScanResult<'a> {
    Code(CodePart<'a>),
    Section(SectionPart<'a>),
    Properties((Option<&'a [u8]>, Properties<'a>)),
    End,
}

#[derive(Debug, PartialEq)]
pub enum LineParseResult<'a> {
    Matched(ScanResult<'a>),
    PartialMatch,
}

#[derive(Debug)]
// The error result of any LineParser
pub enum LineParseError {
    // Not really an error, just indicates the parser didn't match this line (move on)
    NoMatch,
    // We matched start/end tokens but the body had invalid contents. Check strict mode
    InvalidMatch,
}

impl<'a> ParseError<&'a [u8]> for LineParseError {
    fn from_error_kind(_input: &'a [u8], _kind: nom::error::ErrorKind) -> Self {
        LineParseError::NoMatch
    }

    fn append(_input: &'a [u8], _kind: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

#[derive(Debug)]
pub struct InvalidMatchDetails {
    line_start: usize,
    line_end: usize,
    line: String,
}

impl Error for InvalidMatchDetails {}

impl Display for InvalidMatchDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid properties from line {} to line {}: {}",
            self.line_start, self.line_end, self.line,
        )
    }
}

// TODO the line parser approach is very inefficient with long multi line strings
// as it has to continually try and parse for each line. We can improve this, just need
// more sophisticated types
pub trait LineParser<'a>: Parser<&'a [u8], LineParseResult<'a>, LineParseError> {}
impl<'a, F> LineParser<'a> for F where F: Parser<&'a [u8], LineParseResult<'a>, LineParseError> {}

struct LineScanner<'a> {
    // lines stores the end index of each line in the byte slice
    // e.g. data[lines[x]] should always be set to \n
    lines: Vec<usize>,
    slice: (usize, usize), // the start and end of the current working slice
    data: &'a [u8],        // all the bytes in the file
    strict: bool,
    block_start: usize,
}

impl<'a> LineScanner<'a> {
    fn new(data: &'a [u8], strict: bool) -> Self {
        LineScanner {
            lines: Vec::new(),
            slice: (0, 0),
            block_start: 1,
            data,
            strict,
        }
    }
    fn scan<P>(&mut self, parser: &mut P) -> Result<ScanResult<'a>, InvalidMatchDetails>
    where
        P: LineParser<'a>,
    {
        while self.slice.1 != self.data.len() {
            let line = match take_until::<&str, &'a [u8], nom::error::Error<&'a [u8]>>("\n")(
                &self.data[self.slice.1..],
            ) {
                Ok((_, line)) => line,
                Err(_) => &self.data[self.slice.1..],
            };
            self.lines.push(self.slice.1 + line.len());
            let new_end = std::cmp::min(self.data.len(), self.slice.1 + line.len() + 1);
            self.slice = (self.slice.0, new_end);
            match parser.parse(&self.data[self.slice.0..self.slice.1]) {
                Ok((_, result)) => match result {
                    LineParseResult::Matched(m) => {
                        self.slice = (self.slice.1, self.slice.1);
                        return Ok(m);
                    }
                    LineParseResult::PartialMatch => {
                        self.block_start = self.lines.len();
                        return self.scan(parser);
                    }
                },
                Err(err) => {
                    if self.strict {
                        match err {
                            nom::Err::Incomplete(_) => panic!("unreachable in complete parsers"),
                            nom::Err::Error(err) | nom::Err::Failure(err) => match err {
                                LineParseError::InvalidMatch => {
                                    return Err(InvalidMatchDetails {
                                        line_start: self.block_start,
                                        line_end: self.lines.len(),
                                        line: from_utf8(&self.data[self.slice.0..self.slice.1])
                                            .unwrap()
                                            .to_string(),
                                    })
                                }
                                LineParseError::NoMatch => {
                                    self.block_start = self.lines.len() + 1;
                                    self.slice = (self.slice.1, self.slice.1)
                                }
                            },
                        };
                    } else {
                        self.slice = (self.slice.1, self.slice.1)
                    }
                }
            };
        }
        Ok(ScanResult::End)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_betwixt() {
        let btxt = &b"<?btxt+rust tag='test1'
 mode=\"overwrite\" filename='test/src/lib.rs' code=|||
print('foo')||| ignore=false  ?>";
        let betwixt = betwixt(BETWIXT_TOKEN, CLOSE_TOKEN);
        let res = betwixt(&btxt[..]);
        assert!(res.is_ok(), "valid betwixt body should parse successfully");
        let props = res.unwrap().1;
        assert_eq!(
            props,
            LineParseResult::Matched(ScanResult::Properties((
                Some(&b"rust"[..]),
                Properties {
                    tag: Some(&b"test1"[..]),
                    mode: Some(TangleMode::Overwrite),
                    filename: Some(&b"test/src/lib.rs"[..]),
                    code: Some(
                        &b"
print('foo')"[..]
                    ),
                    ignore: Some(false),
                    ..Default::default()
                }
            )))
        );
        let btxt = &b"<?btxt pre=|||package main
import \"fmt\"
func main() {||| post='}' ?>";
        let res = betwixt(&btxt[..]);
        assert!(res.is_ok());
        let props = res.unwrap().1;
        assert_eq!(
            props,
            LineParseResult::Matched(ScanResult::Properties((
                None,
                Properties {
                    prefix: Some(
                        &b"package main
import \"fmt\"
func main() {"[..]
                    ),
                    postfix: Some(&b"}"[..]),
                    ..Default::default()
                }
            )))
        );
    }

    #[test]
    fn test_betwixt_sad_path() {
        let btxt = &b"<?btxt tag=\"test\" filename='moop' mode= append' ?>";
        let res = betwixt(BETWIXT_TOKEN, CLOSE_TOKEN)(&btxt[..]);
        assert!(res.is_err(), "invalid body should not parse");
    }

    #[test]
    fn test_strict_mode_properties() {
        let contents = &b"Some stuff that doesn't matter

More contents
<?btxt filename='foo'
tog='bad' ?>"[..];
        let parsers = MarkdownParsers {
            code: code("```", "```"),
            section: section('#'),
            betwixt: betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
            strict: true,
        };

        let result = Document::from_contents(contents, parsers);
        assert!(result.is_err());
        match result {
            Err(err) => assert_eq!(
                err.to_string(),
                "invalid properties from line 4 to line 5: <?btxt filename='foo'
tog='bad' ?>"
            ),
            Ok(_) => panic!("unreachable"),
        }
    }

    #[test]
    fn test_header_sections() {
        let contents = &b"
Welcome!

## This is some project
with some random body crap

## Help

More content
";
        let mut parser = alt((
            code("```", "```"),
            section('#'),
            betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
        ));
        let mut scanner = LineScanner::new(&contents[..], true);
        let results = scanner.scan(&mut parser);
        assert!(results.is_ok());
        let results = results.unwrap();
        match results {
            ScanResult::Section(section) => {
                assert_eq!(Some(&b"This is some project"[..]), section.heading);
            }
            _ => panic!("invalid scan result"),
        }
        let results = scanner.scan(&mut parser);
        assert!(results.is_ok());
        let results = results.unwrap();
        match results {
            ScanResult::Section(section) => assert_eq!(Some(&b"Help"[..]), section.heading),
            _ => panic!("invalid scan result"),
        }
    }

    #[test]
    fn test_code_blocks() {
        let mut parser = alt((
            code("```", "```"),
            section('#'),
            betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
        ));
        let contents = &b"
This is a big ol' code block
''
With some potential gotchas!
```rust
```this doesn't count
// this is still code
```
And this isn't code anymore
";
        let mut scanner = LineScanner::new(&contents[..], true);
        let results = scanner.scan(&mut parser);
        assert!(results.is_ok());
        match &results.as_ref().unwrap() {
            ScanResult::Code(code) => {
                assert!(code.lang.is_some());
                assert_eq!(code.lang.unwrap(), &b"rust"[..]);
                assert_eq!(
                    code.contents,
                    &b"```this doesn't count
// this is still code
"[..]
                );
            }
            _ => panic!("unexpected scan result"),
        }
    }

    #[test]
    fn test_tangle_mode() {
        let overwrite = &b"overwrite";
        let parsed = TangleMode::from_bytes(&overwrite[..]);
        assert!(
            parsed.is_ok(),
            "parsing valid string 'overwrite' should succeed"
        );
        assert!(
            matches!(parsed.unwrap().1, TangleMode::Overwrite),
            "parsing valid 'overwrite' should yield Overwrite mode"
        );
        let append = &b"append";
        let parsed = TangleMode::from_bytes(&append[..]);
        assert!(
            parsed.is_ok(),
            "parsing valid string 'append' should succeed"
        );
        assert!(
            matches!(parsed.unwrap().1, TangleMode::Append),
            "parsing 'append' should yield Append mode"
        );
        let prepend = &b"prepend";
        let parsed = TangleMode::from_bytes(&prepend[..]);
        assert!(
            parsed.is_ok(),
            "parsing valid string 'prepend' should succeed"
        );
        assert!(
            matches!(parsed.unwrap().1, TangleMode::Prepend),
            "parsing 'prepend' should yield Prepend mode"
        );
        let insert = &b"insert[<<>> INSERT HERE <<>>]";
        let parsed = TangleMode::from_bytes(&insert[..]);
        assert!(
            parsed.is_ok(),
            "parsing valid string 'insert[<<>> INSERT HERE <<>>] should succeed"
        );
        assert!(matches!(
            parsed.unwrap().1,
            TangleMode::Insert(b"<<>> INSERT HERE <<>>")
        ));
        let excess = &b"appends";
        let parsed = TangleMode::from_bytes(&excess[..]);
        assert!(
            parsed.is_err(),
            "parsing invalid string 'appends' should produce parse failure"
        );
        let partial = &b"insert[]";
        let parsed = TangleMode::from_bytes(&partial[..]);
        assert!(
            parsed.is_err(),
            "partial invalid string 'insert[]' should product parsed failure"
        );
    }

    #[test]
    fn test_section_composition() {
        let parsers = MarkdownParsers {
            code: code("```", "```"),
            section: section('#'),
            betwixt: betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
            strict: true,
        };
        let markdown = &b"Test document
<?btxt filename='test.rs' ?> some other stuff
## Section 2A
#And not a new section-
```rust
println!(\"test\");
```
### Section 3A
some content that we don't care about
<?btxt filename='foo.rs'?>
```rust
// some comment goes here
```
##### Section 5A

  ## This doesn't count as a section
  foo bar baz
<?btxt+python filename='foo.py' code=|||
print('this is inline python')
# But it doesn't show up in the markdown!
||| ?>
##### Section 5B

```python
# This code block should no longer have filename='foo.py'
@ As we're now in a sibling of those props
```

#### Section 4A
## Section 2B
```python
print('hello world')
```

This code block shouldn't be included
<?btxt ignore=true ?>
```silly
PrInTlN('foo');
```
Ignore all this fluff";
        let doc = Document::from_contents(&markdown[..], parsers).unwrap();
        let root = doc.root;
        assert_eq!(2, root.children.len());
        // children[0] Section 2A
        assert_eq!(Some(&b"Section 2A"[..]), root.children[0].part.heading);
        assert_eq!(root.children[0].code_block_indexes.len(), 1);
        assert_eq!(
            doc.code_blocks[root.children[0].code_block_indexes[0]]
                .properties
                .filename,
            Some(&b"test.rs"[..])
        );
        assert_eq!(
            doc.code_blocks[root.children[0].code_block_indexes[0]]
                .part
                .contents,
            &b"println!(\"test\");\n"[..]
        );
        assert_eq!(root.children[0].children.len(), 1);
        assert_eq!(
            Some(&b"Section 3A"[..]),
            root.children[0].children[0].part.heading
        );
        assert_eq!(root.children[0].children[0].children.len(), 3);
        assert_eq!(root.children[0].children[0].code_block_indexes.len(), 1);
        assert_eq!(
            doc.code_blocks[root.children[0].children[0].code_block_indexes[0]]
                .properties
                .filename,
            Some(&b"foo.rs"[..])
        );
        assert_eq!(
            root.children[0].children[0].children[0]
                .code_block_indexes
                .len(),
            1
        );

        assert_eq!(
            root.children[0].children[0].children[0].part.heading,
            Some(&b"Section 5A"[..])
        );
        assert_eq!(
            doc.code_blocks[root.children[0].children[0].children[0].code_block_indexes[0]]
                .properties
                .filename,
            Some(&b"foo.py"[..])
        );
        assert_eq!(
            doc.code_blocks[root.children[0].children[0].children[1].code_block_indexes[0]]
                .properties
                .filename,
            Some(&b"foo.rs"[..])
        );
        // children[1] Section 2B
        assert_eq!(Some(&b"Section 2B"[..]), root.children[1].part.heading);
        assert_eq!(1, root.children[1].code_block_indexes.len());
        assert_eq!(
            Some(&b"test.rs"[..]),
            doc.code_blocks[root.children[1].code_block_indexes[0]]
                .properties
                .filename
        );
    }
}
