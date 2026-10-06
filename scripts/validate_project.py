#!/usr/bin/env python3
"""Offline, stdlib-only structural acceptance checks for M²Shelf."""

from __future__ import annotations

import base64
import binascii
import json
import hashlib
import re
import sqlite3
import struct
import sys
import tempfile
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ERRORS: list[str] = []
PASSES: list[str] = []


def fail(message: str) -> None:
    ERRORS.append(message)


def passed(message: str) -> None:
    PASSES.append(message)


def read(relative: str) -> str:
    path = ROOT / relative
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        fail(f"cannot read {relative}: {error}")
        return ""


def require_file(relative: str, minimum_bytes: int = 1) -> None:
    path = ROOT / relative
    if not path.is_file():
        fail(f"required file is missing: {relative}")
    elif path.stat().st_size < minimum_bytes:
        fail(f"required file is unexpectedly small: {relative}")
    else:
        passed(f"required file: {relative}")


def check_json_and_toml() -> None:
    json_files = [
        "package.json",
        "tsconfig.json",
        "tsconfig.node.json",
        "src-tauri/tauri.conf.json",
        "src-tauri/capabilities/default.json",
    ]
    for relative in json_files:
        try:
            json.loads(read(relative))
        except json.JSONDecodeError as error:
            fail(f"invalid JSON {relative}: {error}")
        else:
            passed(f"valid JSON: {relative}")

    try:
        cargo = tomllib.loads(read("src-tauri/Cargo.toml"))
    except tomllib.TOMLDecodeError as error:
        fail(f"invalid TOML src-tauri/Cargo.toml: {error}")
        return
    if cargo.get("package", {}).get("name") != "m2shelf":
        fail("Cargo package name must be m2shelf")
    else:
        passed("valid Cargo.toml")
    if cargo.get("package", {}).get("rust-version") != "1.88":
        fail("Cargo rust-version must match the locked dependency MSRV: 1.88")
    else:
        passed("Cargo MSRV matches the locked dependency graph")


def table_names(connection: sqlite3.Connection) -> set[str]:
    return {
        row[0]
        for row in connection.execute(
            "SELECT name FROM sqlite_master WHERE type='table'"
        )
    }


def columns(connection: sqlite3.Connection, table: str) -> set[str]:
    return {row[1] for row in connection.execute(f'PRAGMA table_info("{table}")')}


def check_migrations() -> None:
    error_count_before = len(ERRORS)
    migration_paths = sorted((ROOT / "src-tauri/migrations").glob("*.sql"))
    expected = [
        "0001_initial.sql",
        "0002_mvp.sql",
        "0003_resources_and_cover_status.sql",
        "0004_multilingual_metadata.sql",
        "0005_user_tags.sql",
        "0006_watch_history.sql",
        "0007_favorite_folders.sql",
        "0008_bangumi_subject_type.sql",
        "0009_library_recognition_mode.sql",
        "0010_confirmed_title_aliases.sql",
        "0011_incremental_scan.sql",
        "0012_provider_aliases.sql",
        "0013_alias_sync.sql",
        "0014_scan_health.sql",
    ]
    if [path.name for path in migration_paths] != expected:
        fail(f"expected exactly migrations {expected}, got {[p.name for p in migration_paths]}")
        return

    with tempfile.TemporaryDirectory(prefix="morimediashelf-validate-") as directory:
        db_path = Path(directory) / "validation.sqlite"
        connection = sqlite3.connect(db_path)
        try:
            connection.execute("PRAGMA foreign_keys = ON")
            for path in migration_paths:
                connection.executescript(path.read_text(encoding="utf-8"))

            required_tables = {
                "library_roots",
                "nodes",
                "media_files",
                "metadata_bindings",
                "settings",
                "scan_runs",
                "resource_files",
                "tags",
                "node_tags",
                "watch_history",
                "favorite_folders",
                "node_favorite_folders",
                "confirmed_title_aliases",
                "library_scan_snapshots",
                "library_scan_health",
            }
            missing = required_tables - table_names(connection)
            if missing:
                fail(f"migration schema missing tables: {sorted(missing)}")

            required_columns = {
                "library_roots": {"recognition_mode"},
                "library_scan_health": {
                    "library_root_id", "last_auto_attempt_at", "last_success_at",
                    "outcome", "error_count", "detail",
                },
                "nodes": {
                    "parent_node_id",
                    "node_type",
                    "manual_type_override",
                    "cover_source",
                    "cover_cache_path",
                    "direct_video_count",
                    "child_media_branch_count",
                    "total_video_count",
                },
                "media_files": {
                    "node_id",
                    "absolute_path",
                    "file_name",
                    "extension",
                    "file_size",
                    "modified_at",
                    "last_seen_at",
                },
                "metadata_bindings": {
                    "node_id",
                    "provider",
                    "provider_subject_id",
                    "provider_subject_type",
                    "provider_title",
                    "provider_title_cn",
                    "provider_title_en",
                    "provider_title_ja",
                    "provider_title_ko",
                    "provider_date",
                    "provider_image_url",
                    "cover_download_error",
                },
                "resource_files": {
                    "node_id",
                    "absolute_path",
                    "file_name",
                    "extension",
                    "file_size",
                    "modified_at",
                    "resource_type",
                    "last_seen_at",
                },
                "tags": {"name", "normalized_name", "created_at", "updated_at"},
                "node_tags": {"node_id", "tag_id", "created_at"},
                "watch_history": {"node_id", "last_watched_at", "watch_count"},
                "favorite_folders": {
                    "name",
                    "normalized_name",
                    "created_at",
                    "updated_at",
                },
                "node_favorite_folders": {"folder_id", "node_id", "added_at"},
                "confirmed_title_aliases": {
                    "normalized_alias",
                    "original_alias",
                    "subject_id",
                    "subject_type",
                    "source_node_id",
                    "confirmed_at",
                },
            }
            for table, expected_columns in required_columns.items():
                missing_columns = expected_columns - columns(connection, table)
                if missing_columns:
                    fail(f"{table} missing columns: {sorted(missing_columns)}")

            connection.execute(
                "INSERT INTO library_roots(path, display_name) VALUES (?, ?)",
                (r"D:\动画 [测试]", "动画 [测试]"),
            )
            root_id = connection.execute("SELECT id FROM library_roots").fetchone()[0]
            connection.execute(
                """INSERT INTO nodes(
                    library_root_id, absolute_path, folder_name, display_name,
                    node_type, manual_type_override
                ) VALUES (?, ?, ?, ?, 'WORK', 1)""",
                (root_id, r"D:\动画 [测试]\作品", "作品", "自定义作品名"),
            )
            node_id = connection.execute("SELECT id FROM nodes").fetchone()[0]
            connection.execute(
                """INSERT INTO media_files(
                    node_id, absolute_path, file_name, extension, file_size, modified_at
                ) VALUES (?, ?, ?, ?, ?, ?)""",
                (
                    node_id,
                    r"D:\动画 [测试]\作品\[01] 日本語.mkv",
                    "[01] 日本語.mkv",
                    "mkv",
                    123,
                    "2026-08-13T00:00:00Z",
                ),
            )
            connection.execute(
                """INSERT INTO resource_files(
                    node_id, absolute_path, file_name, extension, file_size,
                    modified_at, resource_type
                ) VALUES (?, ?, ?, ?, ?, ?, 'OTHER')""",
                (
                    node_id,
                    r"D:\动画 [测试]\作品\data.xyzabc",
                    "data.xyzabc",
                    "xyzabc",
                    456,
                    "2026-08-20T00:00:00Z",
                ),
            )
            connection.execute(
                """INSERT INTO metadata_bindings(
                    node_id, provider, provider_subject_id, provider_title,
                    provider_title_cn, provider_title_en, provider_title_ja,
                    provider_title_ko
                ) VALUES (?, 'BANGUMI', 400602, ?, ?, ?, ?, ?)""",
                (
                    node_id,
                    "葬送のフリーレン",
                    "葬送的芙莉莲",
                    "Frieren: Beyond Journey's End",
                    "葬送のフリーレン",
                    "장송의 프리렌",
                ),
            )
            connection.execute(
                "INSERT INTO tags(name, normalized_name) VALUES (?, ?)",
                ("待看", "待看"),
            )
            tag_id = connection.execute("SELECT id FROM tags").fetchone()[0]
            connection.execute(
                "INSERT INTO node_tags(node_id, tag_id) VALUES (?, ?)",
                (node_id, tag_id),
            )
            connection.execute(
                "INSERT INTO watch_history(node_id, last_watched_at, watch_count) VALUES (?, ?, ?)",
                (node_id, "2026-08-22T12:00:00.000Z", 2),
            )
            connection.execute(
                "INSERT INTO favorite_folders(name, normalized_name) VALUES (?, ?)",
                ("收藏", "收藏"),
            )
            favorite_folder_id = connection.execute(
                "SELECT id FROM favorite_folders"
            ).fetchone()[0]
            connection.execute(
                "INSERT INTO node_favorite_folders(folder_id, node_id) VALUES (?, ?)",
                (favorite_folder_id, node_id),
            )
            connection.execute(
                """INSERT INTO confirmed_title_aliases(
                    normalized_alias, original_alias, subject_id, subject_type, source_node_id
                ) VALUES (?, ?, ?, ?, ?)""",
                ("fantranslatedname", "Fan Translated Name", 400602, 2, node_id),
            )
            localized_titles = connection.execute(
                """SELECT provider_title_cn, provider_title_en, provider_title_ja,
                          provider_title_ko, provider_subject_type
                   FROM metadata_bindings WHERE node_id = ?""",
                (node_id,),
            ).fetchone()
            if localized_titles != (
                "葬送的芙莉莲",
                "Frieren: Beyond Journey's End",
                "葬送のフリーレン",
                "장송의 프리렌",
                2,
            ):
                fail("multilingual Bangumi title columns do not round-trip")

            connection.execute(
                "UPDATE metadata_bindings SET provider_subject_type=6 WHERE node_id=?",
                (node_id,),
            )
            try:
                connection.execute(
                    "UPDATE metadata_bindings SET provider_subject_type=4 WHERE node_id=?",
                    (node_id,),
                )
            except sqlite3.IntegrityError:
                pass
            else:
                fail("Bangumi subject type CHECK accepts an unsupported Subject type")

            try:
                connection.execute(
                    """INSERT INTO metadata_bindings(
                        node_id, provider, provider_subject_id, provider_title
                    ) VALUES (?, 'BANGUMI', 1, 'duplicate')""",
                    (node_id,),
                )
            except sqlite3.IntegrityError:
                pass
            else:
                fail("UNIQUE(node_id, provider) is not enforced")

            try:
                connection.execute(
                    """INSERT INTO nodes(
                        library_root_id, absolute_path, folder_name, display_name, node_type
                    ) VALUES (?, ?, 'bad', 'bad', 'INVALID')""",
                    (root_id, r"D:\invalid"),
                )
            except sqlite3.IntegrityError:
                pass
            else:
                fail("nodes.node_type CHECK is not enforced")

            connection.execute("DELETE FROM library_roots WHERE id = ?", (root_id,))
            if connection.execute("SELECT COUNT(*) FROM nodes").fetchone()[0] != 0:
                fail("library root cascade did not remove node index rows")
            if connection.execute("SELECT COUNT(*) FROM media_files").fetchone()[0] != 0:
                fail("node cascade did not remove media index rows")
            if connection.execute("SELECT COUNT(*) FROM metadata_bindings").fetchone()[0] != 0:
                fail("node cascade did not remove binding rows")
            if connection.execute("SELECT COUNT(*) FROM confirmed_title_aliases").fetchone()[0] != 0:
                fail("node cascade did not remove confirmed title aliases")
            if connection.execute("SELECT COUNT(*) FROM resource_files").fetchone()[0] != 0:
                fail("node cascade did not remove resource index rows")
            if connection.execute("SELECT COUNT(*) FROM node_tags").fetchone()[0] != 0:
                fail("node cascade did not remove tag assignments")
            if connection.execute("SELECT COUNT(*) FROM watch_history").fetchone()[0] != 0:
                fail("node cascade did not remove watch history")
            if connection.execute("SELECT COUNT(*) FROM node_favorite_folders").fetchone()[0] != 0:
                fail("node cascade did not remove favorite-folder membership")
            if connection.execute("SELECT COUNT(*) FROM favorite_folders").fetchone()[0] != 1:
                fail("node cascade incorrectly removed the app-owned favorite folder")
        except sqlite3.Error as error:
            fail(f"migration execution/constraint check failed: {error}")
        finally:
            connection.close()

    migration4 = read("src-tauri/migrations/0004_multilingual_metadata.sql")
    required_additive_columns = {
        "provider_title_en",
        "provider_title_ja",
        "provider_title_ko",
    }
    for column in required_additive_columns:
        if not re.search(
            rf"ALTER\s+TABLE\s+metadata_bindings\s+ADD\s+COLUMN\s+{column}\s+TEXT\b",
            migration4,
            re.IGNORECASE,
        ):
            fail(f"migration 0004 does not add {column} to metadata_bindings")
    if re.search(r"\b(?:DROP|DELETE|TRUNCATE)\b", migration4, re.IGNORECASE):
        fail("migration 0004 must remain additive and non-destructive")

    database_source = read("src-tauri/src/db.rs")
    if 'include_str!("../migrations/0004_multilingual_metadata.sql")' not in database_source:
        fail("Rust migration runner does not register 0004_multilingual_metadata.sql")

    migration5 = read("src-tauri/migrations/0005_user_tags.sql")
    if not all(token in migration5 for token in ("CREATE TABLE IF NOT EXISTS tags", "CREATE TABLE IF NOT EXISTS node_tags", "ON DELETE CASCADE")):
        fail("migration 0005 does not define normalized tags and cascading node assignments")
    if re.search(r"\b(?:DROP|TRUNCATE)\b", migration5, re.IGNORECASE):
        fail("migration 0005 must remain additive and non-destructive")
    if 'include_str!("../migrations/0005_user_tags.sql")' not in database_source:
        fail("Rust migration runner does not register 0005_user_tags.sql")

    migration6 = read("src-tauri/migrations/0006_watch_history.sql")
    if not all(token in migration6 for token in ("CREATE TABLE IF NOT EXISTS watch_history", "last_watched_at", "watch_count", "ON DELETE CASCADE")):
        fail("migration 0006 does not define cascading per-Node watch history")
    if re.search(r"\b(?:DROP|TRUNCATE)\b", migration6, re.IGNORECASE):
        fail("migration 0006 must remain additive and non-destructive")
    if 'include_str!("../migrations/0006_watch_history.sql")' not in database_source:
        fail("Rust migration runner does not register 0006_watch_history.sql")

    migration7 = read("src-tauri/migrations/0007_favorite_folders.sql")
    if not all(
        token in migration7
        for token in (
            "CREATE TABLE IF NOT EXISTS favorite_folders",
            "CREATE TABLE IF NOT EXISTS node_favorite_folders",
            "normalized_name TEXT NOT NULL UNIQUE",
            "FOREIGN KEY (folder_id) REFERENCES favorite_folders(id) ON DELETE CASCADE",
            "FOREIGN KEY (node_id) REFERENCES nodes(id) ON DELETE CASCADE",
        )
    ):
        fail("migration 0007 does not define named favorites and cascading Node membership")
    if re.search(r"\b(?:DROP|TRUNCATE)\b", migration7, re.IGNORECASE):
        fail("migration 0007 must remain additive and non-destructive")
    if 'include_str!("../migrations/0007_favorite_folders.sql")' not in database_source:
        fail("Rust migration runner does not register 0007_favorite_folders.sql")

    migration8 = read("src-tauri/migrations/0008_bangumi_subject_type.sql")
    if not all(
        token in migration8
        for token in (
            "ADD COLUMN provider_subject_type",
            "DEFAULT 2",
            "provider_subject_type IN (2, 6)",
        )
    ):
        fail("migration 0008 does not persist only Bangumi animation/live-action subject types")
    if re.search(r"\b(?:DROP|DELETE|TRUNCATE)\b", migration8, re.IGNORECASE):
        fail("migration 0008 must remain additive and non-destructive")
    if 'include_str!("../migrations/0008_bangumi_subject_type.sql")' not in database_source:
        fail("Rust migration runner does not register 0008_bangumi_subject_type.sql")

    migration9 = read("src-tauri/migrations/0009_library_recognition_mode.sql")
    if not all(
        token in migration9
        for token in (
            "ADD COLUMN recognition_mode",
            "DEFAULT 'FOLDER'",
            "recognition_mode IN ('FOLDER', 'VIDEO_FILE')",
        )
    ):
        fail("migration 0009 does not persist the two supported library recognition modes")
    if re.search(r"\b(?:DROP|DELETE|TRUNCATE)\b", migration9, re.IGNORECASE):
        fail("migration 0009 must remain additive and non-destructive")
    if 'include_str!("../migrations/0009_library_recognition_mode.sql")' not in database_source:
        fail("Rust migration runner does not register 0009_library_recognition_mode.sql")

    # Exercise a real v3 -> v9 upgrade with existing curation data, not only a fresh schema.
    legacy = sqlite3.connect(":memory:")
    try:
        legacy.execute("PRAGMA foreign_keys = ON")
        for path in migration_paths[:3]:
            legacy.executescript(path.read_text(encoding="utf-8"))
        legacy.execute(
            "INSERT INTO library_roots(path, display_name) VALUES (?, ?)",
            (r"D:\Legacy", "Legacy"),
        )
        legacy_root = legacy.execute("SELECT id FROM library_roots").fetchone()[0]
        legacy.execute(
            """INSERT INTO nodes(
                library_root_id, absolute_path, folder_name, display_name,
                node_type, manual_type_override
            ) VALUES (?, ?, ?, ?, 'WORK', 1)""",
            (legacy_root, r"D:\Legacy\Work", "Work", "Curated title"),
        )
        legacy_node = legacy.execute("SELECT id FROM nodes").fetchone()[0]
        legacy.execute(
            """INSERT INTO metadata_bindings(
                node_id, provider, provider_subject_id, provider_title, provider_title_cn
            ) VALUES (?, 'BANGUMI', 400602, ?, ?)""",
            (legacy_node, "Sousou no Frieren", "葬送的芙莉莲"),
        )
        legacy.executescript(migration4)
        upgraded = legacy.execute(
            """SELECT n.display_name, n.manual_type_override,
                      b.provider_subject_id, b.provider_title, b.provider_title_cn,
                      b.provider_title_en, b.provider_title_ja, b.provider_title_ko
               FROM nodes n JOIN metadata_bindings b ON b.node_id = n.id"""
        ).fetchone()
        if upgraded != (
            "Curated title",
            1,
            400602,
            "Sousou no Frieren",
            "葬送的芙莉莲",
            None,
            None,
            None,
        ):
            fail("migration 0004 does not preserve existing names, overrides, or bindings")
        legacy.executescript(migration5)
        legacy.execute(
            "INSERT INTO tags(name, normalized_name) VALUES ('收藏', '收藏')"
        )
        legacy.execute(
            "INSERT INTO node_tags(node_id, tag_id) VALUES (?, 1)",
            (legacy_node,),
        )
        legacy.executescript(migration6)
        legacy.execute(
            "INSERT INTO watch_history(node_id, last_watched_at) VALUES (?, ?)",
            (legacy_node, "2026-08-22T12:00:00.000Z"),
        )
        legacy.executescript(migration7)
        legacy.execute(
            "INSERT INTO favorite_folders(name, normalized_name) VALUES ('收藏', '收藏')"
        )
        legacy.execute(
            "INSERT INTO node_favorite_folders(folder_id, node_id) VALUES (1, ?)",
            (legacy_node,),
        )
        legacy.executescript(migration8)
        legacy.executescript(migration9)
        preserved_after_v8 = legacy.execute(
            """SELECT n.display_name, n.manual_type_override, b.provider_subject_id,
                      b.provider_subject_type,
                      (SELECT COUNT(*) FROM node_tags WHERE node_id=n.id),
                      (SELECT COUNT(*) FROM watch_history WHERE node_id=n.id),
                      (SELECT COUNT(*) FROM node_favorite_folders WHERE node_id=n.id)
               FROM nodes n JOIN metadata_bindings b ON b.node_id=n.id"""
        ).fetchone()
        if preserved_after_v8 != ("Curated title", 1, 400602, 2, 1, 1, 1):
            fail("migrations 0005-0009 do not preserve existing curation and bindings")
        recognition_mode = legacy.execute(
            "SELECT recognition_mode FROM library_roots WHERE id=?", (legacy_root,)
        ).fetchone()[0]
        if recognition_mode != "FOLDER":
            fail("migration 0009 does not preserve old roots with FOLDER recognition")
    except sqlite3.Error as error:
        fail(f"v3 to v9 migration compatibility check failed: {error}")
    finally:
        legacy.close()

    if len(ERRORS) == error_count_before:
        migration_versions_label = ", ".join(str(int(path.name.split("_")[0])) for path in migration_paths)
        passed(f"SQLite migrations {migration_versions_label}, incremental snapshots, library recognition modes, multilingual titles, Bangumi subject types, confirmed aliases, user tags, watch history, favorites, upgrade preservation, and constraints")


