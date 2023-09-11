use cstr::cstr;
use nom::combinator::all_consuming;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    ffi::{CStr, CString},
    fmt::Debug,
    io::Read,
    sync::OnceLock,
};

use self::parser::{parse_reader, CountryLine, Line};
use super::ffi::{CStringPtr, StaticCStrPtr};

pub mod ffi;
pub mod lookup;
pub mod parser;

const MAX_LINE_LENGTH: usize = 256;
const VERSION_LENGTH: usize = 12;
static DUMMY_COUNTRY: OnceLock<Country> = OnceLock::new();
static DUMMY_PREFIX: OnceLock<Prefix> = OnceLock::new();

fn dummy_country() -> &'static Country {
    DUMMY_COUNTRY.get_or_init(Country::dummy)
}

#[no_mangle]
pub extern "C" fn dummy_prefix() -> &'static Prefix {
    DUMMY_PREFIX.get_or_init(Prefix::dummy)
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone)]
pub struct CqZone(pub u8);

impl From<CqZone> for u8 {
    fn from(value: CqZone) -> Self {
        value.0
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone)]
pub struct ItuZone(pub u8);

impl From<ItuZone> for u8 {
    fn from(value: ItuZone) -> Self {
        value.0
    }
}

/// cbindgen:field-names=[pfx, cq, itu, dxcc_ctynr, lat, lon, continent, timezone, exact]
#[repr(C)]
#[derive(Debug)]
pub struct Prefix {
    pub prefix: CStringPtr,
    pub cq_zone: CqZone,
    pub itu_zone: ItuZone,
    pub country_idx: usize, /* cty number is index in dxcc table */
    pub lat: f32,
    pub lon: f32,
    pub continent: StaticCStrPtr,
    pub timezone: f32,
    pub exact: bool,
}

impl Prefix {
    pub fn from_parsed(prefix: &parser::Prefix, country: &Country, country_idx: usize) -> Self {
        let o = &prefix.override_;
        let coords = o.coordinates.unwrap_or((country.lat, country.lon));
        Prefix {
            prefix: CString::new(prefix.prefix).unwrap().into(),
            cq_zone: o.cq_zone.unwrap_or(country.cq_zone),
            itu_zone: o.itu_zone.unwrap_or(country.itu_zone),
            country_idx,
            lat: coords.0,
            lon: coords.1,
            continent: o
                .continent
                .as_ref()
                .map(|c| c.as_cstr().into())
                .unwrap_or(country.continent),
            timezone: o.timezone.unwrap_or(country.timezone),
            exact: prefix.exact,
        }
    }

    fn dummy() -> Self {
        Prefix {
            prefix: CString::new("No Prefix").unwrap().into(),
            cq_zone: CqZone(0),
            itu_zone: ItuZone(0),
            country_idx: 0,
            lat: f32::INFINITY,
            lon: f32::INFINITY,
            continent: cstr!("").into(),
            timezone: f32::INFINITY,
            exact: false,
        }
    }
}

/// cbindgen:field-names=[countryname, cq, itu, continent, lat, lon, timezone, pfx, starred]
#[repr(C)]
#[derive(Debug)]
pub struct Country {
    pub name: CStringPtr,
    pub cq_zone: CqZone,
    pub itu_zone: ItuZone,
    pub continent: StaticCStrPtr,
    pub lat: f32,
    pub lon: f32,
    pub timezone: f32,
    pub main_prefix: CStringPtr,
    pub starred: bool,
}

impl Country {
    fn dummy() -> Self {
        Country {
            name: CString::new("Not Specified").unwrap().into(),
            main_prefix: CString::new("").unwrap().into(),
            cq_zone: CqZone(0),
            itu_zone: ItuZone(0),
            continent: cstr!("--").into(),
            lat: 0.0,
            lon: 0.0,
            timezone: 0.0,
            starred: false,
        }
    }
}

impl From<CountryLine<'_>> for Country {
    fn from(value: CountryLine<'_>) -> Self {
        Country {
            name: CString::new(value.name).unwrap().into(),
            main_prefix: CString::new(value.main_prefix).unwrap().into(),
            cq_zone: value.cq_zone,
            itu_zone: value.itu_zone,
            continent: value.continent.as_cstr().into(),
            lat: value.lat,
            lon: value.lon,
            timezone: value.timezone,
            starred: value.starred,
        }
    }
}

#[derive(Default)]
pub struct CountryData(Vec<Country>);

impl CountryData {
    pub fn push(&mut self, country: Country) {
        self.0.push(country);
    }

    pub fn last(&self) -> Option<(usize, &Country)> {
        self.0.last().map(|c| (self.0.len() - 1, c))
    }

    pub fn get(&self, idx: usize) -> Option<&Country> {
        self.0.get(idx)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Default)]
pub struct PrefixData {
    prefixes: Vec<Prefix>,
    prefix_map: BTreeMap<String, usize>,
    version: [u8; VERSION_LENGTH],
}

