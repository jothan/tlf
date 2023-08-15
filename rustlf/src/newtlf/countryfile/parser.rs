use std::io::Read;

use super::{Continent, CqZone, ItuZone};

use linereader::LineReader;
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::{
        self,
        complete::{none_of, space0, space1},
    },
    combinator::{all_consuming, map, opt, recognize, verify},
    error::ErrorKind,
    multi::{fold_many0, fold_many1, many0},
    number,
    sequence::{delimited, pair, preceded, separated_pair, terminated, tuple},
    IResult, InputTakeAtPosition,
};

type PResult<'a, T> = IResult<&'a str, T>;

#[derive(Debug)]
pub enum Line<'a> {
    Country(CountryLine<'a>),
    Prefixes(Vec<Prefix<'a>>),
    Empty,
}

#[derive(Debug)]
pub struct CountryLine<'a> {
    pub main_prefix: &'a str,
    pub name: &'a str,
    pub cq_zone: CqZone,
    pub itu_zone: ItuZone,
    pub continent: Continent,
    pub lat: f32,
    pub lon: f32,
    pub timezone: f32,
    pub starred: bool,
}

#[derive(Debug)]
pub struct Prefix<'a> {
    pub exact: bool,
    pub prefix: &'a str,
    pub override_: PrefixOverrides,
}

enum PrefixOverride {
    CqZone(CqZone),
    ItuZone(ItuZone),
    Coordinates(f32, f32),
    Continent(Continent),
    Timezone(f32),
}

#[derive(Default, Debug)]
pub struct PrefixOverrides {
    pub cq_zone: Option<CqZone>,
    pub itu_zone: Option<ItuZone>,
    pub coordinates: Option<(f32, f32)>,
    pub continent: Option<Continent>,
    pub timezone: Option<f32>,
}

fn continent(input: &str) -> PResult<Continent> {
    alt((
        map(tag("NA"), |_| Continent::NorthAmerica),
        map(tag("SA"), |_| Continent::SouthAmerica),
        map(tag("EU"), |_| Continent::Europe),
        map(tag("AS"), |_| Continent::Asia),
        map(tag("AF"), |_| Continent::Africa),
        map(tag("OC"), |_| Continent::Oceania),
    ))(input)
}

fn colon(input: &str) -> PResult<&str> {
    tag(":")(input)
}

fn not_colon(input: &str) -> PResult<&str> {
    input.split_at_position1_complete(|c| c == ':', ErrorKind::Char)
}

fn country_name(input: &str) -> PResult<&str> {
    verify(not_colon, |country: &str| {
        !country.starts_with(' ') && !country.starts_with('\t')
    })(input)
}

fn main_prefix(input: &str) -> PResult<(bool, &str)> {
    pair(map(opt(tag("*")), |s| s.is_some()), not_colon)(input)
}

pub fn country_line(input: &str) -> PResult<CountryLine> {
    map(
        tuple((
            terminated(country_name, colon),
            delimited(space0, cq_zone, colon),
            delimited(space0, itu_zone, colon),
            delimited(space0, continent, colon),
            delimited(space0, number::complete::float, colon), // lat
            delimited(space0, number::complete::float, colon), // lon
            delimited(space0, number::complete::float, colon), // "timezone"
            delimited(space0, main_prefix, colon),
        )),
        |(name, cq_zone, itu_zone, continent, lat, lon, timezone, (starred, main_prefix))| {
            CountryLine {
                name,
                cq_zone,
                itu_zone,
                continent,
                lat,
                lon,
                timezone,
                main_prefix,
                starred,
            }
        },
    )(input)
}

fn prefix_string(input: &str) -> PResult<&str> {
    recognize(fold_many1(none_of(",;()[]<>{}~"), || (), |_, _| ()))(input)
}

fn cq_zone(input: &str) -> PResult<CqZone> {
    map(character::complete::u8, CqZone)(input)
}

fn itu_zone(input: &str) -> PResult<ItuZone> {
    map(character::complete::u8, ItuZone)(input)
}

fn prefix_override(input: &str) -> PResult<PrefixOverride> {
    alt((
        map(
            delimited(tag("("), cq_zone, tag(")")),
            PrefixOverride::CqZone,
        ),
        map(
            delimited(tag("["), itu_zone, tag("]")),
            PrefixOverride::ItuZone,
        ),
        map(
            delimited(
                tag("<"),
                separated_pair(number::complete::float, tag("/"), number::complete::float),
                tag(">"),
            ),
            |(lat, lon)| PrefixOverride::Coordinates(lat, lon),
        ),
        map(
            delimited(tag("{"), continent, tag("}")),
            PrefixOverride::Continent,
        ),
        map(
            delimited(tag("~"), number::complete::float, tag("~")),
            PrefixOverride::Timezone,
        ),
    ))(input)
}

fn prefix_overrides(input: &str) -> PResult<PrefixOverrides> {
    fold_many0(
        prefix_override,
        Default::default,
        |mut acc: PrefixOverrides, item| {
            match item {
                PrefixOverride::CqZone(zone) => acc.cq_zone = Some(zone),
                PrefixOverride::ItuZone(zone) => acc.itu_zone = Some(zone),
                PrefixOverride::Coordinates(lat, lon) => acc.coordinates = Some((lat, lon)),
                PrefixOverride::Continent(continent) => acc.continent = Some(continent),
                PrefixOverride::Timezone(tz) => acc.timezone = Some(tz),
            };
            acc
        },
    )(input)
}

fn prefix(input: &str) -> PResult<Prefix> {
    map(
        tuple((
            map(opt(tag("=")), |s| s.is_some()),
            prefix_string,
            prefix_overrides,
        )),
        |(exact, prefix, override_)| Prefix {
            exact,
            prefix,
            override_,
        },
    )(input)
}

pub fn prefix_line(input: &str) -> PResult<Vec<Prefix>> {
        preceded(
            space1, raw_prefix_line)(input)
}

pub fn raw_prefix_line(input: &str) -> PResult<Vec<Prefix>> {
    map(
            pair(
                many0(terminated(prefix, tag(","))),
                opt(terminated(prefix, tag(";"))),
            ),
        |(mut start, end)| {
            if let Some(end) = end {
                start.push(end);
            };
            start
        },
    )(input)
}

pub fn line(input: &str) -> PResult<Line> {
    alt((
        map(country_line, Line::Country),
        map(prefix_line, Line::Prefixes),
        map(space0, |_| Line::Empty),
    ))(input)
}

pub fn parse_reader<E, R: Read, C>(
    reader: R,
    mut consumer: C,
    max_line_length: usize,
) -> Result<(), E>
where
    E: From<std::io::Error>,
    C: FnMut(PResult<Line>) -> Result<(), E>,
{
    let mut reader = LineReader::with_capacity(max_line_length, reader);

    while let Some(input_line) = reader.next_line() {
        let input_line = String::from_utf8_lossy(input_line?);
        let input_line = input_line.trim_end_matches(|c| c == '\r' || c == '\n');
        consumer(all_consuming(line)(input_line))?;
    }

    Ok(())
}