def extract_rust_commands() -> tuple[set[str], set[str]]:
    command_source = read("src-tauri/src/commands.rs")
    definitions = set(
        re.findall(
            r"#\[tauri::command(?:\([^\]]*\))?\]\s*pub\s+(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
            command_source,
        )
    )
    lib_source = read("src-tauri/src/lib.rs")
    handler_match = re.search(
        r"tauri::generate_handler!\s*\[(.*?)\]", lib_source, re.DOTALL
    )
    if not handler_match:
        fail("src-tauri/src/lib.rs has no generate_handler! command list")
        return definitions, set()
    registered = set(
        re.findall(r"commands::([A-Za-z_][A-Za-z0-9_]*)", handler_match.group(1))
    )
    return definitions, registered


def check_command_contract() -> None:
    definitions, registered = extract_rust_commands()
    frontend = set(
        re.findall(
            r"call(?:<[^;\n]+?>)?\(\s*[\"']([a-z][a-z0-9_]*)[\"']",
            read("src/lib/api.ts"),
        )
    )
    if not definitions:
        fail("no #[tauri::command] definitions found")
    if definitions - registered:
        fail(f"Rust commands not registered: {sorted(definitions - registered)}")
    if registered - definitions:
        fail(f"registered commands have no annotated definition: {sorted(registered - definitions)}")
    if frontend - registered:
        fail(f"frontend invokes unregistered commands: {sorted(frontend - registered)}")
    if registered - frontend:
        fail(f"registered commands have no frontend wrapper: {sorted(registered - frontend)}")
    if definitions and definitions == registered == frontend:
        passed(f"frontend/Rust command contract ({len(definitions)} commands)")


def strip_rust_tests(source: str) -> str:
    # An inner cfg(test) makes every item in a standalone fixture module test-only.
    if re.search(r"(?m)^#!\[cfg\(test\)\]$", source):
        return ""
    # A few modules expose small test-only helpers before later production functions.
    # Stop only at the actual trailing test module; stopping at the first cfg(test)
    # would silently exclude production updater/scanner code from safety checks.
    marker = re.search(
        r"#\[cfg\([^\]]*\btest\b[^\]]*\)\]\s*mod\s+tests\s*\{", source
    )
    return source if marker is None else source[: marker.start()]


def check_source_safety() -> None:
    rust_dir = ROOT / "src-tauri/src"
    sources = {
        path.name: path.read_text(encoding="utf-8") for path in rust_dir.glob("*.rs")
    }
    # Rust can gate a file at its declaration in lib.rs instead of duplicating
    # an inner cfg(test). Only exclude modules whose declarations are all gated.
    test_declaration = re.compile(r"#\[cfg\(test\)\]\s*mod\s+(\w+)\s*;")
    test_modules = {
        name for source in sources.values() for name in test_declaration.findall(source)
    }
    remaining_sources = "\n".join(
        test_declaration.sub("", source) for source in sources.values()
    )
    test_only_files = {
        f"{name}.rs"
        for name in test_modules
        if not re.search(rf"\bmod\s+{re.escape(name)}\s*;", remaining_sources)
    }
    production = {
        name: strip_rust_tests(source)
        for name, source in sources.items()
        if name not in test_only_files
    }

    forbidden_shell = re.compile(
        r"(?:Command::new\(\s*[\"'](?:cmd(?:\.exe)?|powershell(?:\.exe)?)[\"']\s*\)|"
        r"[\"']/(?:c|C)[\"']|[\"']-Command[\"'])"
    )
    for name, source in production.items():
        if forbidden_shell.search(source):
            fail(f"shell-mediated process execution found in production Rust: {name}")

    destructive = re.compile(
        r"(?:fs|std::fs|tokio::fs)::(?:remove_file|remove_dir|remove_dir_all|rename|write|copy|create|OpenOptions)\b"
    )
    filesystem_mutation_modules = {
        "cache.rs",
        "bangumi.rs",
        "lib.rs",
        # Update downloads are confined to the application-owned update cache.
        "update.rs",
        # Portable replacement is performed by the native helper after validating a
        # Rust-owned, path-bound request.  The structural checks below keep these two
        # exceptions from becoming a general source-media mutation escape hatch.
        "portable_update.rs",
    }
    for name, source in production.items():
        hits = destructive.findall(source)
        if not hits:
            continue
        if name not in filesystem_mutation_modules:
            fail(f"filesystem mutation outside app cache/bootstrap modules: {name}: {hits}")

    cache_source = production.get("cache.rs", "")
    if "remove_file" in cache_source:
        remove_start = cache_source.find("pub fn remove_cached_file")
        remove_end = cache_source.find("pub fn clear_cover_cache", remove_start)
        owned_start = cache_source.find("fn is_owned_cache_file")
        owned_end = cache_source.find("fn is_same_path", owned_start)
        remove_region = cache_source[remove_start:remove_end]
        owned_region = cache_source[owned_start:owned_end]
        if (
            "is_owned_cache_file(path, cache_root)" not in remove_region
            or "is_equal_or_within(path, cache_root)" not in owned_region
            or 'cache_root.join("bangumi")' not in owned_region
            or 'cache_root.join("manual")' not in owned_region
        ):
            fail("cache deletion lacks cache-root containment and owned-filename guards")
        if (
            "CACHE_MARKER_NAME" not in cache_source
            or "ensure_existing_custom_cache(cache_root)?" not in cache_source
        ):
            fail("custom cache cleanup lacks an application marker guard")

    bangumi_source = production.get("bangumi.rs", "")
    commands_source = production.get("commands.rs", "")
    auto_match_source = production.get("auto_match.rs", "")
    download_start = bangumi_source.find("pub fn download_cover")
    download_end = bangumi_source.find("fn download_response_with_retry", download_start)
    download_region = bangumi_source[download_start:download_end]
    clear_start = commands_source.find("pub fn clear_cover_cache")
    clear_end = commands_source.find("fn start_scan_internal", clear_start)
    clear_region = commands_source[clear_start:clear_end]
    cache_coordination_ok = all(
        (
            "static COVER_CACHE_CLEAR_BARRIER: RwLock<()>" in cache_source,
            "pub(crate) fn begin_cover_cache_operation" in cache_source,
            "pub(crate) fn begin_cover_cache_clear" in cache_source,
            "_cache_clear: &CoverCacheClearGuard" in cache_source,
            "_cache_operation: &CoverCacheOperationGuard" in cache_source,
            "ensure_no_active_scan(&state)?" in clear_region,
            "cache::begin_cover_cache_clear()" in clear_region,
            "cache::begin_cover_cache_operation()" in auto_match_source,
            "cache::create_pending_cache_file(&destination, true)" in download_region,
            "pending_file" in download_region and ".commit_to(&destination)" in download_region,
            'with_extension("download")' not in download_region,
            "destination.exists()" not in download_region,
        )
    )
    if not cache_coordination_ok:
        fail("cover cache clear/download/read coordination or atomic replacement contract is missing")
    else:
        passed("cover cache clear barrier and same-directory atomic cover replacement")

    update_source = production.get("update.rs", "")
    app_bootstrap_source = production.get("lib.rs", "")
    update_cache_mutation_ok = all(
        (
            'let update_cache_dir = app_data_dir.join("updates");'
            in app_bootstrap_source,
            "update::UpdateManager::new(update_cache_dir)" in app_bootstrap_source,
            "portable_update::cleanup_completed_transactions(" in app_bootstrap_source,
            "&update_cache_dir," in app_bootstrap_source,
            "cache_dir: PathBuf" in update_source,
            "let version_dir = ensure_safe_update_subdirectory(" in update_source,
            "fn ensure_plain_update_directory(path: &Path, create: bool)" in update_source,
            "fs::create_dir(path)" in update_source,
            "Uuid::new_v4()" in update_source,
            '".{}.{}.partial"' in update_source,
            "cleanup_owned_partial_downloads(&version_dir, &checked.asset.file_name)"
            in update_source,
            "downloaded > checked.asset.size || downloaded > MAX_ARTIFACT_BYTES"
            in update_source,
            "downloaded != checked.asset.size" in update_source,
            "output" in update_source and ".sync_all()" in update_source,
            "verify_asset_digest_and_signature(" in update_source,
            "fs::rename(&partial, &destination)" in update_source,
            "entries.flatten().take(64)" in update_source,
            "canonical_version(&name).is_err()" in update_source,
            "!is_plain_directory(version_dir)" in update_source,
            "remove_owned_version_cache_files(&path, &name)" in update_source,
            "fs::symlink_metadata(path)" in update_source,
            "FILE_ATTRIBUTE_REPARSE_POINT" in update_source,
            "accept_missing_manifest_for_non_newer_release(response.url(), current_version)"
            in update_source,
            'const PREFIX: &str = "/Undermori/M2Shelf/releases/download/v"'
            in update_source,
            "pub(crate) fn begin_operation" in update_source,
            "state.update_manager.begin_operation()?" in commands_source,
            "state.update_manager.downloaded(&version)?" in commands_source,
            "ensure_no_active_scan(&state)?" in commands_source,
            "crate::update::lock_and_verify_file_against_manifest(" in commands_source,
            "UpdateDistribution::Portable => {" in commands_source,
            "crate::portable_update::prepare_portable_update(" in commands_source,
            "std::mem::forget(prepared);" in commands_source,
            "UpdateDistribution::Nsis => {" in commands_source,
            "std::process::Command::new(&downloaded.path)" in commands_source,
        )
    )
    if not update_cache_mutation_ok:
        fail("update cache mutation lacks bounded size, app-cache path, serialization, or safe cleanup guards")
    else:
        passed("Rust-owned update cache uses bounded verified writes and guarded cleanup")

    portable_update_source = production.get("portable_update.rs", "")
    authentication_position = app_bootstrap_source.find(
        "portable_update::authenticate_update_transaction_before_mutex(transaction_id)"
    )
    update_mutex_position = app_bootstrap_source.find(
        "portable_update::wait_for_update_mutex_before_startup"
    )
    single_instance_position = app_bootstrap_source.find("single_instance::acquire()")
    authenticated_child_before_mutex_ok = (
        -1 < authentication_position < update_mutex_position < single_instance_position
        and "fn authenticate_update_transaction(" in portable_update_source
        and "state.phase != ApplyPhase::Launched" in portable_update_source
        and "state.transaction_id != transaction_id" in portable_update_source
        and "request.helper_ready_path" in portable_update_source
        and "current_executable" in portable_update_source
    )
    recovery_persistence_region = portable_update_source[
        portable_update_source.find("fn persist_recovery_outcome(") :
        portable_update_source.find("fn append_recovery_persistence_error(")
    ]
    recovery_notice_before_terminal_state_ok = (
        recovery_persistence_region.find("write_rollback_notice(request, outcome)?")
        < recovery_persistence_region.find("write_transaction_state(request, phase")
        and recovery_persistence_region.find("write_rollback_notice(request, outcome)?") >= 0
    )
    portable_install_mutation_ok = all(
        (
            "database.backup_for_portable_update(&database_backup_path)?"
            in portable_update_source,
            "_database_update_barrier: DatabaseUpdateBarrier" in portable_update_source,
            "acquire_update_mutex()?" in portable_update_source,
            "mark_interrupted_transactions_recovery_required" in portable_update_source,
            "reject_library_root_overlap(" in portable_update_source,
            "cache::paths_overlap_checked(install_directory, &root)" in portable_update_source,
            "OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX"
            in portable_update_source,
            "fn validate_request_structure" in portable_update_source,
            '.join("updates")' in portable_update_source,
            '.join("transactions")' in portable_update_source,
            "let expected_archive = database_parent" in portable_update_source,
            '"M2Shelf-Portable-{}-x64.zip"' in portable_update_source,
            "request.archive_path != expected_archive" in portable_update_source,
            '".m2shelf-update-staging-{}"' in portable_update_source,
            '".m2shelf-update-backup-{}"' in portable_update_source,
            "fn ensure_plain_directory" in portable_update_source,
            "FILE_ATTRIBUTE_REPARSE_POINT" in portable_update_source,
            "fn seal_verified_archive" in portable_update_source,
            portable_update_source.count("verify_open_file_against_manifest(") >= 2,
            "MAX_ARCHIVE_FILES" in portable_update_source,
            "MAX_EXTRACTED_BYTES" in portable_update_source,
            "MAX_ENTRY_BYTES" in portable_update_source,
            "validate_archive_name(&raw_name)?" in portable_update_source,
            "entry.is_dir()" in portable_update_source,
            "entry.unix_mode()" in portable_update_source,
            "Portable 更新 ZIP 包含重复文件名" in portable_update_source,
            "ReplaceFileW" in portable_update_source,
            "MoveFileExW" in portable_update_source,
            "REPLACEFILE_WRITE_THROUGH" in portable_update_source,
            "MOVEFILE_WRITE_THROUGH" in portable_update_source,
            "fn rollback_files" in portable_update_source,
            "fn restore_database" in portable_update_source,
            "fn wait_for_health" in portable_update_source,
            "ApplyPhase::Completed" in portable_update_source,
            "validate_request_identity(request_path, request)?" in portable_update_source,
            "cleanup_pre_ready_transaction" in portable_update_source,
            "PreparedTransactionCleanup" in portable_update_source,
            authenticated_child_before_mutex_ok,
            recovery_notice_before_terminal_state_ok,
            "entries.take(64)" in portable_update_source,
        )
    )
    if not portable_install_mutation_ok:
        fail("Portable update mutation lacks request containment, bounded extraction, atomic replacement, or rollback guards")
    else:
        passed("Rust-owned Portable installer is path-bound, bounded, atomic, and rollback-aware")

    public_mutation_names = re.findall(
        r"#\[tauri::command(?:\([^\]]*\))?\]\s*pub\s+(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)",
        production.get("commands.rs", ""),
    )
    # These commands mutate only M²Shelf-owned SQLite rows, never source-media paths.
    app_database_only_commands = {
        "remove_library_root",
        "rename_user_tag",
        "delete_user_tag",
        "rename_favorite_folder",
        "delete_favorite_folder",
        "batch_remove_nodes_from_favorite",
    }
    dangerous_names = [
        name
        for name in public_mutation_names
        if name not in app_database_only_commands
        if re.search(r"(?:delete|rename|move|remove_media|remove_file|write_media)", name)
    ]
    if dangerous_names:
        fail(f"dangerous source-media command names exposed: {dangerous_names}")

    player = production.get("player.rs", "")
    if "Command::new(executable)" not in player or ".arg(media)" not in player:
        fail("player must use a validated executable and a separate literal media argument")
    if '.arg("--")' in player:
        fail("generic player launch must not pass the mpv-specific -- sentinel")
    if "validate_executable(executable)" not in player:
        fail("mpv executable is not validated before play")

    scanner = production.get("scanner.rs", "")
    scanner_with_tests = read("src-tauri/src/scanner.rs")
    mutating_tokens = ["remove_file", "remove_dir", "rename(", "fs::write", "fs::copy"]
    scanner_hits = [token for token in mutating_tokens if token in scanner]
    if scanner_hits:
        fail(f"scanner production code mutates filesystem: {scanner_hits}")

    bdmv_root_boundary_ok = all(
        (
            "refresh_ancestors(&connection, Some(scanned_node_id), &canonical_root)?" in scanner_with_tests,
            "has_typical_bdmv(Path::new(&path), canonical_root)" in scanner_with_tests,
            "pub fn has_typical_bdmv(path: &Path, library_root: &Path)" in scanner_with_tests,
            "fs::canonicalize(library_root)" in scanner_with_tests,
            "canonicalize_within_library_root(path, &canonical_root)" in scanner_with_tests,
            "fn find_stream_directory(bdmv: &Path, canonical_root: &Path)" in scanner_with_tests,
            "file_type.is_symlink()" in scanner_with_tests,
            "bdmv_detection_never_reads_a_complete_structure_outside_the_library_root" in scanner_with_tests,
        )
    )
    if not bdmv_root_boundary_ok:
        fail("scanner production BDMV probing can escape the canonical Library Root boundary")
    else:
        passed("BDMV probing canonicalizes every enumerated directory inside its Library Root")

    database = production.get("db.rs", "")
    database_with_tests = read("src-tauri/src/db.rs")
    commands = production.get("commands.rs", "")
    root_overlap_guard = all(
        (
            "canonical_library_root(path)?" in database,
            "ensure_root_does_not_overlap_conn(&transaction, &canonical, None)?" in database,
            "pub fn validate_new_root_path" in database,
            "pub fn validate_scan_root" in database,
            "validate_new_root_path(Path::new(&path))?" in commands,
            "state.database.validate_scan_root(root)?" in commands,
            "database.validate_scan_root(&target.root)" in scanner_with_tests,
            "library_roots_reject_equal_ancestor_and_descendant_paths" in database_with_tests,
            "scan_guard_rejects_legacy_overlapping_root_rows" in database_with_tests,
        )
    )
    if not root_overlap_guard:
        fail("Library Root overlap protection is missing from command, transaction, or scan boundary")

    if not any(
        message.startswith(
            (
                "filesystem mutation",
                "dangerous",
                "scanner production",
                "shell-mediated",
                "cache deletion",
                "update cache mutation",
                "Portable update mutation",
                "mpv",
                "Library Root",
            )
        )
        for message in ERRORS
    ):
        passed("source-media read-only, non-overlapping roots, and literal mpv launch safety")


