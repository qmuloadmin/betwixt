use nom::bytes::complete::{take_until1, take_while1};
use nom::character::is_space;
use nom::combinator::peek;
use nom::sequence::tuple;
use nom::{IResult, InputLength};
use std::collections::HashMap;
use std::fmt::Debug;

use crate::LineParseError;

use super::properties::Properties;
use super::{LineParseResult, ScanResult};

#[derive(Debug, PartialEq)]
pub struct SectionPart<'a> {
    pub heading: Option<&'a [u8]>,
    pub level: usize,
}

#[derive(Clone, Debug, PartialEq)]
// TODO can we get rid of this Clone?
pub struct PropertiesCollection<'a> {
    pub global: Properties<'a>,
    pub languages: HashMap<&'a [u8], Properties<'a>>,
}

impl<'a> PropertiesCollection<'a> {
    pub fn get_code_props(&self, lang: Option<&'a [u8]>) -> Properties<'a> {
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

    pub fn update(&mut self, lang: Option<&'a [u8]>, mut props: Properties<'a>) {
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

#[derive(Debug, PartialEq)]
pub struct Section<'a> {
    pub part: SectionPart<'a>,
    pub properties: PropertiesCollection<'a>,
    pub code_block_indexes: Vec<usize>,
    pub children: Vec<Section<'a>>,
}

impl<'a> Section<'a> {
    pub fn new(part: SectionPart<'a>, properties: PropertiesCollection<'a>) -> Self {
        Section {
            part,
            properties,
            children: Vec::new(),
            code_block_indexes: Vec::new(),
        }
    }
}

// Parse out a section between header levels
pub fn section<'a>(
    mark: char,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], LineParseResult<'a>, LineParseError> {
    move |i: &'a [u8]| {
        let (input, (header, _, heading)) = tuple((
            take_while1(|c| c == mark as u8),
            take_while1(is_space),
            peek(take_until1("\n")),
        ))(i)?;
        Ok((
            input,
            LineParseResult::Matched(ScanResult::Section(SectionPart {
                heading: Some(heading),
                level: header.input_len(),
            })),
        ))
    }
}
