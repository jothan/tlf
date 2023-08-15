use std::fs::File;

use rustlf::newtlf::countryfile::DxccData;

fn main() -> Result<(), std::io::Error> {
    let mut args = std::env::args();
    args.next();

    let file = File::open("/usr/share/tlf/cty.dat")?;
    let data = DxccData::load::<std::io::Error, _>(file)?;
    println!("version: {:?}\n", data.prefixes.version());

    for mut arg in args {
        arg.make_ascii_uppercase();
        let pfx_idx = if let Some(idx) = data.prefixes.find_best_match(&arg) {
            idx
        } else {
            println!("{arg}: not found\n");
            continue;
        };
        let pfx = data.prefixes.get(pfx_idx).unwrap();
        println!("{arg}\nprefix: {pfx:?}");
        let cty = data.countries.get(pfx.country_idx).unwrap();
        println!("country: {cty:?}\n");
    }

    Ok(())
}
