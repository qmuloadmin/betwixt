use std::fmt::Debug;
use std::mem;

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until, take_until1, take_while, take_while1};
use nom::character::complete::{alpha1, newline, space0};
use nom::character::{is_newline, is_space};
use nom::combinator::{all_consuming, map, opt, peek};
use nom::error::ParseError;
use nom::sequence::{delimited, pair, preceded, terminated, tuple};
use nom::{IResult, InputLength, InputTake, Parser};

pub const BETWIXT_TOKEN: &'static str = "<?btxt";
pub const BETWIXT_COM_TOKEN: &'static str = "<!--btxt";
pub const CLOSE_TOKEN: &'static str = "?>";
pub const CLOSE_COM_TOKEN: &'static str = "-->";
const FILENAME_PROP: &'static str = "filename";
const LANG_PROP: &'static str = "lang";
const TAG_PROP: &'static str = "tag";
const TANGLE_MODE_PROP: &'static str = "mode";

#[derive(Debug, Clone)]
pub enum TangleMode<'a> {
    Overwrite,
    Append,
    Prepend,
    Insert(&'a [u8]),
}

impl<'a> TangleMode<'a> {
    pub fn from_bytes(b: &[u8]) -> IResult<&[u8], TangleMode> {
        let overwrite = map(tag("overwrite"), |_| TangleMode::Overwrite);
        let append = map(tag("append"), |_| TangleMode::Append);
        let prepend = map(tag("prepend"), |_| TangleMode::Prepend);
        let insert = map(
            pair(
                tag("insert"),
                delimited(tag("["), take_until1("]"), tag("]")),
            ),
            |(_, s)| TangleMode::Insert(s),
        );
        all_consuming(alt((overwrite, append, prepend, insert)))(b)
    }
}

impl<'a> Default for TangleMode<'a> {
    fn default() -> Self {
        Self::Append
    }
}

#[derive(Default, Clone)]
pub struct Properties<'a> {
    pub filename: Option<&'a [u8]>,
    pub language: Option<&'a [u8]>,
    pub tag: Option<&'a [u8]>,
    pub mode: Option<TangleMode<'a>>,
}

impl<'a> Properties<'a> {
    fn merge_parent(&mut self, parent: Properties<'a>) {
        if self.filename.is_none() {
            self.filename = parent.filename;
        }
        if self.language.is_none() {
            self.language = parent.language;
        }
        if self.tag.is_none() {
            self.tag = parent.tag;
        }
        if self.mode.is_none() {
            self.mode = parent.mode;
        }
    }
}

pub struct Code<'a> {
    pub properties: Properties<'a>,
    pub part: CodePart<'a>,
}

#[derive(Clone)]
pub struct CodePart<'a> {
    pub contents: &'a [u8],
    pub lang: Option<&'a [u8]>,
}

pub struct Section<'a> {
    pub heading: Option<&'a [u8]>,
    pub level: usize,
    code_block_indexes: Vec<usize>,
    pub children: Vec<Section<'a>>,
}

pub struct Document<'a> {
	pub code_blocks: Vec<Code<'a>>,
    pub	root: Section<'a>
}

