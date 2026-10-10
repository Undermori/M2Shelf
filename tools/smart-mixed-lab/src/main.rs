use m2shelf_smart_mixed_lab::{model::*, recognize};
use std::io::{Read, Write};
use std::time::Instant;

const INPUT_LIMIT: u64 = 64 * 1024 * 1024;

#[cfg(windows)]
fn peak_memory() -> Option<usize> {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        page_faults: u32,
        peak_working_set: usize,
        working_set: usize,
        peak_paged: usize,
        paged: usize,
        peak_nonpaged: usize,
        nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut Counters,
            size: u32,
        ) -> i32;
    }
    let mut counters = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        ..Counters::default()
    };
    // OS telemetry only in this CLI; the recognition library remains pure.
    unsafe {
        (K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<Counters>() as u32,
        ) != 0)
            .then_some(counters.peak_working_set)
    }
}
#[cfg(not(windows))]
fn peak_memory() -> Option<usize> {
    None
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--benchmark") {
        let count: usize = args
            .get(1)
            .ok_or("Provide an entry count")?
            .parse()
            .map_err(|_| "Invalid count")?;
        if count > 199_999 {
            return Err("Benchmark limit is 199999 files plus Root".into());
        }
        let snapshot = Snapshot {
            root_id: "synthetic-benchmark".into(),
            media_kind: LibraryKind::Comic,
            complete: true,
            entries: (0..count)
                .map(|i| Entry {
                    path: format!("Independent-{i:06}.pdf"),
                    kind: EntryKind::File,
                    state: EntryState::Available,
                    format: Some(Format::Pdf),
                    verified: true,
                    identity: None,
                    hint: None,
                    metadata: None,
                })
                .collect(),
            page_orders: vec![],
            overrides: vec![],
            prior_units: vec![],
        };
        let start = Instant::now();
        let plan = recognize(&snapshot)?;
        let elapsed = start.elapsed();
        println!(
            "{}",
            serde_json::json!({"input_files":count,"reading_units":plan.reading_units.len(),"groups":plan.groups.len(),"elapsed_ms":elapsed.as_secs_f64()*1000.0,"peak_working_set_bytes":peak_memory(),"memory_scope":"whole fresh process, includes input and output; excludes JSON serialization","rules_version":plan.rules_version})
        );
        return Ok(());
    }
    if args.len() > 1 {
        return Err("Usage: m2shelf-smart-mixed-lab [snapshot.json] | --benchmark COUNT".into());
    }
    let reader: Box<dyn Read> = if let Some(path) = args.first() {
        Box::new(std::fs::File::open(path).map_err(|e| e.to_string())?)
    } else {
        Box::new(std::io::stdin())
    };
    let mut bytes = Vec::new();
    reader
        .take(INPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > INPUT_LIMIT {
        return Err("INPUT_JSON_LIMIT".into());
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let plan = recognize(&snapshot)?;
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    serde_json::to_writer_pretty(&mut out, &plan).map_err(|e| e.to_string())?;
    writeln!(out).map_err(|e| e.to_string())?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{}", serde_json::json!({"error":e}));
        std::process::exit(2);
    }
}
