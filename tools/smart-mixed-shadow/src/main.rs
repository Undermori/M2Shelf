use m2shelf_smart_mixed_shadow::{
    adapter::{ReadIndex, ReadOptions},
    run,
};
use std::{path::PathBuf, time::Instant};

#[cfg(windows)]
fn peak_memory() -> Option<usize> {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        faults: u32,
        peak: usize,
        current: usize,
        paged_peak: usize,
        paged: usize,
        nonpaged_peak: usize,
        nonpaged: usize,
        pagefile: usize,
        pagefile_peak: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn K32GetProcessMemoryInfo(p: *mut std::ffi::c_void, c: *mut Counters, s: u32) -> i32;
    }
    let mut c = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        ..Default::default()
    };
    // Read-only OS telemetry, never a media or filesystem operation.
    unsafe {
        (K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut c,
            std::mem::size_of::<Counters>() as u32,
        ) != 0)
            .then_some(c.peak)
    }
}
#[cfg(not(windows))]
fn peak_memory() -> Option<usize> {
    None
}

#[cfg(feature = "fixtures")]
fn make_fixture(case: &str) -> Result<PathBuf, String> {
    use m2shelf_smart_mixed_lab::model::{LibraryKind, Snapshot};
    use m2shelf_smart_mixed_shadow::fixture::Factory;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp/smart-mixed-phase2/cli-fixtures");
    std::fs::create_dir_all(&root).map_err(|_| "FIXTURE_DIRECTORY_FAILED")?;
    // Fixed app-private location, generated filename, create_new, no destination from CLI.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "CLOCK_ERROR")?
        .as_nanos();
    let path = root.join(format!("synthetic-{}-{stamp}.sqlite", std::process::id()));
    if let Some(n) = case.strip_prefix("benchmark:") {
        let count: usize = n.parse().map_err(|_| "INVALID_COUNT")?;
        if count > 100000 {
            return Err("BENCHMARK_LIMIT".into());
        }
        let mut f = Factory::create(&path, LibraryKind::Comic, "FOLDER")?;
        f.connection
            .execute_batch("BEGIN IMMEDIATE;")
            .map_err(|_| "FIXTURE_BEGIN_FAILED")?;
        for i in 0..count {
            f.file_book(&format!("Independent-{i:06}.pdf"))?;
        }
        f.connection
            .execute_batch("COMMIT;")
            .map_err(|_| "FIXTURE_COMMIT_FAILED")?;
    } else {
        let number: u32 = case.parse().map_err(|_| "INVALID_CASE")?;
        if !(1..=49).contains(&number) {
            return Err("INVALID_CASE".into());
        }
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../docs/smart-mixed/fixtures/{number:02}.json")),
        )
        .map_err(|_| "MISSING_GOLDEN")?;
        let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "BAD_GOLDEN")?;
        let input: Snapshot =
            serde_json::from_value(v["input"].clone()).map_err(|_| "BAD_SNAPSHOT")?;
        let mut f = Factory::create(&path, input.media_kind, "FOLDER")?;
        f.import(&input)?;
    }
    Ok(path)
}
fn execute() -> Result<(), String> {
    let a = std::env::args().skip(1).collect::<Vec<_>>();
    #[cfg(feature = "fixtures")]
    if a.len() == 2 && a[0] == "--make-fixture" {
        let p = make_fixture(&a[1])?;
        println!("{}", serde_json::json!({"synthetic_db":p,"root_id":1}));
        return Ok(());
    }
    if a.len() != 4 || a[0] != "--shadow" || a[3] != "--authorized-index" {
        return Err("Usage: --shadow DATABASE ROOT_ID --authorized-index; fixture build additionally supports --make-fixture CASE|benchmark:COUNT".into());
    }
    let root: i64 = a[2].parse().map_err(|_| "INVALID_ROOT_ID")?;
    let start = Instant::now();
    let mut index = ReadIndex::open(&PathBuf::from(&a[1]))?;
    let (_snapshot, report, timing) = run(&mut index, root, &ReadOptions::default())?;
    // Stream bounded summary; do not serialize full private plan or duplicate it into JSON Value.
    let serialization = Instant::now();
    let summary = serde_json::to_string(&report.summary).map_err(|_| "SERIALIZATION_FAILED")?;
    let serialization_ms = serialization.elapsed().as_secs_f64() * 1000.;
    println!("{{\"summary\":{summary},\"timings\":{},\"summary_serialization_ms\":{serialization_ms},\"open_through_summary_ms\":{},\"peak_working_set_bytes\":{}}}",serde_json::to_string(&timing).map_err(|_|"SERIALIZATION_FAILED")?,start.elapsed().as_secs_f64()*1000.,peak_memory().map_or("null".into(),|n|n.to_string()));
    Ok(())
}
fn main() {
    if let Err(e) = execute() {
        eprintln!("{}", serde_json::json!({"error":e}));
        std::process::exit(2);
    }
}
