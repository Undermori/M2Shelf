# libmobi

Upstream: https://github.com/bfabiszewski/libmobi

Pinned revision: `906274205c11944b628da1c553b255acb1af7c55`.

Copyright Bartek Fabiszewski and the upstream contributors. Licensed under
LGPL-3.0-or-later; see COPYING and the notices in individual source files.
The included miniz implementation retains its upstream permissive licence.

M²Shelf compiles a separate, replaceable `M2ShelfMobi.exe` worker, not a library
linked into M²Shelf. The worker source and its build recipe are provided in
`src-tauri/native/mobi_worker.c`, `build_mobi.rs`, and this directory.
`USE_ENCRYPTION`, XML writing, external libxml2/zlib, and command-line tools are
disabled. The worker accepts a read-only inherited handle and outputs bounded
reconstructed markup/resources. It never extracts files. The application applies
Root/revision checks, resource limits, safe HTML rendering and a Windows Job limit.

This directory preserves upstream code; application-specific integration lives
outside it. No upstream DRM implementation is compiled or called.
