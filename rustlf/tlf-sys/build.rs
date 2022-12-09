use std::{env, error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let glib = pkg_config::Config::new().probe("glib-2.0")?;

    let includes = glib
        .include_paths
        .iter()
        .map(|path| format!("-I{}", path.to_string_lossy()))
        .chain(std::iter::once("-I..".to_owned()));

    let bindings = bindgen::Builder::default()
        .clang_arg("-F../../src")
        .clang_arg("-I../..")
        .opaque_type("channel_cap")
        .header("globalvars.h")
        .header("qtcvars.h")
        .header("fldigixmlrpc.h")
        .header("hamlib_keyer.h")
        .header("src/clear_display.h")
        .header("background_process.h")
        .header("src/splitscreen.h")
        .header("src/rtty.h")
        .header("src/cqww_simulator.h")
        .header("src/gettxinfo.h")
        .header("src/set_tone.h")
        .header("/usr/include/curses.h")
        .clang_args(includes)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .layout_tests(false)
        .generate()
        .map_err(|_| "could not generate bindings")?;

    let out_path = PathBuf::from(env::var("OUT_DIR")?);
    bindings.write_to_file(out_path.join("bindings.rs"))?;
    Ok(())
}
