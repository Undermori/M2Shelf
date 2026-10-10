use crate::model::*;
use crate::signals::*;
use std::collections::{BTreeMap, BTreeSet};

const MAX_ENTRIES: usize = 200_000;
const MAX_PAGES: usize = 10_000;

fn reference(root: &str, kind: &str, key: &str) -> String {
    // A collision-free tuple, NOT a hash, DB ID or persisted group identity.
    serde_json::to_string(&(root, kind, key)).expect("string tuple")
}
fn diag(out: &mut Vec<Diagnostic>, code: &str, path: &str, details: Vec<String>) {
    out.push(Diagnostic {
        code: code.into(),
        path: path.into(),
        details,
    });
}
fn name(entry: &Entry) -> &str {
    if entry.kind == EntryKind::File {
        stem(&entry.path)
    } else {
        basename(&entry.path)
    }
}
fn supported_file(entry: &Entry) -> bool {
    let suffix = entry
        .path
        .rsplit_once('.')
        .map(|(_, s)| s.to_ascii_lowercase());
    matches!(
        (suffix.as_deref(), entry.format),
        (Some("pdf"), Some(Format::Pdf))
            | (Some("epub"), Some(Format::Epub))
            | (Some("cbz"), Some(Format::Cbz))
            | (Some("txt"), Some(Format::Txt))
            | (Some("mobi"), Some(Format::Mobi))
            | (Some("azw3"), Some(Format::Azw3))
    )
}
fn ignored(path: &str) -> bool {
    path.split('/').any(|part| {
        ["__macosx", ".ds_store", "thumbs.db", "desktop.ini"]
            .contains(&part.to_ascii_lowercase().as_str())
    })
}
fn accessible(path: &str, entries: &BTreeMap<String, Entry>) -> bool {
    let mut cursor = Some(path);
    while let Some(p) = cursor {
        if entries
            .get(p)
            .is_some_and(|e| e.state != EntryState::Available || e.kind == EntryKind::Link)
            || ignored(p)
        {
            return false;
        }
        cursor = parent(p);
    }
    true
}
fn barred(path: &str, overrides: &BTreeMap<String, ManualOverride>) -> bool {
    let mut cursor = Some(path);
    while let Some(p) = cursor {
        if overrides.get(p).is_some_and(|o| o.no_merge) {
            return true;
        }
        cursor = parent(p);
    }
    false
}
fn member(
    index: usize,
    units: &[ReadingUnit],
    entries: &BTreeMap<String, Entry>,
    overrides: &BTreeMap<String, ManualOverride>,
    explicit_context: bool,
) -> Member {
    let unit = &units[index];
    let mut cursor = Some(unit.path.as_str());
    let mut volume = None;
    let mut chapter = None;
    let mut is_extra = false;
    let mut is_collection = false;
    let mut manual = false;
    let mut bare = None;
    while let Some(path) = cursor {
        let entry = &entries[path];
        let n = name(entry);
        let sig = signals(n);
        is_extra |= extra(n);
        is_collection |= collection(n);
        if let Some(o) = overrides.get(path) {
            if !manual && (o.volume.is_some() || o.chapter.is_some()) {
                volume = o.volume;
                chapter = o.chapter;
                manual = true;
            }
        }
        if !manual {
            volume = volume
                .or(entry.metadata.as_ref().and_then(|m| m.volume))
                .or(sig.volume);
            chapter = chapter
                .or(entry.metadata.as_ref().and_then(|m| m.chapter))
                .or(sig.chapter);
        }
        bare = bare.or(sig.bare_number);
        cursor = parent(path);
    }
    let mut decision = unit.decision;
    if volume.is_none() && chapter.is_none() && explicit_context && !is_extra && !is_collection {
        volume = bare;
        if bare.is_some() {
            decision = Decision::Review;
        }
    }
    let role = if is_extra {
        MembershipRole::Extra
    } else if is_collection {
        MembershipRole::Collection
    } else if chapter.is_some() {
        MembershipRole::Chapter
    } else if volume.is_some() {
        MembershipRole::Volume
    } else {
        MembershipRole::Main
    };
    if is_extra || is_collection {
        volume = None;
        chapter = None;
    }
    Member {
        source_ref: unit.source_ref.clone(),
        role,
        volume,
        chapter,
        decision,
        evidence: vec![evidence(
            if manual {
                "M01"
            } else if decision == Decision::Review {
                "N02"
            } else {
                "N01"
            },
            vec![
                unit.path.clone(),
                format!("role={role:?}; volume={volume:?}; chapter={chapter:?}"),
            ],
            if manual {
                "Explicit manual numbering takes priority over inferred numbering."
            } else {
                "Only explicit markers or trusted metadata establish numbering; bare digits need explicit work context and review. Extras and collections are not numbered main volumes."
            },
        )],
    }
}

/// Pure Phase 1 recognition. It cannot scan, decode, access a database, fetch metadata or mutate a source.
pub fn recognize(snapshot: &Snapshot) -> Result<Plan, String> {
    recognize_indexed(snapshot, &BTreeSet::new())
}

