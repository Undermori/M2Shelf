//! Read-only index adapter and standalone Shadow runner, reused by Phase 3 production.
pub mod adapter;
#[cfg(feature = "fixtures")]
pub mod fixture;
pub mod model;

use adapter::{ReadOptions, Result};
use m2shelf_smart_mixed_lab::{model::*, recognize};
use model::*;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::Ordering,
    time::Instant,
};

#[derive(Debug, Serialize)]
pub struct Timings {
    pub adapter_ms: f64,
    pub recognize_ms: f64,
    pub diff_ms: f64,
    pub total_ms: f64,
}
fn cancelled(o: &ReadOptions) -> Result<()> {
    if o.cancelled.load(Ordering::Relaxed) {
        Err("CANCELLED_NO_REPORT".into())
    } else {
        Ok(())
    }
}
/// Full snapshot returned only for local harness callers; CLI emits deidentified summary.
pub fn run(
    index: &mut adapter::ReadIndex,
    root: i64,
    options: &ReadOptions,
) -> Result<(IndexSnapshot, ShadowReport, Timings)> {
    let start = Instant::now();
    let snapshot = index.read(root, options)?;
    let a = start.elapsed().as_secs_f64() * 1000.;
    cancelled(options)?;
    let step = Instant::now();
    let mut plan =
        recognize(&snapshot.recognition_input).map_err(|_| "RECOGNITION_REJECTED_NO_REPORT")?;
    let r = step.elapsed().as_secs_f64() * 1000.;
    cancelled(options)?;
    let step = Instant::now();
    let mut uncertain_ancestors = BTreeSet::new();
    for source in snapshot
        .source_id_map
        .values()
        .filter(|s| s.status != "INDEXED_FORMAT_EVIDENCE")
    {
        let mut next = Some(source.path.as_str());
        while let Some(path) = next {
            uncertain_ancestors.insert(path.to_string());
            next = path.rsplit_once('/').map(|(p, _)| p);
        }
    }
    for group in &mut plan.groups {
        if group.kind == GroupKind::Series
            && group
                .directory
                .as_ref()
                .is_some_and(|p| uncertain_ancestors.contains(p))
        {
            group.decision = Decision::Review;
            group.evidence.push(evidence("S02",vec![],"Indexed descendants with incomplete format evidence stay physical fallbacks; review this partial grouping."));
        }
    }
    // Any retained index (even SUCCESS) is not proof of current disk freshness. Partial data
    // additionally downgrades every new grouping decision, never clears existing state.
    if !snapshot.scan_health.complete {
        for g in &mut plan.groups {
            g.decision = Decision::Review;
            for m in &mut g.members {
                m.decision = Decision::Review;
            }
        }
        for u in &mut plan.reading_units {
            u.decision = Decision::Review;
        }
    }
    let report = compare(&snapshot, plan)?;
    let d = step.elapsed().as_secs_f64() * 1000.;
    cancelled(options)?;
    Ok((
        snapshot,
        report,
        Timings {
            adapter_ms: a,
            recognize_ms: r,
            diff_ms: d,
            total_ms: start.elapsed().as_secs_f64() * 1000.,
        },
    ))
}
pub fn compare(snapshot: &IndexSnapshot, plan: Plan) -> Result<ShadowReport> {
    let by_path: BTreeMap<_, _> = snapshot
        .source_id_map
        .values()
        .map(|s| (s.path.as_str(), s.book_id))
        .collect();
    let mut source_map = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for u in &plan.reading_units {
        let id = *by_path
            .get(u.path.as_str())
            .ok_or("PROPOSAL_WITHOUT_INDEXED_SOURCE")?;
        let s = &snapshot.source_id_map[&id];
        if s.status != "INDEXED_FORMAT_EVIDENCE" || s.resource_id.is_some() {
            return Err("UNVERIFIED_SOURCE_PROPOSED".into());
        }
        if !seen.insert(id) {
            return Err("DUPLICATE_PROPOSED_SOURCE".into());
        }
        source_map.insert(u.source_ref.clone(), id);
    }
    let fallback = snapshot
        .source_id_map
        .keys()
        .filter(|id| !seen.contains(id))
        .copied()
        .collect::<Vec<_>>();
    let mut membership = BTreeMap::<i64, usize>::new();
    let mut diffs = Vec::new();
    let mut rules = BTreeMap::new();
    let path_to_node: BTreeMap<_, _> = snapshot
        .nodes
        .values()
        .map(|n| (n.path.as_str(), n.id))
        .collect();
    let details: BTreeMap<_, _> = snapshot
        .current_details
        .iter()
        .map(|d| (d.anchor_node_id, d))
        .collect();
    for g in &plan.groups {
        let mut ids = Vec::new();
        let mut owners = BTreeSet::new();
        let mut reasons = BTreeSet::new();
        for m in &g.members {
            let id = *source_map
                .get(&m.source_ref)
                .ok_or("GROUP_WITHOUT_SOURCE")?;
            *membership.entry(id).or_default() += 1;
            ids.push(id);
            owners.insert(snapshot.source_id_map[&id].node_id);
            for e in &m.evidence {
                reasons.insert(e.rule_id.clone());
                *rules.entry(e.rule_id.clone()).or_default() += 1;
            }
        }
        for e in &g.evidence {
            reasons.insert(e.rule_id.clone());
            *rules.entry(e.rule_id.clone()).or_default() += 1;
        }
        ids.sort_unstable();
        let anchor = g
            .directory
            .as_deref()
            .and_then(|p| path_to_node.get(p))
            .copied();
        let id_set: BTreeSet<_> = ids.iter().copied().collect();
        let extra = anchor
            .and_then(|id| details.get(&id))
            .map(|d| d.book_ids.iter().filter(|id| !id_set.contains(id)).count());
        diffs.push(GroupDiff {
            proposal_ref: g.proposal_ref.clone(),
            book_ids: ids,
            original_node_ids: owners.into_iter().collect(),
            current_anchor_node_id: anchor,
            current_detail_extra_books: extra,
            reasons: reasons.into_iter().collect(),
        });
    }
    if membership.len() != seen.len() || membership.values().any(|n| *n != 1) {
        return Err("SOURCE_MEMBERSHIP_INVARIANT".into());
    }
    let mut roles = BTreeMap::new();
    for d in &plan.directories {
        *roles.entry(format!("{:?}", d.role)).or_default() += 1;
    }
    let mut diagnostics = snapshot.diagnostics.clone();
    for d in &plan.diagnostics {
        *diagnostics.entry(d.code.clone()).or_default() += 1;
    }
    let summary = Summary {
        root_identity: snapshot.root_identity,
        media_type: snapshot.media_type,
        recognition_mode: snapshot.recognition_mode.clone(),
        index_snapshot_version: snapshot.index_snapshot_version.clone(),
        override_revision: snapshot.override_revision.clone(),
        scan_health: snapshot.scan_health.clone(),
        indexed_sources: snapshot.source_id_map.len(),
        proposed_reading_units: seen.len(),
        retained_physical_sources: fallback.len(),
        orphan_sources: 0,
        missing_source_ids: 0,
        duplicate_source_ids: 0,
        series: plan
            .groups
            .iter()
            .filter(|g| g.kind == GroupKind::Series)
            .count(),
        works: plan
            .groups
            .iter()
            .filter(|g| g.kind == GroupKind::Work)
            .count(),
        directory_roles: roles,
        review_groups: plan
            .groups
            .iter()
            .filter(|g| g.decision == Decision::Review)
            .count(),
        editions: plan.editions.len(),
        current_browse_nodes: snapshot.current_browse_node_ids.len(),
        current_browse_readable_count: snapshot.current_browse_book_ids.len()
            + snapshot.current_browse_readable_resource_ids.len(),
        current_details_compared: snapshot.current_details.len(),
        changed_group_count: diffs
            .iter()
            .filter(|d| {
                d.current_anchor_node_id.is_none()
                    || d.current_detail_extra_books.is_some_and(|n| n != 0)
                    || d.original_node_ids.len() > 1
            })
            .count(),
        remaining_attachments: snapshot
            .resources
            .iter()
            .filter(|r| !r.core_duplicate)
            .count(),
        unopened_readable_resources: snapshot
            .current_details
            .iter()
            .flat_map(|d| d.unopened_readable_resource_ids.iter())
            .collect::<BTreeSet<_>>()
            .len(),
        rules,
        diagnostics,
        mutation_policy: "READ_ONLY_PROPOSAL_AND_PHYSICAL_FALLBACK_NO_DELETIONS",
    };
    Ok(ShadowReport {
        summary,
        source_ref_to_book_id: source_map,
        fallback_book_ids: fallback,
        group_diffs: diffs,
        logical_proposal: plan,
    })
}
