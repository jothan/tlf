use std::{env, error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let crate_dir = env::var("CARGO_MANIFEST_DIR")?;

    let glib = pkg_config::Config::new().probe("glib-2.0")?;

    let includes = glib
        .include_paths
        .iter()
        .map(|path| format!("-I{}", path.to_string_lossy()))
        .chain(std::iter::once("-I..".to_owned()));

    let bindings = bindgen::Builder::default()
        .opaque_type("channel_cap")
        .header("../src/globalvars.h")
        .header("../src/qtcvars.h")
        .header("../src/fldigixmlrpc.h")
        .header("../src/netkeyer.h")
        .header("../src/hamlib_keyer.h")
        .header("../src/clear_display.h")
        .header("../src/background_process.h")
        .header("../src/splitscreen.h")
        .header("../src/rtty.h")
        .header("../src/cqww_simulator.h")
        .header("../src/gettxinfo.h")
        .header("../src/set_tone.h")
        .header("/usr/include/curses.h")
        .clang_args(includes)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .layout_tests(false)
        .generate()
        .map_err(|_| "could not generate bindings")?;

    let out_path = PathBuf::from(env::var("OUT_DIR")?);
    bindings.write_to_file(out_path.join("bindings.rs"))?;

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_language(cbindgen::Language::C)
        .with_include_guard("RUST_H")
        .generate()?
        .write_to_file("rust.h");

    Ok(())
}