impl<'a> Document<'a> {
	pub fn from_contents<P1, P2, P3>(
        contents: &'a [u8],
        parsers: &mut MarkdownParsers<P1, P2, P3>,
    ) -> Self
    where
        P1: Parser<&'a [u8], Properties<'a>, nom::error::Error<&'a [u8]>>,
        P2: Parser<&'a [u8], Section<'a>, nom::error::Error<&'a [u8]>>,
        P3: Parser<&'a [u8], CodePart<'a>, nom::error::Error<&'a [u8]>>,
    {
        let mut next = scan(contents, true, parsers);
        let mut properties = Properties{
			..Default::default()
		};
		let mut blocks = Vec::new();
        let mut section = Section {
            heading: None,
            level: 0,
            code_block_indexes: Vec::new(),
            children: Vec::new(),
        };
        // a given index in the stack is the current parent of that level.
        let mut section_frame: [Option<Section>; 10] =
            [None, None, None, None, None, None, None, None, None, None]; // support 9 + root levels of headings
		let mut props_frame: [Option<Properties>; 10] =
			[None, None, None, None, None, None, None, None, None, None];
        while next.is_some() {
            let (input, item) = next.unwrap();
            match item {
                ScanResult::Section(new) => {
                    if new.level == section.level {
                        // parent section isn't changing, just the active section is.
                        section_frame[section.level]
                            .as_mut()
                            .unwrap()
                            .children
                            .push(section);
                        section = new;
                    } else if new.level < section.level {
                        // we're going back to a higher level heading. This means append the section
                        // to the current level's parent. Then find the appropriate parent for the new
                        // level and set the new current parent.
                        section_frame[section.level]
                            .as_mut()
                            .unwrap()
                            .children
                            .push(section);
                        if section_frame[new.level].is_none() {
                            // find the next highest index with a parent
                            for idx in new.level + 1..10 {
                                if section_frame[idx].is_some() {
                                    section_frame.swap(new.level, idx);
									properties = props_frame[idx].clone().unwrap();
                                    break;
                                }
                            }
                        } else {
							properties = props_frame[new.level].clone().unwrap();
						}
						// all children lower (numerically higher) than the new section
						// will never get a chance to be reconciled. We need to do so now.
						for idx in (new.level+1..10).rev() {
							if section_frame[idx].is_some() {
								let mut child = None;
								mem::swap(&mut section_frame[idx], &mut child);
								let child = child.unwrap();
								section_frame[child.level].as_mut().unwrap().children.push(child);
							}
						}
                        section = new;
                    } else { // going to a child section
						props_frame[new.level] = Some(properties.clone());
                        section_frame[new.level] = Some(section);
                        section = new;
                    }
                }
                ScanResult::Code(code) => {
					section.code_block_indexes.push(blocks.len());
					blocks.push(Code{
						properties: properties.clone(),
						part: code,
					});
				},
                ScanResult::Properties(mut props) => {
                    props.merge_parent(properties);
                    properties = props;
                }
            }
            next = scan(input, false, parsers);
        }
        section_frame[section.level]
            .as_mut()
            .unwrap()
            .children
            .push(section);
		for opt in section_frame {
			if opt.is_some() {
				return Document{
					code_blocks: blocks,
					root: opt.unwrap()
				};
			}
		}
		panic!("unreachable");
    }
}

pub struct MarkdownParsers<P1, P2, P3> {
    pub betwixt: P1,
    pub section: P2,
    pub code: P3,
}

enum ScanResult<'a> {
    Code(CodePart<'a>),
    Section(Section<'a>),
    Properties(Properties<'a>),
}

pub fn betwixt<'a>(
    start: &'static str,
    end: &'static str,
) -> impl FnMut(&'a [u8]) -> IResult<&[u8], Properties> {
    move |i: &[u8]| {
        let (input, body) = delimited(tag(start), take_until(end), tag(end))(i)?;
        let properties = properties(body)?;
        Ok((input, properties.1))
    }
}

pub fn code(
    code_start: &'static str,
    code_end: &'static str,
) -> impl Fn(&[u8]) -> IResult<&[u8], CodePart> {
    move |i: &[u8]| {
        let (input, (_, lang, _, _)) = tuple((tag(code_start), opt(alpha1), space0, tag("\n")))(i)?;
        let mut terminator = locate_parser_match(tuple((tag(code_end), space0, newline)));
        let end_idx = terminator(input)?;
        let (excess, _) = take_until1("\n")(&input[end_idx..])?;
        Ok((
            excess,
            CodePart {
                contents: &input[..end_idx],
                lang,
            },
        ))
    }
}

// Locate the index at which point a parser succeeded (returned Ok).

fn locate_parser_match<I, O, P, E>(mut parser: P) -> impl FnMut(I) -> Result<usize, nom::Err<E>>
where
    P: Parser<I, O, E>,
    I: InputLength + InputTake,
{
    move |i: I| {
        let mut last_err = None;
        for idx in 0..i.input_len() {
            match parser.parse(i.take_split(idx).0) {
                Ok(_) => return Ok(idx),
                Err(err) => last_err = Some(err),
            }
        }
        // FIXME We need some way to bounds check -- we'll always have a last_err
        // as long as the input length wasn't 0
        Err(last_err.unwrap())
    }
}

// Parse out a section between header levels
pub fn section(mark: char) -> impl Fn(&[u8]) -> IResult<&[u8], Section> {
    move |i: &[u8]| {
        let (input, (header, _, heading)) = tuple((
            take_while1(|c| c == mark as u8),
            take_while1(is_space),
            peek(take_until1("\n")),
        ))(i)?;
        Ok((
            input,
            Section {
                heading: Some(heading),
                level: header.input_len(),
                code_block_indexes: Vec::new(),
                children: Vec::new(),
            },
        ))
    }
}

fn scan<'a, P1, P2, P3>(
    i: &'a [u8],
	first: bool,
    parsers: &mut MarkdownParsers<P1, P2, P3>,
) -> Option<(&'a [u8], ScanResult<'a>)>
where
    P1: Parser<&'a [u8], Properties<'a>, nom::error::Error<&'a [u8]>>,
    P2: Parser<&'a [u8], Section<'a>, nom::error::Error<&'a [u8]>>,
    P3: Parser<&'a [u8], CodePart<'a>, nom::error::Error<&'a [u8]>>,
{
	let mut new_line = first;
    for idx in 0..i.input_len() {
		if new_line {
			// these parsers should only match on newlines, not mid-line
			match parsers.code.parse(&i[idx..]) {
				Ok(result) => return Some((result.0, ScanResult::Code(result.1))),
				Err(_) => {} // continue
			};
			match parsers.section.parse(&i[idx..]) {
				Ok(result) => return Some((result.0, ScanResult::Section(result.1))),
				Err(_) => {} // continue
			};
			new_line = false;
		}
        match parsers.betwixt.parse(&i[idx..]) {
            Ok(result) => return Some((result.0, ScanResult::Properties(result.1))),
            Err(_) => {} // continue
        }
		if i[idx] == 10 {
			new_line = true;
		}
    }
    None
}