/// Production indexing already establishes image-book identity and reading order. This
/// evidence does not attest image bytes: the reader still validates each opened page.
pub fn recognize_indexed(snapshot: &Snapshot, indexed_image_directories: &BTreeSet<String>) -> Result<Plan, String> {
    if !matches!(
        snapshot.media_kind,
        LibraryKind::Comic | LibraryKind::Ebook | LibraryKind::Doujin | LibraryKind::Artbook
    ) {
        return Err("OUT_OF_SCOPE_VIDEO_LIBRARY".into());
    }
    if snapshot.root_id.is_empty() || snapshot.root_id.len() > 1024 {
        return Err("INVALID_ROOT_ID".into());
    }
    if snapshot.entries.len() > MAX_ENTRIES
        || snapshot.page_orders.len() > MAX_ENTRIES
        || snapshot.overrides.len() > MAX_ENTRIES
        || snapshot.prior_units.len() > MAX_ENTRIES
    {
        return Err("SNAPSHOT_LIMIT".into());
    }
    let mut diagnostics = Vec::new();
    let mut entries = BTreeMap::<String, Entry>::new();
    let mut collisions = BTreeSet::new();
    let mut keys = BTreeMap::<String, String>::new();
    for original in &snapshot.entries {
        let mut entry = original.clone();
        entry.path = normalize_path(&entry.path)?;
        if entry.path.is_empty() && entry.kind != EntryKind::Directory {
            return Err("ROOT_MUST_BE_DIRECTORY".into());
        }
        let key = path_key(&entry.path);
        if let Some(previous) = keys.get(&key) {
            if previous != &entry.path {
                collisions.insert(previous.clone());
                collisions.insert(entry.path.clone());
                let mut pair = [previous.clone(), entry.path.clone()];
                pair.sort();
                diag(
                    &mut diagnostics,
                    "PATH_CASE_COLLISION",
                    &pair[0],
                    vec![pair[1].clone()],
                );
            }
        } else {
            keys.insert(key, entry.path.clone());
        }
        if let Some(previous) = entries.get(&entry.path) {
            if previous != &entry {
                return Err(format!("CONFLICTING_ENTRY:{}", entry.path));
            } else {
                diag(&mut diagnostics, "DUPLICATE_ENTRY", &entry.path, vec![]);
            }
        } else {
            entries.insert(entry.path.clone(), entry);
        }
    }
    let paths: Vec<_> = entries.keys().cloned().collect();
    for path in paths {
        let mut cursor = parent(&path);
        while let Some(p) = cursor {
            if let Some(e) = entries.get(p) {
                if e.kind != EntryKind::Directory {
                    return Err(format!("NON_DIRECTORY_PARENT:{p}"));
                }
            } else {
                entries.insert(
                    p.into(),
                    Entry {
                        path: p.into(),
                        kind: EntryKind::Directory,
                        state: EntryState::Available,
                        format: None,
                        verified: false,
                        identity: None,
                        hint: None,
                        metadata: None,
                    },
                );
                if !p.is_empty() {
                    diag(&mut diagnostics, "INFERRED_PARENT", p, vec![path.clone()]);
                }
            }
            cursor = parent(p);
        }
    }
    entries.entry(String::new()).or_insert(Entry {
        path: String::new(),
        kind: EntryKind::Directory,
        state: EntryState::Available,
        format: None,
        verified: false,
        identity: None,
        hint: None,
        metadata: None,
    });
    if entries.len() > MAX_ENTRIES {
        return Err("EXPANDED_SNAPSHOT_LIMIT".into());
    }
    // Inferred ancestors must obey the same collision policy as explicitly supplied directories.
    let mut ancestor_keys = BTreeMap::<String, String>::new();
    for path in entries.keys() {
        let key = path_key(path);
        if let Some(previous) = ancestor_keys.get(&key) {
            if previous != path {
                collisions.insert(previous.clone());
                collisions.insert(path.clone());
                diag(
                    &mut diagnostics,
                    "PATH_CASE_COLLISION",
                    previous,
                    vec![path.clone()],
                );
            }
        } else {
            ancestor_keys.insert(key, path.clone());
        }
    }
    for path in &collisions {
        if let Some(e) = entries.get_mut(path) {
            e.state = EntryState::Excluded;
        }
    }
    let mut overrides = BTreeMap::<String, ManualOverride>::new();
    for original in &snapshot.overrides {
        let mut o = original.clone();
        o.path = normalize_path(&o.path)?;
        o.series_directory = o.series_directory.map(|p| normalize_path(&p)).transpose()?;
        if !entries.contains_key(&o.path) {
            diag(&mut diagnostics, "OVERRIDE_TARGET_MISSING", &o.path, vec![]);
            continue;
        }
        if o.path.is_empty() {
            diag(&mut diagnostics, "ROOT_OVERRIDE_REJECTED", "", vec![]);
            continue;
        }
        if o.role == Some(DirectoryRole::Root)
            || (o.role.is_some() && entries[&o.path].kind != EntryKind::Directory)
        {
            return Err("INVALID_OVERRIDE_ROLE".into());
        }
        if let Some(target) = &o.series_directory {
            if target.is_empty()
                || !entries
                    .get(target)
                    .is_some_and(|e| e.kind == EntryKind::Directory)
            {
                return Err("INVALID_MANUAL_SERIES_TARGET".into());
            }
        }
        if let Some(previous) = overrides.get(&o.path) {
            if previous != &o {
                diag(&mut diagnostics, "CONFLICTING_OVERRIDE", &o.path, vec![]);
                collisions.insert(o.path.clone());
            }
        } else {
            overrides.insert(o.path.clone(), o);
        }
    }
    for path in &collisions {
        overrides.insert(
            path.clone(),
            ManualOverride {
                path: path.clone(),
                role: (entries[path].kind == EntryKind::Directory)
                    .then_some(DirectoryRole::Ambiguous),
                series_directory: None,
                volume: None,
                chapter: None,
                no_merge: true,
            },
        );
    }
    let mut children = BTreeMap::<String, Vec<String>>::new();
    for path in entries.keys() {
        if let Some(p) = parent(path) {
            children.entry(p.into()).or_default().push(path.clone());
        }
    }
    let mut orders = BTreeMap::<String, PageOrder>::new();
    let mut order_conflicts = BTreeSet::new();
    let mut order_page_budget = 0usize;
    for original in &snapshot.page_orders {
        order_page_budget = order_page_budget
            .checked_add(original.pages.len())
            .ok_or("PAGE_ORDER_LIMIT")?;
        if order_page_budget > 2_000_000 || original.pages.len() > MAX_PAGES {
            return Err("PAGE_ORDER_LIMIT".into());
        }
        let mut order = original.clone();
        order.directory = normalize_path(&order.directory)?;
        order.pages = order
            .pages
            .iter()
            .map(|p| normalize_path(p))
            .collect::<Result<_, _>>()?;
        if let Some(previous) = orders.get(&order.directory) {
            if previous != &order {
                order_conflicts.insert(order.directory.clone());
            }
        } else {
            orders.insert(order.directory.clone(), order);
        }
    }
    for path in &order_conflicts {
        diag(&mut diagnostics, "CONFLICTING_PAGE_ORDER", path, vec![]);
        orders.remove(path);
    }
    let mut units = Vec::<ReadingUnit>::new();
    let mut directories = BTreeMap::<String, DirectoryJudgment>::new();
    for (path, entry) in &entries {
        if !accessible(path, &entries) {
            diag(
                &mut diagnostics,
                if ignored(path) {
                    "EXCLUDED_EXISTING_POLICY"
                } else {
                    "SOURCE_UNAVAILABLE"
                },
                path,
                vec![format!("{:?}", entry.state)],
            );
        }
        if entry.kind == EntryKind::File {
            if accessible(path, &entries) && entry.verified && supported_file(entry) {
                let sig = signals(name(entry));
                let conflict = entry.metadata.as_ref().is_some_and(|m| {
                    (m.volume.is_some() && sig.volume.is_some() && m.volume != sig.volume)
                        || (m.chapter.is_some()
                            && sig.chapter.is_some()
                            && m.chapter != sig.chapter)
                });
                if conflict {
                    diag(
                        &mut diagnostics,
                        "NUMBER_EVIDENCE_CONFLICT",
                        path,
                        vec![format!("name={sig:?}; metadata={:?}", entry.metadata)],
                    );
                }
                // The caller must already have validated this supported format. Plain ZIP/CBR cannot supply a supported format by suffix inference.
                units.push(ReadingUnit { source_ref:reference(&snapshot.root_id,"FILE",entry.identity.as_deref().map_or_else(||path_key(path),|id|format!("native:{id}")).as_str()), path:path.clone(), aliases:vec![], kind:UnitKind::FileBook, format:entry.format, pages:vec![], decision:if conflict{Decision::Review}else{Decision::Apply}, evidence:vec![evidence("B01",vec![path.clone(),format!("{:?}",entry.format)],"A caller-validated supported book is a reading unit independently of grouping.")] });
            } else if entry.format.is_none()
                || entry.format.is_some_and(|f| !f.image()) && !supported_file(entry)
                || (!entry.verified && entry.format.is_some_and(|f| !f.image()))
            {
                diag(
                    &mut diagnostics,
                    "UNSUPPORTED_OR_UNVERIFIED_FILE",
                    path,
                    vec![],
                );
            }
            continue;
        }
        if entry.kind != EntryKind::Directory {
            diag(&mut diagnostics, "LINK_NOT_FOLLOWED", path, vec![]);
            continue;
        }
        let manual = overrides.get(path).and_then(|o| o.role);
        let mut role = if path.is_empty() {
            DirectoryRole::Root
        } else if matches!(
            entry.hint,
            Some(Hint::Author | Hint::Artist | Hint::Publisher)
        ) {
            DirectoryRole::Category
        } else if extra(name(entry)) {
            DirectoryRole::Extras
        } else if intermediate(name(entry)) {
            DirectoryRole::IntermediateContainer
        } else {
            DirectoryRole::Ambiguous
        };
        let mut decision = Decision::KeepSeparate;
        let direct_images: Vec<_> = children
            .get(path)
            .into_iter()
            .flatten()
            .filter(|p| {
                entries[*p].kind == EntryKind::File && entries[*p].format.is_some_and(Format::image)
            })
            .cloned()
            .collect();
        let asset_scene = entry.hint == Some(Hint::Assets)
            || assets(name(entry))
            || direct_images.iter().any(|p| assets(basename(p)));
        let indexed_image = indexed_image_directories.contains(path)
            && entry.identity.is_some()
            && entry.state == EntryState::Available;
        let include_covers = indexed_image || orders
            .get(path)
            .is_some_and(|o| o.include_covers && o.basis == OrderBasis::TrustedBookMetadata);
        let pages: Vec<_> = direct_images
            .iter()
            .filter(|p| include_covers || !cover(basename(p)))
            .cloned()
            .collect();
        let page_set: BTreeSet<_> = pages.iter().collect();
        for p in &direct_images {
            if !page_set.contains(p) {
                diag(&mut diagnostics, "COVER_EXCLUDED", p, vec![path.clone()]);
            }
        }
        if !direct_images.is_empty() {
            let valid = pages.len() <= MAX_PAGES
                && pages
                    .iter()
                    .all(|p| (indexed_image || entries[p].verified) && accessible(p, &entries));
            let order = orders.get(path);
            let expected: BTreeSet<_> = pages.iter().collect();
            let order_valid = order.is_some_and(|o| {
                o.pages.len() == pages.len()
                    && o.pages.iter().collect::<BTreeSet<_>>() == expected
                    && o.pages.iter().all(|p| parent(p) == Some(path.as_str()))
            });
            let ordered = order
                .filter(|_| order_valid)
                .map(|o| o.pages.clone())
                .unwrap_or_default();
            let strong_context = matches!(entry.hint, Some(Hint::Work | Hint::ArtPages))
                || entry.metadata.is_some()
                || manual == Some(DirectoryRole::BookImageDirectory);
            let numbered = ordered_numbered_pages(&ordered);
            let allow = accessible(path, &entries)
                && valid
                && order_valid
                && !asset_scene
                && !pages.is_empty()
                && (indexed_image || pages.len() >= 2 || manual == Some(DirectoryRole::BookImageDirectory))
                && (indexed_image || numbered
                    || indexed_image || strong_context
                    || order.is_some_and(|o| o.basis == OrderBasis::TrustedBookMetadata))
                && (snapshot.media_kind != LibraryKind::Artbook
                    || strong_context
                    || signals(name(entry)).volume.is_some()
                    || order.is_some_and(|o| o.basis == OrderBasis::TrustedBookMetadata))
                && !matches!(
                    manual,
                    Some(
                        DirectoryRole::Category | DirectoryRole::Extras | DirectoryRole::Ambiguous
                    )
                );
            if allow {
                let source = entry
                    .identity
                    .as_ref()
                    .map_or_else(|| path_key(path), |id| format!("native:{id}"));
                units.push(ReadingUnit {source_ref:reference(&snapshot.root_id,"DIRECT_PAGES",&source),path:path.clone(),aliases:vec![],kind:UnitKind::DirectPages,format:None,pages:ordered,decision:Decision::Apply,evidence:vec![evidence(if indexed_image {"I02"} else {"I01"},vec![path.clone(),format!("page_count={}; order={:?}; numbered={numbered}",pages.len(),order.unwrap().basis)],if indexed_image {"Existing indexed image-book identity and page order; binary validation remains at the reader boundary."}else{"Only validated direct pages in an explicitly supplied existing reading order form this image book; descendant pages remain separate."})]});
                if !path.is_empty() {
                    role = DirectoryRole::BookImageDirectory;
                    decision = Decision::Apply;
                }
            } else {
                diag(
                    &mut diagnostics,
                    if asset_scene {
                        "IMAGE_ASSETS_ONLY"
                    } else if pages.len() == 1 {
                        "SINGLE_IMAGE_REVIEW"
                    } else if !order_valid {
                        "MISSING_OR_INVALID_PAGE_ORDER"
                    } else if !valid {
                        "INVALID_IMAGE_EVIDENCE"
                    } else {
                        "IMAGE_CONTEXT_REVIEW"
                    },
                    path,
                    vec![format!(
                        "direct_candidates={}; eligible_pages={}",
                        direct_images.len(),
                        pages.len()
                    )],
                );
                if !path.is_empty() {
                    decision = Decision::Review;
                }
            }
        }
        if let Some(m) = manual {
            role = m;
            decision = Decision::Apply;
        }
        directories.insert(path.clone(),DirectoryJudgment{path:path.clone(),role,decision,evidence:vec![evidence(if manual.is_some(){"M01"}else{"D01"},vec![path.clone(),format!("role={role:?}")],"Root is fixed; explicit category/extra/container signals are kept separate from reading units. Other directories stay conservative until local membership evidence is evaluated.")]});
    }
    units.sort_by(|a, b| a.path.cmp(&b.path));
    let mut unique = BTreeMap::<String, usize>::new();
    let mut retained = Vec::<ReadingUnit>::new();
    for unit in units {
        if let Some(&i) = unique.get(&unit.source_ref) {
            if retained[i].kind == unit.kind
                && retained[i].format == unit.format
                && (unit.kind != UnitKind::DirectPages || retained[i].pages == unit.pages)
            {
                diag(
                    &mut diagnostics,
                    "NATIVE_ID_ALIAS",
                    &unit.path,
                    vec![retained[i].path.clone()],
                );
                retained[i].aliases.push(unit.path);
            } else {
                return Err("CONFLICTING_NATIVE_SOURCE_IDENTITY".into());
            }
        } else {
            unique.insert(unit.source_ref.clone(), retained.len());
            retained.push(unit);
        }
    }
    let units = retained;
    let mut own = BTreeMap::<String, Vec<usize>>::new();
    for (i, u) in units.iter().enumerate() {
        let owner = if u.kind == UnitKind::DirectPages {
            u.path.as_str()
        } else {
            parent(&u.path).unwrap_or("")
        };
        own.entry(owner.into()).or_default().push(i);
    }
    let mut available = BTreeMap::<String, Vec<usize>>::new();
    let mut order: Vec<_> = directories.keys().cloned().collect();
    order.sort_by(|a, b| {
        b.split('/')
            .count()
            .cmp(&a.split('/').count())
            .then(a.cmp(b))
    });
    let mut groups = Vec::<Group>::new();
    let mut assigned = BTreeSet::<usize>::new();
    // Manual source-to-series assignments are evaluated before automatic local grouping.
    let mut manual_members = BTreeMap::<String, Vec<usize>>::new();
    for (i, u) in units.iter().enumerate() {
        let mut p = Some(u.path.as_str());
        while let Some(path) = p {
            if let Some(target) = overrides
                .get(path)
                .and_then(|o| o.series_directory.as_ref())
            {
                manual_members.entry(target.clone()).or_default().push(i);
                break;
            }
            p = parent(path);
        }
    }
    for (dir, indices) in &manual_members {
        if overrides.get(dir).is_some_and(|o| {
            matches!(
                o.role,
                Some(DirectoryRole::Category | DirectoryRole::Ambiguous)
            ) || o.no_merge
        }) {
            diag(
                &mut diagnostics,
                "MANUAL_MEMBERSHIP_CONFLICT",
                dir,
                indices.iter().map(|i| units[*i].path.clone()).collect(),
            );
            continue;
        }
        let members = indices
            .iter()
            .map(|i| member(*i, &units, &entries, &overrides, true))
            .collect();
        groups.push(Group{proposal_ref:reference(&snapshot.root_id,"SERIES",&path_key(dir)),title:basename(dir).into(),kind:GroupKind::Series,directory:Some(dir.clone()),members,decision:Decision::Apply,evidence:vec![evidence("M02",vec![dir.clone()],"Explicit manual memberships establish this proposed series; source references remain unchanged.")]});
        assigned.extend(indices);
        directories.get_mut(dir).unwrap().role = DirectoryRole::Series;
    }
    for dir in order {
        let entry = &entries[&dir];
        let manual = overrides.get(&dir);
        let role = directories[&dir].role;
        let mut indices = own.get(&dir).cloned().unwrap_or_default();
        for child in children.get(&dir).into_iter().flatten() {
            if entries[child].kind != EntryKind::Directory {
                continue;
            }
            let child_role = directories[child].role;
            let sig = signals(basename(child));
            let compatible_title =
                sig.title.is_empty() || title_key(&sig.title) == title_key(basename(&dir));
            let transparent = matches!(
                child_role,
                DirectoryRole::IntermediateContainer | DirectoryRole::Extras
            ) || ((sig.volume.is_some() || sig.chapter.is_some())
                && compatible_title);
            if transparent {
                indices.extend(available.get(child).into_iter().flatten().copied());
            }
        }
        indices.retain(|i| !assigned.contains(i) && !barred(&units[*i].path, &overrides));
        indices.sort_unstable();
        indices.dedup();
        let explicit = matches!(entry.hint, Some(Hint::Work | Hint::Series))
            || own
                .get(&dir)
                .is_some_and(|v| v.iter().any(|i| units[*i].kind == UnitKind::DirectPages))
            || entry.metadata.as_ref().is_some_and(|m| m.series.is_some())
            || manual.is_some_and(|o| o.role == Some(DirectoryRole::Series));
        let members: Vec<_> = indices
            .iter()
            .map(|i| member(*i, &units, &entries, &overrides, explicit))
            .collect();
        let numbers: BTreeSet<_> = members
            .iter()
            .filter(|m| matches!(m.role, MembershipRole::Volume | MembershipRole::Chapter))
            .map(|m| (m.volume, m.chapter))
            .collect();
        let compatible = indices.iter().zip(&members).all(|(i, m)| {
            let u = &units[*i];
            let sig = signals(name(&entries[&u.path]));
            matches!(m.role, MembershipRole::Extra | MembershipRole::Collection)
                || sig.title.is_empty()
                || sig.bare_number.is_some()
                || title_key(&sig.title) == title_key(basename(&dir))
                || entries[&u.path].metadata.as_ref().is_some_and(|m| {
                    m.series
                        .as_ref()
                        .is_some_and(|s| title_key(s) == title_key(basename(&dir)))
                })
                || u.kind == UnitKind::DirectPages
        });
        let can_group = !dir.is_empty()
            && ((signals(basename(&dir)).volume.is_none()
                && signals(basename(&dir)).chapter.is_none())
                || manual.is_some_and(|o| o.role == Some(DirectoryRole::Series)))
            && !barred(&dir, &overrides)
            && !matches!(
                role,
                DirectoryRole::Category
                    | DirectoryRole::Extras
                    | DirectoryRole::IntermediateContainer
            )
            && !manual_members.contains_key(&dir)
            && !manual.is_some_and(|o| {
                matches!(
                    o.role,
                    Some(
                        DirectoryRole::Category
                            | DirectoryRole::Ambiguous
                            | DirectoryRole::IntermediateContainer
                            | DirectoryRole::Extras
                            | DirectoryRole::BookImageDirectory
                    )
                )
            });
        let is_series = can_group
            && !members.is_empty()
            && ((numbers.len() >= 2 && compatible)
                || (explicit && manual.is_some_and(|o| o.role == Some(DirectoryRole::Series)))
                || (explicit
                    && numbers.len() == 1
                    && members.iter().any(|m| m.role == MembershipRole::Main)
                    && compatible));
        if is_series {
            let review =
                members.iter().any(|m| m.decision == Decision::Review) || numbers.len() < 2;
            let decision = if review {
                Decision::Review
            } else {
                Decision::Apply
            };
            let g=Group{proposal_ref:reference(&snapshot.root_id,"SERIES",&path_key(&dir)),title:basename(&dir).into(),kind:GroupKind::Series,directory:Some(dir.clone()),members,decision,evidence:vec![evidence("S01",vec![dir.clone(),format!("distinct_numbered_members={}",numbers.len())],"Compatible explicit volume/chapter evidence within this directory, traversing only organizational containers, proposes a series. Bare numbers and an unnumbered direct-page unit require review.")]};
            assigned.extend(&indices);
            groups.push(g);
            let judgment = directories.get_mut(&dir).unwrap();
            judgment.role = DirectoryRole::Series;
            judgment.decision = decision;
            judgment.evidence.push(evidence(
                "S01",
                vec![format!("members={}", indices.len())],
                "Local numbered members establish a series proposal.",
            ));
        } else if can_group && explicit && !members.is_empty() && compatible {
            groups.push(Group{proposal_ref:reference(&snapshot.root_id,"DIRECTORY_WORK",&path_key(&dir)),title:basename(&dir).into(),kind:GroupKind::Work,directory:Some(dir.clone()),members,decision:Decision::Apply,evidence:vec![evidence("W02",vec![dir.clone()],"Explicit work context associates readable sources while keeping non-reading assets separately navigable.")]});
            assigned.extend(&indices);
        } else if role == DirectoryRole::Ambiguous && manual.is_none() {
            let role = if indices.len() >= 2 && !explicit {
                DirectoryRole::Category
            } else {
                DirectoryRole::Ambiguous
            };
            directories.get_mut(&dir).unwrap().role = role;
            if role == DirectoryRole::Category {
                directories.get_mut(&dir).unwrap().evidence.push(evidence("C01",vec![format!("independent_sources={}",indices.len())],"Multiple independent books without compatible series evidence remain a category."));
            }
        }
        available.insert(
            dir,
            indices
                .into_iter()
                .filter(|i| !assigned.contains(i))
                .collect(),
        );
    }
    // Exact prefix grouping is sibling-scoped; there is no global fuzzy title matching.
    let mut prefix_groups = BTreeMap::<(String, String), Vec<usize>>::new();
    for (i, u) in units.iter().enumerate() {
        if assigned.contains(&i) || barred(&u.path, &overrides) {
            continue;
        }
        let e = &entries[&u.path];
        let sig = signals(name(e));
        let title = e
            .metadata
            .as_ref()
            .and_then(|m| m.series.clone())
            .unwrap_or(sig.title);
        let numbered = sig.volume.is_some()
            || sig.chapter.is_some()
            || e.metadata
                .as_ref()
                .is_some_and(|m| m.volume.is_some() || m.chapter.is_some());
        if numbered && !title.is_empty() && !extra(name(e)) && !collection(name(e)) {
            prefix_groups
                .entry((parent(&u.path).unwrap_or("").into(), title_key(&title)))
                .or_default()
                .push(i);
        }
    }
    for ((scope, title), indices) in prefix_groups {
        if overrides.get(&scope).is_some_and(|o| {
            matches!(
                o.role,
                Some(DirectoryRole::Category | DirectoryRole::Ambiguous)
            ) || o.no_merge
        }) {
            continue;
        }
        let members: Vec<_> = indices
            .iter()
            .map(|i| member(*i, &units, &entries, &overrides, false))
            .collect();
        let numbers: BTreeSet<_> = members.iter().map(|m| (m.volume, m.chapter)).collect();
        if numbers.len() < 2 {
            continue;
        }
        let first = &entries[&units[indices[0]].path];
        let display_title = first
            .metadata
            .as_ref()
            .and_then(|m| m.series.clone())
            .unwrap_or_else(|| signals(name(first)).title);
        let decision = if members.iter().any(|m| m.decision == Decision::Review) {
            Decision::Review
        } else {
            Decision::Apply
        };
        groups.push(Group{proposal_ref:reference(&snapshot.root_id,"VIRTUAL_SERIES",&serde_json::to_string(&(&scope,&title)).unwrap()),title:display_title,kind:GroupKind::Series,directory:None,members,decision,evidence:vec![evidence("S02",vec![scope,title],"Two distinct explicit numbers with the same exact title prefix or trusted series metadata under one physical parent propose a virtual series; no cross-directory matching.")]});
        assigned.extend(indices);
    }
    for (i, u) in units.iter().enumerate() {
        if assigned.contains(&i) {
            continue;
        }
        groups.push(Group{proposal_ref:reference(&snapshot.root_id,"WORK",&u.source_ref),title:entries[&u.path].metadata.as_ref().and_then(|m|m.title.clone()).unwrap_or_else(||if u.path.is_empty(){"Direct pages".into()}else{name(&entries[&u.path]).into()}),kind:GroupKind::Work,directory:None,members:vec![member(i,&units,&entries,&overrides,false)],decision:Decision::KeepSeparate,evidence:vec![evidence("W01",vec![u.path.clone()],"Keep this independently readable source as a work when series evidence is insufficient or grouping is blocked.")]});
    }
    groups.sort_by(|a, b| a.proposal_ref.cmp(&b.proposal_ref));
    let mut editions = Vec::new();
    for g in &groups {
        let mut by_number = BTreeMap::<(Option<u32>, Option<u32>), Vec<String>>::new();
        let mut volumes = BTreeSet::new();
        for m in &g.members {
            if m.volume.is_some() || m.chapter.is_some() {
                by_number
                    .entry((m.volume, m.chapter))
                    .or_default()
                    .push(m.source_ref.clone());
            }
            if let Some(v) = m.volume {
                volumes.insert(v);
            }
        }
        for sources in by_number.into_values().filter(|v| v.len() > 1) {
            editions.push(EditionProposal{sources,decision:Decision::Review,evidence:evidence("E01",vec![g.proposal_ref.clone()],"Same numbered position is only an edition/parallel-source proposal. Separate source identities, progress and bookmarks must remain.")});
        }
        if let (Some(&first), Some(&last)) = (volumes.first(), volumes.last()) {
            let missing: Vec<_> = (first.saturating_add(1)..last)
                .filter(|v| !volumes.contains(v))
                .take(64)
                .map(|v| v.to_string())
                .collect();
            if !missing.is_empty() {
                diag(
                    &mut diagnostics,
                    "VOLUME_GAP",
                    g.directory.as_deref().unwrap_or(&g.title),
                    missing,
                );
            }
        }
    }
    // Same-title parallel formats with only one explicit volume still get an association, never a forced series.
    let mut positions = BTreeMap::<(String, String, Option<u32>, Option<u32>), Vec<String>>::new();
    for u in &units {
        if u.kind != UnitKind::FileBook {
            continue;
        }
        let sig = signals(name(&entries[&u.path]));
        positions
            .entry((
                parent(&u.path).unwrap_or("").into(),
                title_key(&sig.title),
                sig.volume,
                sig.chapter,
            ))
            .or_default()
            .push(u.source_ref.clone());
    }
    let existing: BTreeSet<_> = editions.iter().map(|e| e.sources.clone()).collect();
    for sources in positions.into_values().filter(|v| v.len() > 1) {
        if !existing.contains(&sources) {
            editions.push(EditionProposal{sources,decision:Decision::Review,evidence:evidence("E02",vec![],"Exact sibling title/position can suggest parallel editions, but it does not merge sources or establish a series.")});
        }
    }
    editions.sort_by(|a, b| a.sources.cmp(&b.sources));
    let mut root_items = Vec::new();
    let groups_by_directory: BTreeMap<_, _> = groups
        .iter()
        .filter_map(|g| g.directory.as_ref().map(|d| (d, g)))
        .collect();
    let single_by_path: BTreeMap<_, _> = groups
        .iter()
        .filter(|g| g.members.len() == 1)
        .map(|g| (&units[unique[&g.members[0].source_ref]].path, g))
        .collect();
    for child in children
        .get("")
        .into_iter()
        .flatten()
        .filter(|p| entries[*p].kind != EntryKind::File)
    {
        if ignored(child) {
            continue;
        }
        let g = groups_by_directory
            .get(child)
            .copied()
            .or_else(|| single_by_path.get(child).copied());
        root_items.push(DisplayItem {
            target_ref: g.map_or_else(
                || reference(&snapshot.root_id, "PHYSICAL", &path_key(child)),
                |g| g.proposal_ref.clone(),
            ),
            path: child.clone(),
            title: basename(child).into(),
            kind: g.map_or_else(
                || {
                    if entries[child].kind == EntryKind::Link {
                        "LINK".into()
                    } else {
                        format!("{:?}", directories[child].role).to_ascii_uppercase()
                    }
                },
                |g| format!("{:?}", g.kind).to_ascii_uppercase(),
            ),
            decision: g.map_or(Decision::KeepSeparate, |g| g.decision),
        });
    }
    for g in &groups {
        let direct_members: Vec<_> = g
            .members
            .iter()
            .filter_map(|m| unique.get(&m.source_ref).map(|i| &units[*i]))
            .filter(|u| {
                (u.kind == UnitKind::FileBook && parent(&u.path) == Some("")) || u.path.is_empty()
            })
            .collect();
        if g.directory.is_none() && !direct_members.is_empty() {
            root_items.push(DisplayItem {
                target_ref: g.proposal_ref.clone(),
                path: direct_members[0].path.clone(),
                title: g.title.clone(),
                kind: format!("{:?}", g.kind).to_ascii_uppercase(),
                decision: g.decision,
            });
        }
    }
    root_items.sort_by(|a, b| a.path.cmp(&b.path).then(a.target_ref.cmp(&b.target_ref)));
    let mut details = Vec::new();
    for g in &groups {
        let mut transparent = BTreeSet::new();
        let mut remaining = BTreeSet::new();
        if let Some(dir) = &g.directory {
            for m in &g.members {
                let unit = &units[unique[&m.source_ref]];
                let mut p = parent(&unit.path);
                while let Some(path) = p {
                    if path == dir || !below(path, dir) {
                        break;
                    }
                    transparent.insert(path.into());
                    p = parent(path);
                }
                if unit.kind == UnitKind::DirectPages && unit.path != *dir {
                    transparent.insert(unit.path.clone());
                }
            }
            for child in children
                .get(dir)
                .into_iter()
                .flatten()
                .filter(|p| entries[*p].kind == EntryKind::Directory)
            {
                if !transparent.contains(child) {
                    remaining.insert(child.clone());
                }
            }
        }
        details.push(DetailProposal {
            group_ref: g.proposal_ref.clone(),
            reading_sources: g.members.iter().map(|m| m.source_ref.clone()).collect(),
            transparent_directories: transparent.into_iter().collect(),
            remaining_directories: remaining.into_iter().collect(),
        });
    }
    let mut retained_prior = BTreeMap::<String, PriorUnit>::new();
    for prior in &snapshot.prior_units {
        let path = normalize_path(&prior.path)?;
        if !unique.contains_key(&prior.source_ref) {
            diag(
                &mut diagnostics,
                if snapshot.complete && accessible(&path, &entries) {
                    "PRIOR_NOT_OBSERVED_NO_DELETE"
                } else {
                    "PRIOR_RETAINED_INCOMPLETE"
                },
                &path,
                vec![prior.source_ref.clone()],
            );
            retained_prior.insert(
                prior.source_ref.clone(),
                PriorUnit {
                    source_ref: prior.source_ref.clone(),
                    path,
                },
            );
        }
    }
    diagnostics.sort();
    diagnostics.dedup();
    let physical_tree = entries
        .values()
        .map(|e| PhysicalEntry {
            path: e.path.clone(),
            parent: parent(&e.path).map(str::to_owned),
            kind: e.kind,
            state: e.state,
        })
        .collect();
    Ok(Plan {
        schema_version: 1,
        rules_version: "smart-mixed-lab-1".into(),
        mode: "SMART_MIXED".into(),
        root_id: snapshot.root_id.clone(),
        physical_tree,
        directories: directories.into_values().collect(),
        reading_units: units,
        groups,
        editions,
        root_items,
        details,
        retained_prior: retained_prior.into_values().collect(),
        diagnostics,
        retention_policy: "PROPOSAL_ONLY_NO_DELETIONS_OR_PERSISTENT_MUTATIONS".into(),
    })
}
