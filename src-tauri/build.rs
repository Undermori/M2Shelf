fn main() {
    println!("cargo:rerun-if-env-changed=M2SHELF_BUILD_DATE");
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    let build_date = std::env::var("M2SHELF_BUILD_DATE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=M2SHELF_BUILD_DATE={build_date}");

    let windows = tauri_build::WindowsAttributes::new()
        .window_icon_path("icons/icon.ico")
        .app_manifest(include_str!("windows-app-manifest.xml"));
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attributes).expect("failed to build Tauri resources")
}
