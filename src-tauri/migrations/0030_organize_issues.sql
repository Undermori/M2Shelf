CREATE TABLE match_issue_outcomes (
    node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK(provider IN ('BANGUMI','TMDB')),
    outcome TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(node_id,provider)
);
CREATE TABLE issue_reminders (
    node_id INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    PRIMARY KEY(node_id,kind)
);
-- Existing bounded diagnostics are genuine observations, not inferred unbound states.
INSERT OR IGNORE INTO match_issue_outcomes
SELECT json_extract(j.value,'$.nodeId'),CASE s.key WHEN 'bangumi_auto_diagnostics' THEN 'BANGUMI' ELSE 'TMDB' END,
       json_extract(j.value,'$.outcome'),json_extract(j.value,'$.updatedAt')
FROM settings s,json_each(CASE WHEN json_valid(s.value) THEN s.value ELSE '[]' END) j
WHERE s.key IN ('bangumi_auto_diagnostics','tmdb_auto_diagnostics')
  AND EXISTS(SELECT 1 FROM nodes n WHERE n.id=json_extract(j.value,'$.nodeId'))
  AND json_type(j.value,'$.outcome')='text' AND json_type(j.value,'$.updatedAt')='text';