def check_extensions_and_product_spec() -> None:
    spec = read("docs/PRODUCT_SPEC.md")
    required_contracts = (
        "M²Shelf",
        "源文件只读",
        "附件",
        "Container",
        "直接门槛",
        "MVP 验收标准",
    )
    if len(spec.encode("utf-8")) < 5_000 or any(token not in spec for token in required_contracts):
        fail("docs/PRODUCT_SPEC.md is incomplete or not the expected PRD")
    else:
        passed("complete product specification")

    combined = read("src-tauri/src/scanner.rs") + read("src-tauri/src/db.rs")
    required = {"mkv", "mp4", "m4v", "avi", "mov", "webm", "ts", "m2ts"}
    missing = {extension for extension in required if f'"{extension}"' not in combined}
    if missing:
        fail(f"required media extensions not found in implementation: {sorted(missing)}")
    else:
        passed("required video extensions")


def check_tauri_security_configuration() -> None:
    try:
        capability = json.loads(read("src-tauri/capabilities/default.json"))
        config = json.loads(read("src-tauri/tauri.conf.json"))
    except json.JSONDecodeError:
        return
    permissions = capability.get("permissions", [])
    serialized = json.dumps(permissions)
    if "dialog:allow-open" not in serialized and "dialog:default" not in serialized:
        fail("main capability lacks dialog open permission")
    if "dialog:default" in serialized:
        fail("dialog:default is broader than MVP needs; use dialog:allow-open")
    broad = [token for token in ("shell:allow-spawn", "process:default", "fs:allow-remove", "fs:allow-rename") if token in serialized]
    if broad:
        fail(f"unnecessary broad WebView permissions: {broad}")

    security = config.get("app", {}).get("security", {})
    asset = security.get("assetProtocol", {})
    if asset.get("enable") is not True:
        fail("Tauri asset protocol is not enabled for cached covers")
    scope = asset.get("scope", [])
    allowed_patterns = scope if isinstance(scope, list) else scope.get("allow", []) if isinstance(scope, dict) else []
    scope_text = json.dumps(allowed_patterns)
    if not any(token in scope_text for token in ("$APPDATA", "$APPCACHE")):
        fail("asset protocol scope is not restricted to an application-owned directory")
    if "covers" not in scope_text:
        fail("asset protocol scope must be narrowed to the cover cache, not all app data")
    if any(
        pattern in {"**", "**/*"} or pattern.startswith("$HOME")
        for pattern in allowed_patterns
        if isinstance(pattern, str)
    ):
        fail("asset protocol scope is too broad")

    csp = security.get("csp", "")
    if "asset:" not in csp or "http://asset.localhost" not in csp:
        fail("CSP does not allow Tauri cached-cover asset URLs")
    if not any(message.startswith(("main capability", "unnecessary broad", "Tauri asset", "asset protocol", "CSP")) for message in ERRORS):
        passed("Tauri capability, CSP, and asset scope")


def check_bangumi_contract() -> None:
    source = read("src-tauri/src/bangumi.rs")
    auto_match = read("src-tauri/src/auto_match.rs")
    extractor = read("src-tauri/src/title_extractor.rs")
    models = read("src-tauri/src/models.rs")
    commands = read("src-tauri/src/commands.rs")
    db = read("src-tauri/src/db.rs")
    required_request_fragments = [
        '"keyword": keyword',
        '"filter": { "type": SUPPORTED_SUBJECT_TYPES',
        '"sort": "match"',
        '"nsfw": false',
    ]
    missing = [fragment for fragment in required_request_fragments if fragment not in source]
    if missing:
        fail(f"Bangumi search request is missing contract fields: {missing}")

    if "USER_AGENT" not in source or "user_agent(USER_AGENT)" not in source:
        fail("Bangumi client does not set an explicit User-Agent")
    if "Undermori" not in source or "space.bilibili.com/2903441" not in source:
        fail("Bangumi User-Agent lacks the maintainer identity/project link")

    url_guard = (
        "lain.bgm.tv" in source
        and ("host_str" in source or "url.host" in source or "Url::parse" in source)
    )
    if not url_guard:
        fail("Bangumi cover URL is not restricted to the official lain.bgm.tv host")
    if "content-type" not in source.lower() and "CONTENT_TYPE" not in source:
        fail("Bangumi cover download does not validate an image Content-Type")
    if not re.search(r"MAX_[A-Z_]*(?:COVER|IMAGE|DOWNLOAD)[A-Z_]*", source):
        fail("Bangumi cover download has no explicit byte-size limit")

    confidence_match_contract = all(
        token in auto_match
        for token in (
            "const MAX_QUERIES_PER_NODE: usize = 3",
            "const MAX_CANDIDATES_PER_NODE: usize = 30",
            "const MAX_DETAIL_ENRICHMENTS: usize = 5",
            "pub struct MatchWeights",
            "automatic_threshold: 82",
            "direct_threshold: 60",
            "MatchConfidence::Direct",
            "candidate.primary_query_rank == Some(0)",
            "best.strong_conflicts.is_empty()",
            "save_binding_if_absent",
            "save_rematched_binding_if_unchanged",
            "set_bangumi_cover_for_subject_unless_manual",
        )
    ) and all(
        token in extractor
        for token in (
            "pub struct MatchEvidence",
            "pub parent_title: Option<String>",
            "pub frequent_file_title: Option<String>",
            "pub season_number: Option<u16>",
            "pub edition_kind: EditionKind",
            ".nfkc()",
        )
    ) and all(
        token not in auto_match
        for token in (
            "MatchConfidence::Pending",
            "minimum_margin",
            "pending_threshold",
            "select_first_anime_candidate",
        )
    )
    if not confidence_match_contract:
        fail("Bangumi automatic binding lacks the bounded structured-confidence and mutation-guard contract")

    alias_match_contract = all(
        (
            "pub match_aliases: Vec<String>" in models,
            "const MAX_MATCH_ALIASES: usize = 32" in source,
            "extract_match_aliases(&subject.infobox)" in source,
            "extract_match_aliases(&detail.infobox)" in source,
            "subject.match_aliases.iter().map(String::as_str)" in auto_match,
            "exact_alternate: 50" in auto_match,
            "official_match_alias_participates_in_exact_title_scoring" in auto_match,
            "live_structured_match_accepts_an_official_romanized_alias" in auto_match,
            "recall_confirmed_alias" in auto_match,
            "resolve_confirmed_title_alias" in auto_match,
            "save_confirmed_binding_with_aliases" in commands,
            "confirmed_alias_candidates" in extractor,
            "static HTTP_CLIENT: OnceLock<Client>" in source,
        )
    )
    if not alias_match_contract:
        fail("Bangumi automatic matching does not preserve bounded official aliases or reuse its HTTP client")

    validation_region = commands[
        commands.find("fn validate_bindable_bangumi_subject") : commands.find("fn library_root_paths")
    ]
    bind_region = commands[
        commands.find("pub async fn bind_bangumi") : commands.find("pub fn clear_bangumi_binding")
    ]
    save_region = db[db.find("pub fn save_binding") : db.find("pub fn set_node_cover")]
    combined = validation_region + bind_region + save_region
    supported_types = all(
        token in source
        for token in (
            "pub const SUBJECT_TYPE_ANIME: i64 = 2",
            "pub const SUBJECT_TYPE_LIVE_ACTION: i64 = 6",
            "is_supported_subject_type",
        )
    )
    if (
        not supported_types
        or "validate_bindable_bangumi_subject(&subject)?" not in bind_region
        or "is_supported_subject_type(subject.subject_type)" not in validation_region
    ):
        fail("bind_bangumi does not restrict Rust binding to Bangumi animation/live-action subjects")
    if "subject_id" not in combined or not re.search(r"subject(?:\.|_)id\s*(?:<=|<|==)\s*0", combined):
        fail("bind_bangumi does not reject a non-positive subject ID in Rust")

    prefixes = (
        "Bangumi search request",
        "Bangumi client",
        "Bangumi User-Agent",
        "Bangumi cover",
        "bind_bangumi",
        "Bangumi automatic",
    )
    if not any(message.startswith(prefixes) for message in ERRORS):
        passed("Bangumi request, binding, and cover-download trust boundary")


def check_phase2_contract() -> None:
    scanner = read("src-tauri/src/scanner.rs")
    database = read("src-tauri/src/db.rs")
    commands = read("src-tauri/src/commands.rs")
    extractor = read("src-tauri/src/title_extractor.rs")
    models = read("src-tauri/src/models.rs")
    frontend_format = read("src/lib/format.ts")

    required_fragments = {
        "resource_files scanner index": (scanner, "INSERT INTO resource_files"),
        "unknown resource fallback": (scanner, "_ => ResourceType::Other"),
        "cross-root collection": (database, "list_all_resources"),
        "project-count media filter": (database, "n.total_video_count > 0"),
        "cover failure persistence": (database, "cover_download_error"),
        "cover retry command": (commands, "retry_bangumi_cover"),
        "default resource open": (commands, "open_resource_file"),
        "cover cache directory open": (commands, "open_cover_cache_directory"),
        "root display-name update": (commands, "update_library_root_name"),
        "resource response model": (models, "pub resource_files: Vec<ResourceFile>"),
        "search keyword module": (extractor, "extract_search_keyword"),
        "CJK extractor test": (extractor, "ぼっち・ざ・ろっく！"),
        "bracket-title extractor test": (extractor, "STEINS;GATE"),
        "transparent BDMV resource indexing": (scanner, "index_transparent_bdmv_resources"),
        "BDMV resource stale cleanup test": (scanner, "bdmv_resources_attach_to_parent_without_nodes_or_duplicate_videos"),
        "video-bearing Container Bangumi guard": (models, "self.total_video_count > 0"),
        "localized unknown resource UI label": (frontend_format, 'translateActive("resource.other")'),
    }
    missing = [label for label, (source, fragment) in required_fragments.items() if fragment not in source]
    if missing:
        fail(f"phase-2 implementation fragments missing: {missing}")
    else:
        passed("phase-2 resource, cover, title, root, and all-resources contract")

    bangumi_source = read("src-tauri/src/bangumi.rs")
    cache_source = read("src-tauri/src/cache.rs")
    if (
        "cache::create_pending_cache_file(&destination, true)" not in bangumi_source
        or "options.write(true).create_new(true).read(readable)" not in cache_source
    ):
        fail("Bangumi temporary cover file must be opened read+write for signature validation")
    else:
        passed("Bangumi cover temporary file supports post-download signature read")


def check_next_phase_contract() -> None:
    app = read("src/App.tsx")
    settings_page = read("src/pages/SettingsPage.tsx")
    frontend_api = read("src/lib/api.ts")
    frontend_models = read("src/types/media.ts")
    frontend_format = read("src/lib/format.ts")
    cover_hook = read("src/hooks/useCoverDataUrl.ts")
    i18n = read("src/lib/i18n.tsx")
    css = read("src/styles.css")
    rust_models = read("src-tauri/src/models.rs")
    database = read("src-tauri/src/db.rs")
    commands = read("src-tauri/src/commands.rs")
    cache = read("src-tauri/src/cache.rs")
    bangumi = read("src-tauri/src/bangumi.rs")
    lib = read("src-tauri/src/lib.rs")

    required_fragments = {
        "typed four-locale catalog": (i18n, "const resources: Record<AppLanguage, Messages>"),
        "Simplified Chinese locale": (i18n, '"zh-CN": zhCN'),
        "English locale": (i18n, '"en-US": enUS'),
        "Japanese locale": (i18n, '"ja-JP": jaJP'),
        "Korean locale": (i18n, '"ko-KR": koKR'),
        "localized native dialog labels": (frontend_api, 'translateActive("dialog.coverCacheDirectory")'),
        "localized native command failures": (frontend_api, "commandErrorKeys"),
        "raw native errors retained only as cause": (frontend_api, "causeValue"),
        "localized dialog failures": (frontend_api, 'translateActive("error.dialogFailed")'),
        "localized cover failure toast": (app, 't("error.coverFailed")'),
        "localized player test result": (settings_page, 't("settings.playerUnavailable")'),
        "localized unknown resource label": (frontend_format, 'translateActive("resource.other")'),
        "English binding title model": (frontend_models, "providerTitleEn"),
        "Japanese binding title model": (frontend_models, "providerTitleJa"),
        "Korean binding title model": (frontend_models, "providerTitleKo"),
        "current-locale title selection": (frontend_format, "localizedTitle"),
        "Chinese title fallback": (frontend_format, "providerTitleCn"),
        "localized Container suffix": (frontend_format, 'translate(language, "node.seriesSuffix")'),
        "navigation scroll snapshot": (app, "NavigationSnapshot"),
        "detail scroll reset": (app, "queueScroll(0)"),
        "list scroll restore": (app, "queueScroll(previous.scrollTop)"),
        "system theme listener": (app, 'matchMedia("(prefers-color-scheme: dark)")'),
        "resolved root theme": (app, "document.documentElement.dataset.theme"),
        "settings failure isolation": (settings_page, "Promise.allSettled"),
        "automatic settings change queue": (settings_page, "const changeSettings ="),
        "serialized automatic settings persistence": (settings_page, "const drainAutoSave = async"),
        "immediate cache persistence": (settings_page, "const chooseCacheDirectory = async"),
        "settings persistence command": (settings_page, "api.updateSettings"),
        "dark theme selector": (css, ':root[data-theme="dark"]'),
        "semantic app surface": (css, "--surface-app"),
        "semantic raised surface": (css, "--surface-raised"),
        "semantic input surface": (css, "--surface-input"),
        "semantic strong text": (css, "--text-strong"),
        "semantic normal border": (css, "--border-normal"),
        "Rust multilingual binding model": (rust_models, "provider_title_en"),
        "persisted language setting": (database, '("language", settings.language.clone())'),
        "persisted theme setting": (database, '("theme", settings.theme.clone())'),
        "persisted custom cache setting": (database, '"cover_cache_directory"'),
        "official Bangumi Subject detail": (bangumi, "SUBJECT_DETAIL_URL"),
        "Bangumi infobox title extraction": (bangumi, "extract_infobox_title"),
        "best-effort subject enrichment": (bangumi, "enrich_subject"),
        "custom cache ownership marker": (cache, ".m2shelf-cover-cache"),
        "custom cache/media overlap guard": (cache, "validate_cache_location"),
        "owned-name-only cache clear": (cache, "is_owned_cache_file"),
        "bounded signature-checked cover data": (cache, "cover_data_url"),
        "typed cover data command": (commands, "pub fn get_cover_data_url"),
        "registered cover data command": (lib, "commands::get_cover_data_url"),
        "frontend Node-ID cover request": (frontend_api, 'call<string | null>("get_cover_data_url", { nodeId })'),
    }
    missing = [label for label, (source, fragment) in required_fragments.items() if fragment not in source]
    if missing:
        fail(f"next-phase i18n/theme/cache/scroll contract fragments missing: {missing}")
    else:
        passed("next-phase scroll, four-locale i18n, theme, Bangumi-title, and custom-cache contract")

    localized_error_keys = (
        "error.commandFailed",
        "error.initializationFailed",
        "error.bangumiFailed",
        "error.coverFailed",
        "error.playerFailed",
        "error.settingsFailed",
        "error.cacheFailed",
        "error.dialogFailed",
        "settings.playerUnavailable",
    )
    missing_error_locales = [key for key in localized_error_keys if i18n.count(f'"{key}"') < 4]
    raw_error_surfaces = (
        "{ error: binding.coverDownloadError }" in app
        or "return error.message;\n  return String(error);" in frontend_format
    )
    if missing_error_locales or raw_error_surfaces:
        fail(f"native error i18n boundary is incomplete: {missing_error_locales}")

    locale_union = re.search(r'export type AppLanguage\s*=\s*([^;]+);', frontend_models, re.DOTALL)
    if not locale_union or any(locale not in locale_union.group(1) for locale in ("zh-CN", "en-US", "ja-JP", "ko-KR")):
        fail("AppLanguage does not contain all four supported locale values")
    theme_union = re.search(r'export type AppTheme\s*=\s*([^;]+);', frontend_models, re.DOTALL)
    if not theme_union or any(theme not in theme_union.group(1) for theme in ("system", "light", "dark")):
        fail("AppTheme does not contain system/light/dark")

    fallback_positions = [
        frontend_format.find("localizedTitle?.trim()"),
        frontend_format.find("providerTitleCn?.trim()"),
        frontend_format.find("providerTitle?.trim()"),
    ]
    if -1 in fallback_positions or fallback_positions != sorted(fallback_positions):
        fail("bound title fallback is not current locale -> Chinese -> Bangumi main title")


