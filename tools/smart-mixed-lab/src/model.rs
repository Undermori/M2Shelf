use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LibraryKind {
    Comic,
    Ebook,
    Doujin,
    Artbook,
    Animation,
    LiveAction,
    Video,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EntryKind {
    Directory,
    File,
    Link,
}
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EntryState {
    #[default]
    Available,
    Unreadable,
    Offline,
    Corrupt,
    Excluded,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Format {
    Pdf,
    Epub,
    Cbz,
    Txt,
    Mobi,
    Azw3,
    Jpeg,
    Png,
    Webp,
    Gif,
    Bmp,
    Avif,
}
impl Format {
    pub fn image(self) -> bool {
        matches!(
            self,
            Self::Jpeg | Self::Png | Self::Webp | Self::Gif | Self::Bmp | Self::Avif
        )
    }
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Hint {
    Work,
    Series,
    Author,
    Artist,
    Publisher,
    ArtPages,
    Assets,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Metadata {
    pub title: Option<String>,
    pub series: Option<String>,
    pub volume: Option<u32>,
    pub chapter: Option<u32>,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub kind: EntryKind,
    #[serde(default)]
    pub state: EntryState,
    pub format: Option<Format>,
    /// Evidence supplied by the caller's existing validated decoder/index, never inferred from suffix alone.
    #[serde(default)]
    pub verified: bool,
    /// Optional native/index source identity. Opaque and scoped to this Root, NOT a hash of book content.
    pub identity: Option<String>,
    pub hint: Option<Hint>,
    pub metadata: Option<Metadata>,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderBasis {
    ExistingIndexedNaturalOrder,
    TrustedBookMetadata,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PageOrder {
    pub directory: String,
    pub pages: Vec<String>,
    pub basis: OrderBasis,
    #[serde(default)]
    pub include_covers: bool,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DirectoryRole {
    Root,
    Series,
    Category,
    BookImageDirectory,
    IntermediateContainer,
    Extras,
    Ambiguous,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ManualOverride {
    pub path: String,
    pub role: Option<DirectoryRole>,
    pub series_directory: Option<String>,
    pub volume: Option<u32>,
    pub chapter: Option<u32>,
    #[serde(default)]
    pub no_merge: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PriorUnit {
    pub source_ref: String,
    pub path: String,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Snapshot {
    pub root_id: String,
    pub media_kind: LibraryKind,
    #[serde(default)]
    pub complete: bool,
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub page_orders: Vec<PageOrder>,
    #[serde(default)]
    pub overrides: Vec<ManualOverride>,
    #[serde(default)]
    pub prior_units: Vec<PriorUnit>,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    Apply,
    Review,
    KeepSeparate,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Evidence {
    pub rule_id: String,
    pub facts: Vec<String>,
    pub explanation: String,
}
pub fn evidence(rule: &str, facts: Vec<String>, explanation: &str) -> Evidence {
    Evidence {
        rule_id: rule.into(),
        facts,
        explanation: explanation.into(),
    }
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PhysicalEntry {
    pub path: String,
    pub parent: Option<String>,
    pub kind: EntryKind,
    pub state: EntryState,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DirectoryJudgment {
    pub path: String,
    pub role: DirectoryRole,
    pub decision: Decision,
    pub evidence: Vec<Evidence>,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UnitKind {
    FileBook,
    DirectPages,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReadingUnit {
    pub source_ref: String,
    pub path: String,
    pub aliases: Vec<String>,
    pub kind: UnitKind,
    pub format: Option<Format>,
    pub pages: Vec<String>,
    pub decision: Decision,
    pub evidence: Vec<Evidence>,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MembershipRole {
    Main,
    Volume,
    Chapter,
    Collection,
    Extra,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Member {
    pub source_ref: String,
    pub role: MembershipRole,
    pub volume: Option<u32>,
    pub chapter: Option<u32>,
    pub decision: Decision,
    pub evidence: Vec<Evidence>,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupKind {
    Work,
    Series,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Group {
    pub proposal_ref: String,
    pub title: String,
    pub kind: GroupKind,
    pub directory: Option<String>,
    pub members: Vec<Member>,
    pub decision: Decision,
    pub evidence: Vec<Evidence>,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EditionProposal {
    pub sources: Vec<String>,
    pub decision: Decision,
    pub evidence: Evidence,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DisplayItem {
    pub target_ref: String,
    pub path: String,
    pub title: String,
    pub kind: String,
    pub decision: Decision,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DetailProposal {
    pub group_ref: String,
    pub reading_sources: Vec<String>,
    pub transparent_directories: Vec<String>,
    pub remaining_directories: Vec<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diagnostic {
    pub code: String,
    pub path: String,
    pub details: Vec<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Plan {
    pub schema_version: u32,
    pub rules_version: String,
    pub mode: String,
    pub root_id: String,
    pub physical_tree: Vec<PhysicalEntry>,
    pub directories: Vec<DirectoryJudgment>,
    pub reading_units: Vec<ReadingUnit>,
    pub groups: Vec<Group>,
    pub editions: Vec<EditionProposal>,
    pub root_items: Vec<DisplayItem>,
    pub details: Vec<DetailProposal>,
    pub retained_prior: Vec<PriorUnit>,
    pub diagnostics: Vec<Diagnostic>,
    /// Phase 1 does not emit a deletion/DB-mutation command, even for a complete snapshot.
    pub retention_policy: String,
}