impl PrefixData {
    pub fn push(&mut self, prefix: Prefix) {
        let prefix_raw = prefix.prefix.as_cstr();
        let prefix_b = prefix_raw.to_bytes_with_nul();
        if prefix_b.starts_with(b"VER") && prefix_b.len() == VERSION_LENGTH && prefix_b.is_ascii() {
            self.version.as_mut_slice().copy_from_slice(prefix_b);
        }
        self.prefix_map
            .insert(prefix_raw.to_str().unwrap().to_owned(), self.prefixes.len());
        self.prefixes.push(prefix);
    }

    pub fn push_parsed(
        &mut self,
        prefixes: &[parser::Prefix],
        country: &Country,
        country_idx: usize,
    ) {
        for prefix in prefixes {
            let prefix = Prefix::from_parsed(prefix, country, country_idx);
            self.push(prefix);
        }
    }

    pub fn version(&self) -> Option<&str> {
        if self.version[0] == 0 {
            None
        } else {
            Some(std::str::from_utf8(&self.version[..VERSION_LENGTH - 1]).unwrap())
        }
    }

    pub fn find_full_match(&self, call: &str) -> Option<usize> {
        self.prefix_map.get(call).copied()
    }

    pub fn find_best_match(&self, call: &str) -> Option<usize> {
        if let Some(idx) = self.find_full_match(call) {
            return Some(idx);
        }

        let mut call = call.to_owned();
        while !call.is_empty() {
            call.pop();

            if let Some(idx) = self
                .prefix_map
                .get(&call)
                .copied()
                .filter(|idx| !self.prefixes[*idx].exact)
            {
                return Some(idx);
            }
        }

        None
    }

    pub fn get(&self, idx: usize) -> Option<&Prefix> {
        self.prefixes.get(idx)
    }

    pub fn getpfxindex<'a>(&self, call: &'a str) -> (Option<usize>, Cow<'a, str>) {
        let stripped_call = lookup::strip_call(call);
        let (check_call, abnormal) = lookup::normalize_call(stripped_call);

        let mut idx = if abnormal {
            self.find_full_match(stripped_call)
        } else {
            self.find_best_match(stripped_call)
        };

        if stripped_call != check_call {
            // only if not found in prefix full call exception list
            idx = idx.or_else(|| self.find_best_match(&check_call))
        }

        (idx, check_call)
    }

    pub fn call_prefix(&self, call: &str) -> Option<&Prefix> {
        let (idx, _) = self.getpfxindex(call);
        idx.and_then(|idx| self.get(idx))
    }

    pub fn clear(&mut self) {
        self.prefixes.clear();
        self.prefix_map.clear();
        self.version = Default::default();
    }

    pub fn len(&self) -> usize {
        self.prefixes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty()
    }
}

#[derive(Default)]
pub struct DxccData {
    pub countries: CountryData,
    pub prefixes: PrefixData,
}

impl DxccData {
    pub fn load<E, R: Read>(reader: R) -> Result<DxccData, std::io::Error> {
        let mut countries = CountryData::default();
        let mut prefixes = PrefixData::default();

        countries.push(Country::dummy());

        parse_reader(
            reader,
            |line: Result<_, _>| match line {
                Ok((_, Line::Country(country))) => {
                    countries.push(country.into());
                    Ok(())
                }
                Ok((_, Line::Prefixes(prefix_line))) => {
                    let (country_idx, country) =
                        countries.last().unwrap_or_else(|| (0, dummy_country()));

                    prefixes.push_parsed(&prefix_line, country, country_idx);
                    Ok(())
                }
                Ok((_, Line::Empty)) => Ok(()),
                Err(_) => Err(std::io::Error::from(std::io::ErrorKind::InvalidData)),
            },
            MAX_LINE_LENGTH,
        )?;

        Ok(DxccData {
            countries,
            prefixes,
        })
    }

    fn push_country_str<'a>(
        &mut self,
        line: &'a str,
    ) -> Result<(), nom::Err<nom::error::Error<&'a str>>> {
        let (_, country_line) = all_consuming(parser::country_line)(line)?;
        self.countries.push(country_line.into());

        Ok(())
    }

    fn push_prefix_str<'a>(
        &mut self,
        line: &'a str,
    ) -> Result<(), nom::Err<nom::error::Error<&'a str>>> {
        let (_, prefix_line) = all_consuming(parser::raw_prefix_line)(line)?;

        let (country_idx, last_country) = self
            .countries
            .last()
            .unwrap_or_else(|| (0, dummy_country()));

        self.prefixes
            .push_parsed(&prefix_line, last_country, country_idx);
        Ok(())
    }

    pub fn call_info(&self, call: &str) -> (Option<&Prefix>, Option<&Country>) {
        let prefix = self.prefixes.call_prefix(call);
        let country = prefix.and_then(|prefix| self.countries.get(prefix.country_idx));

        (prefix, country)
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum Continent {
    NorthAmerica,
    SouthAmerica,
    Europe,
    Asia,
    Africa,
    Oceania,
}

impl Continent {
    fn as_cstr(&self) -> &'static CStr {
        match self {
            Continent::NorthAmerica => cstr!(NA),
            Continent::SouthAmerica => cstr!(SA),
            Continent::Europe => cstr!(EU),
            Continent::Asia => cstr!(AS),
            Continent::Africa => cstr!(AF),
            Continent::Oceania => cstr!(OC),
        }
    }
}
