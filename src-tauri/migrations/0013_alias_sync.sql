CREATE TABLE provider_alias_sync (
    subject_id INTEGER NOT NULL,
    subject_type INTEGER NOT NULL,
    aliases_json TEXT NOT NULL DEFAULT '[]',
    completed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(subject_id, subject_type)
);
