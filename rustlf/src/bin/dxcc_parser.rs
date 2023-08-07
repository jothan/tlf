use std::fs::File;

use rustlf::newtlf::countryfile::CountryData;

fn main() -> Result<(), std::io::Error> {
    let mut args = std::env::args();
    args.next();

    let file = File::open("/usr/share/tlf/cty.dat")?;
    let data = CountryData::load::<std::io::Error, _>(file)?;
    println!("version: {:?}\n", data.version());

    for mut arg in args {
        arg.make_ascii_uppercase();
        let pfx_idx = if let Some(idx) = data.find_best_match(&arg) {
            idx
        } else {
            println!("{arg}: not found\n");
            continue;
        };
        let pfx = data.prefix_by_index(pfx_idx).unwrap();
        println!("{arg}\nprefix: {pfx:?}");
        let cty = data.country_by_index(pfx.country_idx).unwrap();
        println!("country: {cty:?}\n");
    }

    Ok(())
}
