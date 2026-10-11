use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "windows")]
use std::{marker::PhantomData, os::windows::ffi::OsStrExt, ptr, rc::Rc};

#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::RPC_E_CHANGED_MODE,
    System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
    UI::{
        Shell::{
            Common::ITEMIDLIST, ILClone, ILCreateFromPathW, ILFindLastID, ILFree, ILRemoveLastID,
            SHOpenFolderAndSelectItems, ShellExecuteW,
        },
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};

use crate::{
    db::AppResult,
    models::{AppSettings, PlayerMode, PlayerTestResult},
};

pub fn resolve(settings: &AppSettings) -> AppResult<Option<std::path::PathBuf>> {
    let path = match settings.player_mode {
        PlayerMode::System => return Ok(None),
        PlayerMode::Custom => settings
            .mpv_path
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .map(std::path::PathBuf::from)
            .ok_or("PLAYER_CUSTOM_MISSING")?,
    };
    validate_executable(&path).map_err(|_| "PLAYER_CUSTOM_MISSING".to_string())?;
    Ok(Some(path))
}

pub fn play_selected(executable: Option<&Path>, media: &Path) -> AppResult<()> {
    match executable {
        Some(executable) => play(executable, media),
        None if media.is_absolute() => open_with_default_application(&shell_path(media)),
        None => Err("PLAYER_MEDIA_MISSING".into()),
    }
}

pub fn test(path: &Path) -> AppResult<PlayerTestResult> {
    validate_executable(path)?;
    if !supports_version_probe(path) {
        return Ok(PlayerTestResult {
            ok: true,
            message: "已选择有效的播放器程序；播放时会直接传入视频文件路径。".into(),
            version: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
        });
    }
    let mut command = Command::new(path);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Ok(PlayerTestResult {
                ok: false,
                message: format!("无法启动播放器：{error}"),
                version: None,
            });
        }
    };
    // Drain both pipes concurrently. A noisy or misconfigured player must not deadlock
    // the five-second probe, grow IPC indefinitely or supply a diagnostic as its version.
    let stdout = probe_output(child.stdout.take().expect("piped stdout"));
    let stderr = probe_output(child.stderr.take().expect("piped stderr"));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout
                    .recv_timeout(Duration::from_millis(150))
                    .unwrap_or_default();
                let stderr = stderr
                    .recv_timeout(Duration::from_millis(150))
                    .unwrap_or_default();
                return Ok(probe_result(
                    status.success(),
                    &status.to_string(),
                    &stdout,
                    &stderr,
                ));
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(PlayerTestResult {
                    ok: false,
                    message: "播放器测试超时（5 秒）。".into(),
                    version: None,
                });
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(PlayerTestResult {
                    ok: false,
                    message: format!("检测播放器状态失败：{error}"),
                    version: None,
                });
            }
        }
    }
}

fn probe_output(stream: impl Read + Send + 'static) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (send, receive) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stream.take(8192).read_to_end(&mut bytes);
        let _ = send.send(bytes);
    });
    receive
}

fn probe_result(success: bool, exit: &str, stdout: &[u8], stderr: &[u8]) -> PlayerTestResult {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let config_error = [&stdout, &stderr]
        .iter()
        .any(|text| text.contains("Error parsing option"));
    let version = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| {
            line.len() <= 180
                && ["mpv ", "mpvnet ", "VLC media player "]
                    .iter()
                    .any(|prefix| line.starts_with(prefix))
        })
        .map(str::to_owned);
    let ok = success && !config_error;
    PlayerTestResult {
        ok,
        version,
        message: if ok {
            "播放器可正常启动。".into()
        } else {
            format!(
                "{}\nExit: {exit}\n{}\n{}",
                if config_error {
                    "PLAYER_CONFIG_ERROR"
                } else {
                    "PLAYER_TEST_FAILED"
                },
                stdout.trim(),
                stderr.trim()
            )
        },
    }
}

fn supports_version_probe(path: &Path) -> bool {
    path.file_stem()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "mpv" | "mpvnet" | "vlc"
            )
        })
}

pub fn play(executable: &Path, media: &Path) -> AppResult<()> {
    validate_executable(executable)?;
    if !media.is_file() {
        return Err("视频文件已不存在；请重新扫描资源库。".into());
    }
    if !media.is_absolute() {
        return Err("视频路径不是绝对路径；请重新扫描资源库。".into());
    }
    // Arguments are passed directly to CreateProcess/std::process, never through a shell.
    Command::new(executable)
        .arg(media)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("启动播放器失败：{error}"))?;
    Ok(())
}

