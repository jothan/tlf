extern crate bindgen;
extern crate cbindgen;

use std::env;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    /*bindgen::Builder::default()
        .header("../src/globalvars.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .clang_args(&[
            "-I/usr/include/glib-2.0",
            "-I/usr/lib/x86_64-linux-gnu/glib-2.0/include",
        ]).generate()
        .expect("Can't generate bindings");*/

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_language(cbindgen::Language::C)
        .with_include_guard("RUST_H")
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file("rust.h");
}