def check_brand_release_and_icons() -> None:
    try:
        package = json.loads(read("package.json"))
        package_lock = json.loads(read("package-lock.json"))
        config = json.loads(read("src-tauri/tauri.conf.json"))
        cargo = tomllib.loads(read("src-tauri/Cargo.toml"))
        cargo_lock = tomllib.loads(read("src-tauri/Cargo.lock"))
    except (json.JSONDecodeError, tomllib.TOMLDecodeError):
        return

    cargo_lock_version = next(
        (
            str(entry.get("version", ""))
            for entry in cargo_lock.get("package", [])
            if entry.get("name") == "m2shelf"
        ),
        "",
    )

    versions = {
        str(package.get("version", "")),
        str(package_lock.get("version", "")),
        str(package_lock.get("packages", {}).get("", {}).get("version", "")),
        str(config.get("version", "")),
        str(cargo.get("package", {}).get("version", "")),
        cargo_lock_version,
    }
    if len(versions) != 1 or "" in versions:
        fail(f"package/npm lock/Tauri/Cargo/Cargo lock versions are not synchronized: {sorted(versions)}")
    else:
        passed(f"synchronized release version {next(iter(versions))}")

    if package.get("name") != "m2shelf" or cargo.get("package", {}).get("name") != "m2shelf":
        fail("technical package name must be m2shelf")
    if config.get("productName") != "M²Shelf":
        fail("Tauri productName must use the user-facing M²Shelf brand")
    nsis = config.get("bundle", {}).get("windows", {}).get("nsis", {})
    if nsis.get("installerIcon") != "icons/icon.ico" or nsis.get("uninstallerIcon") != "icons/icon.ico":
        fail("NSIS installer and uninstaller must use the owned M² icon")
    windows = config.get("app", {}).get("windows", [])
    if not windows or windows[0].get("title") != "M²Shelf":
        fail("main window title must use M²Shelf")
    index = read("index.html")
    if "<title>M²Shelf</title>" not in index:
        fail("HTML title does not use M²Shelf")

    font_assets = [
        path.relative_to(ROOT).as_posix()
        for suffix in ("*.ttf", "*.otf", "*.ttc")
        for path in ROOT.rglob(suffix)
        if "node_modules" not in path.parts and "target" not in path.parts
    ]
    if font_assets:
        fail(f"bundled font files are forbidden: {font_assets}")
    css = read("src/styles.css")
    if re.search(r"\b(?:Georgia|Inter)\b|font-family\s*:[^;]*(?<!sans-)\bserif\b", css, re.IGNORECASE):
        fail("UI CSS still references a non-system UI/serif brand font")
    elif "system-ui" not in css or "Segoe UI" not in css:
        fail("UI CSS lacks the required system font stack")
    else:
        passed("system UI font stack with no bundled font assets")

    original_path = ROOT / "src-tauri/icons/logo-input-original.png"
    expected_original_hash = "AB6B86469FEE688E03468A160820F2A4E209B9904C0F7C239FC6918222295B72"
    try:
        original_hash = hashlib.sha256(original_path.read_bytes()).hexdigest().upper()
        if original_hash != expected_original_hash:
            fail(f"original user Logo input changed: hash={original_hash}")
        else:
            passed("byte-exact original user Logo input")
    except OSError as error:
        fail(f"cannot validate original user Logo input: {error}")

    source_path = ROOT / "src-tauri/icons/icon-source.png"
    expected_source_hash = "027D80A3665955E59A8A443B075C03BD17116155B6E2B6872B7E3D48F36ACA1B"
    try:
        source_data = source_path.read_bytes()
        source_hash = hashlib.sha256(source_data).hexdigest().upper()
        source_size = struct.unpack_from(">II", source_data, 16) if source_data.startswith(b"\x89PNG\r\n\x1a\n") else None
        source_color_type = source_data[25] if len(source_data) > 25 else None
        if source_hash != expected_source_hash or source_size != (1254, 1254) or source_color_type != 6:
            fail(
                f"transparent-corner canonical Logo source changed: "
                f"hash={source_hash}, size={source_size}, color_type={source_color_type}"
            )
        else:
            passed("byte-exact transparent-corner 1254x1254 Logo source")
    except (OSError, struct.error) as error:
        fail(f"cannot validate canonical Logo source: {error}")

    expected_pngs = {
        "16x16.png": (16, 16),
        "24x24.png": (24, 24),
        "32x32.png": (32, 32),
        "48x48.png": (48, 48),
        "64x64.png": (64, 64),
        "128x128.png": (128, 128),
        "256x256.png": (256, 256),
        "128x128@2x.png": (256, 256),
        "icon.png": (512, 512),
        "512x512.png": (512, 512),
        "icon-master.png": (1024, 1024),
    }
    invalid_pngs: list[str] = []
    for name, expected_size in expected_pngs.items():
        try:
            data = (ROOT / "src-tauri/icons" / name).read_bytes()
            actual_size = struct.unpack_from(">II", data, 16) if data.startswith(b"\x89PNG\r\n\x1a\n") else None
            if actual_size != expected_size:
                invalid_pngs.append(f"{name}={actual_size}")
        except (OSError, struct.error) as error:
            invalid_pngs.append(f"{name}={error}")
    if invalid_pngs:
        fail(f"Logo PNG sizes are incomplete or invalid: {invalid_pngs}")
    else:
        passed("all Logo PNG sizes derived from the approved master")

    svg = read("src-tauri/icons/icon.svg")
    if '<image href="icon-source.png"' not in svg or "<path" in svg or "<text" in svg:
        fail("M² SVG must be an untraced compatibility wrapper around icon-source.png")

    generator = read("scripts/generate_icons.ps1")
    if (
        expected_source_hash not in generator
        or "HighQualityBicubic" not in generator
        or "New-RoundedRectanglePath" in generator
        or "AddPolygon" in generator
    ):
        fail("Logo generator must only resize the byte-exact approved raster source")

    preparer = read("scripts/prepare_logo_source.ps1")
    if expected_original_hash not in preparer or "RemoveBlackCorners" not in preparer:
        fail("Logo source preparer must reproduce the explicitly requested transparent corners")

    brand_rule = re.search(r"\.brand-mark\s*\{([^}]*)\}", css, re.DOTALL)
    if not brand_rule or any(token in brand_rule.group(1) for token in ("box-shadow", "border-radius", "filter")):
        fail("rendered brand mark must not crop or visually alter the approved image")

    ico_path = ROOT / "src-tauri/icons/icon.ico"
    expected_ordered_sizes = [256, 128, 64, 48, 32, 24, 16]
    expected_sizes = set(expected_ordered_sizes)
    try:
        data = ico_path.read_bytes()
        reserved, icon_type, count = struct.unpack_from("<HHH", data, 0)
        ordered_sizes: list[int] = []
        for index_value in range(count):
            width, height = struct.unpack_from("<BB", data, 6 + 16 * index_value)
            width = width or 256
            height = height or 256
            if width == height:
                ordered_sizes.append(width)
        sizes = set(ordered_sizes)
        if reserved != 0 or icon_type != 1 or sizes != expected_sizes:
            fail(f"Windows ICO frames are incomplete or unexpected: {ordered_sizes}")
        elif ordered_sizes != expected_ordered_sizes:
            fail(
                "Windows ICO must put its 256 px frame first so Tauri's runtime "
                f"window/taskbar icon is not upscaled from a small frame: {ordered_sizes}"
            )
        else:
            passed("user-approved M² PNG/ICO with 256 px runtime frame and complete Windows sizes")
    except (OSError, struct.error) as error:
        fail(f"cannot validate Windows ICO: {error}")

    public_key_text = read("src-tauri/update-public-key.txt").strip()
    try:
        public_key_bytes = base64.b64decode(public_key_text, validate=True)
    except (binascii.Error, ValueError):
        public_key_bytes = b""
    public_key_ok = all(
        (
            public_key_text != "UNCONFIGURED",
            len(public_key_bytes) == 32,
            base64.b64encode(public_key_bytes).decode("ascii") == public_key_text,
            public_key_bytes != bytes(32),
        )
    )
    if not public_key_ok:
        fail("updater public key must be a configured, canonical 32-byte Ed25519 key")
    else:
        passed("configured canonical 32-byte Ed25519 update public key")

    update_source = read("src-tauri/src/update.rs")
    portable_update_source = read("src-tauri/src/portable_update.rs")
    updater_cli = read("src-tauri/src/bin/m2shelf_updater.rs")
    update_manifest_runtime_contract = (
        'const PUBLIC_KEY_TEXT: &str = include_str!("../update-public-key.txt")',
        "const MAX_MANIFEST_BYTES: u64 = 256 * 1024",
        "pub const MAX_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024",
        '#[serde(rename_all = "camelCase", deny_unknown_fields)]',
        "fn validate_manifest_with_key(",
        "fn validate_asset_with_key(",
        'format!("M2Shelf-Portable-{version}-x64.zip")',
        'format!("M2Shelf-Setup-{version}-x64.exe")',
        "if asset.file_name != expected_file_name",
        "fn canonical_version(",
        "!version.pre.is_empty() || !version.build.is_empty()",
        'const SIGNATURE_DOMAIN: &[u8] = b"M2Shelf.Update.v1\\0"',
        "message.extend_from_slice(APP_ID.as_bytes())",
        "message.extend_from_slice(&size.to_le_bytes())",
        "message.extend_from_slice(sha256)",
        "verify_asset_digest_and_signature(",
        "lock_and_verify_file_against_manifest(",
        ".https_only(true)",
        "Policy::custom",
        "attempt.previous().len() >= 5",
        "read_bounded_response(response, MAX_MANIFEST_BYTES",
        ".take(limit + 1)",
        'Some("github.com"',
        '"release-assets.githubusercontent.com"',
        '"objects.githubusercontent.com"',
        "signing_key.verifying_key() != expected",
    )
    updater_identity_contract = (
        '"identity" =>',
        "if args.len() != 1",
        "sign(parse_options(&args[1..], &[\"version\", \"platform\", \"file\"])?",
        '"verify" => verify(parse_options(',
        '&["version", "platform", "file", "signature"]',
        "parse_options(&args[1..], &[\"request\"])?",
        "signer_identity_for_cli()",
        "sign_artifact_for_cli(&file, version, platform, &private_key)",
        "verify_artifact_for_cli(&file, version, platform, signature)",
        'env::var("M2SHELF_UPDATE_PRIVATE_KEY")',
    )
    signer_identity_source_contract = (
        "pub struct SignerIdentityOutput",
        "pub fn signer_identity_for_cli()",
        "schema_version: 1",
        "app_id: APP_ID",
        'version: env!("CARGO_PKG_VERSION")',
        "public_key: BASE64_STANDARD.encode(key.to_bytes())",
        "ensure_signing_key_matches_text(&key, PUBLIC_KEY_TEXT)?",
        "pub fn verify_artifact_for_cli(",
        "verify_artifact_with_key(",
        "&configured_verifying_key()?",
        "cli_verifier_rehashes_the_file_and_rejects_mismatched_signatures",
    )
    if (
        any(token not in update_source for token in update_manifest_runtime_contract)
        or any(token not in updater_cli for token in updater_identity_contract)
        or any(token not in update_source for token in signer_identity_source_contract)
    ):
        fail("signed update manifest, bounded transport, or updater-helper identity contract is incomplete")
    else:
        passed("strict signed manifest, bounded HTTPS transport, and embedded-key helper identity")

    offline_key_manifest = read("tools/offline-key-init/Cargo.toml")
    offline_key_source = read("tools/offline-key-init/src/main.rs")
    offline_key_lock = read("tools/offline-key-init/Cargo.lock")
    offline_key_builder = read("scripts/build_offline_key_init.ps1")
    offline_key_contract = (
        'publish = false',
        'name = "M2ShelfOfflineKeyInit"',
        'base64 = "=0.22.1"',
        'ed25519-dalek = { version = "=2.2.0"',
        'windows-sys = { version = "=0.60.2"',
        'zeroize = "=1.9.0"',
        "CryptProtectData",
        "CryptUnprotectData",
        "CRYPTPROTECT_UI_FORBIDDEN",
        "Zeroizing::new(signing_key.to_bytes())",
        "Zeroizing::new(ciphertext.to_vec())",
        "drop(recovered)",
        "seed.zeroize()",
        "struct DpapiOutput",
        "wipe_before_free",
        "slice::from_raw_parts_mut",
        "LocalFree(self.blob.pbData.cast())",
        "MoveFileExW",
        "MOVEFILE_WRITE_THROUGH",
        "GetVolumePathNameW",
        "GetDriveTypeW",
        "DRIVE_FIXED",
        "FILE_ATTRIBUTE_REPARSE_POINT",
        ".create_new(true)",
        "validate_existing_path_chain",
        "validate_destination_still_new",
        "validate_windows_path_component",
        "is_reserved_dos_device_name",
        "MAX_DPAPI_CIPHERTEXT_LEN",
        "validate_dpapi_ciphertext",
        "validate_commit_preconditions",
        "verify_committed_directory",
        "commit_verified_directory",
        "reject_source_repository",
        "cleanup_exact_staging(path: &Path) -> Result<(), String>",
        "CurrentUser-DPAPI",
    )
    offline_key_forbidden = (
        "M2SHELF_UPDATE_PRIVATE_KEY={",
        "BASE64_STANDARD.encode(seed",
        "CRYPTPROTECT_LOCAL_MACHINE",
        "MOVEFILE_REPLACE_EXISTING",
        "remove_dir_all",
    )
    if '"keygen"' in updater_cli or "production-capable private key" in updater_cli:
        fail("distributed updater must not expose a production-key generator")
    elif any(
        token not in offline_key_manifest + offline_key_source for token in offline_key_contract
    ):
        fail("offline production-key initializer lacks pinned dependencies or fail-closed DPAPI handling")
    elif any(token in offline_key_source for token in offline_key_forbidden):
        fail("offline production-key initializer may expose, weaken, overwrite, or broadly delete key material")
    elif 'name = "m2shelf-offline-key-init"' not in offline_key_lock:
        fail("offline production-key initializer must have an independent checked-in Cargo lockfile")
    elif re.search(r"(?i)[A-Z]:\\Users\\[^\\\s]+", offline_key_source + offline_key_manifest):
        fail("offline production-key initializer contains a literal user-profile path")
    else:
        passed("non-distributed offline DPAPI key initializer with no stdout private-key path")

    offline_key_builder_contract = (
        "CARGO_ENCODED_RUSTFLAGS",
        "--remap-path-prefix=",
        "cargo fmt --manifest-path $manifestPath -- --check",
        "cargo test --manifest-path $manifestPath --locked",
        "cargo clippy --manifest-path $manifestPath --all-targets --locked -- -D warnings",
        "cargo build --manifest-path $manifestPath --release --locked --bin M2ShelfOfflineKeyInit",
        "distribution directory already exists; refusing to overwrite it",
        "distribution directory appeared during the build; refusing to overwrite it",
        "[System.IO.File]::Copy($sourceItem.FullName, $output, $false)",
        "[System.IO.FileMode]::CreateNew",
        "Assert-X64Pe -Path",
        "Assert-NoPrivateBuildPath -Path",
        '"bundle\\offline-key-init-v$version"',
        "Get-FileHash -LiteralPath $outputItem.FullName -Algorithm SHA256",
        "Existing offline key initializer checksum is not a plain file.",
    )
    if any(token not in offline_key_builder for token in offline_key_builder_contract):
        fail("offline key initializer builder lacks locked x64, path-remapping, privacy, or checksum guards")
    elif any(
        token in offline_key_builder
        for token in (
            "M2SHELF_UPDATE_PRIVATE_KEY",
            "production-seed.dpapi",
            " --confirm ",
            " init ",
            "Copy-Item -LiteralPath $sourceItem.FullName -Destination $output -Force",
            "[System.IO.Directory]::CreateDirectory($outputDirectory)",
        )
    ):
        fail("offline key initializer builder must compile only and never access or create key material")
    else:
        passed("offline key initializer build is locked, x64, privacy-remapped, and compile-only")

    expected_portable_payload = [
        "M2Shelf.exe",
        "M2ShelfUpdater.exe",
        "M2Shelf.portable.json",
        "README_zh-CN.txt",
        "SHA256SUMS.txt",
    ]

    def rust_payload_array(name: str) -> list[str] | None:
        match = re.search(
            rf"const\s+{re.escape(name)}\s*:\s*\[&str;\s*(\d+)\]\s*=\s*\[(.*?)\];",
            portable_update_source,
            re.DOTALL,
        )
        if not match:
            return None
        aliases = {
            "UPDATER_FILE": "M2ShelfUpdater.exe",
            "PORTABLE_MARKER_FILE": "M2Shelf.portable.json",
        }
        entries: list[str] = []
        for item in match.group(2).split(","):
            item = item.strip()
            if not item:
                continue
            literal = re.fullmatch(r'"([^"\\]*)"', item)
            if literal:
                entries.append(literal.group(1))
            elif item in aliases:
                entries.append(aliases[item])
            else:
                return None
        if int(match.group(1)) != len(entries):
            return None
        return entries

    portable = read("scripts/build_portable.ps1")
    portable_contract = (
        "M2Shelf-Portable-$version-$Architecture.zip",
        '"M2Shelf.exe"',
        '"M2ShelfUpdater.exe"',
        '"M2Shelf.portable.json"',
        "schemaVersion = 1",
        'appId = "app.morimediashelf.desktop"',
        'distribution = "portable"',
        '"SHA256SUMS.txt"',
        "$binaryVersion.ProductVersion",
        "Updater public key must contain exactly 32 Ed25519 bytes",
        "$actualMachine",
        "$sourceUpdaterHash",
        "$releaseUpdater.LastWriteTimeUtc",
        "Assert-NoPrivateBuildPath -Path $sourceUpdater",
        "$newestInput",
        '"package-lock.json"',
        '"index.html"',
        '"vite.config.ts"',
        '"tsconfig.json"',
        '"tsconfig.node.json"',
        '"src-tauri\\capabilities"',
        '"src-tauri\\update-public-key.txt"',
        '(Join-Path $repoRoot "src")',
        "$identityOutput = @(& $sourceUpdater identity)",
        '$expectedIdentityProperties = @("appId", "publicKey", "schemaVersion", "version")',
        '[string]$identity.appId -cne "app.morimediashelf.desktop"',
        "[string]$identity.version -cne $version",
        "[string]$identity.publicKey -cne $publicKeyText",
        "$missingStageFiles",
        "$unexpectedStageFiles",
    )
    stage_files_match = re.search(
        r"\$requiredStageFiles\s*=\s*@\((.*?)\)", portable, re.DOTALL
    )
    stage_files = (
        re.findall(r'"([^"\r\n]+)"', stage_files_match.group(1))
        if stage_files_match
        else []
    )
    rust_required_payload = rust_payload_array("REQUIRED_PAYLOAD_FILES")
    rust_allowed_payload = rust_payload_array("ALLOWED_PAYLOAD_FILES")
    portable_payload_contract = (
        stage_files == expected_portable_payload
        and rust_required_payload == expected_portable_payload
        and rust_allowed_payload == expected_portable_payload
        and "!allowed.contains(&folded)" in portable_update_source
        and "!ALLOWED_PAYLOAD_FILES.contains(&raw_name.as_str())"
        in portable_update_source
        and "for required in REQUIRED_PAYLOAD_FILES" in portable_update_source
        and "Portable 更新 ZIP 包含未知文件" in portable_update_source
        and "Portable 更新 ZIP 缺少 {required}" in portable_update_source
    )
    if any(token not in portable for token in portable_contract) or not portable_payload_contract:
        fail("Portable builder/runtime does not enforce the exact five-file payload and helper identity")
    else:
        passed("exact five-file Portable payload with helper identity, PE, freshness, privacy, and checksums")

    update_manifest = read("scripts/generate_update_manifest.ps1")
    update_manifest_contract = (
        "$env:M2SHELF_UPDATE_PRIVATE_KEY",
        "sign --version $version --platform $Platform --file $Path",
        '"windows-x64-portable"',
        '"windows-x64-nsis"',
        "schemaVersion = 1",
        "publishedAt =",
        "notes = $notes",
        "platforms =",
        '"zh-CN"',
        '"en-US"',
        '"ja-JP"',
        '"ko-KR"',
        "https://github.com/Undermori/M2Shelf/releases/download/v$version/",
        'expectedProperties = @("fileName", "sha256", "signature", "size")',
        "$signatureBytes.Length -ne 64",
        "Get-FileHash -LiteralPath $Path -Algorithm SHA256",
        "Ensure-HashSidecar -Path $portablePath",
        "Ensure-HashSidecar -Path $installerPath",
        "M2SHELF_UPDATE_PRIVATE_KEY is required",
        "$TrustedSignerSha256 -notmatch '^[0-9A-Fa-f]{64}$'",
        "$actualSignerSha256 -cne $TrustedSignerSha256.ToLowerInvariant()",
        "$identityOutput = @(& $resolvedUpdater identity)",
        '$expectedIdentityProperties = @("appId", "publicKey", "schemaVersion", "version")',
        '[string]$identity.appId -cne "app.morimediashelf.desktop"',
        "[string]$identity.version -cne $version",
        "[string]$identity.publicKey -cne $publicKeyText",
    )
    if any(token not in update_manifest for token in update_manifest_contract):
        fail("signed update-manifest generator does not implement the frozen v1 schema and signer CLI")
    elif re.search(r"(?i)[A-Z]:\\Users\\[^\\\s]+", update_manifest):
        fail("update-manifest generator contains a literal user-profile path")
    elif re.search(
        r"(?is)(?:WriteAllText|WriteAllLines|Set-Content|Out-File).{0,160}M2SHELF_UPDATE_PRIVATE_KEY|"
        r"M2SHELF_UPDATE_PRIVATE_KEY.{0,160}(?:WriteAllText|WriteAllLines|Set-Content|Out-File)",
        update_manifest,
    ):
        fail("update signing private key may be persisted by the release script")
    else:
        passed("offline manifest generator verifies signer identity and keeps private key process-only")

    offline_signer = read("scripts/sign_update_offline.ps1")
    offline_signer_contract = (
        "#Requires -Version 7.2",
        "[Parameter(Mandatory = $true)][string]$CandidateDirectory",
        "[Parameter(Mandatory = $true)][string]$EncryptedSeedPath",
        "[Parameter(Mandatory = $true)][string]$UpdaterPath",
        "[Parameter(Mandatory = $true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$TrustedSignerSha256",
        "[Parameter(Mandatory = $true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$TrustedManifestGeneratorSha256",
        "[Parameter(Mandatory = $true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$TrustedPortableSha256",
        "[Parameter(Mandatory = $true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$TrustedNsisSha256",
        "$actualManifestGeneratorSha256 -cne $TrustedManifestGeneratorSha256.ToLowerInvariant()",
        "$actualPortableSha256 -cne $TrustedPortableSha256.ToLowerInvariant()",
        "$actualNsisSha256 -cne $TrustedNsisSha256.ToLowerInvariant()",
        "$trustedInputLocks = [System.Collections.Generic.List[System.IO.FileStream]]::new()",
        "[System.IO.FileShare]::Read",
        "A trusted signing input changed while it was being locked.",
        "[System.OperatingSystem]::IsWindows()",
        "DataProtectionScope]::CurrentUser",
        "[System.Security.Cryptography.ProtectedData]::Unprotect(",
        "$seedItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint",
        "$seedPath.StartsWith($repoPrefix",
        "$seedItem.Length -lt 1 -or $seedItem.Length -gt 16384",
        'GetEnvironmentVariable("M2SHELF_UPDATE_PRIVATE_KEY", [System.EnvironmentVariableTarget]::Process)',
        "$seedBytes.Length -ne 32",
        "[System.Convert]::ToBase64String($seedBytes)",
        'SetEnvironmentVariable(\n    "M2SHELF_UPDATE_PRIVATE_KEY"',
        "TrustedSignerSha256 = $TrustedSignerSha256.ToLowerInvariant()",
        "& $manifestScript @manifestArguments",
        "} finally {",
        "$privateKeyText = $null",
        "[System.Array]::Clear($seedBytes, 0, $seedBytes.Length)",
        "[System.Array]::Clear($protectedBytes, 0, $protectedBytes.Length)",
    )
    offline_signer_forbidden = (
        "Invoke-WebRequest",
        "Invoke-RestMethod",
        "Start-BitsTransfer",
        "gh release",
        "curl ",
        "secrets.",
        "Write-Output $privateKeyText",
        "Write-Host $privateKeyText",
    )
    if any(token not in offline_signer for token in offline_signer_contract):
        fail("offline DPAPI signing wrapper lacks seed, signer-fingerprint, or process-secret guards")
    elif any(token in offline_signer for token in offline_signer_forbidden):
        fail("offline DPAPI signing wrapper may use network/CI secrets or print private material")
    elif re.search(r"(?i)[A-Z]:\\Users\\[^\\\s]+", offline_signer):
        fail("offline signing wrapper contains a literal user-profile path")
    else:
        passed("offline signing requires independent script, helper, and candidate fingerprints before DPAPI decrypt")

    portable_key_manifest = read("tools/portable-key-tool/Cargo.toml")
    portable_key_lock = read("tools/portable-key-tool/Cargo.lock")
    portable_key_gitignore = read(".gitignore")
    portable_key_source = "\n".join(
        read(path)
        for path in (
            "tools/portable-key-tool/src/main.rs",
            "tools/portable-key-tool/src/commands.rs",
            "tools/portable-key-tool/src/key_container.rs",
            "tools/portable-key-tool/src/password.rs",
        )
    )
    portable_key_contract = (
        'name = "m2shelf-portable-key-tool"',
        'publish = false',
        'argon2 = "=0.5.3"',
        'chacha20poly1305 = "=0.10.1"',
        'ed25519-dalek = { version = "=2.2.0", features = ["zeroize"] }',
        'rpassword = "=7.4.0"',
        'zeroize = "=1.8.1"',
        'm2shelf_lib = { package = "m2shelf", path = "../../src-tauri" }',
        '"migrate-dpapi" => migrate_dpapi(options)',
        '"verify-key" => verify_key(options)',
        '"sign-release" => sign_release(options)',
        'std::env::var_os("M2SHELF_UPDATE_PRIVATE_KEY").is_some()',
        "CryptUnprotectData",
        "ARGON2_MEMORY_KIB",
        "memory_kib != ARGON2_MEMORY_KIB",
        "iterations != ARGON2_ITERATIONS",
        "parallelism != ARGON2_PARALLELISM",
        "Argon2::new(Algorithm::Argon2id, Version::V0x13",
        "XChaCha20Poly1305",
        "Zeroizing",
        'pub const KEY_FILE_NAME: &str = "encrypted-private-key.m2key"',
        'pub const KEY_DIRECTORY_NAME: &str = "M2Shelf-Production-Key"',
        'pub const METADATA_FILE_NAME: &str = "key-metadata.json"',
        'pub const README_FILE_NAME: &str = "README.txt"',
        "sign_digest(",
        "verify_signature(",
        "RETURN_FILE_COUNT: usize = 8",
        "#[serde(deny_unknown_fields)]",
        '"windows-x64-portable"',
        '"windows-x64-nsis"',
        "The original DPAPI file was left unchanged.",
    )
    portable_key_forbidden = (
        "std::env::set_var",
        "Command::new(",
        "sign_artifact_for_cli(",
        "decode_signing_key(",
        "remove_file(&args.input",
        "Write-Host $privateKey",
        "Write-Output $privateKey",
    )
    if any(
        token not in portable_key_manifest + portable_key_source
        for token in portable_key_contract
    ):
        fail("portable USB key tool lacks the encrypted-key, in-process signing, or exact-release contract")
    elif any(token in portable_key_source for token in portable_key_forbidden):
        fail("portable USB key tool may export, spawn with, or delete production key material")
    elif 'name = "m2shelf-portable-key-tool"' not in portable_key_lock:
        fail("portable USB key tool must have an independent checked-in Cargo lockfile")
    elif any(
        token not in portable_key_gitignore
        for token in (
            "tools/portable-key-tool/target/",
            "M2Shelf-Production-Key/",
            "*.m2key",
        )
    ):
        fail("portable USB key material and build cache must be ignored by Git")
    elif re.search(
        r"(?i)[A-Z]:\\Users\\[^\\\s]+",
        portable_key_source + portable_key_manifest,
    ):
        fail("portable USB key tool contains a literal user-profile path")
    else:
        passed("password-encrypted portable key with testable DPAPI migration and in-process signing")

    portable_key_builder = read("scripts/build_portable_key_tool.ps1")
    portable_key_builder_contract = (
        "CARGO_ENCODED_RUSTFLAGS",
        "--remap-path-prefix=",
        "cargo fmt --manifest-path $manifestPath -- --check",
        "cargo test --manifest-path $manifestPath --locked",
        "cargo clippy --manifest-path $manifestPath --all-targets --locked -- -D warnings",
        "cargo build --manifest-path $manifestPath --release --locked --bin M2ShelfPortableKeyTool",
        "Assert-X64Pe -Path",
        "Assert-NoPrivateBuildPath -Path",
        "Get-FileHash -LiteralPath $outputItem.FullName -Algorithm SHA256",
        '"bundle\\portable-key-tool-v$version"',
    )
    if any(token not in portable_key_builder for token in portable_key_builder_contract):
        fail("portable key tool builder lacks locked tests, x64 validation, path remapping, or checksum output")
    elif any(
        token in portable_key_builder
        for token in (
            "M2SHELF_UPDATE_PRIVATE_KEY",
            "production-seed.dpapi",
            "encrypted-private-key.m2key",
            " migrate-dpapi ",
            " sign-release ",
        )
    ):
        fail("portable key tool builder must compile only and never access key material")
    else:
        passed("portable key tool builder is locked, x64, path-remapped, and compile-only")

    usb_signer = read("scripts/sign_update_from_usb.ps1")
    usb_signer_contract = (
        "[System.IO.DriveInfo]::GetDrives()",
        "[System.IO.DriveType]::Removable",
        '"M2Shelf-Production-Key\\encrypted-private-key.m2key"',
        '"sign-release"',
        '"--key", $resolvedKeyPath',
        '"--public-key", $resolvedPublicKeyPath',
        '"--candidate-directory", $resolvedCandidateDirectory',
        '"--notes", $resolvedNotesPath',
        '"--provenance", $resolvedProvenancePath',
        '"--output-directory", $resolvedOutputDirectory',
        "& $resolvedToolPath @arguments",
        "Signed return directory does not contain the exact eight Release files.",
        "SIGNED-RETURN SHA-256 sidecar does not match the archive.",
    )
    usb_signer_forbidden = (
        "M2SHELF_UPDATE_PRIVATE_KEY",
        "production-seed.dpapi",
        "Get-Content -LiteralPath $resolvedKeyPath",
        "Read-Host",
        "Invoke-WebRequest",
        "Invoke-RestMethod",
        "Start-BitsTransfer",
        "gh release",
    )
    if any(token not in usb_signer for token in usb_signer_contract):
        fail("USB signing wrapper lacks removable-drive discovery, private in-process signing, or output verification")
    elif any(token in usb_signer for token in usb_signer_forbidden):
        fail("USB signing wrapper may read key material, accept legacy seed transport, or use the network")
    elif re.search(r"(?i)[A-Z]:\\Users\\[^\\\s]+", usb_signer):
        fail("USB signing wrapper contains a literal user-profile path")
    else:
        passed("USB signing wrapper locates one removable encrypted key and verifies the exact return package")

    release_workflow = read(".github/workflows/windows-release.yml")
    workflow_contract = (
        "workflow_dispatch:",
        'tags:',
        '"v*"',
        "npm run typecheck",
        "npm run build",
        "npm run validate",
        "cargo test --manifest-path src-tauri/Cargo.toml --locked",
        "cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings",
        "build_windows_release.ps1 -Bundles nsis",
        "build_portable.ps1 -SkipBuild",
        "permissions:",
        "contents: read",
        "id-token: write",
        "attestations: write",
        "persist-credentials: false",
        'git rev-parse --verify "$tag^{commit}"',
        "points to a different commit; refusing to attach this build",
        "candidate-provenance.json",
        "schemaVersion = 1",
        'repository = "${{ github.repository }}"',
        '"commit_sha=$headCommit" >> $env:GITHUB_OUTPUT',
        'commitSha = "${{ steps.release.outputs.commit_sha }}"',
        "Attest unsigned release candidates",
        "if: ${{ github.ref_type == 'tag' }}",
        "actions/attest@508db95dd578ae2727ebd6217d5ba78e4fbda05d # v4.2.1",
        "subject-path: |",
        "bundle/M2Shelf-Portable-${{ steps.release.outputs.version }}-x64.zip",
        "bundle/M2Shelf-Setup-${{ steps.release.outputs.version }}-x64.exe",
        "actions/upload-artifact@",
        "if-no-files-found: error",
        "retention-days: 14",
        "windows-x64-unsigned-candidate",
        "M2Shelf-Portable-${{ steps.release.outputs.version }}-x64.zip",
        "M2Shelf-Setup-${{ steps.release.outputs.version }}-x64.exe",
        "M2Shelf-Setup-${{ steps.release.outputs.version }}-x64.exe.sha256",
        "bundle/candidate-provenance.json",
    )
    if any(token not in release_workflow for token in workflow_contract):
        fail("Windows CI workflow lacks a read-only unsigned-candidate, provenance, build, or quality gate")
    elif any(
        token in release_workflow
        for token in (
            "secrets.",
            "M2SHELF_UPDATE_PRIVATE_KEY",
            "UPDATE_SIGNING_KEY",
            "generate_update_manifest.ps1",
            "gh release",
            "bundle/latest.json",
            "contents: write",
            "--clobber",
        )
    ):
        fail("Windows CI candidate workflow may sign, publish, use a secret, or request write access")
    elif re.search(r"(?i)[A-Z]:\\Users\\[^\\\s]+", release_workflow):
        fail("Windows GitHub Release workflow contains a literal user-profile path")
    else:
        passed(
            "read-only Windows CI produces an attested unsigned provenance-bound candidate without secrets"
        )

    offline_publisher = read("scripts/publish_signed_release.ps1")
    offline_publisher_contract = (
        "#Requires -Version 7.2",
        "[Parameter(Mandatory = $true)][string]$CandidateDirectory",
        "[Parameter(Mandatory = $true)][string]$VerifierPath",
        "[Parameter(Mandatory = $true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$TrustedVerifierSha256",
        "[Parameter(Mandatory = $true)][string]$ReleaseNotesPath",
        "[switch]$Publish",
        "$repository = \"Undermori/M2Shelf\"",
        'Invoke-GitText -Arguments @("status", "--porcelain=v1", "--untracked-files=normal")',
        'Invoke-GitText -Arguments @("rev-parse", "--verify", "HEAD")',
        'Invoke-GitText -Arguments @("rev-parse", "--verify", "$tag^{commit}")',
        'Invoke-GitText -Arguments @("rev-parse", "--verify", "refs/tags/$tag")',
        "$tagCommit -cne $headCommit",
        "Assert-RemoteTagMatches -Tag $tag -ExpectedTagObjectSha $tagObjectSha",
        'Assert-ExactProperties -Object $provenance -Expected @(',
        'Assert-ExactProperties -Object $provenance.artifacts -Expected @("nsis", "portable")',
        'Assert-ExactProperties -Object $provenance.artifacts.portable -Expected @("fileName", "sha256", "size")',
        'Assert-ExactProperties -Object $provenance.artifacts.nsis -Expected @("fileName", "sha256", "size")',
        "$provenance.tagExists -isnot [bool] -or -not $provenance.tagExists",
        '[string]$provenance.repository -cne $repository',
        '[string]$provenance.commitSha -cne $headCommit',
        '[string]$provenance.sourceRef -cne "refs/tags/$tag"',
        'portableSignature = (Join-Path $candidatePath "$portableName.sig")',
        'nsisSignature = (Join-Path $candidatePath "$installerName.sig")',
        "Read-SignatureSidecar",
        "[System.Convert]::ToBase64String($signatureBytes) -cne $signature",
        "$actualVerifierSha256 -cne $TrustedVerifierSha256.ToLowerInvariant()",
        "$identityOutput = @(& $resolvedVerifier identity)",
        'Assert-ExactProperties -Object $identity -Expected @("appId", "publicKey", "schemaVersion", "version")',
        '[string]$identity.publicKey -cne $publicKeyText',
        "Invoke-ArtifactVerifier",
        "verify --version $version --platform $Platform --file $Path --signature $Signature",
        'Assert-ExactProperties -Object $verified -Expected @("fileName", "sha256", "signature", "size")',
        "[string]$verified.signature -cne $Signature",
        'Assert-ExactProperties -Object $manifest -Expected @("notes", "platforms", "publishedAt", "schemaVersion", "version")',
        'Assert-ExactProperties -Object $manifest.notes -Expected @("en-US", "ja-JP", "ko-KR", "zh-CN")',
        'Assert-ExactProperties -Object $manifest.platforms -Expected @("windows-x64-nsis", "windows-x64-portable")',
        'Assert-ExactProperties -Object $manifestAsset -Expected @("fileName", "sha256", "signature", "size", "url")',
        "$actualSha256 -cne $provenanceSha256",
        "Assert-HashSidecar -Path $contract.HashPath",
        "$manifestAsset.signature -cne $sidecarSignature",
        '$expectedUrl = "https://github.com/$repository/releases/download/$tag/$($contract.FileName)"',
        "if (-not $Publish)",
        'Write-Output "No GitHub changes were made. Pass -Publish',
        "[System.IO.FileAccess]::Read",
        "A signed release input changed while it was being locked for upload.",
        "A GitHub release already exists for $tag; immutable releases are never modified.",
        '"--draft"',
        '"--verify-tag"',
        '"--notes-file", $resolvedReleaseNotes',
        "gh release upload $tag @uploadPaths",
        "@($expectedUploadNames) -ccontains $remoteName",
        "$remoteAssets.Count -ne $expectedUploadFiles.Count",
        "$null -eq $remoteAsset.digest",
        "[string]::IsNullOrWhiteSpace([string]$remoteAsset.digest)",
        '[string]$remoteAsset.digest -cne "sha256:$($expectedAsset.Sha256)"',
        "gh release edit $tag --repo $repository --draft=false",
    )
    offline_publisher_forbidden = (
        "M2SHELF_UPDATE_PRIVATE_KEY",
        "UPDATE_SIGNING_KEY",
        "EncryptedSeed",
        "ProtectedData",
        "openssl",
        "secrets.",
        "--clobber",
    )
    if any(token not in offline_publisher for token in offline_publisher_contract):
        fail("offline publisher lacks exact provenance/manifest/assets, immutable tag, or draft verification")
    elif any(token in offline_publisher for token in offline_publisher_forbidden):
        fail("offline publisher may access signing material, CI secrets, or overwrite release assets")
    elif re.search(r"(?i)[A-Z]:\\Users\\[^\\\s]+", offline_publisher):
        fail("offline publisher contains a literal user-profile path")
    else:
        passed("offline publisher verifies exact signed inputs and immutable tag before draft publication")

    portable_readme = read("docs/PORTABLE_README_zh-CN.txt")
    if any(
        token not in portable_readme
        for token in (
            "M2ShelfUpdater.exe",
            "M2Shelf.portable.json",
            "Ed25519",
            "SHA256SUMS.txt",
            ".sha256",
        )
    ):
        fail("Portable README does not explain helper, marker, signed updates, and checksums")
    else:
        passed("Portable README documents signed helper-based updates and integrity files")

    release_builder = read("scripts/build_windows_release.ps1")
    release_contract = (
        "$env:USERPROFILE",
        "$repoRoot",
        "CARGO_ENCODED_RUSTFLAGS",
        "--remap-path-prefix=",
        "Updater public key must contain exactly 32 Ed25519 bytes",
        "npm run tauri build -- --bundles $Bundles",
        "cargo build --manifest-path src-tauri/Cargo.toml --release --locked --bin M2ShelfUpdater",
        '$installerName = "${productName}_${version}_x64-setup.exe"',
        '"M2Shelf-Setup-$version-x64.exe"',
        "Assert-X64Pe -Path $releaseExecutable",
        "Assert-X64Pe -Path $releaseUpdater",
        "Assert-PlainDestinationOrMissing -Path $stableInstaller",
        "Remove-Item -LiteralPath $stableInstaller -Force",
        "[System.IO.File]::WriteAllText(",
    )
    release_surfaces = (
        release_builder
        + portable
        + read(".github/workflows/windows-release.yml")
        + read("src-tauri/tauri.conf.json")
        + read("src-tauri/Cargo.toml")
    )
    if "M2ShelfOfflineKeyInit" in release_surfaces:
        fail("offline production-key initializer must never enter CI, NSIS, Portable, or the main crate")
    elif any(token not in release_builder for token in release_contract):
        fail("public Windows release builder lacks path remapping, x64 identity, or stable NSIS output guards")
    elif not (
        release_builder.find("Push-Location $repoRoot")
        < release_builder.find("npm run tauri build -- --bundles $Bundles")
        < release_builder.find(
            "cargo build --manifest-path src-tauri/Cargo.toml --release --locked --bin M2ShelfUpdater"
        )
        < release_builder.find(
            "} finally {",
            release_builder.find(
                "cargo build --manifest-path src-tauri/Cargo.toml --release --locked --bin M2ShelfUpdater"
            ),
        )
    ):
        fail("M2ShelfUpdater must be built after Tauri inputs inside the same path-remapping scope")
    elif re.search(r"(?i)[A-Z]:\\Users\\[^\\\s]+", release_builder):
        fail("public Windows release builder must not contain a literal user-profile path")
    else:
        passed("public Windows release build remaps paths and emits verified canonical x64 NSIS artifacts")

    validate_launcher = read("scripts/run_validate.mjs")
    validate_launcher_contract = (
        "M2SHELF_PYTHON",
        '[...prefix, "--version"]',
        "if (probe.status !== 0) continue;",
    )
    if any(token not in validate_launcher for token in validate_launcher_contract):
        fail("validation launcher must reject broken aliases and support an explicit portable Python path")
    elif re.search(r"(?i)[A-Z]:\\Users\\[^\\\s]+", validate_launcher):
        fail("validation launcher must not contain a literal user-profile path")
    else:
        passed("validation launcher probes interpreters and contains no personal Python path")

    brand_archive = read("scripts/build_brand_assets.ps1")
    if (
        "M2Shelf-Brand-Assets-$version-$Architecture.zip" not in brand_archive
        or '"icon-source.png"' not in brand_archive
        or '"icon.svg"' not in brand_archive
        or '"icon.ico"' not in brand_archive
    ):
        fail("brand-asset builder does not package the canonical source, PNG/ICO, and SVG wrapper")
    else:
        passed("versioned canonical brand-asset archive contract")


