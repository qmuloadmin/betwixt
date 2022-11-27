use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display};
use std::mem;

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until, take_until1, take_while, take_while1};
use nom::character::complete::{alpha1, newline, space0};
use nom::character::{is_alphanumeric, is_newline, is_space};
use nom::combinator::{all_consuming, map, opt, peek};
use nom::error::ParseError;
use nom::sequence::{delimited, pair, preceded, terminated, tuple};
use nom::{IResult, InputLength, InputTake, Parser};

pub const BETWIXT_TOKEN: &'static str = "<?btxt";
pub const BETWIXT_COM_TOKEN: &'static str = "<!--btxt";
pub const CLOSE_TOKEN: &'static str = "?>";
pub const CLOSE_COM_TOKEN: &'static str = "-->";
const FILENAME_PROP: &'static str = "filename";
const TAG_PROP: &'static str = "tag";
const CODE_PROP: &'static str = "code";
const TANGLE_MODE_PROP: &'static str = "mode";
const IGNORE_PROP: &'static str = "ignore";
const PREFIX_PROP: &'static str = "pre";
const POSTFIX_PROP: &'static str = "post";

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

#[derive(Clone)]
// TODO can we get rid of this Clone?
struct PropertiesCollection<'a> {
    global: Properties<'a>,
    languages: HashMap<&'a [u8], Properties<'a>>,
}