fn property<'a>(t: &'static str) -> impl Fn(&[u8]) -> IResult<&[u8], &[u8]> {
    move |i: &[u8]| {
        let (input, _) = take_while(|c| is_space(c) || is_newline(c))(i)?;
        let (input, quote) =
            preceded(tuple((tag(t), tag("="))), alt((tag("'"), tag("\""))))(input)?;
        let (input, bytes) = terminated(take_until(quote), pair(tag(quote), space0))(input)?;
        Ok((input, bytes))
    }
}

// Checks all permutations of input parsers repeatedly against the input until
// all have matched or all remaining fail. Returns None for any unmatches parsers
// TODO make this a macro cause this is silly.
fn opt_permutation<P, I, O, E>(
    mut parsers: (P, P, P, P),
) -> impl FnMut(I) -> IResult<I, (Option<O>, Option<O>, Option<O>, Option<O>), E>
where
    P: Parser<I, O, E>,
    E: ParseError<I>,
    I: Clone + Debug,
{
    move |i: I| {
        let mut success = true;
        let mut results = (None, None, None, None);
        let mut input = i;
        while success {
            success = false;
            if results.0.is_none() {
                if let Ok((i, output)) = parsers.0.parse(input.clone()) {
                    results.0 = Some(output);
                    success = true;
                    input = i;
                }
            }
            if results.1.is_none() {
                if let Ok((i, output)) = parsers.1.parse(input.clone()) {
                    results.1 = Some(output);
                    success = true;
                    input = i;
                }
            }
            if results.2.is_none() {
                if let Ok((i, output)) = parsers.2.parse(input.clone()) {
                    results.2 = Some(output);
                    success = true;
                    input = i;
                }
            }
            if results.3.is_none() {
                if let Ok((i, output)) = parsers.3.parse(input.clone()) {
                    results.3 = Some(output);
                    success = true;
                    input = i;
                }
            }
        }
        Ok((input, results))
    }
}

