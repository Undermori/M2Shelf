use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

fn collect_sources(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(path).expect("read worker sources") {
        let path = entry.expect("worker source entry").path();
        if path.is_dir() {
            collect_sources(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn source_package(profile: &Path) {
    let mut files = Vec::new();
    collect_sources(Path::new("vendor/libmobi"), &mut files);
    files.extend(
        [
            "native/mobi_worker.c",
            "native/build_mobi.rs",
            "native/BUILD-MOBI.md",
            "native/build_mobi_worker.ps1",
            "native/THIRD-PARTY-NOTICES.txt",
        ]
        .map(PathBuf::from),
    );
    files.sort();
    fs::create_dir_all("resources").expect("worker resource directory");
    let path = Path::new("resources/M2ShelfMobi-source.zip");
    let mut archive = zip::ZipWriter::new(fs::File::create(path).expect("worker source package"));
    for source in files {
        archive
            .start_file(
                source.to_string_lossy().replace('\\', "/"),
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .expect("worker source file");
        archive
            .write_all(&fs::read(&source).expect("read worker source"))
            .expect("write worker source");
        println!("cargo:rerun-if-changed={}", source.display());
    }
    archive.finish().expect("complete worker source package");
    fs::copy(path, profile.join("M2ShelfMobi-source.zip")).expect("copy worker source package");
    fs::copy(
        "native/THIRD-PARTY-NOTICES.txt",
        profile.join("THIRD-PARTY-NOTICES.txt"),
    )
    .expect("copy worker notices");
}

pub fn build() {
    // The standalone LGPL worker keeps the application independent of libmobi's
    // linking licence and gives untrusted native parsing its own memory/time cap.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let mut build = cc::Build::new();
    build
        .include("vendor/libmobi/src")
        .define("USE_MINIZ", None)
        .define("PACKAGE_VERSION", "\"0.12\"")
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .define("MINIZ_NO_STDIO", None)
        .define("MINIZ_NO_ZLIB_COMPATIBLE_NAMES", None)
        .define("MINIZ_NO_TIME", None)
        .define("MINIZ_NO_ARCHIVE_APIS", None)
        .define("MINIZ_NO_ARCHIVE_WRITING_APIS", None)
        .debug(false)
        .opt_level(2);
    let compiler = build.get_compiler();
    assert!(
        compiler.is_like_msvc(),
        "Windows MOBI worker requires the supported MSVC toolchain"
    );
    let files = [
        "buffer",
        "compression",
        "debug",
        "index",
        "memory",
        "meta",
        "parse_rawml",
        "read",
        "structure",
        "util",
        "write",
        "miniz",
    ];
    let mut objects = Vec::new();
    for (name, source) in files
        .iter()
        .map(|name| (*name, format!("vendor/libmobi/src/{name}.c")))
        .chain(std::iter::once((
            "mobi_worker",
            "native/mobi_worker.c".into(),
        )))
    {
        let object = out.join(format!("{name}.obj"));
        let mut command = compiler.to_command();
        command
            .arg("/c")
            .arg(&source)
            .arg(format!("/Fo{}", object.display()));
        assert!(
            command.status().expect("compile MOBI worker").success(),
            "MOBI worker compilation failed: {name}"
        );
        println!("cargo:rerun-if-changed={source}");
        objects.push(object);
    }
    let worker = out.join("M2ShelfMobi.exe");
    let mut link: Command = compiler.to_command();
    link.args(&objects)
        .arg(format!("/Fe{}", worker.display()))
        .arg("/link")
        .arg("/Brepro");
    assert!(
        link.status().expect("link MOBI worker").success(),
        "MOBI worker link failed"
    );
    let profile = out.ancestors().nth(3).expect("target profile");
    fs::copy(&worker, profile.join("M2ShelfMobi.exe")).expect("copy MOBI worker");
    fs::create_dir_all("binaries").expect("worker sidecar directory");
    let target = env::var("TARGET").expect("target triple");
    fs::copy(&worker, format!("binaries/M2ShelfMobi-{target}.exe"))
        .expect("copy bundled MOBI worker");
    source_package(profile);
    println!("cargo:rerun-if-changed=vendor/libmobi/src");
    println!("cargo:rerun-if-changed=native/build_mobi.rs");
}
