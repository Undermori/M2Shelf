//! Durable warmup checkpoint and bounded diagnostics, separate from AppSettings snapshots.
use crate::{
    db::{AppResult, Database},
    models::PosterCacheFailure,
};
use rusqlite::{params, OptionalExtension};

const KEY: &str = "poster_cache_checkpoint";

impl Database {
    pub(crate) fn poster_checkpoint(&self) -> AppResult<Option<String>> {
        self.connect()?
            .query_row(
                "SELECT value FROM settings WHERE key=?1 AND length(value)<=65536",
                [KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    /// Commit progress and this Node's outcome together, so restart cannot lose a failure or
    /// advance past work that was never recorded. A removed Node stays removed.
    pub(crate) fn save_poster_checkpoint(
        &self,
        json: &str,
        outcome: Option<(i64, Option<(&str, &str)>)>,
        reset: bool,
    ) -> AppResult<()> {
        if json.len() > 65536 {
            return Err("封面进度记录超过限额。".into());
        }
        let mut connection = self.connect()?;
        let tx = connection.transaction().map_err(|e| e.to_string())?;
        if reset {
            tx.execute("DELETE FROM poster_cache_failures", [])
                .map_err(|e| e.to_string())?;
        }
        if let Some((node_id, failure)) = outcome {
            if let Some((reason, detail)) = failure {
                tx.execute("INSERT INTO poster_cache_failures(node_id,reason,detail)
                    SELECT id,?2,?3 FROM nodes WHERE id=?1
                    ON CONFLICT(node_id) DO UPDATE SET reason=excluded.reason,detail=excluded.detail",
                    params![node_id,reason,detail]).map_err(|e| e.to_string())?;
            } else {
                tx.execute(
                    "DELETE FROM poster_cache_failures WHERE node_id=?1",
                    [node_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![KEY,json]).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    pub(crate) fn clear_poster_checkpoint(&self) -> AppResult<()> {
        let mut connection = self.connect()?;
        let tx = connection.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM settings WHERE key=?1", [KEY])
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM poster_cache_failures", [])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    pub(crate) fn poster_failure_count(&self) -> AppResult<u64> {
        self.connect()?.query_row(&format!("{} SELECT COUNT(*) FROM poster_cache_failures f JOIN nodes n ON n.id=f.node_id WHERE {}", crate::comics::PRESENTATION_CTE, crate::comics::POSTER_ELIGIBILITY), [], |row| row.get(0)).map_err(|e| e.to_string())
    }

    pub(crate) fn poster_failed_nodes_after(&self, after: i64) -> AppResult<Vec<i64>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(&format!("{} SELECT f.node_id FROM poster_cache_failures f JOIN nodes n ON n.id=f.node_id WHERE f.node_id>?1 AND {} ORDER BY f.node_id LIMIT 128", crate::comics::PRESENTATION_CTE, crate::comics::POSTER_ELIGIBILITY)).map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([after], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string());
        rows
    }

    pub(crate) fn poster_failures(&self) -> AppResult<Vec<PosterCacheFailure>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(&format!("{} SELECT f.node_id,n.display_name,f.reason,f.detail FROM poster_cache_failures f JOIN nodes n ON n.id=f.node_id WHERE {} ORDER BY f.node_id LIMIT 100", crate::comics::PRESENTATION_CTE, crate::comics::POSTER_ELIGIBILITY)).map_err(|e| e.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(PosterCacheFailure {
                    node_id: row.get(0)?,
                    name: row.get(1)?,
                    reason: row.get(2)?,
                    detail: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string());
        rows
    }
}
