use std::fmt::Debug;

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until, take_until1, take_while};
use nom::character::complete::space0;
use nom::character::{is_alphanumeric, is_newline, is_space};
use nom::combinator::{all_consuming, map, opt};
use nom::error::ParseError;
use nom::sequence::{delimited, pair, preceded, terminated, tuple};
use nom::{IResult, Parser};

use crate::LineParseError;

use super::{LineParseResult, ScanResult};

const FILENAME_PROP: &'static str = "filename";
const TAG_PROP: &'static str = "tag";
const CODE_PROP: &'static str = "code";
const TANGLE_MODE_PROP: &'static str = "mode";
const IGNORE_PROP: &'static str = "ignore";
const PREFIX_PROP: &'static str = "pre";
const POSTFIX_PROP: &'static str = "post";

#[derive(Default, Clone, Debug, PartialEq)]
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
    pub code: Option<&'a [u8]>,
}

#[derive(Debug, Clone, PartialEq)]
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

impl<'a> Properties<'a> {
    pub fn merge(&mut self, parent: &Properties<'a>) {
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

pub fn betwixt<'a>(
    start: &'static str,
    end: &'static str,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], LineParseResult<'a>, LineParseError<'a>> {
    move |i: &[u8]| {
        let (input, _) = tag(start)(i)?;
        let (input, (lang, body)) = match terminated(
            pair(
                opt(preceded(
                    tag::<&str, &'a [u8], nom::error::Error<&'a [u8]>>("+"),
                    take_while(is_alphanumeric),
                )),
                take_until(end),
            ),
            tag(end),
        )(input)
        {
            Ok(result) => result,
            Err(_) => return Ok((input, LineParseResult::PartialMatch)),
        };
        let properties = properties(body).map_err(|err| match err {
            nom::Err::Failure(err) | nom::Err::Error(err) => {
                nom::Err::Failure(LineParseError::InvalidMatch(err.input))
            }
            _ => panic!("unreachable when dealing with complete bytes"),
        })?;
        Ok((
            input,
            LineParseResult::Matched(ScanResult::Properties((lang, properties.1))),
        ))
    }
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

fn properties<'a>(i: &'a [u8]) -> IResult<&'a [u8], Properties> {
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
