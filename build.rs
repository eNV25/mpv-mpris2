use std::{env, error, path::Path};

fn main() -> Result<(), Box<dyn error::Error>> {
    const MPV_SYMBOLS: &str = "(MPV|mpv)_.*";
    let header = Path::new(&pkg_config::get_variable("mpv", "includedir")?).join("mpv/client.h");
    let header = <&str>::try_from(header.as_os_str())?;
    let output = Path::new(&env::var("OUT_DIR")?).join("ffi.rs");
    bindgen::builder()
        .header(header)
        .clang_arg("-Wp,-D_FORTIFY_SOURCE=2")
        .clang_arg("-Wp,-DMPV_ENABLE_DEPRECATED=0")
        .opaque_type("mpv_handle")
        .allowlist_item(MPV_SYMBOLS)
        .constified_enum(MPV_SYMBOLS)
        .prepend_enum_name(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()?
        .write_to_file(output)?;
    Ok(())
}
