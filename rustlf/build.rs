use std::{env, error::Error};

fn main() -> Result<(), Box<dyn Error>> {
    let crate_dir = env::var("CARGO_MANIFEST_DIR")?;

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_language(cbindgen::Language::C)
        .with_include_guard("RUST_H")
        .with_sys_include("curses.h")
        .with_sys_include("hamlib/rig.h")
        .generate()?
        .write_to_file("rust.h");

    Ok(())
}
