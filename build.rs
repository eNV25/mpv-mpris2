use std::{env, error, path::Path};

fn main() -> Result<(), Box<dyn error::Error>> {
    let header = Path::new(&pkg_config::get_variable("mpv", "includedir")?).join("mpv/client.h");

    const MPV_SYMBOLS: &str = "(?i:mpv_).*";
    bindgen::builder()
        .header(<&str>::try_from(header.as_os_str())?)
        .clang_arg("-Wp,-D_FORTIFY_SOURCE=2")
        .clang_arg("-Wp,-DMPV_ENABLE_DEPRECATED=0")
        .impl_debug(true)
        .allowlist_var(MPV_SYMBOLS)
        .allowlist_type(MPV_SYMBOLS)
        .allowlist_function(MPV_SYMBOLS)
        .constified_enum(MPV_SYMBOLS)
        .prepend_enum_name(false)
        .opaque_type("mpv_handle")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()?
        .write_to_file(Path::new(&env::var("OUT_DIR")?).join("mpv.rs"))?;
    Ok(())
}
