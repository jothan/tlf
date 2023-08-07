use cstr::cstr;
use fxhash::FxHashMap;
use std::{
    ffi::{CStr, CString},
    io::Read,
};

use self::parser::{parse_reader, CountryLine, Line};

pub mod parser;

const MAX_LINE_LENGTH: usize = 256;
const VERSION_LENGTH: usize = 12;

#[repr(transparent)]
#[derive(Debug, Copy, Clone)]
pub struct CqZone(pub(crate) u8);

impl From<CqZone> for u8 {
    fn from(value: CqZone) -> Self {
        value.0
    }
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone)]
pub struct ItuZone(pub(crate) u8);

impl From<ItuZone> for u8 {
    fn from(value: ItuZone) -> Self {
        value.0
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct Prefix {
    pub prefix: CString,
    pub cq_zone: CqZone,
    pub itu_zone: ItuZone,
    pub country_idx: usize, /* cty number is index in dxcc table */
    pub lat: f32,
    pub lon: f32,
    pub continent: &'static CStr,
    pub timezone: f32,
    pub exact: bool,
}

impl Prefix {
    fn from_parsed(prefix: &parser::Prefix, country: &Country, country_idx: usize) -> Self {
        let o = &prefix.override_;
        let coords = o.coordinates.unwrap_or((country.lat, country.lon));
        Prefix {
            prefix: CString::new(prefix.prefix).unwrap(),
            cq_zone: o.cq_zone.unwrap_or(country.cq_zone),
            itu_zone: o.itu_zone.unwrap_or(country.itu_zone),
            country_idx,
            lat: coords.0,
            lon: coords.1,
            continent: o
                .continent
                .as_ref()
                .map(|c| c.as_cstr())
                .unwrap_or(country.continent),
            timezone: o.timezone.unwrap_or(country.timezone),
            exact: prefix.exact,
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct Country {
    pub main_prefix: CString,
    pub name: CString,
    pub cq_zone: CqZone,
    pub itu_zone: ItuZone,
    pub continent: &'static CStr,
    pub lat: f32,
    pub lon: f32,
    pub timezone: f32,
    pub starred: bool,
}

impl Default for Country {
    fn default() -> Self {
        Country {
            name: CString::new("Not Specified").unwrap(),
            main_prefix: CString::new("").unwrap(),
            cq_zone: CqZone(0),
            itu_zone: ItuZone(0),
            continent: cstr!("--"),
            lat: 0.0,
            lon: 0.0,
            timezone: 0.0,
            starred: false,
        }
    }
}

impl From<&CountryLine<'_>> for Country {
    fn from(value: &CountryLine<'_>) -> Self {
        Country {
            name: CString::new(value.name).unwrap(),
            main_prefix: CString::new(value.main_prefix).unwrap(),
            cq_zone: value.cq_zone,
            itu_zone: value.itu_zone,
            continent: value.continent.as_cstr(),
            lat: value.lat,
            lon: value.lon,
            timezone: value.timezone,
            starred: value.starred,
        }
    }
}

#[derive(Default)]
pub struct CountryData {
    countries: Vec<Country>,
    prefixes: Vec<Prefix>,
    prefix_map: FxHashMap<String, usize>,
    version: [u8; VERSION_LENGTH],
}

impl CountryData {
    fn push_country(&mut self, country: CountryLine) {
        self.countries.push((&country).into());
    }

    fn push_prefixes(&mut self, prefixes: &[parser::Prefix]) {
        let country = self.countries.last().expect("FIXME");
        let country_idx = self.countries.len() - 1;

        for prefix in prefixes {
            let prefix = Prefix::from_parsed(prefix, country, country_idx);
            Self::push_prefix(
                prefix,
                &mut self.prefixes,
                &mut self.prefix_map,
                &mut self.version,
            );
        }
    }

    fn push_prefix(
        prefix: Prefix,
        prefixes: &mut Vec<Prefix>,
        prefix_map: &mut FxHashMap<String, usize>,
        version: &mut [u8; VERSION_LENGTH],
    ) {
        let prefix_b = prefix.prefix.as_bytes_with_nul();
        if prefix_b.starts_with(b"VER") && prefix_b.len() == VERSION_LENGTH && prefix_b.is_ascii() {
            version.as_mut_slice().copy_from_slice(prefix_b);
        }
        prefix_map.insert(prefix.prefix.to_str().unwrap().to_owned(), prefixes.len());
        prefixes.push(prefix);
    }

    pub fn load<E, R: Read>(reader: R) -> Result<CountryData, std::io::Error> {
        let mut data = CountryData::default();

        parse_reader(
            reader,
            |line: Result<_, _>| match line {
                Ok((_, Line::Country(country))) => {
                    data.push_country(country);
                    Ok(())
                }
                Ok((_, Line::Prefixes(prefixes))) => {
                    data.push_prefixes(&prefixes);
                    Ok(())
                }
                Ok((_, Line::Empty)) => Ok(()),
                Err(_) => Err(std::io::Error::from(std::io::ErrorKind::InvalidData)),
            },
            MAX_LINE_LENGTH,
        )?;

        Ok(data)
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

    pub fn prefix_by_index(&self, idx: usize) -> Option<&Prefix> {
        self.prefixes.get(idx)
    }

    pub fn country_by_index(&self, idx: usize) -> Option<&Country> {
        self.countries.get(idx)
    }

    pub fn version(&self) -> Option<&str> {
        if self.version[0] == 0 {
            None
        } else {
            Some(std::str::from_utf8(&self.version[..VERSION_LENGTH - 1]).unwrap())
        }
    }
}

#[repr(u8)]
#[derive(Debug)]
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