pub fn reveal(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Err("路径已不存在；请重新扫描资源库。".into());
    }
    #[cfg(target_os = "windows")]
    {
        let shell_path = shell_path(path);
        let path = shell_path.as_path();
        if path.is_file() {
            reveal_file_in_explorer(path)
        } else {
            Command::new("explorer.exe")
                .arg(path)
                .spawn()
                .map_err(|error| format!("打开资源管理器失败：{error}"))?;
            Ok(())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let target = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map_err(|error| format!("打开文件管理器失败：{error}"))?;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
struct ShellComApartment {
    initialized: bool,
    // COM initialization and its matching release must stay on the same thread.
    _thread_bound: PhantomData<Rc<()>>,
}

#[cfg(target_os = "windows")]
impl ShellComApartment {
    fn new() -> AppResult<Self> {
        let result = unsafe { CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        if result < 0 && result != RPC_E_CHANGED_MODE {
            return Err(format!(
                "Windows Shell 初始化失败（HRESULT 0x{:08X}）。",
                result as u32
            ));
        }
        Ok(Self {
            // S_OK and S_FALSE both acquire a reference. An existing different apartment
            // remains usable, but RPC_E_CHANGED_MODE does not acquire a reference.
            initialized: result >= 0,
            _thread_bound: PhantomData,
        })
    }
}

#[cfg(target_os = "windows")]
impl Drop for ShellComApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(target_os = "windows")]
struct OwnedItemIdList(*mut ITEMIDLIST);

#[cfg(target_os = "windows")]
impl OwnedItemIdList {
    fn from_path(path: &Path) -> AppResult<Self> {
        let wide_path = path_to_wide(&shell_path(path));
        let pidl = unsafe { ILCreateFromPathW(wide_path.as_ptr()) };
        if pidl.is_null() {
            Err("Windows Shell 无法识别该文件路径。".into())
        } else {
            Ok(Self(pidl))
        }
    }

    fn clone_list(&self) -> AppResult<Self> {
        let pidl = unsafe { ILClone(self.0) };
        if pidl.is_null() {
            Err("Windows Shell 无法准备文件位置。".into())
        } else {
            Ok(Self(pidl))
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for OwnedItemIdList {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ILFree(self.0) };
        }
    }
}

#[cfg(target_os = "windows")]
fn path_to_wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Rust canonical paths use verbatim prefixes that Explorer's Shell parser rejects.
/// Convert only disk/UNC prefixes at the Shell boundary, preserving every UTF-16 unit.
#[cfg(target_os = "windows")]
fn shell_path(path: &Path) -> std::path::PathBuf {
    use std::{
        ffi::OsString,
        os::windows::ffi::OsStringExt,
        path::{Component, Prefix},
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::VerbatimDisk(_) => OsString::from_wide(&wide[4..]).into(),
            Prefix::VerbatimUNC(_, _) => {
                let mut normal = vec![b'\\' as u16, b'\\' as u16];
                normal.extend_from_slice(&wide[8..]);
                OsString::from_wide(&normal).into()
            }
            _ => path.to_path_buf(),
        },
        _ => path.to_path_buf(),
    }
}

#[cfg(target_os = "windows")]
fn reveal_file_in_explorer(path: &Path) -> AppResult<()> {
    let _apartment = ShellComApartment::new()?;
    let item = OwnedItemIdList::from_path(path)?;
    let parent = item.clone_list()?;
    let child = unsafe { ILFindLastID(item.0) };
    if child.is_null() {
        return Err("Windows Shell 无法定位该文件。".into());
    }
    if unsafe { ILRemoveLastID(parent.0) } == 0 {
        return Err("Windows Shell 无法确定该文件的父目录。".into());
    }

    let children = [child.cast_const()];
    let result = unsafe { SHOpenFolderAndSelectItems(parent.0, 1, children.as_ptr(), 0) };
    if result < 0 {
        Err(format!(
            "Windows 资源管理器无法选中该文件（HRESULT 0x{:08X}）。",
            result as u32
        ))
    } else {
        Ok(())
    }
}

pub fn open_with_default_application(path: &Path) -> AppResult<()> {
    if !path.is_file() {
        return Err("文件已不存在；请重新扫描资源库。".into());
    }
    open_shell_target(path.as_os_str(), "无法使用系统默认程序打开文件")
}

pub fn open_external_url(url: &str) -> AppResult<()> {
    open_shell_target(std::ffi::OsStr::new(url), "无法使用系统默认浏览器打开链接")
}

#[cfg(target_os = "windows")]
fn open_shell_target(target: &std::ffi::OsStr, error_prefix: &str) -> AppResult<()> {
    let operation = std::ffi::OsStr::new("open")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = target
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // ShellExecuteW receives the path as a literal wide string. No command shell or string
    // interpolation is involved, so Unicode, spaces, brackets and '&' stay intact.
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        Err(format!(
            "{error_prefix}（ShellExecuteW 错误码 {}）。",
            result as isize
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
fn open_shell_target(target: &std::ffi::OsStr, error_prefix: &str) -> AppResult<()> {
    Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map_err(|error| format!("{error_prefix}：{error}"))?;
    Ok(())
}

fn validate_executable(path: &Path) -> AppResult<()> {
    if !path.is_file() {
        return Err("请先在设置中选择有效的播放器可执行文件。".into());
    }
    #[cfg(target_os = "windows")]
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("exe"))
    {
        return Err("播放器路径必须指向 Windows .exe 可执行文件。".into());
    }
    Ok(())
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows_sys::Win32::System::Com::COINIT_MULTITHREADED;

    #[test]
    fn system_default_and_legacy_modes_preserve_explicit_custom_path() {
        let temp = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(temp.path().join("settings.db"));
        db.migrate().unwrap();
        let mut settings = db.get_settings(temp.path()).unwrap();
        assert_eq!(settings.player_mode, PlayerMode::System);
        assert!(resolve(&settings).unwrap().is_none());
        settings.player_mode = PlayerMode::Custom;
        settings.mpv_path = Some(temp.path().join("missing.exe").to_string_lossy().into());
        assert_eq!(resolve(&settings).unwrap_err(), "PLAYER_CUSTOM_MISSING");
        settings.player_mode = PlayerMode::System;
        assert!(resolve(&settings).unwrap().is_none());
        db.update_settings(&settings, temp.path()).unwrap();
        assert_eq!(
            db.get_settings(temp.path()).unwrap().mpv_path,
            settings.mpv_path
        );
        let connection = rusqlite::Connection::open(temp.path().join("settings.db")).unwrap();
        for legacy in ["AUTO", "BUNDLED"] {
            connection
                .execute(
                    "UPDATE settings SET value=?1 WHERE key='player_mode'",
                    [legacy],
                )
                .unwrap();
            let restored = db.get_settings(temp.path()).unwrap();
            assert_eq!(restored.player_mode, PlayerMode::System);
            assert_eq!(restored.mpv_path, settings.mpv_path);
            assert!(resolve(&restored).unwrap().is_none());
            assert_eq!(
                serde_json::from_str::<PlayerMode>(&format!("\"{legacy}\"")).unwrap(),
                PlayerMode::System
            );
        }
        connection
            .execute("DELETE FROM settings WHERE key='player_mode'", [])
            .unwrap();
        assert_eq!(
            db.get_settings(temp.path()).unwrap().player_mode,
            PlayerMode::Custom
        );
    }

    #[test]
    fn shell_com_apartment_balances_nested_initialization() {
        thread::spawn(|| {
            let outer = ShellComApartment::new().unwrap();
            let inner = ShellComApartment::new().unwrap();
            assert!(outer.initialized && inner.initialized);
            drop(inner);
            drop(outer);

            let result = unsafe { CoInitializeEx(ptr::null(), COINIT_MULTITHREADED as u32) };
            assert_eq!(result, 0, "both STA references should have been released");
            unsafe { CoUninitialize() };
        })
        .join()
        .unwrap();
    }

    #[test]
    fn shell_com_apartment_preserves_an_existing_different_apartment() {
        thread::spawn(|| {
            let result = unsafe { CoInitializeEx(ptr::null(), COINIT_MULTITHREADED as u32) };
            assert_eq!(result, 0);
            let shell = ShellComApartment::new().unwrap();
            assert!(!shell.initialized);
            drop(shell);

            let result = unsafe { CoInitializeEx(ptr::null(), COINIT_MULTITHREADED as u32) };
            assert_eq!(result, 1, "the caller's MTA must remain initialized");
            unsafe {
                CoUninitialize();
                CoUninitialize();
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn player_receives_only_the_literal_media_path() {
        let executable = Path::new(r"C:\Players\播放器.exe");
        let media = Path::new(r"C:\媒体 库\日本語\[特典] A&B (01).mkv");
        let mut command = Command::new(executable);
        command.arg(media);
        let arguments = command.get_args().collect::<Vec<_>>();

        assert_eq!(arguments, vec![media.as_os_str()]);
    }

    #[test]
    fn configuration_output_is_not_a_version_or_a_success() {
        let result = probe_result(
            true,
            "0",
            b"Error parsing option screenshot-flags (option not found)\nmpv v0.41.0\n",
            b"",
        );
        assert!(!result.ok);
        assert_eq!(result.version.as_deref(), Some("mpv v0.41.0"));
        assert!(result.message.starts_with("PLAYER_CONFIG_ERROR"));
        assert!(result.message.contains("screenshot-flags"));
        let stderr = probe_result(
            false,
            "1",
            b"",
            b"Error parsing option screenshot-flags (option not found)",
        );
        assert!(stderr.version.is_none());
        assert!(stderr.message.contains("screenshot-flags"));
        let success = probe_result(true, "0", b"startup warning\nmpv v0.41.0\n", b"");
        assert!(success.ok);
        assert_eq!(success.version.as_deref(), Some("mpv v0.41.0"));
    }

    #[test]
    fn probe_capture_is_bounded() {
        let capture = probe_output(std::io::Cursor::new(vec![b'x'; 100_000]));
        assert_eq!(
            capture.recv_timeout(Duration::from_secs(1)).unwrap().len(),
            8192
        );
    }

    #[test]
    fn version_probe_is_limited_to_players_that_support_it() {
        assert!(supports_version_probe(Path::new(r"C:\Players\mpv.exe")));
        assert!(supports_version_probe(Path::new(r"C:\Players\VLC.EXE")));
        assert!(!supports_version_probe(Path::new(
            r"C:\Players\PotPlayerMini64.exe"
        )));
        assert!(!supports_version_probe(Path::new(
            r"C:\Players\mpc-hc64.exe"
        )));
    }

    #[test]
    fn wide_shell_path_preserves_unicode_spaces_and_special_characters() {
        let path = Path::new(r"C:\媒体 库\日本語\[特典] A&B (01).mkv");
        let encoded = path_to_wide(path);

        assert_eq!(encoded.last(), Some(&0));
        assert!(!encoded[..encoded.len() - 1].contains(&0));
        assert_eq!(
            OsString::from_wide(&encoded[..encoded.len() - 1]),
            path.as_os_str()
        );
    }

    #[test]
    fn shell_item_id_lists_accept_a_unicode_special_character_file() {
        let _apartment = ShellComApartment::new().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("媒体 日本語 [A&B]");
        std::fs::create_dir(&nested).unwrap();
        let file = nested.join("第 01 話 (特典).mkv");
        std::fs::write(&file, []).unwrap();

        let item = OwnedItemIdList::from_path(&file).unwrap();
        let canonical = std::fs::canonicalize(&file).unwrap();
        let shell_canonical = shell_path(&canonical);
        assert_eq!(shell_canonical.file_name(), file.file_name());
        // Windows may expand an 8.3 TEMP directory while canonicalizing it.
        assert_eq!(std::fs::canonicalize(&shell_canonical).unwrap(), canonical);
        // This is the path form sent by the checked comic/PDF source reader.
        let canonical_item = OwnedItemIdList::from_path(&canonical).unwrap();
        assert!(!canonical_item.0.is_null());
        let parent = item.clone_list().unwrap();
        let child = unsafe { ILFindLastID(item.0) };

        assert!(!child.is_null());
        assert_ne!(unsafe { ILRemoveLastID(parent.0) }, 0);
    }

    #[test]
    fn shell_paths_convert_verbatim_disk_and_unc_without_changing_names() {
        for (input, expected) in [
            (
                r"\\?\C:\媒体 日本語\[A&B] 第 01 卷.pdf",
                r"C:\媒体 日本語\[A&B] 第 01 卷.pdf",
            ),
            (
                r"\\?\UNC\server\书库\[A&B] 01.pdf",
                r"\\server\书库\[A&B] 01.pdf",
            ),
            (r"C:\媒体\01.pdf", r"C:\媒体\01.pdf"),
            (r"\\server\书库\01.pdf", r"\\server\书库\01.pdf"),
        ] {
            assert_eq!(shell_path(Path::new(input)), Path::new(expected));
        }
    }
}
