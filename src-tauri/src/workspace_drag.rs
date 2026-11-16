//! Workbench-only, read-only native file export. The WebView supplies identities,
//! never arbitrary absolute paths. OLE advertises COPY only, including same-drive drops.
use crate::db::AppResult;
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) const MAX_FILES: usize = 100;
const MAX_DROP_BYTES: usize = 1024 * 1024;
const DROPEFFECT_COPY: u32 = 1;
static DRAGGING: AtomicBool = AtomicBool::new(false);

pub(crate) struct Source {
    path: PathBuf,
    // Keep the validated files pinned while OLE/its consumer retains the data object.
    _file: File,
}
impl Source {
    pub(crate) fn checked(path: PathBuf, root: &Path, size: u64, stamp: &str) -> AppResult<Self> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // No FILE_SHARE_WRITE/DELETE: a drag cannot race a replacement or move.
            options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
        }
        let file = options.open(&path).map_err(|_| "WORKSPACE_CHANGED")?;
        crate::comic_reader::validate_file_handle(&file, &path, root, size, stamp)
            .map_err(|_| "WORKSPACE_CHANGED")?;
        Ok(Self { path, _file: file })
    }
}

fn drop_bytes(paths: &[PathBuf]) -> AppResult<Vec<u8>> {
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err("WORKSPACE_LIMIT".into());
    }
    // DROPFILES: pFiles, POINT(x,y), fNC, fWide. Fixed 20-byte Win32 layout.
    let mut bytes = Vec::from(20u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 12]);
    bytes.extend_from_slice(&1u32.to_le_bytes());
    let mut seen = HashSet::new();
    for path in paths {
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
        {
            return Err("WORKSPACE_PATH".into());
        }
        // Shell-compatible spelling is produced only at this Windows boundary.
        let text = crate::db::display_path(path);
        if text.contains('\0') || text.encode_utf16().count() > 32767 {
            return Err("WORKSPACE_PATH".into());
        }
        if !seen.insert(text.replace('/', "\\").to_lowercase()) {
            continue;
        }
        for unit in text.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        if bytes.len() > MAX_DROP_BYTES - 2 {
            return Err("WORKSPACE_LIMIT".into());
        }
    }
    bytes.extend_from_slice(&[0, 0]); // The path list ends in a double UTF-16 NUL.
    Ok(bytes)
}

struct DragGuard;
impl Drop for DragGuard {
    fn drop(&mut self) {
        DRAGGING.store(false, Ordering::Release);
    }
}