def check_ui_windows_interaction_contract() -> None:
    app = read("src/App.tsx")
    all_page = read("src/pages/AllResourcesPage.tsx")
    browse_page = read("src/pages/BrowsePage.tsx")
    search_page = read("src/pages/SearchPage.tsx")
    settings_page = read("src/pages/SettingsPage.tsx")
    media_card = read("src/components/MediaCard.tsx")
    work_detail = read("src/pages/WorkDetailPage.tsx")
    poster_image = read("src/components/PosterImage.tsx")
    context_menu = read("src/components/ContextMenu.tsx")
    batch_context_menu = read("src/components/BatchContextMenu.tsx")
    batch_tag_dialog = read("src/components/BatchTagDialog.tsx")
    selection_toolbar = read("src/components/SelectionToolbar.tsx")
    tag_dialog = read("src/components/TagManagerDialog.tsx")
    tag_filter = read("src/components/TagFilter.tsx")
    sidebar = read("src/components/Sidebar.tsx")
    poster_grid = read("src/components/PosterGrid.tsx")
    recent_page = read("src/pages/RecentlyWatchedPage.tsx")
    favorite_page = read("src/pages/FavoritesPage.tsx")
    favorite_assignment = read("src/components/FavoriteAssignmentDialog.tsx")
    favorite_folder_dialog = read("src/components/FavoriteFolderDialog.tsx")
    frontend_api = read("src/lib/api.ts")
    frontend_models = read("src/types/media.ts")
    frontend_format = read("src/lib/format.ts")
    cover_hook = read("src/hooks/useCoverDataUrl.ts")
    poster_viewport_hook = read("src/hooks/usePosterViewportLifecycle.ts")
    poster_helper = read("src/lib/poster.ts")
    i18n = read("src/lib/i18n.tsx")
    css = read("src/styles.css")
    rust_commands = read("src-tauri/src/commands.rs")
    rust_database = read("src-tauri/src/db.rs")
    rust_lib = read("src-tauri/src/lib.rs")
    rust_models = read("src-tauri/src/models.rs")
    window_state = read("src-tauri/src/window_state.rs")
    player = read("src-tauri/src/player.rs")
    build = read("src-tauri/build.rs")
    manifest = read("src-tauri/windows-app-manifest.xml")

    navigation_fragments = (
        "NavigationSnapshot",
        "captureNavigation",
        "rememberNavigation",
        "window.history.pushState",
        'window.addEventListener("popstate"',
        "event.altKey",
        'event.key === "ArrowLeft"',
        "allFilter",
        "allSort",
        "browseFilter",
        "browseSort",
        "previous.viewMode",
        "previous.scrollTop",
        "navigationGeneration",
        "requestGeneration !== navigationGeneration.current",
        "scrollRestoreEpoch",
    )
    missing_navigation = [token for token in navigation_fragments if token not in app]
    controlled_pages = all(
        token in source
        for source in (all_page, browse_page)
        for token in ("filter: string", "onFilter:", "sort: CollectionSort", "onSort:")
    )
    detail_entry_guard = (
        'page === "library" && !detail && !browseData && !contentLoading' in app
    )
    back_boundary_guard = (
        'if (page !== "library") return;' in app
        and 'if (parent) void openBreadcrumb(parent.id, false);' in app
        and 'else goRoot(false)' not in app
    )
    if missing_navigation or not controlled_pages or not detail_entry_guard or not back_boundary_guard:
        fail(f"Windows-style navigation state contract is incomplete: {missing_navigation}")
    else:
        passed("back button, Alt+Left/mouse history, scroll, filter, sort, and view snapshots")

    loader_ordering_ok = all(
        token in app
        for token in (
            "const rootsLoadGeneration = useRef(0)",
            "const allResourcesLoadGeneration = useRef(0)",
            "const recentlyWatchedLoadGeneration = useRef(0)",
            "const favoriteFoldersLoadGeneration = useRef(0)",
            "const favoriteNodesLoadGeneration = useRef(0)",
            "const currentRefreshGeneration = useRef(0)",
            "const favoritesLoadingState = useRef",
            "state.pending.add(token)",
            "state.pending.delete(request.token)",
            "const invalidateFavoritesLoads = useCallback",
            "requestGeneration === allResourcesLoadGeneration.current",
            "requestGeneration === recentlyWatchedLoadGeneration.current",
            "requestGeneration === favoriteFoldersLoadGeneration.current",
            "requestGeneration === favoriteNodesLoadGeneration.current",
            "const isCurrentRefresh = () =>",
            "const refreshCurrentAndAllResources = useCallback",
            'page === "all" ? Promise.resolve(null) : loadAllResources()',
        )
    )
    duplicate_all_refresh_absent = all(
        fragment not in app
        for fragment in (
            "Promise.all([refreshCurrent(), loadRoots(selectedRootId), loadAllResources()])",
            "Promise.all([refreshCurrent(), loadAllResources()])",
            "Promise.all([refreshCurrent(), loadAllResources(), loadRoots(rootId)])",
        )
    )
    if not loader_ordering_ok or not duplicate_all_refresh_absent:
        fail("collection loaders can regress to stale-response commits, early loading close, or duplicate All Resources refreshes")
    else:
        passed("collection loaders commit only the latest response and coordinate loading without duplicate All Resources queries")

    section_restore_ok = all(
        (
            'type BrowsingSectionKey = "all" | "search" | "recent" | "favorites" | `library:${number}`' in app,
            "const sectionSnapshots = useRef<Map<BrowsingSectionKey, NavigationSnapshot>>" in app,
            "const rememberSectionNavigation" in app,
            "const restoreSectionNavigation" in app,
            "const navigateRootSection" in app,
            "sectionSnapshots.current.get(targetKey)" in app,
            "browsingSectionKey(current) === targetKey" in app,
            "rememberSectionNavigation(current);" in app,
            "api.browse(rootId, parentNodeId)" in app,
            "api.listFavoriteFolderNodes(selectedFavoriteFolderId)" in app,
            "validSearchRootId" in app,
            'liveNavigation.page === "search" && liveNavigation.searchRootId === root.id' in app,
            "[initialQuery, rootId]" in search_page,
            "searchRequest" in search_page,
            "withoutRemovedRoot" in app,
            "withoutRemovedFavoriteFolder" in app,
            "onSelectRoot={navigateRootSection}" in app,
            "patchNavigationSnapshot(snapshot, nodeId, update)" in app,
            "contentScrollRef.current.scrollTop = 0" not in app,
        )
    )
    if not section_restore_ok:
        fail("sidebar browsing sections do not preserve independent in-session collection snapshots")
    else:
        passed("sidebar browsing sections restore independent scroll, filter, sort, and view snapshots")

    sort_app_settings_match = re.search(r"pub struct AppSettings\s*\{(?P<body>.*?)\n\}", rust_models, re.S)
    collection_sort_memory_ok = all(
        (
            "pub enum CollectionSort" in rust_models,
            "pub enum CollectionSortScope" in rust_models,
            "pub struct CollectionSortPreferences" in rust_models,
            'Self::All => "collection_sort_all"' in rust_models,
            'Self::Browse => "collection_sort_browse"' in rust_models,
            'Self::Favorites => "collection_sort_favorites"' in rust_models,
            "pub fn get_collection_sort_preferences" in rust_database,
            "pub fn update_collection_sort_preference" in rust_database,
            "get_collection_sort_preferences" in rust_commands,
            "update_collection_sort_preference" in rust_commands,
            "commands::get_collection_sort_preferences" in rust_lib,
            "commands::update_collection_sort_preference" in rust_lib,
            "getCollectionSortPreferences" in frontend_api,
            "updateCollectionSortPreference" in frontend_api,
            "api.getCollectionSortPreferences()" in app,
            "sortSaveQueue" in app,
            "sortSaveRevisions" in app,
            "sourceSequence" in app,
            "displayedSortPreferences" in app,
            "navigationSnapshots.current.set(sourceSequence" in app,
            "onSort={changeAllSort}" in app,
            "onSort={changeBrowseSort}" in app,
            "onSort={changeFavoriteSort}" in app,
            'setBrowseSort("title-asc")' not in app,
            'setFavoriteSort("title-asc")' not in app,
            sort_app_settings_match is not None
            and "collection_sort" not in sort_app_settings_match.group("body"),
            "collection_sort_preferences_are_independent_and_survive_settings_saves" in rust_database,
        )
    )
    if not collection_sort_memory_ok:
        fail("per-collection sort memory is not independently persisted and restored")
    else:
        passed("All/Browse/Favorites sort choices persist independently without stale settings snapshots")

    delayed_search_scroll_ok = all(
        (
            'if (page === "search") return;' in app,
            "onResultsReady={applyPendingScroll}" in app,
            "useLayoutEffect" in search_page,
            "onResultsReady?.()" in search_page,
        )
    )
    if not delayed_search_scroll_ok:
        fail("search-result navigation restores scroll before asynchronous results are ready")
    else:
        passed("search-result navigation delays scroll restoration until result layout is ready")

    settings_unmount_guard_ok = all(
        (
            "onPersistenceFailure(candidate, rollback, errorMessage(error), appearanceRevision)"
            in settings_page,
            "parentAppearanceRevision.current = onAppearanceChange(next)" in settings_page,
            "const handleSettingsPersistenceFailure = useCallback" in app,
            "settingsAppearanceRevision.current === appearanceRevision" in app,
            "onPersistenceFailure={handleSettingsPersistenceFailure}" in app,
        )
    )
    if not settings_unmount_guard_ok:
        fail("settings autosave lacks parent-owned revision-safe rollback after unmount")
    else:
        passed("settings autosave rolls back through a revision-safe parent callback after unmount")

    context_menu_viewport_ok = all(
        (
            "getBoundingClientRect" in context_menu,
            "window.innerHeight - bounds.height" in context_menu,
            "max-height: calc(100vh - 16px)" in css,
            "overflow-y: auto" in css,
        )
    )
    if not context_menu_viewport_ok:
        fail("node context menu is not measured and constrained to the current viewport")
    else:
        passed("node context menu uses measured viewport placement and bounded scrolling")

    tag_commands = (
        "list_user_tags",
        "create_or_assign_user_tag",
        "assign_user_tag",
        "rename_user_tag",
        "unassign_user_tag",
        "delete_user_tag",
    )
    missing_tag_contract = [command for command in tag_commands if command not in frontend_api]
    tag_ui_ok = all(
        (
            'case "tags"' in app,
            "TagManagerDialog" in app,
            'action("tags")' in context_menu,
            "createOrAssignUserTag" in tag_dialog,
            "renameUserTag" in tag_dialog,
            "unassignUserTag" in tag_dialog,
            "deleteUserTag" in tag_dialog,
            "userTags" in frontend_models,
            "user-tag-pill" in media_card,
            "filter.allTags" in tag_filter,
            "node.userTags" in tag_filter,
            "onChange(null)" in tag_filter,
            "tagFilterId" in all_page,
            "tagFilterId" in browse_page,
            "allTagFilterId" in app,
            "browseTagFilterId" in app,
        )
    )
    if missing_tag_contract or not tag_ui_ok:
        fail(f"custom user-tag UI/API contract is incomplete: {missing_tag_contract}")
    else:
        passed("persistent custom tag management, text matching, filtering, and navigation snapshot contract")

    tag_select_matches_sort = all(
        (
            'className="sort-field tag-filter-field"' in tag_filter,
            ".sort-field select" in css,
            ".tag-filter-field select" in css,
            ".sort-field select option" in css,
        )
    )
    if not tag_select_matches_sort:
        fail("All tags selector does not share the title-sort dropdown structure and theme")
    else:
        passed("All tags selector shares the title-sort dropdown structure and theme")

    existing_match_and_edit_ok = all(
        (
            'call<ScanStarted>("match_existing_content"' in frontend_api,
            "pub fn match_existing_content" in rust_commands,
            "commands::match_existing_content" in rust_lib,
            "run_existing_content_match" in read("src-tauri/src/scanner.rs"),
            "list_bangumi_match_candidates" in rust_database,
            "finishedScanIds" in app,
            "const currentStatus = await api.scanStatus()" in app,
            "currentStatus?.scanId === started.scanId" in app,
            't("all.matchExisting")' in all_page,
            't("all.scanAndMatch")' in all_page,
            all_page.find('t("all.matchExisting")') < all_page.find('t("all.scanAndMatch")'),
            "editMode" in all_page and "editMode" in browse_page,
            "selectedNodeIds" in all_page and "selectedNodeIds" in browse_page,
            "SelectionToolbar" in all_page and "SelectionToolbar" in browse_page,
            'action("other")' in batch_context_menu,
            'action("ignore")' in batch_context_menu,
            'action("rematch")' in batch_context_menu,
            "batchAssignTag" in batch_tag_dialog,
            "batchCreateAndAssignTag" in batch_tag_dialog,
            "batch_set_node_type" in rust_commands,
            "batch_reset_node_type" in rust_commands,
            "batch_assign_tag" in rust_commands,
            "batch_create_and_assign_tag" in rust_commands,
            "TransactionBehavior::Immediate" in rust_database,
            't("selection.rematchSelected")' in selection_toolbar,
        )
    )
    if not existing_match_and_edit_ok:
        fail("existing-content matching or transactional multi-select edit-mode contract is incomplete")
    else:
        passed("existing-content matching and transactional multi-select edit mode")

    bound_title_sort_and_local_patch_ok = all(
        (
            "nodeDisplayTitle(a, language)" in frontend_format,
            "nodeDisplayTitle(b, language)" in frontend_format,
            "naturalCollators" in frontend_format,
            "patchNodeEverywhere" in app,
            "nodeWithBinding" in app,
            "patchNodeEverywhere(binding.nodeId" in app,
            "patchNodeEverywhere(node.id" in app,
            "nodeWithRefreshedCover" in app,
            'case "clear-bangumi"' in app,
            'case "clear-cover"' in app,
            "const coverRevision = 0" in app,
            "memo(MediaCardComponent)" in media_card,
            "clientCoverRevision" in frontend_models,
            "clientCoverRevision" in cover_hook,
            "subjectId" in cover_hook,
            "coverPath" in cover_hook,
        )
    )
    if not bound_title_sort_and_local_patch_ok:
        fail("bound localized-title sorting or single-Node cover/binding refresh optimization is incomplete")
    else:
        passed("bound localized-title sorting and single-Node cover/binding refresh optimization")

    cover_command_region = rust_commands[
        rust_commands.find("pub fn get_cover_data_url") : rust_commands.find("pub fn get_settings")
    ]
    startup_cover_loading_ok = all(
        (
            "cover_read_context" in cover_command_region,
            "get_node" not in cover_command_region,
            "list_roots" not in cover_command_region,
            "pub fn cover_read_context" in rust_database,
            "SELECT cover_cache_path FROM nodes" in rust_database,
            "SELECT path FROM library_roots" in rust_database,
            "MAX_CONCURRENT_COVER_REQUESTS = 4" in cover_hook,
            "MAX_RESOLVED_COVER_ENTRIES = 128" in cover_hook,
            "MAX_RESOLVED_COVER_CHARACTERS = 32 * 1024 * 1024" in cover_hook,
            "inFlightCoverRequests" in cover_hook,
            "resolvedCoverUrls" in cover_hook,
            "rememberResolvedCover" in cover_hook,
            "coverRequests.size > 600" not in cover_hook,
            "scheduleCoverRequest" in cover_hook,
            "Route changes must not cancel work" in cover_hook,
            "warms the cross-page LRU" in cover_hook,
            "resolvedCoverEvictionListeners" in cover_hook,
            "subscribeResolvedCoverEviction" in cover_hook,
            "notifyResolvedCoverEvicted" in cover_hook,
            "resolvedCoverEvictionListeners.delete(key)" in cover_hook,
            "resolvedCoverEvictionListeners.get(key) === listeners" in cover_hook,
            "latestCoverKeyByNode" in cover_hook,
            "if (latestCoverKeyByNode.get(nodeId) === key) rememberResolvedCover(key, url)" in cover_hook,
            "resolvedCoverUrls.get(cacheKey)" in cover_hook,
            "useLayoutEffect(() =>" in cover_hook,
            "if (enabled || !hasCachedCover || !cacheKey) return" in cover_hook,
            "evictedCacheKey !== cacheKey" in cover_hook,
            "cancelIfQueued" not in cover_hook,
            "subscribers" not in cover_hook,
            "usePosterViewportLifecycle" in media_card,
            "IntersectionObserver" in poster_viewport_hook,
            'target.closest(".content-scroll")' in poster_viewport_hook,
            "MAX_RETAINED_POSTERS = 64" in poster_viewport_hook,
            "retainedPosters = new Map" in poster_viewport_hook,
            "trimRetainedPosters" in poster_viewport_hook,
            "!poster.nearViewport" in poster_viewport_hook,
            "activationMarginPx: 1_000" in media_card,
            "retentionEnabled: hasCachedCover" in media_card,
            "retentionMarginPx: 1_800" in media_card,
            "updateRetainedPosterProximity" in poster_viewport_hook,
            'image.decoding = "async"' in poster_image,
            "canvasReady" in poster_image,
            "poster-image-preview" in poster_image,
            'loading={active ? "eager" : "lazy"}' in poster_image,
            'loading="lazy"' not in media_card,
            "new IntersectionObserver" in poster_viewport_hook,
            "!enabled || state.key !== cacheKey || state.loading" in cover_hook,
        )
    )
    if not startup_cover_loading_ok:
        fail("startup cover loading can regress to eager full-Node/root-stat IPC or deferred-cover failure state")
    else:
        passed("cross-page poster preview cache, bounded lazy cover IPC, and lightweight safety context")

    scan_listener_region = app[
        app.find("const handleScanProgress") : app.find("const selectRoot")
    ]
    scan_listener_lifecycle_ok = all(
        (
            "scanProgressHandlerRef" in scan_listener_region,
            "scanFinishedHandlerRef" in scan_listener_region,
            "let disposed = false" in scan_listener_region,
            "if (disposed) unlisten()" in scan_listener_region,
            "unlisteners.splice(0)" in scan_listener_region,
            "if (finishedIds.has(progress.scanId)) return" in scan_listener_region,
            "api.scanStatus()" in scan_listener_region,
            'current.status === "RUNNING" || current.status === "CANCELLING"' in scan_listener_region,
            app.count("onScanProgress(") == 1,
            app.count("onScanFinished(") == 1,
        )
    )
    if not scan_listener_lifecycle_ok:
        fail("scan event subscriptions can leak, duplicate completion effects, or miss a terminal registration-window event")
    else:
        passed("single-lifetime scan listeners close async cleanup gaps and reconcile terminal registration races")

    list_all_resources_region = rust_database[
        rust_database.find("pub fn list_all_resources") : rust_database.find("pub fn record_node_watched")
    ]
    bulk_metadata_hydration_ok = all(
        (
            re.search(r"hydrate_nodes_metadata_conn\(&?connection,\s*&mut nodes\)", list_all_resources_region) is not None,
            "self.read_snapshot" in list_all_resources_region,
            "get_binding_conn" not in list_all_resources_region,
            "list_node_tags_conn" not in list_all_resources_region,
            "const NODE_METADATA_CHUNK_SIZE: usize = 500" in rust_database,
            "fn hydrate_nodes_metadata_conn" in rust_database,
            "node_ids.chunks(NODE_METADATA_CHUNK_SIZE)" in rust_database,
            "bulk_card_metadata_hydration_preserves_bindings_and_natural_tag_order_across_chunks" in rust_database,
        )
    )
    if not bulk_metadata_hydration_ok:
        fail("All Resources metadata hydration can regress to per-Node binding/tag SQL queries")
    else:
        passed("All Resources and Browse hydrate bindings/tags in bounded SQL batches")

    search_region = rust_database[
        rust_database.find("pub fn search(&self") : rust_database.find("pub fn list_unbound_bangumi_candidates")
    ]
    batched_search_hydration_ok = all(
        (
            "transaction_with_behavior(TransactionBehavior::Deferred)" in search_region,
            "load_node_rows_by_ids_conn(" in search_region,
            "hydrate_nodes_metadata_conn(&transaction, &mut nodes)" in search_region,
            "get_binding_conn" not in search_region,
            "list_node_tags_conn" not in search_region,
            "batched_search_hydration_matches_legacy_results_for_many_unicode_hits" in rust_database,
        )
    )
    if not batched_search_hydration_ok:
        fail("Search can regress to torn metadata snapshots or per-hit binding/tag SQL queries")
    else:
        passed("Search batches file-owner Nodes and metadata inside one deferred SQLite snapshot")

    search_cover_ok = all(
        (
            "function SearchResult" in search_page,
            "useCoverDataUrl(hit.node, coverRevision, coverRequested)" in search_page,
            "<PosterImage active={coverVisible}" in search_page,
            "usePosterViewportLifecycle" in search_page,
            "activationMarginPx: 800" in search_page,
            "retentionEnabled: hasCachedCover" in search_page,
            "retentionMarginPx: 1_400" in search_page,
            "ref={resultRef}" in search_page,
            'className={`search-hit-cover' in search_page,
            ".search-hit-cover .poster-image" in css,
        )
    )
    empty_search_placeholders = i18n.count('"search.placeholder": ""') >= 4
    old_search_examples = any(
        example in i18n for example in ("齐木楠雄的灾难", "命运石之门", "叛逆的鲁路修")
    )
    if not search_cover_ok or not empty_search_placeholders or old_search_examples:
        fail("search cover rendering or empty four-locale placeholder contract is incomplete")
    else:
        passed("search results use Node-ID covers and all four search placeholders are empty")

    recent_watch_ok = all(
        (
            'page === "recent"' in app,
            'onNavigate("recent")' in sidebar,
            "listRecentlyWatched" in frontend_api,
            "RecentlyWatchedEntry" in frontend_models,
            "RecentlyWatchedPage" in recent_page,
            "watchedAtByNodeId" in recent_page,
            "watchedAt={props.watchedAtByNodeId?.get(node.id)}" in poster_grid,
            'className="media-card-watch-time"' in media_card,
            't("recent.watchedAt"' in media_card,
        )
    )
    recent_locale_keys = (
        "sidebar.recentlyWatched",
        "recent.title",
        "recent.watchedAt",
        "error.recentFailed",
    )
    missing_recent_locales = [key for key in recent_locale_keys if i18n.count(f'"{key}"') < 4]
    if not recent_watch_ok or missing_recent_locales:
        fail(f"Recently Watched UI/API/i18n contract is incomplete: {missing_recent_locales}")
    else:
        passed("Recently Watched sidebar, poster time, API, and four-locale contract")

    favorite_commands = (
        "listFavoriteFolders",
        "createFavoriteFolder",
        "renameFavoriteFolder",
        "deleteFavoriteFolder",
        "listFavoriteFolderNodes",
        "batchAddNodesToFavorite",
        "batchRemoveNodesFromFavorite",
    )
    missing_favorite_commands = [command for command in favorite_commands if command not in frontend_api]
    favorite_ui_ok = all(
        (
            'page === "favorites"' in app,
            'onNavigate("favorites")' in sidebar,
            "FavoritesPage" in favorite_page,
            "favoriteNodes" in app,
            "previous.favoriteNodes" in app,
            'case "favorites"' in app,
            'action("favorites")' in context_menu,
            'action("favorites")' in batch_context_menu,
            "onFavorites" in selection_toolbar,
            "batchAddNodesToFavorite" in favorite_assignment,
            "createFavoriteFolder" in favorite_assignment,
            "FavoriteFolderDialog" in favorite_folder_dialog,
            "onRemoveFromFavorite" in selection_toolbar,
            "onRemoveSelected" in favorite_page,
            "favorite-folder-grid" in css,
        )
    )
    favorite_locale_keys = (
        "sidebar.favorites",
        "menu.addToFavorites",
        "selection.addToFavorites",
        "favorites.title",
        "favorites.createTitle",
        "favorites.folderEmptyTitle",
        "error.favoritesFailed",
    )
    missing_favorite_locales = [key for key in favorite_locale_keys if i18n.count(f'"{key}"') < 4]
    if missing_favorite_commands or not favorite_ui_ok or missing_favorite_locales:
        fail(
            "Favorites UI/API/history/i18n contract is incomplete: "
            f"commands={missing_favorite_commands}, locales={missing_favorite_locales}"
        )
    else:
        passed("named Favorites sidebar, CRUD, history, batch actions, and four-locale contract")

    bilibili_url = "https://space.bilibili.com/2903441"
    x_url = "https://x.com/f_undermori"
    about_backend_ok = all(
        (
            f'const BILIBILI_URL: &str = "{bilibili_url}";' in rust_commands,
            f'const X_URL: &str = "{x_url}";' in rust_commands,
            "website_url: BILIBILI_URL" in rust_commands,
            "x_url: X_URL" in rust_commands,
            "allowed_external_url(&url)" in rust_commands,
            "Some(BILIBILI_URL)" in rust_commands,
            "Some(X_URL)" in rust_commands,
            "player::open_external_url(allowed_url)" in rust_commands,
            "pub website_url: &'static str" in rust_models,
            "pub x_url: &'static str" in rust_models,
            "websiteUrl: string" in frontend_models,
            "xUrl: string" in frontend_models,
        )
    )
    about_frontend_ok = all(
        (
            'className="about-author-links"' in settings_page,
            "openAuthorLink(bootstrap?.websiteUrl)" in settings_page,
            "openAuthorLink(bootstrap?.xUrl)" in settings_page,
            't("settings.websiteLabel")' in settings_page,
            't("settings.xLabel")' in settings_page,
            't("settings.originalAuthor")' in settings_page,
            't("settings.contributor")' in settings_page,
            "Juvenile_A" in settings_page,
            bilibili_url not in settings_page,
            x_url not in settings_page,
        )
    )
    about_locale_values = {
        match.group(1)
        for match in re.finditer(r'"settings\.website":\s*"([^"]*)"', i18n)
    }
    expected_about_locale_values = {
        "关注作者",
        "Follow the author",
        "作者をフォロー",
        "제작자 팔로우",
    }
    about_i18n_ok = all(
        (
            about_locale_values == expected_about_locale_values,
            i18n.count('"settings.websiteLabel"') == 4,
            i18n.count('"settings.originalAuthor"') == 4,
            i18n.count('"settings.contributor"') == 4,
            i18n.count('"settings.xLabel": "Undermori · X"') == 4,
            "官网" not in i18n,
        )
    )
    if not about_backend_ok or not about_frontend_ok or not about_i18n_ok:
        fail("About author links are not a typed two-link UI backed by the exact Rust allowlist")
    else:
        passed("About author links use exact Bilibili/X allowlisting and four-locale labels")

    page_description_rules = [
        match.group("body")
        for match in re.finditer(
            r"\.page-title-row\s*>\s*div:first-child\s*>\s*p:last-child\s*\{(?P<body>[^}]*)\}",
            css,
            re.S,
        )
    ]
    page_description_css = "\n".join(page_description_rules)
    page_description_wrap_ok = all(
        (
            bool(page_description_rules),
            re.search(r"white-space\s*:\s*normal", page_description_css) is not None,
            re.search(r"overflow-wrap\s*:\s*anywhere", page_description_css) is not None,
            re.search(r"white-space\s*:\s*nowrap", page_description_css) is None,
            re.search(r"text-overflow\s*:\s*ellipsis", page_description_css) is None,
            re.search(r"overflow\s*:\s*hidden", page_description_css) is None,
        )
    )
    favorite_open_start = app.find("const openFavoriteFolder")
    favorite_open_end = app.find("\n  useEffect", favorite_open_start)
    favorite_open_region = (
        app[favorite_open_start:favorite_open_end]
        if favorite_open_start >= 0 and favorite_open_end > favorite_open_start
        else ""
    )
    favorite_back_history_ok = all(
        (
            "captureNavigation()" in favorite_open_region,
            "rememberNavigation(returnSnapshot)" in favorite_open_region,
            "queueScroll(0)" in favorite_open_region,
            "onBack={goBack}" in app,
            "closeFavoriteFolder" not in app,
            "window.history.back()" in app,
        )
    )
    favorite_button_match = re.search(
        r"button\.favorites-location\s*\{(?P<body>[^}]*)\}", css, re.S
    )
    favorite_button_body = favorite_button_match.group("body") if favorite_button_match else ""
    favorite_button_style_ok = all(
        (
            favorite_button_match is not None,
            re.search(r"border\s*:\s*0", favorite_button_body) is not None,
            re.search(r"border-radius\s*:\s*0", favorite_button_body) is not None,
            re.search(r"background\s*:\s*transparent", favorite_button_body) is not None,
            "box-shadow" not in favorite_button_body,
            'className="all-resources-location favorites-location"' in favorite_page,
        )
    )
    if not page_description_wrap_ok or not favorite_back_history_ok or not favorite_button_style_ok:
        fail("page-description wrapping or Favorites history/back-button visual contract is incomplete")
    else:
        passed("page descriptions wrap fully and Favorites back restores history with transparent styling")

    favorite_folder_descriptions = {
        match.group(1)
        for match in re.finditer(r'"favorites\.folderDescription":\s*"([^"]*)"', i18n)
    }
    expected_favorite_folder_descriptions = {
        "移出收藏夹不会删除索引或硬盘文件。",
        "Removing one never deletes its index or files.",
        "外してもインデックスやファイルは削除されません。",
        "제거해도 색인이나 파일은 삭제되지 않습니다.",
    }
    if favorite_folder_descriptions != expected_favorite_folder_descriptions:
        fail("Favorites folder descriptions must contain only the four localized removal-safety sentences")
    else:
        passed("Favorites folder descriptions omit the redundant work-introduction sentence in all locales")

    solid_responsive_background_ok = all(
        (
            re.search(r"\.content-scroll\s*\{[^}]*overflow-x:\s*hidden", css) is not None,
            re.search(r"\.content-scroll\s*\{[^}]*overflow-y:\s*auto", css) is not None,
            re.search(r"\.(?:app-shell|search-hero|onboarding-page)\s*\{[^}]*radial-gradient", css) is None,
            "flex-wrap: wrap" in css,
        )
    )
    if not solid_responsive_background_ok:
        fail("page background or responsive overflow contract can regress to glow/horizontal scrolling")
    else:
        passed("solid theme backgrounds and vertical-only responsive content scrolling")

    tauri_config = json.loads(read("src-tauri/tauri.conf.json"))
    main_window = tauri_config.get("app", {}).get("windows", [{}])[0]
    app_settings_match = re.search(r"pub struct AppSettings\s*\{(?P<body>.*?)\n\}", rust_models, re.S)
    window_size_memory_ok = all(
        (
            'const WINDOW_SIZE_SETTING_KEY: &str = "main_window_size"' in rust_database,
            "pub fn get_window_size" in rust_database,
            "pub fn save_window_size" in rust_database,
            "WindowEvent::Resized" in rust_lib,
            "WindowEvent::ScaleFactorChanged" in rust_lib,
            "RunEvent::ExitRequested" in rust_lib,
            rust_lib.find("commands::cancel_scan_on_exit(app);")
            < rust_lib.find("state.database.save_window_size(size)"),
            "is_minimized" in window_state,
            "is_maximized" in window_state,
            "is_fullscreen" in window_state,
            "monitor.work_area()" in window_state,
            "MIN_WINDOW_WIDTH: u32 = 900" in window_state,
            "MIN_WINDOW_HEIGHT: u32 = 640" in window_state,
            main_window.get("minWidth") == 900,
            main_window.get("minHeight") == 640,
            app_settings_match is not None
            and "window" not in app_settings_match.group("body").lower(),
        )
    )
    if not window_size_memory_ok:
        fail("native main-window size persistence, DPI/work-area validation, or shutdown ordering is incomplete")
    else:
        passed("native main-window size memory uses one exit write with DPI/work-area safety")

    first_frame_window_restore_ok = all(
        (
            main_window.get("visible") is False,
            "pub fn show_main_window" in rust_commands,
            "commands::show_main_window" in rust_lib,
            'showMainWindow: () => call<void>("show_main_window")' in frontend_api,
            "startupPresentationReady" in app,
            "setStartupPresentationReady(true)" in app,
            "api.showMainWindow()" in app,
            'useLayoutEffect(() => {\n    const media = window.matchMedia' in app,
            app.find('useLayoutEffect(() => {\n    const media = window.matchMedia')
            < app.find("api.showMainWindow()"),
            "resolve_startup_size" in window_state,
            rust_lib.find("window_state::restore_window_size")
            < rust_lib.find("app.manage(AppState"),
        )
    )
    if not first_frame_window_restore_ok:
        fail("main window can become visible before restored size and themed React shell are ready")
    else:
        passed("main window stays hidden until safe size restore and themed first React frame")

    other_resource_action_ok = all(
        (
            '| "other"' in context_menu,
            'action("other")' in context_menu,
            'case "other"' in app,
            'api.setNodeType(node.id, "MIXED")' in app,
            '"WORK" | "CONTAINER" | "MIXED"' in frontend_api,
        )
    )
    if not other_resource_action_ok:
        fail("Set as Other resources does not reuse the compatible MIXED node type")
    else:
        passed("Set as Series/Other resources UI preserves compatible CONTAINER/MIXED storage")

    if 'hit.node.nodeType === "MIXED" ? "archive"' not in search_page:
        fail("Other resources search hits still use the Work icon")
    else:
        passed("Other resources search hits use the archive icon")

    if "binding-dot" in media_card or "manual-pill" in media_card:
        fail("poster still renders a circular binding/manual check badge")
    elif "nodeTypeLabel" in media_card:
        fail("poster still renders the full automatic node-type label set")
    elif not all(key in media_card for key in ("card.systemWork", "card.systemSeries", "card.systemOtherResources")):
        fail("poster system labels are not limited to Work/Series/Other resources")
    else:
        passed("poster badges reduced to Work/Series/Other resources with no Bangumi check badge")

    css_without_comments = re.sub(r"/\*.*?\*/", "", css, flags=re.DOTALL)
    poster_css_rules: list[tuple[str, dict[str, str]]] = []
    for match in re.finditer(
        r"(?P<selectors>[^{}]+)\{(?P<body>[^{}]*)\}",
        css_without_comments,
        re.DOTALL,
    ):
        declarations: dict[str, str] = {}
        for declaration in match.group("body").split(";"):
            if ":" not in declaration:
                continue
            property_name, value = declaration.split(":", 1)
            declarations[property_name.strip().lower()] = re.sub(
                r"\s+", " ", value.strip().lower()
            )
        for selector in match.group("selectors").split(","):
            normalized_selector = re.sub(r"\s+", " ", selector.strip().lower())
            normalized_selector = re.sub(r"\s*>\s*", " > ", normalized_selector)
            if normalized_selector:
                poster_css_rules.append((normalized_selector, declarations))

    def poster_image_match(selector: str, target: str, class_name: str | None = None) -> bool:
        suffix = rf"\.{re.escape(class_name)}" if class_name else ""
        return re.search(
            rf"{re.escape(target)}(?:\s*>\s*|\s+)img{suffix}$",
            selector,
        ) is not None

    def any_poster_image_match(selector: str, target: str) -> bool:
        compound_suffix = r"(?:[.#][\w-]+|\[[^\]]+\]|:{1,2}[\w-]+(?:\([^)]*\))?)*"
        image_compound = rf"img{compound_suffix}"
        direct_poster_image = re.search(
            rf"{re.escape(target)}{compound_suffix}(?:\s*>\s*|\s+){image_compound}$",
            selector,
        ) is not None
        card_button_image = re.search(
            rf"(?:^|[ >+~])\.media-card-open{compound_suffix}(?:\s*>\s*|\s+){image_compound}$",
            selector,
        ) is not None
        globally_known_wide_image = (
            re.search(r"(?:^|[ >+~])img(?:[.#][\w-]+)*\.is-wide-artwork(?:[:\[].*)?$", selector)
            is not None
            or re.search(r"(?:^|[ >+~])\.is-wide-artwork(?:[:\[].*)?$", selector)
            is not None
        )
        return direct_poster_image or card_button_image or globally_known_wide_image

    def base_poster_declarations(target: str) -> dict[str, str]:
        merged: dict[str, str] = {}
        for selector, declarations in poster_css_rules:
            if poster_image_match(selector, target):
                merged.update(declarations)
        return merged

    def wide_poster_declarations(target: str) -> dict[str, str]:
        merged: dict[str, str] = {}
        for selector, declarations in poster_css_rules:
            if poster_image_match(selector, target, "is-wide-artwork"):
                merged.update(declarations)
        return merged

    cover_image_declarations = base_poster_declarations(".cover-frame")
    detail_image_declarations = base_poster_declarations(".detail-cover")
    wide_cover_declarations = wide_poster_declarations(".cover-frame")
    wide_detail_declarations = wide_poster_declarations(".detail-cover")

    motion_properties = {
        "transform",
        "-webkit-transform",
        "translate",
        "scale",
        "rotate",
        "animation",
        "animation-name",
    }
    transformed_poster_selectors: list[str] = []
    degraded_interpolation: list[str] = []
    for selector, declarations in poster_css_rules:
        poster_surface = (
            ".cover-frame" in selector
            or ".detail-cover" in selector
            or re.search(
                r"(?:^|[ >+~])\.media-card(?:-list|-grid|-open)?(?=$|[.:\[ >+~])", selector
            )
            is not None
        )
        if poster_surface and (
            any(property_name in motion_properties for property_name in declarations)
            or any(
                property_name in {"transition", "transition-property", "will-change"}
                and re.search(r"\b(?:transform|translate|scale|rotate)\b", value)
                is not None
                for property_name, value in declarations.items()
            )
        ):
            transformed_poster_selectors.append(selector)
        if any(
            any_poster_image_match(selector, target)
            for target in (".cover-frame", ".detail-cover")
        ):
            interpolation = declarations.get("image-rendering")
            if interpolation is not None and interpolation != "auto":
                degraded_interpolation.append(selector)

    poster_image_quality_ok = all(
        (
            not transformed_poster_selectors,
            not degraded_interpolation,
            "shouldContainPosterArtwork" in poster_helper,
            "posterRenderLayout" in poster_helper,
            re.search(r"naturalWidth\s*/\s*naturalHeight", poster_helper) is not None,
            "<PosterImage" in media_card,
            "<PosterImage" in work_detail,
            'resizeQuality: "high"' in poster_image,
            'context.imageSmoothingQuality = "high"' in poster_image,
            "context.imageSmoothingEnabled = true" in poster_image,
            "window.devicePixelRatio" in poster_image,
            "MAX_DEVICE_PIXEL_RATIO = 2" in poster_image,
            "MAX_DOWNSCALE_RATIO_PER_PASS = 2" in poster_image,
            "createProgressivelyDownscaledBitmap" in poster_image,
            "const firstWidth = nextDimension(sourceWidth, destinationWidth)" in poster_image,
            "resizeWidth: firstWidth" in poster_image,
            "shouldCancel: () => boolean" in poster_image,
            "if (cancelled())" in poster_image,
            "next.close()" in poster_image,
            "current.close()" in poster_image,
            "MAX_CONCURRENT_POSTER_RENDERS = 2" in poster_image,
            "MAX_CACHED_POSTER_BITMAPS = 128" in poster_image,
            "MAX_CACHED_POSTER_BITMAP_BYTES = 128 * 1024 * 1024" in poster_image,
            "cachedPosterBitmaps" in poster_image,
            "cachedPosterBitmapBytes" in poster_image,
            "getCachedPosterBitmap" in poster_image,
            "rememberCachedPosterBitmap" in poster_image,
            "existing.bitmap.close()" in poster_image,
            "bytes: bitmap.width * bitmap.height * 4" in poster_image,
            "drawCachedPosterBitmap" in poster_image,
            "isInsideVisibleScrollport" in poster_image,
            "renderGenerationRef" in poster_image,
            "renderGenerationRef.current !== renderGeneration" in poster_image,
            "useLayoutEffect(() => () =>" in poster_image,
            poster_image.count("renderGenerationRef.current += 1") >= 2,
            "key={cacheKey}" in poster_image,
            "errorHandlerRef.current()" in poster_image,
            poster_image.count("errorHandlerRef.current()") == 1,
            "posterRenderQueue.indexOf(queued)" in poster_image,
            "scheduledRenderTask?.cancel()" in poster_image,
            "devicePixelContentBoxSize" in poster_image,
            'box: "device-pixel-content-box"' in poster_image,
            "new ResizeObserver((entries)" in poster_image,
            'window.addEventListener("resize", scheduleRender)' in poster_image,
            'resolutionQuery?.addEventListener("change", handleResolutionChange)' in poster_image,
            "bitmap?.close()" in poster_image,
            'image.removeAttribute("src")' in poster_image,
            "canvas.width = 1" in poster_image,
            "canvas.height = 1" in poster_image,
            "useLayoutEffect" in poster_image,
            "!canvasReady" in poster_image,
            "poster-image-preview" in poster_image,
            'loading={active ? "eager" : "lazy"}' in poster_image,
            "cacheKey={coverCacheKey}" in media_card,
            "cacheKey={coverCacheKey}" in search_page,
            "cacheKey={coverCacheKey}" in work_detail,
            "<PosterImage active={coverVisible}" in media_card,
            "<PosterImage active={coverVisible}" in search_page,
            re.search(
                r"\.poster-image\s*\{[^}]*display:\s*block;[^}]*width:\s*100%;[^}]*height:\s*100%;[^}]*image-rendering:\s*auto",
                css,
            ) is not None,
            ".poster-image-preview.is-wide-artwork" in css,
            re.search(
                r"\.cover-frame\s+\.poster-image,\s*\.detail-cover\s+\.poster-image,\s*\.search-hit-cover\s+\.poster-image\s*\{[^}]*position:\s*absolute;[^}]*inset:\s*0",
                css,
            ) is not None,
            re.search(r"\.cover-frame\s*\{[^}]*border:\s*0", css) is not None,
            re.search(r"\.detail-cover\s*\{[^}]*border:\s*0", css) is not None,
            ".cover-frame::after, .detail-cover::after" in css,
            re.search(
                r"\.poster-grid-grid\s*\{\s*grid-template-columns:\s*repeat\(auto-fill,\s*\d+px\)",
                css,
            )
            is not None,
            re.search(
                r"\.poster-grid-grid\s*\{[^}]*grid-template-columns:[^}]*\b1fr\b",
                css,
            )
            is None,
            "--poster-aspect-ratio: 2 / 2.82" in css,
            re.search(
                r"\.cover-frame\s*\{[^}]*aspect-ratio:\s*var\(--poster-aspect-ratio\)",
                css,
            )
            is not None,
            re.search(
                r"\.detail-cover\s*\{[^}]*aspect-ratio:\s*var\(--poster-aspect-ratio\)",
                css,
            )
            is not None,
            'loading="lazy"' not in media_card,
        )
    )
    if not poster_image_quality_ok:
        fail("poster images must use bounded DPR-aware high-quality sampling, shared frame geometry, one lazy-load gate, and no transformed hover surface")
    else:
        passed("bounded DPR-aware poster sampling, shared geometry, and non-transformed rendering contract")

    poster_overlay_controls_ok = (
        re.search(r"\.quick-bind\s*\{[^}]*z-index:\s*5", css) is not None
        and 'className="double-click-hint"' not in browse_page
        and ".double-click-hint" not in css
    )
    if not poster_overlay_controls_ok:
        fail(
            "poster card actions must render above the frame border and Browse must not repeat the video-play hint"
        )
    else:
        passed(
            "poster card actions stay above the frame border and Browse omits the duplicate video-play hint"
        )

    if "all.waterfall" in all_page:
        fail("All Resources page still renders the unified-waterfall eyebrow")
    else:
        passed("All Resources aggregation retained without unified-waterfall copy")

    localized_keys = (
        "common.sort",
        "sort.titleAsc",
        "sort.titleDesc",
        "sort.addedDesc",
        "sort.addedAsc",
        "menu.manageTags",
        "card.systemWork",
        "card.systemSeries",
        "card.systemOtherResources",
        "tags.title",
        "tags.deleteEverywhere",
        "all.matchExisting",
        "all.scanAndMatch",
        "selection.editMode",
        "selection.rematchSelected",
        "selection.applyTag",
        "selection.ignoreSelected",
    )
    missing_locales = [key for key in localized_keys if i18n.count(f'"{key}"') < 4]
    if missing_locales:
        fail(f"new navigation/tag copy is not translated in all four locales: {missing_locales}")
    else:
        passed("new sort and tag copy covered by all four locales")

    windows_fragments = {
        "native exact Explorer selection": (player, "SHOpenFolderAndSelectItems"),
        "literal Unicode shell PIDL": (player, "ILCreateFromPathW"),
        "explicit Windows ICO binding": (build, '.window_icon_path("icons/icon.ico")'),
        "custom Windows app manifest": (build, 'include_str!("windows-app-manifest.xml")'),
        "Per-Monitor V2 DPI awareness": (manifest, "PerMonitorV2,PerMonitor"),
    }
    missing_windows = [label for label, (source, token) in windows_fragments.items() if token not in source]
    if missing_windows:
        fail(f"Windows Explorer/icon/DPI contract is incomplete: {missing_windows}")
    else:
        passed("native exact Explorer selection and Per-Monitor V2 icon/DPI contract")


