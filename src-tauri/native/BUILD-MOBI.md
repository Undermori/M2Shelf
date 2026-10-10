# Rebuilding the replaceable MOBI worker

The source archive is self-contained. Extract it into a new directory, open an
**x64 Native Tools Command Prompt for Visual Studio 2022**, and run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File native/build_mobi_worker.ps1
```

Requirements: Windows x64, Visual Studio C++ MSVC compiler and Windows SDK.
No Kindle, Calibre, XML library, external zlib, Python or Rust is required for
this standalone recipe. The output is `out/M2ShelfMobi.exe`; replace the worker
beside `M2Shelf.exe` while M2Shelf is closed. In the full project, Cargo's
`native/build_mobi.rs` uses the same sources, flags and MSVC toolchain.

Upstream libmobi is pinned at 906274205c11944b628da1c553b255acb1af7c55.
See `vendor/libmobi/COPYING` (LGPL-3.0), `COPYING.GPL3` and individual source
notices. The vendor code is unmodified. The separate M2Shelf integration is
`native/mobi_worker.c`, licensed under LGPL-3.0-or-later as part of this worker.
M2ShelfMobi is independently replaceable and communicates using protocol
`--read-stdio-v1`. Its source book is inherited read-only on stderr; stdin's
`S` gate is released after the parent assigns the Windows memory/process Job.
It writes binary records to stdout and never extracts files or executes book
content. USE_ENCRYPTION is never defined and no encryption source is built.

Supported: unencrypted MOBI6 and KF8 reflowable books, UTF-8/CP1252,
uncompressed/PalmDOC/HuffDic, reconstructed markup and local raster images.
Protected books, dictionaries, Print Replica and newer unrecognized formats
are explicitly rejected. Application-side Root, revision, format, resource and
safe-rendering checks remain necessary when integrating the worker elsewhere.