fn properties(i: &[u8]) -> IResult<&[u8], Properties> {
    let fname = property(FILENAME_PROP);
    let lang = property(LANG_PROP);
    let tag = property(TAG_PROP);
    let mode = property(TANGLE_MODE_PROP);
    let (input, (filename, language, tag, mode)) =
        all_consuming(opt_permutation((fname, lang, tag, mode)))(i)?;
    Ok((
        input,
        Properties {
            filename,
            language,
            tag,
            mode: match mode {
                Some(mode) => Some(TangleMode::from_bytes(mode)?.1),
                None => None,
            },
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_betwixt() {
        let btxt = &b"<?btxt tag='test1'
lang=\"rust\" mode=\"overwrite\" filename='test/src/lib.rs'   ?>";
        let mut betwixt = betwixt(BETWIXT_TOKEN, CLOSE_TOKEN);
        let res = betwixt(&btxt[..]);
        assert!(res.is_ok(), "valid betwixt body should parse successfully");
        let props = res.unwrap().1;
        assert_eq!(
            props.tag,
            Some(&b"test1"[..]),
            "should parse 'tag' successfully"
        );
        assert_eq!(
            props.filename,
            Some(&b"test/src/lib.rs"[..]),
            "should parse 'filename' successfully"
        );
        assert!(props.mode.is_some());
        assert!(matches!(props.mode.unwrap(), TangleMode::Overwrite));
        assert_eq!(
            props.language,
            Some(&b"rust"[..]),
            "should parse 'lang' successfully"
        );
    }

    #[test]
    fn test_betwixt_sad_path() {
        let btxt = &b"<?btxt tag=\"test\" filename='moop' mode= append' ?>";
        let res = betwixt(BETWIXT_TOKEN, CLOSE_TOKEN)(&btxt[..]);
        assert!(res.is_err(), "invalid body should not parse");
    }

    #[test]
    fn test_header_sections() {
        let mut parsers = MarkdownParsers {
            code: code("```", "```"),
            section: section('#'),
            betwixt: betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
        };
        let contents = &b"
Welcome!

## This is some project
with some random body crap
and oh yeah,
Here's some code
```python
print('foo')
```

## Help

More content
";
        let results = scan(&contents[..], true, &mut parsers);
        assert!(results.is_some());
        let results = results.unwrap();
        match results.1 {
            ScanResult::Section(section) => {
                assert_eq!(Some(&b"This is some project"[..]), section.heading);
            }
            _ => panic!("invalid scan result"),
        }
    }

    #[test]
    fn test_code_blocks() {
        let mut parsers = MarkdownParsers {
            code: code("```", "```"),
            section: section('#'),
            betwixt: betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
        };
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
        let results = scan(&contents[..], true, &mut parsers);
        assert!(results.is_some());
        match &results.as_ref().unwrap().1 {
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
        assert_eq!(
            results.unwrap().0,
            &b"\nAnd this isn't code anymore
"[..]
        );
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
        let mut parsers = MarkdownParsers {
            code: code("```", "```"),
            section: section('#'),
            betwixt: betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
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
<?btxt lang='rust' filename='foo.rs'?>
```rust
// some comment goes here
```
##### Section 5A

  ## This doesn't count as a section
  foo bar baz

#### Section 4A
## Section 2B
```python
print('hello world')
```
Ignore all this fluff";
		let doc = Document::from_contents(&markdown[..], &mut parsers);
        let root = doc.root;
        assert_eq!(2, root.children.len());
		// children[0] Section 2A
		assert_eq!(Some(&b"Section 2A"[..]), root.children[0].heading);
        assert_eq!(root.children[0].code_block_indexes.len(), 1);
        assert_eq!(
            doc.code_blocks[root.children[0].code_block_indexes[0]].properties.filename,
            Some(&b"test.rs"[..])
        );
        assert_eq!(
            doc.code_blocks[root.children[0].code_block_indexes[0]].part.contents,
            &b"println!(\"test\");\n"[..]
        );
        assert_eq!(doc.code_blocks[root.children[0].code_block_indexes[0]].properties.language, None);
        assert_eq!(root.children[0].children.len(), 1);
		assert_eq!(Some(&b"Section 3A"[..]), root.children[0].children[0].heading);
		assert_eq!(root.children[0].children[0].children.len(), 2);
		assert_eq!(root.children[0].children[0].code_block_indexes.len(), 1);
		assert_eq!(doc.code_blocks[root.children[0].children[0].code_block_indexes[0]].properties.filename, Some(&b"foo.rs"[..]));
		assert_eq!(doc.code_blocks[root.children[0].children[0].code_block_indexes[0]].properties.language, Some(&b"rust"[..]));

		// children[1] Section 2B
		assert_eq!(Some(&b"Section 2B"[..]), root.children[1].heading);
		assert_eq!(1, root.children[1].code_block_indexes.len());
		assert_eq!(None, doc.code_blocks[root.children[1].code_block_indexes[0]].properties.language);
		assert_eq!(Some(&b"test.rs"[..]), doc.code_blocks[root.children[1].code_block_indexes[0]].properties.filename);
    }
}
