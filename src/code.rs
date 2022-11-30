use std::fmt::Debug;

use nom::bytes::complete::{tag, take_until};
use nom::character::complete::{alpha1, newline, space0};
use nom::combinator::opt;
use nom::sequence::tuple;
use nom::{IResult, InputLength, InputTake, Parser};

use crate::LineParseError;

use super::properties::Properties;
use super::{LineParseResult, ScanResult};

pub struct Code<'a> {
    pub properties: Properties<'a>,
    pub part: CodePart<'a>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodePart<'a> {
    pub contents: &'a [u8],
    pub lang: Option<&'a [u8]>,
}

// Locate the index at which point a parser succeeded (returned Ok).
fn locate_parser_match<I, O, P, E>(mut parser: P) -> impl FnMut(I) -> Option<usize>
where
    P: Parser<I, O, E>,
    I: InputLength + InputTake,
{
    move |i: I| {
        for idx in 0..i.input_len() {
            match parser.parse(i.take_split(idx).0) {
                Ok(_) => return Some(idx),
                Err(_) => {}
            }
        }
        // FIXME We need some way to bounds check -- we'll always have a last_err
        // as long as the input length wasn't 0
        None
    }
}

pub fn code<'a>(
    code_start: &'static str,
    code_end: &'static str,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], LineParseResult<'a>, LineParseError> {
    move |i: &[u8]| {
        let (input, (_, lang, _, _)) = tuple((tag(code_start), opt(alpha1), space0, tag("\n")))(i)?;
        let mut terminator = locate_parser_match(tuple((
            tag(code_end),
            space0::<&'a [u8], nom::error::Error<&'a [u8]>>,
            newline,
        )));
        let end_idx = match terminator(input) {
            Some(result) => result,
            None => return Ok((input, LineParseResult::PartialMatch)),
        };
        let (excess, _) =
            take_until::<&str, &'a [u8], nom::error::Error<&'a [u8]>>("\n")(&input[end_idx..])
                .unwrap();
        Ok((
            excess,
            LineParseResult::Matched(ScanResult::Code(CodePart {
                contents: &input[..end_idx],
                lang,
            })),
        ))
    }
}
