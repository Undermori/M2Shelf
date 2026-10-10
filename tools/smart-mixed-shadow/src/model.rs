use m2shelf_smart_mixed_lab::model::{LibraryKind, Plan, Snapshot};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: i64,
    pub parent: Option<i64>,
    pub path: String,
    pub node_type: String,
    pub manual: bool,
    pub binding: Option<(i64, i64)>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub id: i64,
    pub page_index: i64,
    pub name: String,
    /// Unmodified indexed locator; never opened by the adapter. Private reports only.
    pub source_locator: String,
    pub file_size: i64,
    pub crc32: Option<i64>,
    pub modified_at: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub book_id: i64,
    pub node_id: i64,
    pub resource_id: Option<i64>,
    pub path: String,
    /// Exact SQLite source_path, for private provenance only; never a filesystem instruction.
    pub indexed_source_path: String,
    pub revision: String,
    pub source_kind: String,
    pub reader_format: Option<String>,
    pub document_format: Option<String>,
    pub text_encoding: Option<String>,
    pub file_size: i64,
    pub modified_at: String,
    pub source_resource_stamp: Option<String>,
    pub page_count: i64,
    pub index_error: Option<String>,
    pub pages: Vec<Page>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Resource {
    pub id: i64,
    pub node_id: i64,
    pub path: String,
    pub indexed_source_path: String,
    pub file_size: i64,
    pub modified_at: String,
    pub extension: String,
    pub resource_type: String,
    pub readable_suffix: bool,
    pub core_duplicate: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct ScanHealth {
    pub outcome: String,
    pub error_count: i64,
    pub last_success: Option<String>,
    pub last_attempt: Option<String>,
    pub latest_run: Option<(String, String, Option<String>, i64)>,
    pub complete: bool,
    /// Health describes the index only; no current disk-online check is made.
    pub freshness: String,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DetailView {
    pub anchor_node_id: i64,
    pub book_ids: Vec<i64>,
    pub remaining_child_ids: Vec<i64>,
    pub attachment_resource_ids: Vec<i64>,
    pub unopened_readable_resource_ids: Vec<i64>,
    pub other_resource_ids: Vec<i64>,
    pub display_readable_count: usize,
}
#[derive(Debug, Serialize)]
pub struct IndexSnapshot {
    pub root_identity: i64,
    pub indexed_root_path: String,
    pub media_type: LibraryKind,
    pub recognition_mode: String,
    /// Root-scoped SHA-256 of relevant rows in ONE read transaction, not a global DB revision.
    pub index_snapshot_version: String,
    pub override_revision: String,
    pub scan_health: ScanHealth,
    pub nodes: BTreeMap<i64, Node>,
    pub source_id_map: BTreeMap<i64, Source>,
    pub resources: Vec<Resource>,
    pub recognition_input: Snapshot,
    pub current_browse_node_ids: Vec<i64>,
    pub current_browse_book_ids: Vec<i64>,
    pub current_browse_readable_resource_ids: Vec<i64>,
    pub current_browse_other_resource_ids: Vec<i64>,
    pub current_details: Vec<DetailView>,
    pub diagnostics: BTreeMap<String, usize>,
}
#[derive(Debug, Serialize)]
pub struct GroupDiff {
    pub proposal_ref: String,
    pub book_ids: Vec<i64>,
    pub original_node_ids: Vec<i64>,
    pub current_anchor_node_id: Option<i64>,
    /// None means no corresponding sampled old detail, never a fabricated zero difference.
    pub current_detail_extra_books: Option<usize>,
    pub reasons: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Summary {
    pub root_identity: i64,
    pub media_type: LibraryKind,
    pub recognition_mode: String,
    pub index_snapshot_version: String,
    pub override_revision: String,
    pub scan_health: ScanHealth,
    pub indexed_sources: usize,
    pub proposed_reading_units: usize,
    pub retained_physical_sources: usize,
    pub orphan_sources: usize,
    pub missing_source_ids: usize,
    pub duplicate_source_ids: usize,
    pub series: usize,
    pub works: usize,
    pub directory_roles: BTreeMap<String, usize>,
    pub review_groups: usize,
    pub editions: usize,
    pub current_browse_nodes: usize,
    pub current_browse_readable_count: usize,
    pub current_details_compared: usize,
    pub changed_group_count: usize,
    pub remaining_attachments: usize,
    pub unopened_readable_resources: usize,
    pub rules: BTreeMap<String, usize>,
    pub diagnostics: BTreeMap<String, usize>,
    pub mutation_policy: &'static str,
}
#[derive(Debug, Serialize)]
pub struct ShadowReport {
    pub summary: Summary,
    pub source_ref_to_book_id: BTreeMap<String, i64>,
    pub fallback_book_ids: Vec<i64>,
    pub group_diffs: Vec<GroupDiff>,
    pub logical_proposal: Plan,
}