pub(crate) async fn start(app: tauri::AppHandle, sources: Vec<Source>) -> AppResult<bool> {
    let bytes = drop_bytes(&sources.iter().map(|s| s.path.clone()).collect::<Vec<_>>())?;
    if DRAGGING.swap(true, Ordering::AcqRel) {
        return Err("WORKSPACE_BUSY".into());
    }
    let guard = DragGuard;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let _guard = guard;
        #[cfg(windows)]
        let result = native::drag(bytes, sources);
        #[cfg(not(windows))]
        let result = {
            let _ = (bytes, sources);
            Err("WORKSPACE_OPEN_FAILED".into())
        };
        let _ = sender.send(result);
    })
    .map_err(|_| "WORKSPACE_OPEN_FAILED")?;
    tauri::async_runtime::spawn_blocking(move || {
        receiver.recv().map_err(|_| "WORKSPACE_OPEN_FAILED")?
    })
    .await
    .map_err(|_| "WORKSPACE_OPEN_FAILED")?
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{ffi::c_void, ptr, sync::atomic::AtomicU32};
    use windows_sys::{
        core::GUID,
        Win32::{System::Com::FORMATETC, UI::Shell::SHCreateStdEnumFmtEtc},
    };
    type Hr = i32;
    const S_OK: Hr = 0;
    const E_NOINTERFACE: Hr = 0x80004002u32 as i32;
    const E_POINTER: Hr = 0x80004003u32 as i32;
    const E_NOTIMPL: Hr = 0x80004001u32 as i32;
    const E_OUTOFMEMORY: Hr = 0x8007000eu32 as i32;
    const DV_E_FORMATETC: Hr = 0x80040064u32 as i32;
    const OLE_E_ADVISENOTSUPPORTED: Hr = 0x80040003u32 as i32;
    const DRAGDROP_S_DROP: Hr = 0x00040100;
    const DRAGDROP_S_CANCEL: Hr = 0x00040101;
    const DRAGDROP_S_USEDEFAULTCURSORS: Hr = 0x00040102;
    const CF_HDROP: u16 = 15;
    const TYMED_HGLOBAL: u32 = 1;
    const DVASPECT_CONTENT: u32 = 1;
    const IID_UNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);
    const IID_DATA: GUID = GUID::from_u128(0x0000010e_0000_0000_c000_000000000046);
    const IID_DROP: GUID = GUID::from_u128(0x00000121_0000_0000_c000_000000000046);

    // STGMEDIUM's union alternatives are all native pointers/handles. Defining
    // its C layout avoids adding windows-sys features or changing the lockfile.
    #[repr(C)]
    struct Medium {
        tymed: u32,
        handle: *mut c_void,
        release: *mut c_void,
    }
    #[link(name = "ole32")]
    extern "system" {
        fn OleInitialize(reserved: *mut c_void) -> Hr;
        fn OleUninitialize();
        fn DoDragDrop(data: *mut c_void, source: *mut c_void, effects: u32, result: *mut u32)
            -> Hr;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalAlloc(flags: u32, size: usize) -> *mut c_void;
        fn GlobalLock(memory: *mut c_void) -> *mut c_void;
        fn GlobalUnlock(memory: *mut c_void) -> i32;
        fn GlobalFree(memory: *mut c_void) -> *mut c_void;
    }
    #[link(name = "user32")]
    extern "system" {
        fn ReleaseCapture() -> i32;
        fn GetAsyncKeyState(key: i32) -> i16;
    }
    fn same_guid(a: &GUID, b: &GUID) -> bool {
        a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
    }
    fn format() -> FORMATETC {
        FORMATETC {
            cfFormat: CF_HDROP,
            ptd: ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT,
            lindex: -1,
            tymed: TYMED_HGLOBAL,
        }
    }
    #[repr(C)]
    struct DataVtable {
        query: unsafe extern "system" fn(*mut Data, *const GUID, *mut *mut c_void) -> Hr,
        add_ref: unsafe extern "system" fn(*mut Data) -> u32,
        release: unsafe extern "system" fn(*mut Data) -> u32,
        get_data: unsafe extern "system" fn(*mut Data, *const FORMATETC, *mut Medium) -> Hr,
        get_data_here: unsafe extern "system" fn(*mut Data, *const FORMATETC, *mut Medium) -> Hr,
        query_get_data: unsafe extern "system" fn(*mut Data, *const FORMATETC) -> Hr,
        canonical: unsafe extern "system" fn(*mut Data, *const FORMATETC, *mut FORMATETC) -> Hr,
        set_data: unsafe extern "system" fn(*mut Data, *const FORMATETC, *const Medium, i32) -> Hr,
        enumerate: unsafe extern "system" fn(*mut Data, u32, *mut *mut c_void) -> Hr,
        advise: unsafe extern "system" fn(
            *mut Data,
            *const FORMATETC,
            u32,
            *mut c_void,
            *mut u32,
        ) -> Hr,
        unadvise: unsafe extern "system" fn(*mut Data, u32) -> Hr,
        enum_advise: unsafe extern "system" fn(*mut Data, *mut *mut c_void) -> Hr,
    }
    #[repr(C)]
    struct Data {
        vtable: &'static DataVtable,
        refs: AtomicU32,
        bytes: Vec<u8>,
        _sources: Vec<Source>,
    }
    unsafe extern "system" fn data_query(
        this: *mut Data,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> Hr {
        if out.is_null() || iid.is_null() {
            return E_POINTER;
        }
        *out = ptr::null_mut();
        if !same_guid(&*iid, &IID_UNKNOWN) && !same_guid(&*iid, &IID_DATA) {
            return E_NOINTERFACE;
        }
        data_add(this);
        *out = this.cast();
        S_OK
    }
    unsafe extern "system" fn data_add(this: *mut Data) -> u32 {
        (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "system" fn data_release(this: *mut Data) -> u32 {
        let left = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
        if left == 0 {
            drop(Box::from_raw(this));
        }
        left
    }
    unsafe extern "system" fn query_get(_: *mut Data, f: *const FORMATETC) -> Hr {
        if f.is_null() {
            return E_POINTER;
        }
        if (*f).cfFormat == CF_HDROP
            && (*f).dwAspect == DVASPECT_CONTENT
            && (*f).lindex == -1
            && (*f).tymed & TYMED_HGLOBAL != 0
            && (*f).ptd.is_null()
        {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }
    unsafe extern "system" fn get_data(
        this: *mut Data,
        f: *const FORMATETC,
        out: *mut Medium,
    ) -> Hr {
        if out.is_null() {
            return E_POINTER;
        }
        *out = Medium {
            tymed: 0,
            handle: ptr::null_mut(),
            release: ptr::null_mut(),
        };
        let result = query_get(this, f);
        if result != S_OK {
            return result;
        }
        let bytes = &(*this).bytes;
        let memory = GlobalAlloc(2, bytes.len()); // GMEM_MOVEABLE
        if memory.is_null() {
            return E_OUTOFMEMORY;
        }
        let destination = GlobalLock(memory);
        if destination.is_null() {
            GlobalFree(memory);
            return E_OUTOFMEMORY;
        }
        ptr::copy_nonoverlapping(bytes.as_ptr(), destination.cast::<u8>(), bytes.len());
        GlobalUnlock(memory);
        *out = Medium {
            tymed: TYMED_HGLOBAL,
            handle: memory,
            release: ptr::null_mut(),
        };
        S_OK // The target now owns this HGLOBAL through ReleaseStgMedium.
    }
    unsafe extern "system" fn get_here(_: *mut Data, _: *const FORMATETC, _: *mut Medium) -> Hr {
        E_NOTIMPL
    }
    unsafe extern "system" fn canonical(
        _: *mut Data,
        _: *const FORMATETC,
        out: *mut FORMATETC,
    ) -> Hr {
        if out.is_null() {
            return E_POINTER;
        }
        (*out).ptd = ptr::null_mut();
        E_NOTIMPL
    }
    unsafe extern "system" fn set_data(
        _: *mut Data,
        _: *const FORMATETC,
        _: *const Medium,
        _: i32,
    ) -> Hr {
        E_NOTIMPL
    }
    unsafe extern "system" fn enumerate(_: *mut Data, direction: u32, out: *mut *mut c_void) -> Hr {
        if out.is_null() {
            return E_POINTER;
        }
        *out = ptr::null_mut();
        if direction != 1 {
            return E_NOTIMPL;
        } // DATADIR_GET only.
        SHCreateStdEnumFmtEtc(1, &format(), out)
    }
    unsafe extern "system" fn advise(
        _: *mut Data,
        _: *const FORMATETC,
        _: u32,
        _: *mut c_void,
        connection: *mut u32,
    ) -> Hr {
        if !connection.is_null() {
            *connection = 0;
        }
        OLE_E_ADVISENOTSUPPORTED
    }
    unsafe extern "system" fn unadvise(_: *mut Data, _: u32) -> Hr {
        OLE_E_ADVISENOTSUPPORTED
    }
    unsafe extern "system" fn enum_advise(_: *mut Data, out: *mut *mut c_void) -> Hr {
        if !out.is_null() {
            *out = ptr::null_mut();
        }
        OLE_E_ADVISENOTSUPPORTED
    }
    static DATA_VTABLE: DataVtable = DataVtable {
        query: data_query,
        add_ref: data_add,
        release: data_release,
        get_data,
        get_data_here: get_here,
        query_get_data: query_get,
        canonical,
        set_data,
        enumerate,
        advise,
        unadvise,
        enum_advise,
    };

    #[repr(C)]
    struct DropVtable {
        query: unsafe extern "system" fn(*mut DropSource, *const GUID, *mut *mut c_void) -> Hr,
        add_ref: unsafe extern "system" fn(*mut DropSource) -> u32,
        release: unsafe extern "system" fn(*mut DropSource) -> u32,
        continue_drag: unsafe extern "system" fn(*mut DropSource, i32, u32) -> Hr,
        feedback: unsafe extern "system" fn(*mut DropSource, u32) -> Hr,
    }
    #[repr(C)]
    struct DropSource {
        vtable: &'static DropVtable,
        refs: AtomicU32,
    }
    unsafe extern "system" fn drop_query(
        this: *mut DropSource,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> Hr {
        if out.is_null() || iid.is_null() {
            return E_POINTER;
        }
        *out = ptr::null_mut();
        if !same_guid(&*iid, &IID_UNKNOWN) && !same_guid(&*iid, &IID_DROP) {
            return E_NOINTERFACE;
        }
        drop_add(this);
        *out = this.cast();
        S_OK
    }
    unsafe extern "system" fn drop_add(this: *mut DropSource) -> u32 {
        (*this).refs.fetch_add(1, Ordering::Relaxed) + 1
    }
    unsafe extern "system" fn drop_release(this: *mut DropSource) -> u32 {
        let left = (*this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
        if left == 0 {
            drop(Box::from_raw(this));
        }
        left
    }
    unsafe extern "system" fn continue_drag(_: *mut DropSource, escape: i32, keys: u32) -> Hr {
        if escape != 0 || keys & 2 != 0 {
            DRAGDROP_S_CANCEL
        } else if keys & 1 == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }
    unsafe extern "system" fn feedback(_: *mut DropSource, _: u32) -> Hr {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
    static DROP_VTABLE: DropVtable = DropVtable {
        query: drop_query,
        add_ref: drop_add,
        release: drop_release,
        continue_drag,
        feedback,
    };
    struct OleGuard;
    impl Drop for OleGuard {
        fn drop(&mut self) {
            unsafe {
                OleUninitialize();
            }
        }
    }

    pub(super) fn drag(bytes: Vec<u8>, sources: Vec<Source>) -> AppResult<bool> {
        // Called exclusively via run_on_main_thread; never from a test or worker.
        unsafe {
            if GetAsyncKeyState(1) >= 0 {
                return Ok(false);
            }
            if OleInitialize(ptr::null_mut()) < 0 {
                return Err("WORKSPACE_OPEN_FAILED".into());
            }
            let _ole = OleGuard;
            ReleaseCapture();
            let data = Box::into_raw(Box::new(Data {
                vtable: &DATA_VTABLE,
                refs: AtomicU32::new(1),
                bytes,
                _sources: sources,
            }));
            let source = Box::into_raw(Box::new(DropSource {
                vtable: &DROP_VTABLE,
                refs: AtomicU32::new(1),
            }));
            let mut effect = 0;
            // This is the sole allowed effect. Do not add MOVE/LINK even for modifiers.
            let result = DoDragDrop(data.cast(), source.cast(), DROPEFFECT_COPY, &mut effect);
            data_release(data);
            drop_release(source);
            if result < 0 {
                Err("WORKSPACE_OPEN_FAILED".into())
            } else {
                Ok(result == DRAGDROP_S_DROP && effect == DROPEFFECT_COPY)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round10_hdrop_is_unicode_deduplicated_bounded_and_copy_only() {
        let paths = [
            PathBuf::from(r"C:\Synthetic\文稿 [01].psd"),
            PathBuf::from(r"C:\Synthetic\shot.png"),
            PathBuf::from(r"C:\Synthetic\SHOT.png"),
        ];
        let bytes = drop_bytes(&paths).unwrap();
        assert_eq!(&bytes[..4], &20u32.to_le_bytes());
        assert_eq!(&bytes[4..16], &[0; 12]);
        assert_eq!(&bytes[16..20], &1u32.to_le_bytes());
        let words: Vec<_> = bytes[20..]
            .chunks_exact(2)
            .map(|x| u16::from_le_bytes([x[0], x[1]]))
            .collect();
        assert_eq!(
            String::from_utf16(&words).unwrap(),
            "C:\\Synthetic\\文稿 [01].psd\0C:\\Synthetic\\shot.png\0\0"
        );
        assert_eq!(DROPEFFECT_COPY, 1);
        assert!(drop_bytes(&[]).is_err());
        assert!(drop_bytes(&vec![paths[0].clone(); MAX_FILES + 1]).is_err());
        assert!(drop_bytes(&[PathBuf::from("relative.psd")]).is_err());
        assert!(drop_bytes(&[PathBuf::from("C:\\Synthetic\\bad\0file")]).is_err());
        assert!(drop_bytes(&[PathBuf::from(r"C:\Synthetic\..\outside.psd")]).is_err());
    }
}
