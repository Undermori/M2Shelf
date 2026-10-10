use m2shelf_smart_mixed_lab::{model::*, recognize};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Fixture {
    id: u32,
    title: String,
    input: Snapshot,
    expected: Expected,
    negative_assertion: String,
}
#[derive(Deserialize)]
struct Expected {
    units: Vec<ExpectedUnit>,
    root_paths: Vec<String>,
    series: Vec<ExpectedSeries>,
    directory_roles: BTreeMap<String, DirectoryRole>,
    diagnostics: Vec<String>,
    editions: usize,
    retained_prior: usize,
}
#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ExpectedUnit {
    path: String,
    kind: UnitKind,
    pages: Vec<String>,
}
#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ExpectedMember {
    path: String,
    volume: Option<u32>,
    chapter: Option<u32>,
    role: MembershipRole,
    decision: Decision,
}
#[derive(Deserialize)]
struct ExpectedSeries {
    directory: Option<String>,
    title: String,
    members: Vec<ExpectedMember>,
    decision: Decision,
}

fn check(id: u32) {
    let path = format!(
        "{}/../../docs/smart-mixed/fixtures/{id:02}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let case: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(id, case.id);
    let plan = recognize(&case.input).unwrap();
    let exp = case.expected;
    let mut actual: Vec<_> = plan
        .reading_units
        .iter()
        .map(|u| ExpectedUnit {
            path: u.path.clone(),
            kind: u.kind,
            pages: u.pages.clone(),
        })
        .collect();
    actual.sort();
    let mut expected = exp.units;
    expected.sort();
    assert_eq!(
        actual, expected,
        "{}; {}",
        case.title, case.negative_assertion
    );
    let mut roots: Vec<_> = plan.root_items.iter().map(|r| r.path.clone()).collect();
    roots.sort();
    let mut expected_roots = exp.root_paths;
    expected_roots.sort();
    assert_eq!(roots, expected_roots, "root presentation {}", case.title);
    let by_ref: BTreeMap<_, _> = plan
        .reading_units
        .iter()
        .map(|u| (&u.source_ref, &u.path))
        .collect();
    let actual_series: Vec<_> = plan
        .groups
        .iter()
        .filter(|g| g.kind == GroupKind::Series)
        .collect();
    assert_eq!(
        actual_series.len(),
        exp.series.len(),
        "unexpected/absent Series {}",
        case.title
    );
    for expected in exp.series {
        let group = actual_series
            .iter()
            .find(|g| g.directory == expected.directory && g.title == expected.title)
            .unwrap_or_else(|| panic!("missing series {}: {:?}", expected.title, actual_series));
        assert_eq!(group.decision, expected.decision);
        let mut members: Vec<_> = group
            .members
            .iter()
            .map(|m| ExpectedMember {
                path: by_ref[&m.source_ref].clone(),
                volume: m.volume,
                chapter: m.chapter,
                role: m.role,
                decision: m.decision,
            })
            .collect();
        members.sort();
        let mut expected_members = expected.members;
        expected_members.sort();
        assert_eq!(members, expected_members, "series members {}", case.title);
    }
    for (path, role) in exp.directory_roles {
        assert_eq!(
            plan.directories
                .iter()
                .find(|d| d.path == path)
                .unwrap()
                .role,
            role
        );
    }
    for code in exp.diagnostics {
        assert!(
            plan.diagnostics.iter().any(|d| d.code == code),
            "missing diagnostic {code} in {}: {:?}",
            case.title,
            plan.diagnostics
        );
    }
    assert_eq!(
        plan.editions.len(),
        exp.editions,
        "edition proposals {}",
        case.title
    );
    assert_eq!(plan.retained_prior.len(), exp.retained_prior);
    assert_eq!(
        plan.directories
            .iter()
            .find(|d| d.path.is_empty())
            .unwrap()
            .role,
        DirectoryRole::Root
    );
    let refs: BTreeSet<_> = plan.reading_units.iter().map(|u| &u.source_ref).collect();
    assert_eq!(refs.len(), plan.reading_units.len());
    let memberships: Vec<_> = plan
        .groups
        .iter()
        .flat_map(|g| g.members.iter().map(|m| &m.source_ref))
        .collect();
    assert_eq!(
        memberships.len(),
        refs.len(),
        "every reading source belongs once"
    );
    assert_eq!(memberships.into_iter().collect::<BTreeSet<_>>(), refs);
    for input in case.input.entries {
        assert!(
            plan.physical_tree
                .iter()
                .any(|e| e.path == input.path.replace('\\', "/")),
            "physical entry lost {}",
            input.path
        );
    }
    for u in &plan.reading_units {
        assert!(plan.physical_tree.iter().any(|e| e.path == u.path));
        assert!(!u.evidence.is_empty());
    }
    assert!(!plan.retention_policy.contains("DELETE_ALLOWED"));
}
macro_rules! cases {($($name:ident:$id:literal),*$(,)?)=>{$(#[test]fn $name(){check($id);})*};}
cases! {
case_01_single_pdf:1,case_02_300_independent:2,case_03_two_series_100_files:3,case_04_explicit_volumes:4,
case_05_pdf_cbz:5,case_06_pdf_epub_txt:6,case_07_image_volume:7,case_08_range_container:8,
case_09_volume_chapter:9,case_10_author:10,case_11_doujin_artist:11,case_12_artbook_assets:12,
case_13_title_20:13,case_14_title_86:14,case_15_virtual_series:15,case_16_one_volume:16,
case_17_root_numbers:17,case_18_context_numbers:18,case_19_volume_gap:19,case_20_editions:20,
case_21_same_title_distinct_parents:21,case_22_collection:22,case_23_extra:23,case_24_reference:24,
case_25_cover:25,case_26_assets:26,case_27_image_attachment:27,case_28_nested_images:28,
case_29_images_and_file:29,case_30_single_image:30,case_31_natural_order:31,case_32_empty:32,
case_33_zip_cbz:33,case_34_cbr_rar:34,case_35_case_unicode:35,case_36_shuffled_mobi_azw3:36,
case_37_repeat:37,case_38_offline_retention:38,case_39_manual_category:39,case_40_manual_members:40,
case_41_remove_override:41,case_42_format_containers:42,case_43_no_fabricated_volume:43,case_44_sections:44,
case_45_ebook_author:45,case_46_corrupt_unknown:46,case_47_existing_exclusions:47,case_48_root_file_and_directory:48,
case_49_combined_mixed_example:49,
}
