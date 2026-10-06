use std::{collections::BTreeMap, env, path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("M2ShelfUpdater: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let command = args
        .first()
        .and_then(|value| value.to_str())
        .ok_or_else(usage)?;
    match command {
        "identity" => {
            if args.len() != 1 {
                return Err(usage());
            }
            identity()
        }
        "sign" => sign(parse_options(&args[1..], &["version", "platform", "file"])?),
        "verify" => verify(parse_options(
            &args[1..],
            &["version", "platform", "file", "signature"],
        )?),
        "apply" => {
            let options = parse_options(&args[1..], &["request"])?;
            let request = PathBuf::from(options.get("request").expect("required option"));
            m2shelf_lib::portable_update::run_apply_request(&request)
        }
        _ => Err(usage()),
    }
}

fn identity() -> Result<(), String> {
    let identity = m2shelf_lib::update::signer_identity_for_cli()?;
    println!(
        "{}",
        serde_json::to_string(&identity)
            .map_err(|error| format!("failed to serialize signer identity: {error}"))?
    );
    Ok(())
}

fn sign(options: BTreeMap<String, String>) -> Result<(), String> {
    let version = options.get("version").expect("required option");
    let platform = options.get("platform").expect("required option");
    let file = PathBuf::from(options.get("file").expect("required option"));
    let private_key = env::var("M2SHELF_UPDATE_PRIVATE_KEY").map_err(|_| {
        "M2SHELF_UPDATE_PRIVATE_KEY is required and is read only from the environment.".to_string()
    })?;
    let output =
        m2shelf_lib::update::sign_artifact_for_cli(&file, version, platform, &private_key)?;
    println!(
        "{}",
        serde_json::to_string(&output)
            .map_err(|error| format!("failed to serialize signing result: {error}"))?
    );
    Ok(())
}

fn verify(options: BTreeMap<String, String>) -> Result<(), String> {
    let version = options.get("version").expect("required option");
    let platform = options.get("platform").expect("required option");
    let file = PathBuf::from(options.get("file").expect("required option"));
    let signature = options.get("signature").expect("required option");
    let output = m2shelf_lib::update::verify_artifact_for_cli(&file, version, platform, signature)?;
    println!(
        "{}",
        serde_json::to_string(&output)
            .map_err(|error| format!("failed to serialize verification result: {error}"))?
    );
    Ok(())
}

fn parse_options(
    args: &[std::ffi::OsString],
    required: &[&str],
) -> Result<BTreeMap<String, String>, String> {
    if args.len() != required.len() * 2 {
        return Err(usage());
    }
    let mut parsed = BTreeMap::new();
    for pair in args.as_chunks::<2>().0 {
        let flag = pair[0].to_str().ok_or_else(usage)?;
        let key = flag.strip_prefix("--").ok_or_else(usage)?;
        if !required.contains(&key) || parsed.contains_key(key) {
            return Err(usage());
        }
        let value = pair[1]
            .to_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(usage)?;
        parsed.insert(key.to_owned(), value.to_owned());
    }
    if required.iter().any(|key| !parsed.contains_key(*key)) {
        return Err(usage());
    }
    Ok(parsed)
}

fn usage() -> String {
    "usage: M2ShelfUpdater.exe identity | sign --version <semver> --platform <windows-x64-portable|windows-x64-nsis> --file <path> | verify --version <semver> --platform <windows-x64-portable|windows-x64-nsis> --file <path> --signature <base64> | apply --request <absolute-path>".into()
}
