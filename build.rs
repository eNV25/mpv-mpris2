use std::{env, error, path};

fn main() -> Result<(), Box<dyn error::Error>> {
    const MPV_SYMBOLS: &str = "(?i:mpv_).*";
    bindgen::builder()
        .header("src/mpv.h")
        .clang_arg("-Wp,-D_FORTIFY_SOURCE=2")
        .clang_arg("-Wp,-DMPV_ENABLE_DEPRECATED=0")
        .impl_debug(true)
        .allowlist_var(MPV_SYMBOLS)
        .allowlist_type(MPV_SYMBOLS)
        .allowlist_function(MPV_SYMBOLS)
        .rustified_non_exhaustive_enum(MPV_SYMBOLS)
        .opaque_type("mpv_handle")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()?
        .write_to_file(path::PathBuf::from(env::var("OUT_DIR")?).join("mpv.rs"))?;
    Ok(())
}