impl<'a> PropertiesCollection<'a> {
    fn get_code_props(&self, lang: Option<&'a [u8]>) -> Properties<'a> {
        match lang {
            None => self.global.clone(),
            Some(lang) => match self.languages.get(lang) {
                None => self.global.clone(),
                Some(lang_props) => {
                    let mut lang_props = lang_props.clone();
                    lang_props.merge(&self.global);
                    lang_props
                }
            },
        }
    }

    fn update(&mut self, lang: Option<&'a [u8]>, mut props: Properties<'a>) {
        match lang {
            Some(lang) => {
                if self.languages.contains_key(lang) {
                    props.merge(self.languages.get(lang).unwrap());
                }
                self.languages.insert(lang, props);
            }
            None => {
                props.merge(&self.global);
                self.global = props;
            }
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct Properties<'a> {
    pub filename: Option<&'a [u8]>,
    pub tag: Option<&'a [u8]>,
    pub mode: Option<TangleMode<'a>>,
    pub ignore: Option<bool>,
    pub prefix: Option<&'a [u8]>,
    pub postfix: Option<&'a [u8]>,
    // TODO there is an alternative where parsing properties with code
    // simply returns a code block with the applied properties. At the moment,
    // though, this is the solution that seems less hacky
    code: Option<&'a [u8]>,
}

impl<'a> Properties<'a> {
    fn merge(&mut self, parent: &Properties<'a>) {
        if self.filename.is_none() {
            self.filename = parent.filename;
        }
        if self.tag.is_none() {
            self.tag = parent.tag;
        }
        if self.mode.is_none() {
            self.mode = parent.mode.clone();
        }
        if self.ignore.is_none() {
            self.ignore = parent.ignore;
        }
        if self.prefix.is_none() {
            self.prefix = parent.prefix;
        }
        if self.postfix.is_none() {
            self.postfix = parent.postfix;
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

pub struct SectionPart<'a> {
    pub heading: Option<&'a [u8]>,
    pub level: usize,
}

pub struct Section<'a> {
    pub part: SectionPart<'a>,
    properties: PropertiesCollection<'a>,
    code_block_indexes: Vec<usize>,
    pub children: Vec<Section<'a>>,
}

impl<'a> Section<'a> {
    fn new(part: SectionPart<'a>, properties: PropertiesCollection<'a>) -> Self {
        Section {
            part,
            properties,
            children: Vec::new(),
            code_block_indexes: Vec::new(),
        }
    }
}

pub struct Document<'a> {
    pub code_blocks: Vec<Code<'a>>,
    pub root: Section<'a>,
}

impl<'a> Document<'a> {
    pub fn from_contents<P1, P2, P3>(
        contents: &'a [u8],
        parsers: &mut MarkdownParsers<P1, P2, P3>,
    ) -> Result<Self, BetwixtParseError>
    where
        P1: PropertiesParser<'a>,
        P2: SectionParser<'a>,
        P3: CodeParser<'a>,
    {
        let mut next = scan(contents, true, parsers);
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
                Ok((input, item)) => {
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
                                        lang: lang,
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
                    next = scan(input, false, parsers);
                }
                Err(err) => {
                    if parsers.strict {
                        return Err(err);
                    }
                    next = scan(contents, true, parsers);
                }
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

enum ScanResult<'a> {
    Code(CodePart<'a>),
    Section(SectionPart<'a>),
    Properties((Option<&'a [u8]>, Properties<'a>)),
    End,
}

// BetwixtParseError occurs when the beginning and end <?btxt ?> tags are matched
// but the properties fail to consume the content completely. This suggests a
// typo and we need to indicate this to the user
#[derive(Debug)]
pub enum BetwixtParseError {
    // NoMatch means the Betwixt blocks didn't match open/close tags
    // essentially this means everything is fine -- just a byte stream
    // that isn't a betwixt block
    NoMatch(nom::error::ErrorKind),
    // InvalidProperties means the start/end tags <?btxt ?> were matched
    // but the contents didn't all part to valid properties. It returns the
    // properties that did successfully match
    InvalidProperties,
}

impl Error for BetwixtParseError {}

impl Display for BetwixtParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Self::InvalidProperties => "invalid properties for btxt block found",
                Self::NoMatch(_) =>
                    "no property match. If you're seeing this error, there's a bug. Report it!",
            }
        )
    }
}

impl ParseError<&[u8]> for BetwixtParseError {
    fn from_error_kind(_input: &[u8], kind: nom::error::ErrorKind) -> Self {
        Self::NoMatch(kind)
    }

    fn append(_input: &[u8], _kind: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

pub fn betwixt<'a>(
    start: &'static str,
    end: &'static str,
) -> impl FnMut(&'a [u8]) -> IResult<&'a [u8], (Option<&'a [u8]>, Properties), BetwixtParseError> {
    move |i: &[u8]| {
        let (input, (lang, body)) = delimited(
            tag(start),
            pair(
                opt(preceded(tag("+"), take_while(is_alphanumeric))),
                take_until(end),
            ),
            tag(end),
        )(i)?;
        let properties = properties(body)
            .map_err(|_| nom::Err::Failure(BetwixtParseError::InvalidProperties))?;
        Ok((input, (lang, properties.1)))
    }
}

pub fn code<'a>(
    code_start: &'static str,
    code_end: &'static str,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], CodePart> {
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
pub fn section<'a>(mark: char) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], SectionPart> {
    move |i: &'a [u8]| {
        let (input, (header, _, heading)) = tuple((
            take_while1(|c| c == mark as u8),
            take_while1(is_space),
            peek(take_until1("\n")),
        ))(i)?;
        Ok((
            input,
            SectionPart {
                heading: Some(heading),
                level: header.input_len(),
            },
        ))
    }
}

pub trait PropertiesParser<'a>:
    Parser<&'a [u8], (Option<&'a [u8]>, Properties<'a>), BetwixtParseError>
{
}
impl<'a, T> PropertiesParser<'a> for T where
    T: Parser<&'a [u8], (Option<&'a [u8]>, Properties<'a>), BetwixtParseError>
{
}
pub trait SectionParser<'a>:
    Parser<&'a [u8], SectionPart<'a>, nom::error::Error<&'a [u8]>>
{
}
impl<'a, T> SectionParser<'a> for T where
    T: Parser<&'a [u8], SectionPart<'a>, nom::error::Error<&'a [u8]>>
{
}
pub trait CodeParser<'a>: Parser<&'a [u8], CodePart<'a>, nom::error::Error<&'a [u8]>> {}
impl<'a, T> CodeParser<'a> for T where T: Parser<&'a [u8], CodePart<'a>, nom::error::Error<&'a [u8]>>
{}

fn scan<'a, P1, P2, P3>(
    i: &'a [u8],
    first: bool,
    parsers: &mut MarkdownParsers<P1, P2, P3>,
) -> Result<(&'a [u8], ScanResult<'a>), BetwixtParseError>
where
    P1: PropertiesParser<'a>,
    P2: SectionParser<'a>,
    P3: CodeParser<'a>,
{
    let mut new_line = first;
    for idx in 0..i.input_len() {
        if new_line {
            // these parsers should only match on newlines, not mid-line
            match parsers.code.parse(&i[idx..]) {
                Ok(result) => return Ok((result.0, ScanResult::Code(result.1))),
                Err(_) => {} // continue
            };
            match parsers.section.parse(&i[idx..]) {
                Ok(result) => return Ok((result.0, ScanResult::Section(result.1))),
                Err(_) => {} // continue
            };
            new_line = false;
        }
        match parsers.betwixt.parse(&i[idx..]) {
            Ok(result) => return Ok((result.0, ScanResult::Properties(result.1))),
            Err(err) => match err {
                nom::Err::Failure(err) | nom::Err::Error(err) => match err {
                    BetwixtParseError::InvalidProperties => return Err(err),
                    _ => {}
                },
                _ => panic!("unreachable"), // shouldn't occur when dealing with "complete" types
            },
        }
        if i[idx] == 10 {
            new_line = true;
        }
    }
    Ok((&i[i.input_len()..], ScanResult::End))
}

fn property<'a>(t: &'static str) -> impl Fn(&[u8]) -> IResult<&[u8], &[u8]> {
    move |i: &[u8]| {
        let (input, _) = take_while(|c| is_space(c) || is_newline(c))(i)?;
        let (input, quote) = preceded(
            tuple((tag(t), tag("="))),
            alt((tag("'"), tag("\""), tag("|||"))),
        )(input)?;
        let (input, bytes) = terminated(take_until(quote), pair(tag(quote), space0))(input)?;
        Ok((input, bytes))
    }
}

