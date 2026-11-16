//! Session-local mpv JSON IPC. No player discovery, source scans, scripts or config writes.
use crate::{
    db::{AppResult, Database},
    models::MediaFile,
    player,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone, Serialize)]
#[serde(
    tag = "state",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub enum PlaybackEvent {
    Started {
        session_id: u64,
    },
    Connected {
        session_id: u64,
    },
    File {
        session_id: u64,
        file_id: i64,
        owner_node_id: i64,
    },
    Finished {
        session_id: u64,
        file_id: i64,
        owner_node_id: i64,
    },
    Ended {
        session_id: u64,
    },
    Unavailable {
        session_id: u64,
        reason: FailureReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureReason {
    ConnectTimeout,
    ConnectionLost,
    ResponseInvalid,
    StateSaveFailed,
    ProcessMismatch,
}

#[derive(Clone, Default)]
pub struct Monitor {
    active: Arc<Mutex<Option<String>>>,
    sequence: Arc<AtomicU64>,
}
impl Monitor {
    #[cfg(test)]
    fn stop(&self) {
        let _ = self.stop_session();
    }
    fn stop_session(&self) -> Option<u64> {
        self.active
            .lock()
            .ok()?
            .take()
            .map(|_| self.sequence.load(Ordering::Relaxed))
    }
    pub fn launch(
        &self,
        db: &Database,
        executable: Option<&Path>,
        media: &MediaFile,
        emit: impl Fn(PlaybackEvent) + Send + Sync + 'static,
    ) -> AppResult<()> {
        validate_launch_media(db, media)?;
        #[cfg(windows)]
        if let Some(executable) = executable.filter(|p| is_mpv(p)) {
            return self.launch_mpv(db, executable, media, Arc::new(emit));
        }
        player::play_selected(executable, Path::new(&media.absolute_path))?;
        if let Some(session_id) = self.stop_session() {
            emit(PlaybackEvent::Ended { session_id });
        }
        Ok(())
    }
}

/// Every player mode uses the current indexed identity and ancestor visibility
/// before any process/Shell side effect. A stale detail cannot launch a moved row.
pub(crate) fn validate_launch_media(db: &Database, media: &MediaFile) -> AppResult<()> {
    db.read_snapshot(|c| {
        crate::db::ensure_node_visible_conn(c, media.node_id)?;
        let current: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM media_files WHERE id=?1 AND node_id=?2 AND absolute_path=?3 AND file_size=?4 AND modified_at=?5)",
            rusqlite::params![media.id, media.node_id, media.absolute_path, media.file_size, media.modified_at],
            |r| r.get(0),
        ).map_err(|e| e.to_string())?;
        if !current {
            return Err("PLAYER_LAUNCH_FAILED".into());
        }
        Ok(())
    })
}
fn is_mpv(path: &Path) -> bool {
    path.file_name()
        .and_then(|p| p.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("mpv.exe") || name.eq_ignore_ascii_case("mpv")
        })
}

/// Lexical lookup variants only. Never canonicalize or open an untrusted reported path.
fn indexed_paths(path: &str, cwd: &str) -> Vec<String> {
    if path.len() > 32768
        || cwd.len() > 32768
        || path.contains('\0')
        || cwd.contains('\0')
        || path.contains("://")
    {
        return Vec::new();
    }
    let path = Path::new(path);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else if Path::new(cwd).is_absolute() {
        Path::new(cwd).join(path)
    } else {
        return Vec::new();
    };
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                if !matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    return Vec::new();
                }
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    if !normalized.is_absolute() {
        return Vec::new();
    }
    let text = normalized.to_string_lossy().replace('/', "\\");
    let plain = if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned()
    };
    let verbatim = if let Some(unc) = plain.strip_prefix("\\\\") {
        format!("\\\\?\\UNC\\{unc}")
    } else {
        format!("\\\\?\\{plain}")
    };
    let mut values = vec![text, plain.clone(), plain.replace('\\', "/"), verbatim];
    values.sort();
    values.dedup();
    values
}

