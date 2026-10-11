use std::{env, fs, path::PathBuf};

pub fn build() {
    println!("cargo:rerun-if-env-changed=M2SHELF_TMDB_TOKEN_FILE");
    println!("cargo:rerun-if-env-changed=M2SHELF_REQUIRE_DEFAULT_TMDB");
    let input = env::var_os("M2SHELF_TMDB_TOKEN_FILE");
    let value = input
        .as_ref()
        .map(|path| {
            // Only the path is a build dependency. Never print the input or a transport error.
            println!("cargo:rerun-if-changed={}", PathBuf::from(path).display());
            fs::read_to_string(path).expect("TMDb build input could not be read")
        })
        .unwrap_or_default();
    let value = value.trim();
    let valid = value.len() >= 80
        && value.len() <= 2048
        && value.split('.').count() == 3
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if !value.is_empty() && !valid {
        panic!("TMDb build input is not an API Read Access Token");
    }
    if env::var_os("M2SHELF_REQUIRE_DEFAULT_TMDB").is_some() && !valid {
        panic!("Release requires a validated default TMDb build input");
    }
    let generated = format!(
        "pub static DEFAULT_TOKEN: &str = {:?};\n",
        if valid { value } else { "" }
    );
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("tmdb_default.rs"),
        generated,
    )
    .expect("TMDb build output could not be written");
}