fn bool_property<'a>(t: &'static str) -> impl Fn(&[u8]) -> IResult<&[u8], bool> {
    move |i: &[u8]| {
        let (input, _) = take_while(|c| is_space(c) || is_newline(c))(i)?;
        let (input, bytes) = delimited(
            pair(tag(t), tag("=")),
            alt((tag("true"), tag("false"))),
            opt(space0),
        )(input)?;
        Ok((
            input,
            match bytes {
                b"true" => true,
                _ => false,
            },
        ))
    }
}

// Checks all permutations of input parsers repeatedly against the input until
// all have matched or all remaining fail. Returns None for any unmatches parsers
// TODO make this a macro cause this is silly.
fn opt_permutation<P, PBOOL, I, O, OBOOL, E>(
    mut parsers: (P, P, P, P, P, P, PBOOL),
) -> impl FnMut(
    I,
) -> IResult<
    I,
    (
        Option<O>,
        Option<O>,
        Option<O>,
        Option<O>,
        Option<O>,
        Option<O>,
        Option<OBOOL>,
    ),
    E,
>
where
    P: Parser<I, O, E>,
    PBOOL: Parser<I, OBOOL, E>,
    E: ParseError<I>,
    I: Clone + Debug,
{
    move |i: I| {
        let mut success = true;
        let mut results = (None, None, None, None, None, None, None);
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
            if results.4.is_none() {
                if let Ok((i, output)) = parsers.4.parse(input.clone()) {
                    results.4 = Some(output);
                    success = true;
                    input = i;
                }
            }
            if results.5.is_none() {
                if let Ok((i, output)) = parsers.5.parse(input.clone()) {
                    results.5 = Some(output);
                    success = true;
                    input = i;
                }
            }
            if results.6.is_none() {
                if let Ok((i, output)) = parsers.6.parse(input.clone()) {
                    results.6 = Some(output);
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
    let tag = property(TAG_PROP);
    let mode = property(TANGLE_MODE_PROP);
    let code = property(CODE_PROP);
    let ignore = bool_property(IGNORE_PROP);
    let prefix = property(PREFIX_PROP);
    let postfix = property(POSTFIX_PROP);
    let (input, (filename, prefix, postfix, tag, mode, code, ignore)) = all_consuming(
        opt_permutation((fname, prefix, postfix, tag, mode, code, ignore)),
    )(i)?;
    Ok((
        input,
        Properties {
            filename,
            tag,
            prefix,
            postfix,
            mode: match mode {
                Some(mode) => Some(TangleMode::from_bytes(mode)?.1),
                None => None,
            },
            code,
            ignore,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_betwixt() {
        let btxt = &b"<?btxt+rust tag='test1'
 mode=\"overwrite\" filename='test/src/lib.rs' code=|||
print('foo')||| ignore=false  ?>";
        let mut betwixt = betwixt(BETWIXT_TOKEN, CLOSE_TOKEN);
        let res = betwixt(&btxt[..]);
        assert!(res.is_ok(), "valid betwixt body should parse successfully");
        let props = res.unwrap().1;
        assert_eq!(
            props.1.tag,
            Some(&b"test1"[..]),
            "should parse 'tag' successfully"
        );
        assert_eq!(
            props.1.filename,
            Some(&b"test/src/lib.rs"[..]),
            "should parse 'filename' successfully"
        );
        assert!(props.1.mode.is_some());
        assert!(matches!(props.1.mode.unwrap(), TangleMode::Overwrite));
        assert_eq!(
            props.0,
            Some(&b"rust"[..]),
            "should parse lang successfully"
        );
        assert_eq!(
            props.1.code,
            Some(
                &b"
print('foo')"[..]
            )
        );
        assert_eq!(props.1.ignore, Some(false));
        let btxt = &b"<?btxt pre=|||package main
import \"fmt\"
func main() {||| post='}' ?>";
        let res = betwixt(&btxt[..]);
        assert!(res.is_ok());
        let props = res.unwrap().1;
        assert_eq!(
            props.1.prefix,
            Some(
                &b"package main
import \"fmt\"
func main() {"[..]
            )
        );
        assert_eq!(props.1.postfix, Some(&b"}"[..]));
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
<?btxt filename='foo' tog='bad' ?>"[..];
        let mut parsers = MarkdownParsers {
            code: code("```", "```"),
            section: section('#'),
            betwixt: betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
            strict: true,
        };

        assert!(Document::from_contents(contents, &mut parsers).is_err());
    }

    #[test]
    fn test_header_sections() {
        let mut parsers = MarkdownParsers {
            code: code("```", "```"),
            section: section('#'),
            betwixt: betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
            strict: true,
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
        assert!(results.is_ok());
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
            strict: true,
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
        assert!(results.is_ok());
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
        let doc = Document::from_contents(&markdown[..], &mut parsers).unwrap();
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