// This is a recording of this application's verified mpv connection, never an
// inference from scans or a connection to a separately launched player.
fn timestamp_ms() -> i64 {
    chrono::Utc::now().timestamp_millis().max(0)
}
fn screenshot_directory(path: &str, cwd: &str) -> Option<String> {
    if path.starts_with('~') || path.contains("://") {
        return None;
    }
    indexed_paths(if path.is_empty() { cwd } else { path }, cwd)
        .into_iter()
        .find(|p| !p.starts_with(r"\\?\"))
}
fn screenshot_expansion(path: &str) -> Option<Value> {
    // mpv, rather than a guessed USERPROFILE/Desktop path, knows redirected
    // Windows known folders and its own ~~/~~desktop/~/ expansion semantics.
    // This command only expands text; it never enumerates or reads that folder.
    (path.starts_with('~') && path.len() <= 32768 && !path.contains(['\0', '\n', '\r']))
        .then(|| json!({"command":["expand-path",path],"request_id":-9002}))
}
fn start_window(
    db: &Database,
    token: &str,
    root: (i64, &str),
    media: (i64, i64),
    directory: Option<&str>,
    now: i64,
) -> AppResult<i64> {
    let mut c = db.connect()?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    crate::db::ensure_node_visible_conn(&tx, media.1)?;
    // Recheck the actual file's Root and the logical owner while saving; a stale
    // asynchronous IPC reply cannot create a new session for a relocated index.
    let (source,title): (i64,String) = tx.query_row(
        "SELECT source.id,owner.display_name FROM media_files m JOIN nodes source ON source.id=m.node_id JOIN nodes owner ON owner.id=?2 JOIN library_roots r ON r.id=source.library_root_id WHERE m.id=?1 AND source.library_root_id=?3 AND owner.library_root_id=?3 AND r.path=?4",
        rusqlite::params![media.0, media.1, root.0,root.1], |r| Ok((r.get(0)?,r.get(1)?)),
    ).map_err(|e| e.to_string())?;
    crate::db::ensure_node_visible_conn(&tx, source)?;
    let index = crate::logical_works::LogicalWorkIndex::load_for_detail(&tx)?;
    if index.owners.get(&source).copied().unwrap_or(source) != media.1 {
        return Err("PLAYER_STATE_FAILED".into());
    }
    tx.execute(
        "UPDATE mpv_playback_sessions SET ended_at=max(started_at,CASE WHEN session_token=?2 THEN min(?1,heartbeat_at+15000) ELSE heartbeat_at END) WHERE ended_at IS NULL AND (session_token=?2 OR heartbeat_at<?1-30000)",
        rusqlite::params![now,token],
    ).map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO mpv_playback_sessions(session_token,node_id,media_file_id,started_at,heartbeat_at,screenshot_directory,video_title) VALUES(?1,?2,?3,?4,?4,?5,?6)",
        rusqlite::params![token,media.1,media.0,now,directory,title],
    ).map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.commit().map_err(|e| e.to_string())?;
    Ok(id)
}
fn touch_window(db: &Database, id: i64, token: &str, now: i64) -> AppResult<()> {
    db.connect()?.execute(
        "UPDATE mpv_playback_sessions SET heartbeat_at=max(heartbeat_at,?1) WHERE id=?2 AND session_token=?3 AND ended_at IS NULL",
        rusqlite::params![now,id,token],
    ).map_err(|e| e.to_string())?;
    Ok(())
}
fn close_window(db: &Database, id: i64, token: &str, now: i64) -> AppResult<()> {
    db.connect()?.execute(
        "UPDATE mpv_playback_sessions SET ended_at=max(started_at,min(?1,heartbeat_at+15000)) WHERE id=?2 AND session_token=?3 AND ended_at IS NULL",
        rusqlite::params![now,id,token],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Default)]
struct Protocol {
    sequence: i64,
    pending: Option<Replies>,
    synced: bool,
    confirmed: Option<(String, String)>,
}
struct Replies {
    base: i64,
    values: [Option<Value>; 3],
}
enum Action {
    None,
    Query(Vec<Value>),
    File(String, String),
    Finished(String, String),
    Stop,
}
impl Protocol {
    fn query(&mut self) -> Action {
        if self.synced || self.pending.is_some() {
            return Action::None;
        }
        self.sequence += 4;
        let base = self.sequence;
        self.pending = Some(Replies {
            base,
            values: [None, None, None],
        });
        Action::Query(
            ["path", "working-directory", "time-pos"]
                .into_iter()
                .enumerate()
                .map(
                    |(i, name)| json!({"command":["get_property",name],"request_id":base+i as i64}),
                )
                .collect(),
        )
    }
    fn receive(&mut self, value: Value) -> Action {
        match value.get("event").and_then(Value::as_str) {
            Some("shutdown") => return Action::Stop,
            Some("start-file") => {
                self.pending = None;
                self.synced = false;
                self.confirmed = None;
                return Action::None;
            }
            Some("end-file") => {
                self.pending = None;
                let confirmed = self.confirmed.take();
                return match confirmed {
                    Some((path, cwd))
                        if value.get("reason").and_then(Value::as_str) == Some("eof") =>
                    {
                        Action::Finished(path, cwd)
                    }
                    _ => Action::None,
                };
            }
            Some("file-loaded" | "playback-restart") => return self.query(),
            Some("property-change")
                if value.get("id").and_then(Value::as_u64) == Some(91)
                    && value.get("name").and_then(Value::as_str) == Some("time-pos")
                    && value
                        .get("data")
                        .and_then(Value::as_f64)
                        .is_some_and(|n| n.is_finite() && n >= 0.0) =>
            {
                return self.query()
            }
            _ => {}
        }
        let Some(reply) = self.pending.as_mut() else {
            return Action::None;
        };
        let Some(id) = value.get("request_id").and_then(Value::as_i64) else {
            return Action::None;
        };
        if !(reply.base..reply.base + 3).contains(&id) {
            return Action::None;
        }
        reply.values[(id - reply.base) as usize] = Some(
            if value.get("error").and_then(Value::as_str) == Some("success") {
                value.get("data").cloned().unwrap_or(Value::Null)
            } else {
                Value::Null
            },
        );
        if !reply.values.iter().all(Option::is_some) {
            return Action::None;
        }
        let reply = self.pending.take().expect("complete reply");
        if let (Some(path), Some(cwd), Some(position)) = (
            reply.values[0].as_ref().and_then(Value::as_str),
            reply.values[1].as_ref().and_then(Value::as_str),
            reply.values[2].as_ref().and_then(Value::as_f64),
        ) {
            if !path.is_empty() && position.is_finite() && position >= 0.0 {
                self.synced = true;
                self.confirmed = Some((path.into(), cwd.into()));
                return Action::File(path.into(), cwd.into());
            }
        }
        Action::None
    }
}

#[cfg(windows)]
impl Monitor {
    fn launch_mpv(
        &self,
        db: &Database,
        executable: &Path,
        media: &MediaFile,
        emit: Arc<dyn Fn(PlaybackEvent) + Send + Sync>,
    ) -> AppResult<()> {
        use std::process::{Command, Stdio};
        player::validate_playback(executable, Path::new(&media.absolute_path))?;
        let (root,path):(i64,String)=db.read_snapshot(|c|{crate::db::ensure_node_visible_conn(c,media.node_id)?;c.query_row("SELECT r.id,r.path FROM library_roots r JOIN nodes n ON n.library_root_id=r.id WHERE n.id=?1",[media.node_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())})?;
        let token = uuid::Uuid::new_v4().to_string();
        let pipe = format!(r"\\.\pipe\M2Shelf-mpv-{token}");
        let child = Command::new(executable)
            .arg(format!("--input-ipc-server={pipe}"))
            .arg("--")
            .arg(&media.absolute_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("启动播放器失败：{e}"))?;
        let session_id = {
            let mut active = self.active.lock().map_err(|_| "PLAYER_STATE_FAILED")?;
            let session_id = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
            *active = Some(token.clone());
            session_id
        };
        emit(PlaybackEvent::Started { session_id });
        let initial_file = media.id;
        let active = self.active.clone();
        let database = db.clone();
        tauri::async_runtime::spawn(async move {
            let result = windows::observe(
                child,
                pipe,
                database,
                root,
                path,
                token.clone(),
                active.clone(),
                emit.clone(),
                initial_file,
                session_id,
            )
            .await;
            if active
                .lock()
                .is_ok_and(|s| s.as_deref() == Some(token.as_str()))
            {
                match result {
                    Ok(()) => emit(PlaybackEvent::Ended { session_id }),
                    Err(reason) => emit(PlaybackEvent::Unavailable { session_id, reason }),
                }
            }
        });
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        io,
        os::windows::io::AsRawHandle,
        process::Child,
        time::{Duration, Instant},
    };
    use tokio::{
        io::AsyncWriteExt,
        net::windows::named_pipe::{ClientOptions, NamedPipeClient},
        time::{sleep, timeout},
    };
    use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;

    #[derive(Clone, Copy)]
    struct Timing {
        connect: Duration,
        exit: Duration,
        reconnect: Duration,
    }
    impl Default for Timing {
        fn default() -> Self {
            Self {
                connect: Duration::from_secs(8),
                exit: Duration::from_secs(2),
                reconnect: Duration::from_secs(3),
            }
        }
    }
    enum ConnectionEnd {
        Ended,
        Lost { healthy: bool },
    }
    struct Session {
        child: Child,
        name: String,
        db: Database,
        root: i64,
        path: String,
        token: String,
        active: Arc<Mutex<Option<String>>>,
        emit: Arc<dyn Fn(PlaybackEvent) + Send + Sync>,
        initial_file: i64,
        session_id: u64,
        protocol: Protocol,
        first: bool,
        last_reported: Option<Vec<String>>,
        last_file: Option<(i64, i64)>,
        playback_window: Option<i64>,
        screenshot_requested: bool,
        screenshot_setting: Option<String>,
        current_cwd: String,
        heartbeat: Instant,
        timing: Timing,
    }
    impl Session {
        fn current(&self) -> bool {
            self.active
                .lock()
                .is_ok_and(|s| s.as_deref() == Some(self.token.as_str()))
        }
        fn stopped(&mut self) -> Result<bool, FailureReason> {
            Ok(!self.current()
                || self
                    .child
                    .try_wait()
                    .map_err(|_| FailureReason::ConnectionLost)?
                    .is_some())
        }
        fn emit(&self, event: PlaybackEvent) {
            if self.current() {
                (self.emit)(event);
            }
        }
        async fn connect(
            &mut self,
            recovering: bool,
        ) -> Result<Option<NamedPipeClient>, FailureReason> {
            let deadline = Instant::now()
                + if recovering {
                    self.timing.reconnect
                } else {
                    self.timing.connect
                };
            loop {
                // A completed launch session has no live playback left to synchronize.
                if self.stopped()? {
                    return Ok(None);
                }
                match ClientOptions::new().open(&self.name) {
                    Ok(pipe) => {
                        let mut pid = 0;
                        if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) }
                            == 0
                            || pid != self.child.id()
                        {
                            return Err(FailureReason::ProcessMismatch);
                        }
                        return Ok(Some(pipe));
                    }
                    Err(_) if Instant::now() < deadline => sleep(Duration::from_millis(80)).await,
                    Err(_) => {
                        return Err(if recovering {
                            FailureReason::ConnectionLost
                        } else {
                            FailureReason::ConnectTimeout
                        })
                    }
                }
            }
        }
        async fn wait_for_exit(&mut self) -> Result<bool, FailureReason> {
            let deadline = Instant::now() + self.timing.exit;
            loop {
                if self.stopped()? {
                    return Ok(true);
                }
                if Instant::now() >= deadline {
                    return Ok(false);
                }
                sleep(Duration::from_millis(40)).await;
            }
        }
        async fn finish_window(&mut self) {
            if let Some(id) = self.playback_window.take() {
                let db = self.db.clone();
                let token = self.token.clone();
                let now = timestamp_ms();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    close_window(&db, id, &token, now)
                })
                .await;
            }
        }
        async fn update_heartbeat(&mut self) {
            if self.heartbeat.elapsed() < Duration::from_secs(5) {
                return;
            }
            self.heartbeat = Instant::now();
            if let Some(id) = self.playback_window {
                let db = self.db.clone();
                let token = self.token.clone();
                let now = timestamp_ms();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    touch_window(&db, id, &token, now)
                })
                .await;
            }
        }
        async fn record_window(&mut self, media: (i64, i64), cwd: &str) {
            self.current_cwd = cwd.to_owned();
            let directory = self
                .screenshot_setting
                .as_deref()
                .and_then(|v| screenshot_directory(v, cwd));
            let db = self.db.clone();
            let root = self.root;
            let root_path = self.path.clone();
            let token = self.token.clone();
            let gate = self.active.clone();
            let now = timestamp_ms();
            self.playback_window = tauri::async_runtime::spawn_blocking(move || {
                let guard = gate.lock().map_err(|_| "PLAYER_STATE_FAILED")?;
                if guard.as_deref() != Some(token.as_str()) {
                    return Err("PLAYER_STATE_FAILED".into());
                }
                start_window(
                    &db,
                    &token,
                    (root, &root_path),
                    media,
                    directory.as_deref(),
                    now,
                )
            })
            .await
            .ok()
            .and_then(Result::ok);
            self.heartbeat = Instant::now();
        }
        async fn receive_screenshot_setting(
            &mut self,
            packet: &Value,
            pipe: &mut NamedPipeClient,
        ) -> bool {
            let request = packet.get("request_id").and_then(Value::as_i64);
            if !matches!(request, Some(-9001 | -9002)) {
                return false;
            }
            self.screenshot_setting = (packet.get("error").and_then(Value::as_str)
                == Some("success"))
            .then(|| packet.get("data").and_then(Value::as_str))
            .flatten()
            .filter(|s| s.len() <= 32768 && !s.contains('\0'))
            .filter(|s| request != Some(-9002) || Path::new(s).is_absolute())
            .map(str::to_owned);
            if request == Some(-9001) {
                if let Some(command) = self
                    .screenshot_setting
                    .as_deref()
                    .and_then(screenshot_expansion)
                {
                    // An unsupported expansion remains None at the directory
                    // boundary, retaining the conservative unique-time fallback.
                    let _ = send(pipe, vec![command]).await;
                }
            }
            if let Some(id) = self.playback_window {
                let directory = self
                    .screenshot_setting
                    .as_deref()
                    .and_then(|v| screenshot_directory(v, &self.current_cwd));
                let db = self.db.clone();
                let token = self.token.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
                    db.connect()?.execute("UPDATE mpv_playback_sessions SET screenshot_directory=?1 WHERE id=?2 AND session_token=?3 AND ended_at IS NULL", rusqlite::params![directory,id,token]).map_err(|e|e.to_string())?;
                    Ok(())
                }).await;
            }
            true
        }
        async fn run(mut self) -> Result<(), FailureReason> {
            let result = self.run_inner().await;
            self.finish_window().await;
            result
        }
        async fn run_inner(&mut self) -> Result<(), FailureReason> {
            let mut recovering = false;
            let mut consecutive_losses = 0;
            loop {
                let Some(mut pipe) = self.connect(recovering).await? else {
                    return Ok(());
                };
                self.protocol.pending = None;
                self.protocol.synced = false;
                self.protocol.confirmed = None;
                self.emit(PlaybackEvent::Connected {
                    session_id: self.session_id,
                });
                match self.read_connection(&mut pipe, recovering).await? {
                    ConnectionEnd::Ended => return Ok(()),
                    ConnectionEnd::Lost { healthy } => {
                        drop(pipe);
                        // mpv may close IPC before GPU/process teardown completes.
                        if self.wait_for_exit().await? {
                            return Ok(());
                        }
                        if healthy {
                            consecutive_losses = 0;
                        }
                        consecutive_losses += 1;
                        if consecutive_losses > 2 {
                            return Err(FailureReason::ConnectionLost);
                        }
                        recovering = true;
                    }
                }
            }
        }
        async fn read_connection(
            &mut self,
            pipe: &mut NamedPipeClient,
            recovering: bool,
        ) -> Result<ConnectionEnd, FailureReason> {
            let mut healthy = false;
            let mut recovery_snapshot = recovering;
            let observed = send(
                pipe,
                vec![json!({"command":["observe_property",91,"time-pos"]})],
            )
            .await
            .is_ok();
            if !observed && !self.stopped()? {
                return Ok(ConnectionEnd::Lost { healthy });
            }
            if observed {
                // Query once for this launched session. Failure is deliberately
                // optional and cannot disable playback or progress synchronization.
                if !self.screenshot_requested {
                    self.screenshot_requested = true;
                    let _ = send(pipe, vec![json!({"command":["get_property","screenshot-directory"],"request_id":-9001})]).await;
                }
                if let Action::Query(commands) = self.protocol.query() {
                    if send(pipe, commands).await.is_err() && !self.stopped()? {
                        return Ok(ConnectionEnd::Lost { healthy });
                    }
                }
            }
            let mut buffer = Vec::new();
            let mut chunk = [0u8; 8192];
            let mut malformed = 0;
            loop {
                // The child can exit while its final messages are still buffered.
                // Cancellation stops immediately; process exit is checked only
                // after the pipe has had a chance to deliver those messages.
                if !self.current() {
                    return Ok(ConnectionEnd::Ended);
                }
                self.update_heartbeat().await;
                match timeout(Duration::from_millis(100), pipe.readable()).await {
                    Err(_) => {
                        if self.stopped()? {
                            return Ok(ConnectionEnd::Ended);
                        }
                        continue;
                    }
                    Ok(Err(_)) => return Ok(ConnectionEnd::Lost { healthy }),
                    Ok(Ok(())) => {}
                }
                match pipe.try_read(&mut chunk) {
                    Ok(0) => return Ok(ConnectionEnd::Lost { healthy }),
                    Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(_) => return Ok(ConnectionEnd::Lost { healthy }),
                }
                if buffer.len() > 262144 {
                    return Err(FailureReason::ResponseInvalid);
                }
                while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                    if end > 65536 {
                        return Err(FailureReason::ResponseInvalid);
                    }
                    let line = buffer.drain(..=end).collect::<Vec<_>>();
                    if line.iter().all(u8::is_ascii_whitespace) {
                        continue;
                    }
                    let packet: Value = match serde_json::from_slice(&line) {
                        Ok(value) => {
                            malformed = 0;
                            value
                        }
                        Err(_) => {
                            malformed += 1;
                            if malformed >= 3 {
                                return Err(FailureReason::ResponseInvalid);
                            }
                            continue;
                        }
                    };
                    if self.receive_screenshot_setting(&packet, pipe).await {
                        continue;
                    }
                    if matches!(
                        packet.get("event").and_then(Value::as_str),
                        Some("start-file" | "end-file")
                    ) {
                        self.finish_window().await;
                    }
                    match self.protocol.receive(packet) {
                        Action::None => {}
                        Action::Stop => return Ok(ConnectionEnd::Ended),
                        Action::Finished(file, cwd) => {
                            let paths = indexed_paths(&file, &cwd);
                            if !paths.is_empty() && self.last_reported.as_ref() == Some(&paths) {
                                if let Some((file_id, owner_node_id)) = self.last_file {
                                    // Completion reconciles the existing committed state.
                                    // Never write it twice or undo a manual clear.
                                    self.emit(PlaybackEvent::Finished {
                                        session_id: self.session_id,
                                        file_id,
                                        owner_node_id,
                                    });
                                }
                            }
                        }
                        Action::Query(commands) => {
                            if send(pipe, commands).await.is_err() && !self.stopped()? {
                                return Ok(ConnectionEnd::Lost { healthy });
                            }
                        }
                        Action::File(file, cwd) => {
                            healthy = true;
                            let paths = indexed_paths(&file, &cwd);
                            if paths.is_empty() {
                                self.finish_window().await;
                                continue;
                            }
                            let duplicate =
                                recovery_snapshot && self.last_reported.as_ref() == Some(&paths);
                            recovery_snapshot = false;
                            if duplicate {
                                if self.playback_window.is_none() {
                                    if let Some(media) = self.last_file {
                                        self.record_window(media, &cwd).await;
                                    }
                                }
                                continue;
                            } // Preserve manual clear through reconnecting the same file.
                            let db = self.db.clone();
                            let root = self.root;
                            let root_path = self.path.clone();
                            let gate = self.active.clone();
                            let token = self.token.clone();
                            let skip_initial = self.first.then_some(self.initial_file);
                            self.first = false;
                            let lookup = paths.clone();
                            let result = tauri::async_runtime::spawn_blocking(move || {
                                let guard = gate.lock().map_err(|_| "PLAYER_STATE_FAILED")?;
                                if guard.as_deref() != Some(token.as_str()) {
                                    return Ok(None);
                                }
                                db.record_mpv_file(root, &root_path, &lookup, skip_initial)
                            })
                            .await
                            .map_err(|_| FailureReason::StateSaveFailed)?
                            .map_err(|_| FailureReason::StateSaveFailed)?;
                            self.last_reported = Some(paths);
                            self.last_file = result;
                            if let Some((file_id, owner_node_id)) = result {
                                self.record_window((file_id, owner_node_id), &cwd).await;
                                self.emit(PlaybackEvent::File {
                                    session_id: self.session_id,
                                    file_id,
                                    owner_node_id,
                                });
                            }
                        }
                    }
                }
                if buffer.len() > 65536 {
                    return Err(FailureReason::ResponseInvalid);
                }
            }
        }
    }
    async fn send(pipe: &mut NamedPipeClient, commands: Vec<Value>) -> Result<(), ()> {
        let bytes = commands
            .into_iter()
            .map(|c| c.to_string() + "\n")
            .collect::<String>();
        timeout(Duration::from_secs(2), pipe.write_all(bytes.as_bytes()))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn observe(
        child: Child,
        name: String,
        db: Database,
        root: i64,
        path: String,
        token: String,
        active: Arc<Mutex<Option<String>>>,
        emit: Arc<dyn Fn(PlaybackEvent) + Send + Sync>,
        initial_file: i64,
        session_id: u64,
    ) -> Result<(), FailureReason> {
        Session {
            child,
            name,
            db,
            root,
            path,
            token,
            active,
            emit,
            initial_file,
            session_id,
            protocol: Protocol::default(),
            first: true,
            last_reported: None,
            last_file: None,
            playback_window: None,
            screenshot_requested: false,
            screenshot_setting: None,
            current_cwd: String::new(),
            heartbeat: Instant::now(),
            timing: Timing::default(),
        }
        .run()
        .await
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        use tokio::net::windows::named_pipe::ServerOptions;
        #[test]
        #[ignore = "synthetic child process fixture; invoked only by the lifecycle test"]
        fn pipe_fixture_worker() {
            let Ok(name) = std::env::var("M2SHELF_TEST_PIPE_NAME") else {
                return;
            };
            assert!(name.starts_with(r"\\.\pipe\M2Shelf-test-"));
            if std::env::var("M2SHELF_TEST_PIPE_MODE").as_deref() == Ok("timeout") {
                std::thread::sleep(Duration::from_millis(900));
                return;
            }
            tauri::async_runtime::block_on(async {
                let pipe = ServerOptions::new().create(name).unwrap();
                pipe.connect().await.unwrap();
                if std::env::var("M2SHELF_TEST_PIPE_MODE").as_deref() == Ok("eof-queued") {
                    let mut pipe = pipe;
                    pipe.write_all(
                        b"{\"event\":\"end-file\",\"reason\":\"eof\"}\n{\"event\":\"shutdown\"}\n",
                    )
                    .await
                    .unwrap();
                    sleep(Duration::from_millis(80)).await;
                    return;
                }
                sleep(Duration::from_millis(80)).await;
                drop(pipe);
                std::thread::sleep(Duration::from_millis(600));
            });
        }
        fn session(mode: &str) -> Session {
            let name = format!(r"\\.\pipe\M2Shelf-test-{}", uuid::Uuid::new_v4());
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "mpv_ipc::windows::tests::pipe_fixture_worker",
                    "--ignored",
                    "--nocapture",
                ])
                .env("M2SHELF_TEST_PIPE_NAME", &name)
                .env("M2SHELF_TEST_PIPE_MODE", mode)
                .creation_flags(0x08000000)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            Session {
                child,
                name,
                db: Database::new(PathBuf::from("unused-synthetic.db")),
                root: 1,
                path: String::new(),
                token: "synthetic".into(),
                active: Arc::new(Mutex::new(Some("synthetic".into()))),
                emit: Arc::new(|_| {}),
                initial_file: 1,
                session_id: 1,
                protocol: Protocol::default(),
                first: true,
                last_reported: None,
                last_file: None,
                playback_window: None,
                screenshot_requested: false,
                screenshot_setting: None,
                current_cwd: String::new(),
                heartbeat: Instant::now(),
                timing: Timing {
                    connect: Duration::from_secs(5),
                    exit: Duration::from_secs(2),
                    reconnect: Duration::from_millis(100),
                },
            }
        }
        #[test]
        fn pipe_closes_before_process_exit_is_a_normal_end() {
            assert_eq!(
                tauri::async_runtime::block_on(session("delayed-exit").run()),
                Ok(())
            );
        }
        #[test]
        fn exited_child_still_delivers_its_buffered_completion() {
            let mut session = session("eof-queued");
            let file = "C:/Synthetic/Finished.mkv".to_owned();
            let cwd = "C:/Synthetic".to_owned();
            session.protocol.synced = true;
            session.protocol.confirmed = Some((file.clone(), cwd.clone()));
            session.last_reported = Some(indexed_paths(&file, &cwd));
            session.last_file = Some((41, 12));
            let events = Arc::new(Mutex::new(Vec::new()));
            let captured = events.clone();
            session.emit = Arc::new(move |event| captured.lock().unwrap().push(event));
            tauri::async_runtime::block_on(async {
                let mut pipe = session.connect(false).await.unwrap().unwrap();
                // Deterministically reproduce exit before the receiver's next read.
                session.child.wait().unwrap();
                assert!(matches!(
                    session.read_connection(&mut pipe, false).await.unwrap(),
                    ConnectionEnd::Ended
                ));
            });
            assert!(matches!(
                events.lock().unwrap().as_slice(),
                [PlaybackEvent::Finished {
                    file_id: 41,
                    owner_node_id: 12,
                    ..
                }]
            ));
        }
        #[test]
        fn live_process_without_ipc_still_reports_a_real_timeout() {
            let mut session = session("timeout");
            session.timing.connect = Duration::from_millis(100);
            assert_eq!(
                tauri::async_runtime::block_on(session.run()),
                Err(FailureReason::ConnectTimeout)
            );
        }
        #[test]
        fn canceled_session_ends_without_a_warning() {
            let session = session("timeout");
            *session.active.lock().unwrap() = None;
            assert_eq!(tauri::async_runtime::block_on(session.run()), Ok(()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_playback_windows_switch_close_heartbeat_and_reject_stale_root() {
        let temp = crate::db::test_temp_dir();
        let db = Database::new(temp.path().join("sessions.db"));
        db.migrate().unwrap();
        let c = db.connect().unwrap();
        c.execute_batch("INSERT INTO library_roots(id,path,display_name) VALUES(1,'synthetic-library','Library'),(2,'other-library','Other'); INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,total_video_count) VALUES(1,1,NULL,'synthetic-library','Root','Root','CONTAINER',2),(2,1,1,'synthetic-library/a','A','A','WORK',2); INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(1,2,'synthetic-library/a/01.mkv','01.mkv','mkv',1,'synthetic'),(2,2,'synthetic-library/a/02.mkv','02.mkv','mkv',1,'synthetic');").unwrap();
        let first = start_window(
            &db,
            "session-a",
            (1, "synthetic-library"),
            (1, 2),
            Some("C:/Synthetic/Shots"),
            100_000,
        )
        .unwrap();
        touch_window(&db, first, "wrong-token", 110_000).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT heartbeat_at FROM mpv_playback_sessions WHERE id=?1",
                [first],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            100_000
        );
        touch_window(&db, first, "session-a", 110_000).unwrap();
        let second = start_window(
            &db,
            "session-a",
            (1, "synthetic-library"),
            (2, 2),
            None,
            112_000,
        )
        .unwrap();
        assert_eq!(
            c.query_row(
                "SELECT ended_at FROM mpv_playback_sessions WHERE id=?1",
                [first],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            112_000
        );
        close_window(&db, second, "session-a", 500_000).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT ended_at FROM mpv_playback_sessions WHERE id=?1",
                [second],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            127_000
        );
        assert!(
            start_window(&db, "bad-root", (2, "other-library"), (1, 2), None, 600_000).is_err()
        );
        let crashed = start_window(
            &db,
            "crashed",
            (1, "synthetic-library"),
            (1, 2),
            None,
            700_000,
        )
        .unwrap();
        start_window(
            &db,
            "new-session",
            (1, "synthetic-library"),
            (2, 2),
            None,
            800_000,
        )
        .unwrap();
        assert_eq!(
            c.query_row(
                "SELECT ended_at FROM mpv_playback_sessions WHERE id=?1",
                [crashed],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            700_000
        );
    }
    #[test]
    fn screenshot_directory_uses_only_bounded_lexical_local_paths() {
        assert_eq!(
            screenshot_directory("shots", r"C:\Synthetic").map(|path| path.replace('/', "\\")),
            Some(r"C:\Synthetic\shots".into())
        );
        assert_eq!(
            screenshot_directory("", r"C:\Synthetic").map(|path| path.replace('/', "\\")),
            Some(r"C:\Synthetic".into())
        );
        assert_eq!(screenshot_directory("~~desktop/", r"C:\Synthetic"), None);
        assert_eq!(
            screenshot_directory("https://invalid", r"C:\Synthetic"),
            None
        );
        assert_eq!(screenshot_directory("bad\0path", r"C:\Synthetic"), None);
        assert_eq!(screenshot_directory("x", "relative"), None);
    }
    #[test]
    fn round10_mpv_expands_known_folders_itself_without_reading_the_user_profile() {
        for raw in ["~~desktop/", "~/Pictures", "~~home/screenshots"] {
            let command = screenshot_expansion(raw).unwrap();
            assert_eq!(command["command"], json!(["expand-path", raw]));
            assert_eq!(command["request_id"], -9002);
            assert_eq!(screenshot_directory(raw, r"C:\Synthetic"), None);
        }
        // Synthetic reply also covers a redirected Desktop, rather than guessing
        // USERPROFILE/Desktop and accidentally rejecting its registered folder.
        assert_eq!(
            screenshot_directory(r"D:\Synthetic\Redirected Desktop", r"C:\Synthetic")
                .map(|p| p.replace('/', "\\")),
            Some(r"D:\Synthetic\Redirected Desktop".into())
        );
        assert!(screenshot_expansion("~/bad\0path").is_none());
        assert!(screenshot_expansion(&format!("~/{}", "x".repeat(32768))).is_none());
        assert!(screenshot_expansion("shots").is_none());
    }
    fn confirmed_protocol() -> Protocol {
        let mut p = Protocol::default();
        p.query();
        p.receive(json!({"request_id":4,"error":"success","data":"one.mkv"}));
        p.receive(json!({"request_id":5,"error":"success","data":"C:/Synthetic"}));
        assert!(matches!(
            p.receive(json!({"request_id":6,"error":"success","data":1})),
            Action::File(_, _)
        ));
        p
    }
    #[test]
    fn natural_end_reconciles_the_confirmed_file_once() {
        let mut p = confirmed_protocol();
        assert!(
            matches!(p.receive(json!({"event":"end-file","reason":"eof"})),Action::Finished(path,cwd) if path=="one.mkv"&&cwd=="C:/Synthetic")
        );
        assert!(matches!(
            p.receive(json!({"event":"end-file","reason":"eof"})),
            Action::None
        ));
        assert!(matches!(
            p.receive(json!({"event":"shutdown"})),
            Action::Stop
        ));
    }
    #[test]
    fn quit_error_unconfirmed_and_switched_files_are_not_completion() {
        for reason in ["stop", "quit", "error", "redirect", "unknown"] {
            let mut p = confirmed_protocol();
            assert!(matches!(
                p.receive(json!({"event":"end-file","reason":reason})),
                Action::None
            ));
            assert!(matches!(
                p.receive(json!({"event":"end-file","reason":"eof"})),
                Action::None
            ));
        }
        let mut p = Protocol::default();
        assert!(matches!(
            p.receive(json!({"event":"end-file","reason":"eof"})),
            Action::None
        ));
        let mut p = confirmed_protocol();
        p.receive(json!({"event":"start-file"}));
        assert!(matches!(
            p.receive(json!({"event":"end-file","reason":"eof"})),
            Action::None
        ));
    }
    #[test]
    fn protocol_requires_confirmed_playback_and_rejects_old_replies_after_switch() {
        let mut p = Protocol::default();
        assert!(matches!(p.query(), Action::Query(_)));
        p.receive(json!({"request_id":4,"error":"success","data":"one.mkv"}));
        p.receive(json!({"event":"start-file"}));
        assert!(matches!(
            p.receive(json!({"request_id":6,"error":"success","data":1})),
            Action::None
        ));
        assert!(matches!(
            p.receive(json!({"event":"file-loaded"})),
            Action::Query(_)
        ));
        assert!(matches!(
            p.receive(json!({"request_id":8,"error":"success","data":"two.mkv"})),
            Action::None
        ));
        p.receive(json!({"request_id":9,"error":"success","data":"C:/Synthetic"}));
        assert!(
            matches!(p.receive(json!({"request_id":10,"error":"success","data":0.1})),Action::File(path,_) if path=="two.mkv")
        );
        assert!(matches!(
            p.receive(json!({"event":"playback-restart"})),
            Action::None
        ));
        assert!(matches!(
            p.receive(json!({"event":"property-change","id":91,"name":"time-pos","data":12})),
            Action::None
        ));
    }
    #[test]
    fn never_adds_mpv_arguments_to_other_players_or_accepts_remote_paths() {
        assert!(is_mpv(Path::new("C:/Synthetic/mpv.exe")));
        assert!(!is_mpv(Path::new("C:/Synthetic/vlc.exe")));
        assert!(!is_mpv(Path::new("C:/Synthetic/mpvnet.exe")));
        assert!(indexed_paths("https://example.invalid/video.mkv", "C:/Synthetic").is_empty());
        assert!(indexed_paths("relative.mkv", "relative").is_empty());
        assert!(indexed_paths("bad\0.mkv", "C:/Synthetic").is_empty());
        assert!(indexed_paths("E02.mkv", "C:\\Synthetic")
            .iter()
            .any(|p| p == "C:\\Synthetic\\E02.mkv"));
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires explicitly supplied isolated official mpv test runtime"]
    fn real_mpv_named_pipe_file_switch_clear_and_stop() {
        use std::{fs, sync::mpsc, time::Duration};
        use tokio::{io::AsyncWriteExt, net::windows::named_pipe::ClientOptions};
        let executable = PathBuf::from(
            std::env::var_os("M2SHELF_MPV_TEST_EXE").expect("explicit test-only mpv"),
        );
        assert!(executable
            .components()
            .any(|c| c.as_os_str() == "mpv-test-runtime"));
        let temp = crate::db::test_temp_dir();
        let root = temp.path().join("Synthetic");
        fs::create_dir(&root).unwrap();
        let db = Database::new(temp.path().join("mpv.db"));
        db.migrate().unwrap();
        let c = db.connect().unwrap();
        c.execute("INSERT INTO library_roots(id,path,display_name,media_kind,video_subject_scope) VALUES(1,?1,'Synthetic','VIDEO','ANIMATION')",[root.to_string_lossy()]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,absolute_path,folder_name,display_name,node_type) VALUES(1,1,?1,'Synthetic','Synthetic','CONTAINER')",[root.to_string_lossy()]).unwrap();
        c.execute("INSERT INTO nodes(id,library_root_id,parent_node_id,absolute_path,folder_name,display_name,node_type,manual_type_override,direct_video_count,total_video_count) VALUES(2,1,1,?1,'Synthetic Show','Synthetic Show','WORK',1,3,3)",[root.join("Show").to_string_lossy()]).unwrap();
        let mut wave = Vec::new();
        let bytes = 8000u32 * 2 * 90;
        wave.extend_from_slice(b"RIFF");
        wave.extend_from_slice(&(36 + bytes).to_le_bytes());
        wave.extend_from_slice(b"WAVEfmt ");
        wave.extend_from_slice(&16u32.to_le_bytes());
        wave.extend_from_slice(&1u16.to_le_bytes());
        wave.extend_from_slice(&1u16.to_le_bytes());
        wave.extend_from_slice(&8000u32.to_le_bytes());
        wave.extend_from_slice(&16000u32.to_le_bytes());
        wave.extend_from_slice(&2u16.to_le_bytes());
        wave.extend_from_slice(&16u16.to_le_bytes());
        wave.extend_from_slice(b"data");
        wave.extend_from_slice(&bytes.to_le_bytes());
        wave.resize(44 + bytes as usize, 0);
        for id in 1..=3 {
            let name = format!("テスト [版本] S01E{id:02}.mkv");
            let path = root.join(name.clone());
            fs::write(&path, &wave).unwrap();
            c.execute("INSERT INTO media_files(id,node_id,absolute_path,file_name,extension,file_size,modified_at) VALUES(?1,2,?2,?3,'mkv',?4,'2026-10-25')",rusqlite::params![id,path.to_string_lossy(),name,wave.len()]).unwrap();
        }
        let monitor = Monitor::default();
        let (send, receive) = mpsc::channel();
        monitor
            .launch(
                &db,
                Some(&executable),
                &db.get_media_file(1).unwrap(),
                move |event| {
                    let _ = send.send(event);
                },
            )
            .unwrap();
        let next_file = |seconds| loop {
            match receive.recv_timeout(Duration::from_secs(seconds)).unwrap() {
                event @ PlaybackEvent::File { .. } => break event,
                PlaybackEvent::Unavailable { reason, .. } => {
                    panic!("unexpected sync failure: {reason:?}")
                }
                _ => {}
            }
        };
        let token = monitor.active.lock().unwrap().clone().unwrap();
        let pipe = format!(r"\\.\pipe\M2Shelf-mpv-{token}");
        let command = |value: Value| {
            tauri::async_runtime::block_on(async {
                let mut p = ClientOptions::new().open(&pipe).unwrap();
                p.write_all((value.to_string() + "\n").as_bytes())
                    .await
                    .unwrap();
                let _ = tokio::time::timeout(Duration::from_secs(2), p.readable()).await;
            })
        };
        assert!(matches!(
            next_file(10),
            PlaybackEvent::File { file_id: 1, .. }
        ));
        command(
            json!({"command":["loadfile",db.get_media_file(2).unwrap().absolute_path,"append"]}),
        );
        command(
            json!({"command":["loadfile",db.get_media_file(3).unwrap().absolute_path,"append"]}),
        );
        command(json!({"command":["playlist-next","force"]}));
        assert!(matches!(
            next_file(8),
            PlaybackEvent::File { file_id: 2, .. }
        ));
        let state = db.episode_watch_snapshot(2, false).unwrap();
        assert_eq!(state.watched_episodes, 2);
        assert_eq!(state.latest_watched_file_id, Some(2));
        db.clear_episode_watch(2, false, &state.snapshot).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(
            db.episode_watch_snapshot(2, false)
                .unwrap()
                .watched_episodes,
            0
        );
        command(json!({"command":["playlist-next","force"]}));
        assert!(matches!(
            next_file(8),
            PlaybackEvent::File { file_id: 3, .. }
        ));
        let unknown = root.join("Unindexed.mkv");
        fs::write(&unknown, &wave).unwrap();
        command(json!({"command":["loadfile",unknown.to_string_lossy(),"replace"]}));
        assert!(receive.recv_timeout(Duration::from_millis(500)).is_err());
        assert_eq!(
            db.episode_watch_snapshot(2, false)
                .unwrap()
                .latest_watched_file_id,
            Some(3)
        );
        monitor.stop();
        command(
            json!({"command":["loadfile",db.get_media_file(1).unwrap().absolute_path,"replace"]}),
        );
        assert!(receive.recv_timeout(Duration::from_millis(300)).is_err());
        assert_eq!(
            db.episode_watch_snapshot(2, false)
                .unwrap()
                .latest_watched_file_id,
            Some(3)
        );
        command(json!({"command":["quit"]}));
    }
}
