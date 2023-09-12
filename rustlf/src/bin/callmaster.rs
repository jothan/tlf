use std::{ffi::CString, fs::File};

use newtlf::callmaster::CallMaster;

fn main() -> Result<(), std::io::Error> {
    let mut args = std::env::args();
    args.next();

    let file = File::open("/usr/share/hamradio-files/MASTER.SCP")?;
    let data = CallMaster::load(file, 128, false)?;

    for mut arg in args {
        arg.make_ascii_uppercase();
        let query = CString::new(arg).unwrap();

        eprintln!("starting with {query:?}: ");
        data.starting_with(&query).for_each(|call| {
            let call = call.to_str().unwrap();
            eprintln!("{call}");
        });
        eprintln!();

        eprintln!("containing {query:?}: ");
        data.containing(&query).for_each(|call| {
            let call = call.to_str().unwrap();
            eprintln!("{call}");
        });
    }

    Ok(())
}