def check_durable_context() -> None:
    agents = read("AGENTS.md")
    product = read("docs/PRODUCT_SPEC.md")
    context = read("docs/PROJECT_CONTEXT.md")
    decisions = read("docs/DECISIONS.md")
    required_agent_tokens = [
        "M²Shelf",
        "M2Shelf",
        "docs/PRODUCT_SPEC.md",
        "docs/PROJECT_CONTEXT.md",
        "docs/DECISIONS.md",
        "source",
        "Bangumi",
        "zh-CN",
        "system",
        "custom cover cache",
        "icon-source.png",
        "User tags",
        "Alt+Left",
        "Per-Monitor V2",
        "Recently watched",
        "Favorites",
    ]
    required_context_tokens = [
        "Tauri",
        "React",
        "SQLite",
        "resource_files",
        "title_extractor.rs",
        "build_windows_release.ps1",
        "0007_favorite_folders.sql",
        "0008_bangumi_subject_type.sql",
        "watch_history",
        "node_favorite_folders",
        "lib/i18n.tsx",
        "useCoverDataUrl.ts",
    ]
    required_decision_tokens = [
        "只读",
        "Library Root",
        "Work / Container / Mixed",
        "Bangumi",
        "M²Shelf",
        "Portable",
        "zh-CN",
        "icon-source.png",
        "收藏夹",
        "最近观看",
        "shell",
    ]
    missing = []
    for label, source, tokens in (
        ("AGENTS.md", agents, required_agent_tokens),
        ("PROJECT_CONTEXT.md", context, required_context_tokens),
        ("DECISIONS.md", decisions, required_decision_tokens),
    ):
        absent = [token for token in tokens if token not in source]
        if absent:
            missing.append(f"{label}: {absent}")
    if "Container" not in product or "附件" not in product or "M²Shelf" not in product:
        missing.append("PRODUCT_SPEC.md is not synchronized with phase-2 behavior")
    if missing:
        fail(f"durable project context is incomplete: {missing}")
    else:
        passed("durable cross-account/agent project context")


