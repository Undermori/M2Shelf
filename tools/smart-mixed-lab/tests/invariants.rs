use m2shelf_smart_mixed_lab::{model::*, recognize, signals};
use std::collections::{BTreeMap, BTreeSet};

fn fixture(id: u32) -> Snapshot {
    let path = format!(
        "{}/../../docs/smart-mixed/fixtures/{id:02}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    serde_json::from_value(value["input"].clone()).unwrap()
}
fn shuffle<T>(items: &mut [T], seed: &mut u64) {
    for i in (1..items.len()).rev() {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        items.swap(i, (*seed as usize) % (i + 1));
    }
}
#[test]
fn property_unique_membership_and_coverage_all_fixtures() {
    for id in 1..=49 {
        let input = fixture(id);
        let plan = recognize(&input).unwrap();
        let refs: BTreeSet<_> = plan
            .reading_units
            .iter()
            .map(|u| u.source_ref.clone())
            .collect();
        assert_eq!(refs.len(), plan.reading_units.len());
        let members: Vec<_> = plan
            .groups
            .iter()
            .flat_map(|g| g.members.iter().map(|m| m.source_ref.clone()))
            .collect();
        assert_eq!(members.len(), refs.len());
        assert_eq!(members.into_iter().collect::<BTreeSet<_>>(), refs);
        for u in &plan.reading_units {
            assert!(plan.physical_tree.iter().any(|e| e.path == u.path));
        }
        for e in input.entries {
            assert!(plan.physical_tree.iter().any(|p| p.path == e.path));
        }
    }
}
#[test]
fn property_determinism_12_permutations_every_fixture() {
    let mut seed = 0x20261009;
    for id in 1..=49 {
        let mut input = fixture(id);
        let baseline = recognize(&input).unwrap();
        for _ in 0..12 {
            shuffle(&mut input.entries, &mut seed);
            shuffle(&mut input.page_orders, &mut seed);
            shuffle(&mut input.overrides, &mut seed);
            assert_eq!(recognize(&input).unwrap(), baseline, "fixture {id}");
        }
    }
}
#[test]
fn property_repeated_serialization_identical() {
    let input = fixture(49);
    let before = serde_json::to_vec(&input).unwrap();
    let a = serde_json::to_vec(&recognize(&input).unwrap()).unwrap();
    let b = serde_json::to_vec(&recognize(&input).unwrap()).unwrap();
    assert_eq!(a, b);
    assert_eq!(
        serde_json::to_vec(&input).unwrap(),
        before,
        "input is immutable"
    );
}
#[test]
fn property_physical_and_logical_graphs_acyclic() {
    for id in 1..=49 {
        let p = recognize(&fixture(id)).unwrap();
        let tree: BTreeMap<_, _> = p
            .physical_tree
            .iter()
            .map(|e| (&e.path, &e.parent))
            .collect();
        for e in &p.physical_tree {
            let mut seen = BTreeSet::new();
            let mut next = Some(&e.path);
            while let Some(path) = next {
                assert!(seen.insert(path));
                next = tree[path].as_ref();
            }
        }
        let groups: BTreeSet<_> = p.groups.iter().map(|g| &g.proposal_ref).collect();
        let sources: BTreeSet<_> = p.reading_units.iter().map(|u| &u.source_ref).collect();
        assert!(groups.is_disjoint(&sources));
        for g in &p.groups {
            for m in &g.members {
                assert!(sources.contains(&m.source_ref));
                assert!(!groups.contains(&m.source_ref));
            }
        }
    }
}
#[test]
fn property_root_scope_isolation() {
    let mut input = fixture(21);
    let a = recognize(&input).unwrap();
    input.root_id = "other-root".into();
    let b = recognize(&input).unwrap();
    let refs: BTreeSet<_> = a.reading_units.iter().map(|u| &u.source_ref).collect();
    assert!(b
        .reading_units
        .iter()
        .all(|u| !refs.contains(&u.source_ref)));
    assert_eq!(a.groups.len(), b.groups.len());
}
#[test]
fn property_all_video_kinds_rejected() {
    for kind in [
        LibraryKind::Animation,
        LibraryKind::LiveAction,
        LibraryKind::Video,
    ] {
        let mut s = fixture(4);
        s.media_kind = kind;
        assert_eq!(recognize(&s).unwrap_err(), "OUT_OF_SCOPE_VIDEO_LIBRARY");
    }
}
#[test]
fn property_offline_metadata_free_complete_result() {
    let input = fixture(49);
    assert!(input.entries.iter().all(|e| e.metadata.is_none()));
    let p = recognize(&input).unwrap();
    assert_eq!(p.reading_units.len(), 7);
    assert_eq!(p.groups.len(), 1);
}
#[test]
fn property_override_change_is_local_and_refs_stable() {
    let mut input = fixture(4);
    input.entries.push(Entry {
        path: "untouched.pdf".into(),
        kind: EntryKind::File,
        state: EntryState::Available,
        format: Some(Format::Pdf),
        verified: true,
        identity: None,
        hint: None,
        metadata: None,
    });
    let auto = recognize(&input).unwrap();
    input.overrides.push(ManualOverride {
        path: "东京食尸鬼".into(),
        role: Some(DirectoryRole::Category),
        series_directory: None,
        volume: None,
        chapter: None,
        no_merge: false,
    });
    let manual = recognize(&input).unwrap();
    assert_eq!(auto.reading_units, manual.reading_units);
    let independent = auto.groups.iter().find(|g| g.title == "untouched").unwrap();
    assert!(manual.groups.contains(independent));
    assert!(!manual.groups.iter().any(|g| g.kind == GroupKind::Series));
    input.overrides.clear();
    assert_eq!(recognize(&input).unwrap(), auto);
}
#[test]
fn property_partial_and_complete_absence_never_delete() {
    let mut input = fixture(38);
    for complete in [true, false] {
        input.complete = complete;
        let p = recognize(&input).unwrap();
        assert_eq!(p.retained_prior[0].source_ref, "prior-stable-id");
        assert_eq!(
            p.retention_policy,
            "PROPOSAL_ONLY_NO_DELETIONS_OR_PERSISTENT_MUTATIONS"
        );
    }
    input.entries.clear();
    input.complete = true;
    let p = recognize(&input).unwrap();
    assert_eq!(p.retained_prior.len(), 1);
    assert!(p
        .diagnostics
        .iter()
        .any(|d| d.code == "PRIOR_NOT_OBSERVED_NO_DELETE"));
}
#[test]
fn property_source_reference_independent_of_group_metadata() {
    let mut input = fixture(4);
    let a = recognize(&input).unwrap();
    input.entries[1].metadata = Some(Metadata {
        title: Some("手工标题".into()),
        ..Metadata::default()
    });
    let b = recognize(&input).unwrap();
    assert_eq!(
        a.reading_units
            .iter()
            .map(|u| &u.source_ref)
            .collect::<Vec<_>>(),
        b.reading_units
            .iter()
            .map(|u| &u.source_ref)
            .collect::<Vec<_>>()
    );
}
#[test]
fn property_direct_page_sets_disjoint() {
    let p = recognize(&fixture(43)).unwrap();
    let pages: Vec<_> = p
        .reading_units
        .iter()
        .flat_map(|u| u.pages.iter())
        .collect();
    assert_eq!(pages.len(), pages.iter().collect::<BTreeSet<_>>().len());
}
#[test]
fn page_order_foreign_child_rejected() {
    let mut s = fixture(43);
    let foreign = s.page_orders[1].pages[0].clone();
    s.page_orders[0].pages.push(foreign);
    let p = recognize(&s).unwrap();
    assert_eq!(p.reading_units.len(), 1);
    assert!(p
        .diagnostics
        .iter()
        .any(|d| d.code == "MISSING_OR_INVALID_PAGE_ORDER"));
}
#[test]
fn missing_page_order_never_fabricated() {
    let mut s = fixture(31);
    s.page_orders.clear();
    assert!(recognize(&s).unwrap().reading_units.is_empty());
}
#[test]
fn duplicate_page_order_entry_rejected() {
    let mut s = fixture(31);
    s.page_orders[0].pages[1] = s.page_orders[0].pages[0].clone();
    assert!(recognize(&s).unwrap().reading_units.is_empty());
}
#[test]
fn reversed_number_order_is_not_trusted_as_natural() {
    let mut s = fixture(31);
    s.page_orders[0].pages.reverse();
    let p = recognize(&s).unwrap();
    assert!(p.reading_units.is_empty());
    assert!(p
        .diagnostics
        .iter()
        .any(|d| d.code == "IMAGE_CONTEXT_REVIEW"));
}
#[test]
fn validated_signature_can_be_jpeg_with_png_suffix() {
    let mut s = fixture(31);
    s.entries[1].format = Some(Format::Jpeg);
    assert_eq!(recognize(&s).unwrap().reading_units.len(), 1);
}
#[test]
fn unreadable_page_does_not_form_reading_unit() {
    let mut s = fixture(31);
    s.entries[1].state = EntryState::Unreadable;
    assert!(recognize(&s).unwrap().reading_units.is_empty());
}
#[test]
fn explicit_image_override_accepts_single_valid_page() {
    let mut s = fixture(30);
    s.overrides.push(ManualOverride {
        path: "单图".into(),
        role: Some(DirectoryRole::BookImageDirectory),
        series_directory: None,
        volume: None,
        chapter: None,
        no_merge: false,
    });
    assert_eq!(recognize(&s).unwrap().reading_units.len(), 1);
    s.page_orders.clear();
    assert!(recognize(&s).unwrap().reading_units.is_empty());
}
#[test]
fn explicit_trusted_cover_inclusion_only() {
    let mut s = fixture(25);
    s.page_orders[0].include_covers = true;
    s.page_orders[0].basis = OrderBasis::TrustedBookMetadata;
    s.page_orders[0].pages.insert(0, "图片书/cover.jpg".into());
    let p = recognize(&s).unwrap();
    assert_eq!(p.reading_units[0].pages.len(), 3);
    s.page_orders[0].basis = OrderBasis::ExistingIndexedNaturalOrder;
    assert!(recognize(&s).unwrap().reading_units.is_empty());
}
#[test]
fn multilingual_case_insensitive_cover_helpers() {
    for n in [
        "COVER.JPG",
        "Folder.png",
        "thumb02.jpg",
        "Poster.jpg",
        "封面.jpg",
        "カバー.png",
        "표지.png",
    ] {
        assert!(signals::cover(n), "{n}");
    }
}
#[test]
fn multilingual_volume_and_chapter_markers() {
    for (n, v, c) in [
        ("Vol.01", Some(1), None),
        ("Volume 2", Some(2), None),
        ("第十二卷", Some(12), None),
        ("2巻", Some(2), None),
        ("Ch.003", None, Some(3)),
        ("第03话", None, Some(3)),
        ("第3話", None, Some(3)),
        ("第十章", None, Some(10)),
    ] {
        let s = signals::signals(n);
        assert_eq!((s.volume, s.chapter), (v, c), "{n}");
    }
    for n in ["20世纪少年", "86", "3月的狮子", "第04-06卷"] {
        let s = signals::signals(n);
        assert_eq!((s.volume, s.chapter), (None, None));
    }
}
#[test]
fn invalid_paths_and_non_directory_parent_rejected() {
    for path in [
        "../x.pdf",
        "C:/x.pdf",
        "/x.pdf",
        "a//x.pdf",
        "a/./x.pdf",
        "a./x.pdf",
        "a /x.pdf",
        "\\\\host\\share\\x.pdf",
    ] {
        let mut s = fixture(1);
        s.entries[0].path = path.into();
        assert!(recognize(&s).is_err(), "{path}");
    }
    let mut s = fixture(1);
    let mut child = s.entries[0].clone();
    child.path = "独立.pdf/child.pdf".into();
    s.entries.push(child);
    assert!(recognize(&s).is_err());
}
#[test]
fn links_and_excluded_ancestors_not_followed() {
    let mut s = fixture(4);
    s.entries[0].kind = EntryKind::Link;
    assert!(recognize(&s).is_err());
    s.entries[0].kind = EntryKind::Directory;
    s.entries[0].state = EntryState::Excluded;
    assert!(recognize(&s).unwrap().reading_units.is_empty());
}
#[test]
fn native_identity_alias_dedup_scoped() {
    let mut s = fixture(21);
    for e in s.entries.iter_mut().filter(|e| e.kind == EntryKind::File) {
        e.identity = Some("trusted-open-handle-id".into());
    }
    let p = recognize(&s).unwrap();
    assert_eq!(p.reading_units.len(), 1);
    assert_eq!(p.reading_units[0].aliases.len(), 1);
    assert_eq!(p.physical_tree.len(), 5);
}
#[test]
fn native_identity_format_conflict_fails_closed() {
    let mut s = fixture(20);
    for e in s.entries.iter_mut().filter(|e| e.kind == EntryKind::File) {
        e.identity = Some("same".into());
    }
    assert_eq!(
        recognize(&s).unwrap_err(),
        "CONFLICTING_NATIVE_SOURCE_IDENTITY"
    );
}
#[test]
fn explicit_no_merge_preserves_units() {
    let mut s = fixture(4);
    s.overrides.push(ManualOverride {
        path: "东京食尸鬼".into(),
        role: None,
        series_directory: None,
        volume: None,
        chapter: None,
        no_merge: true,
    });
    let p = recognize(&s).unwrap();
    assert_eq!(p.reading_units.len(), 2);
    assert!(!p.groups.iter().any(|g| g.kind == GroupKind::Series));
}
#[test]
fn explicit_directory_series_without_manual_members() {
    let mut s = fixture(10);
    s.entries[0].hint = None;
    s.overrides.push(ManualOverride {
        path: "藤本树".into(),
        role: Some(DirectoryRole::Series),
        series_directory: None,
        volume: None,
        chapter: None,
        no_merge: false,
    });
    let p = recognize(&s).unwrap();
    assert_eq!(p.groups.len(), 1);
    assert_eq!(p.groups[0].kind, GroupKind::Series);
    assert_eq!(p.groups[0].members.len(), 2);
}
#[test]
fn conflicting_overrides_quarantine_and_determinism() {
    let mut s = fixture(39);
    let mut conflicting = s.overrides[0].clone();
    conflicting.role = Some(DirectoryRole::Series);
    s.overrides.push(conflicting);
    let a = recognize(&s).unwrap();
    s.overrides.reverse();
    assert_eq!(recognize(&s).unwrap(), a);
    assert!(!a.groups.iter().any(|g| g.kind == GroupKind::Series));
    assert!(a
        .diagnostics
        .iter()
        .any(|d| d.code == "CONFLICTING_OVERRIDE"));
}
#[test]
fn trusted_metadata_groups_only_local_siblings() {
    let mut s = fixture(17);
    for (i, e) in s.entries.iter_mut().enumerate() {
        e.metadata = Some(Metadata {
            series: Some("可信系列".into()),
            volume: Some((i + 1) as u32),
            ..Metadata::default()
        });
    }
    let p = recognize(&s).unwrap();
    assert_eq!(p.groups.len(), 1);
    assert_eq!(p.groups[0].title, "可信系列");
    assert_eq!(p.groups[0].kind, GroupKind::Series);
}
#[test]
fn artbook_numbered_images_need_semantic_evidence() {
    let mut s = fixture(31);
    s.media_kind = LibraryKind::Artbook;
    assert!(recognize(&s).unwrap().reading_units.is_empty());
    s.entries[0].hint = Some(Hint::ArtPages);
    assert_eq!(recognize(&s).unwrap().reading_units.len(), 1);
}
#[test]
fn suffix_cannot_promote_zip_or_unverified_file() {
    let mut s = fixture(33);
    s.entries[0].format = Some(Format::Cbz);
    s.entries[0].verified = true;
    let p = recognize(&s).unwrap();
    assert_eq!(p.reading_units.len(), 1);
    assert_eq!(p.reading_units[0].path, "comic.cbz");
}
#[test]
fn core_members_not_duplicated_as_remaining_directories() {
    let p = recognize(&fixture(49)).unwrap();
    let d = &p.details[0];
    assert_eq!(d.reading_sources.len(), 7);
    assert!(d.remaining_directories.is_empty());
    for path in ["混合系列/第03卷", "混合系列/第04-06卷", "混合系列/特典"] {
        assert!(d.transparent_directories.contains(&path.into()));
    }
}
#[test]
fn path_case_and_separator_changes_keep_source_reference() {
    let s = fixture(1);
    let a = recognize(&s).unwrap();
    let mut s2 = s.clone();
    s2.entries[0].path = "独立.PDF".into();
    assert_eq!(
        recognize(&s2).unwrap().reading_units[0].source_ref,
        a.reading_units[0].source_ref
    );
    let mut s3 = fixture(4);
    let a = recognize(&s3).unwrap();
    for e in &mut s3.entries {
        e.path = e.path.replace('/', "\\");
    }
    assert_eq!(recognize(&s3).unwrap(), a);
}
#[test]
fn metadata_number_conflict_requires_review() {
    let mut s = fixture(4);
    s.entries[1].metadata = Some(Metadata {
        volume: Some(9),
        ..Metadata::default()
    });
    let p = recognize(&s).unwrap();
    assert_eq!(p.reading_units.len(), 2);
    assert!(p
        .diagnostics
        .iter()
        .any(|d| d.code == "NUMBER_EVIDENCE_CONFLICT"));
    assert_eq!(p.groups[0].decision, Decision::Review);
}
#[test]
fn inferred_parent_case_collision_blocks_descendants() {
    let mut s = fixture(17);
    s.entries[0].path = "A/01.pdf".into();
    s.entries[1].path = "a/02.pdf".into();
    let p = recognize(&s).unwrap();
    assert!(p.reading_units.is_empty());
    assert_eq!(p.physical_tree.len(), 5);
    assert!(p
        .diagnostics
        .iter()
        .any(|d| d.code == "PATH_CASE_COLLISION"));
}