def main() -> int:
    for relative, minimum in (
        ("AGENTS.md", 2_000),
        ("docs/PRODUCT_SPEC.md", 5_000),
        ("docs/PROJECT_CONTEXT.md", 3_000),
        ("docs/DECISIONS.md", 3_000),
        ("src-tauri/migrations/0001_initial.sql", 500),
        ("src-tauri/migrations/0002_mvp.sql", 500),
        ("src-tauri/migrations/0003_resources_and_cover_status.sql", 500),
        ("src-tauri/migrations/0004_multilingual_metadata.sql", 100),
        ("src-tauri/migrations/0005_user_tags.sql", 300),
        ("src-tauri/migrations/0006_watch_history.sql", 250),
        ("src-tauri/migrations/0007_favorite_folders.sql", 500),
        ("src-tauri/migrations/0008_bangumi_subject_type.sql", 80),
        ("src-tauri/migrations/0009_library_recognition_mode.sql", 80),
        ("src-tauri/windows-app-manifest.xml", 300),
        ("src-tauri/update-public-key.txt", 40),
        ("src-tauri/src/lib.rs", 100),
        ("src-tauri/src/commands.rs", 1_000),
        ("src-tauri/src/update.rs", 10_000),
        ("src-tauri/src/portable_update.rs", 10_000),
        ("src-tauri/src/bin/m2shelf_updater.rs", 2_000),
        ("tools/offline-key-init/Cargo.toml", 500),
        ("tools/offline-key-init/Cargo.lock", 2_000),
        ("tools/offline-key-init/src/main.rs", 8_000),
        ("tools/portable-key-tool/Cargo.toml", 800),
        ("tools/portable-key-tool/Cargo.lock", 100_000),
        ("tools/portable-key-tool/src/main.rs", 300),
        ("tools/portable-key-tool/src/commands.rs", 30_000),
        ("tools/portable-key-tool/src/key_container.rs", 10_000),
        ("tools/portable-key-tool/src/password.rs", 1_000),
        ("src/lib/api.ts", 1_000),
        ("src/components/TagManagerDialog.tsx", 2_000),
        ("scripts/build_portable.ps1", 1_000),
        ("scripts/build_offline_key_init.ps1", 3_000),
        ("scripts/build_windows_release.ps1", 1_000),
        ("scripts/generate_update_manifest.ps1", 3_000),
        ("scripts/sign_update_offline.ps1", 3_000),
        ("scripts/build_portable_key_tool.ps1", 5_000),
        ("scripts/sign_update_from_usb.ps1", 8_000),
        ("scripts/publish_signed_release.ps1", 3_000),
        ("scripts/build_brand_assets.ps1", 1_000),
        ("scripts/prepare_logo_source.ps1", 1_000),
        ("scripts/generate_icons.ps1", 1_000),
        ("docs/PORTABLE_README_zh-CN.txt", 500),
        (".github/workflows/windows-release.yml", 3_000),
    ):
        require_file(relative, minimum)

    check_json_and_toml()
    check_migrations()
    check_command_contract()
    check_source_safety()
    check_extensions_and_product_spec()
    check_tauri_security_configuration()
    check_bangumi_contract()
    check_phase2_contract()
    check_next_phase_contract()
    check_brand_release_and_icons()
    check_ui_windows_interaction_contract()
    check_durable_context()

    for message in PASSES:
        print(f"PASS  {message}")
    for message in ERRORS:
        print(f"FAIL  {message}", file=sys.stderr)
    print(f"\n{len(PASSES)} passed, {len(ERRORS)} failed")
    return 1 if ERRORS else 0


if __name__ == "__main__":
    raise SystemExit(main())
