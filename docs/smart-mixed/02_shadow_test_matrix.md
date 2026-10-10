# SMART_MIXED Phase 2：SQLite 适配与 Shadow 测试矩阵

最终执行：**42 PASS / 0 FAIL / 0 IGNORED**，其中 1 个 adapter 内部只读连接测试、41 个集成测试。所有测试使用 `.tmp/smart-mixed-phase2/` 下的合成当前 schema 数据库；没有个人 DB 或媒体读取。

## 1. 与 Phase 1 的区别

- Phase 1 的 49 个结构测试 + 35 个性质测试重新执行：84/84；其原源码、fixtures、报告字节不变。
- Phase 1 release CLI 49 个 golden 全部再次逐项对照，不能将这 49 个算成本轮新增 SQLite 测试。
- 本轮 42 个 Rust 测试从真实 schema 的 SQLite 行进入适配器，再进入 recognizer/diff，覆盖现有 ownership、FK、页序、扫描健康、WAL 快照等 Phase 1 内存 JSON 无法证明的行为。
- 另有 11 个 release SQLite CLI 场景重复执行、错误 CLI 校验、1k/10k/100k × 3 次新进程基准。它们是独立执行证据，不混算进 42 个 Rust test 数。

工厂：`tools/smart-mixed-shadow/src/fixture.rs:7` 使用全部 24 个当前生产 migration，`Factory::create:41` 拒绝覆盖旧文件，Root 为固定合成路径。空 DB 不需要生产旧数据 reclassification hook。没有伪造尚不存在的产品 enum；ARTBOOK、MOBI/AZW3 已在当前 schema/代码存在。

## 2. 任务 15 项要求的对应

| 要求 | 覆盖与断言 |
|---|---|
| 1. 两系列 + 100 PDF | fixture 03；104 真实 book IDs、2 Series、100 Work、102 Root 入口；旧首页 2 Node + 100 direct books |
| 2. 混合系列、多层格式 | fixture 49；7 IDs、PDF/CBZ/图片原路径与页序保留；6 提案 + 1 image fallback；部分系列 REVIEW |
| 3. 作者两部独立作品 | fixture 10；2 Work、1 Category、0 Series |
| 4. DOUJIN/EBOOK/ARTBOOK | fixtures 11/45/12；按实际 subtype 映射；所有来源保留，原画图片不冒充 verified |
| 5. 直属与子目录图片 | fixture 28；父/子两个 book IDs 和原页边界不混合 |
| 6. 核心/附件去重 | 核心 PDF 与 image pages 不再作普通附件；README/ZIP/未开 MOBI 保留；已开附件 FK 不升 Work |
| 7. 人工/绑定/忽略 | IGNORED 祖先、manual WORK/CONTAINER/MIXED、不同绑定、同绑定保守 fence、override 不可削弱 |
| 8. 同卷多格式/合集 | fixtures 20/22/42；每个真实 ID 独立；不合并进度 |
| 9. 状态只读 | seed 进度、书签、history、favorites、tags、binding、manual cover/settings；全合成 DB 逻辑摘要与字节不变 |
| 10. 旧模式/视频隔离 | 两种 mode 原值不变；三种 video Root 在书籍查询前拒绝 |
| 11. 缺卷/裸数字/Unicode | fixtures 19/17/35/21、合成路径对照；Review/独立保留，无遗漏/重复 |
| 12. 重复/乱序/override | 重复快照和序列化相同；倒序插入按路径投影相同；仅内存 overrides 高优先级 |
| 13. 部分/离线/取消 | 合成 health + scan_runs；所有来源保留、REVIEW，无 destructive cleanup |
| 14. 错误/stale/rollback | 实际 SQLite progress interrupt、WAL 并发提交、旧 expected_version、pin 后 fault、连接复用 |
| 15. 大集合 | 独立 release CLI 1k/10k/100k × 3，新进程峰值与 adapter/recognize/diff 分时 |

## 3. 全部 Rust 测试索引

下表的源码行号由最终源码提取，行为断言以实际测试为准。

| # | 实际 test 符号 | 源码位置 | 结果 |
|---:|---|---|---|
| 1 | `actual_adapter_connection_is_query_only_and_read_only` | `tools/smart-mixed-shadow/src/adapter.rs:812` | PASS |
| 2 | `two_series_and_100_independent_pdfs_are_104_real_sources` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:97` | PASS |
| 3 | `mixed_series_preserves_png_order_and_all_seven_ids` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:112` | PASS |
| 4 | `author_is_two_independent_works_not_a_series` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:136` | PASS |
| 5 | `doujin_artist_ebook_author_and_artbook_raw_images_keep_sources` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:146` | PASS |
| 6 | `direct_and_nested_image_collections_remain_separate_fallback_books` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:161` | PASS |
| 7 | `core_resources_dedupe_but_readme_zip_and_unopened_mobi_remain` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:173` | PASS |
| 8 | `opened_attachment_retains_resource_identity_and_never_becomes_work` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:202` | PASS |
| 9 | `ignored_subtree_never_groups_but_indexed_rows_remain` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:216` | PASS |
| 10 | `manual_work_container_mixed_are_hard_fences` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:229` | PASS |
| 11 | `differently_bound_child_edition_stays_out_of_parent_ownership` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:243` | PASS |
| 12 | `equal_bound_children_match_existing_detail_contract_but_shadow_is_conservative` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:261` | PASS |
| 13 | `parallel_formats_and_collection_never_merge_real_ids` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:278` | PASS |
| 14 | `every_persisted_metadata_table_and_database_byte_is_unchanged` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:290` | PASS |
| 15 | `both_existing_recognition_modes_remain_immutable` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:306` | PASS |
| 16 | `all_three_video_kinds_are_rejected_before_book_queries` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:318` | PASS |
| 17 | `gaps_bare_numbers_unicode_and_same_names_in_different_parents_are_safe` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:338` | PASS |
| 18 | `inferred_ascii_case_collision_keeps_physical_fallback` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:358` | PASS |
| 19 | `repeated_snapshot_and_serialization_are_deterministic` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:368` | PASS |
| 20 | `reversed_sql_insertion_keeps_same_path_based_structure` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:379` | PASS |
| 21 | `shadow_overrides_have_priority_without_mutating_persisted_types` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:415` | PASS |
| 22 | `overrides_cannot_weaken_manual_binding_fences` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:443` | PASS |
| 23 | `partial_offline_failed_and_cancelled_runs_retain_and_mark_review` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:461` | PASS |
| 24 | `explicit_cancellation_produces_no_report` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:489` | PASS |
| 25 | `actual_sqlite_interrupt_rolls_back_and_reader_is_reusable` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:499` | PASS |
| 26 | `pinned_wal_read_is_consistent_across_concurrent_index_commit` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:515` | PASS |
| 27 | `rollback_injected_after_pin_has_no_half_state` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:555` | PASS |
| 28 | `cross_root_sources_and_traversal_paths_fail_closed` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:572` | PASS |
| 29 | `windows_verbatim_unc_and_literal_unicode_paths_are_lexically_scoped` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:596` | PASS |
| 30 | `missing_or_future_schema_fails_closed` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:608` | PASS |
| 31 | `source_format_disagreement_and_missing_revision_are_not_verified` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:636` | PASS |
| 32 | `noncontiguous_page_index_is_preserved_not_renumbered` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:655` | PASS |
| 33 | `page_locator_escape_and_cross_root_resource_fk_are_rejected` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:670` | PASS |
| 34 | `oversized_index_text_is_rejected_before_allocating_a_string` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:701` | PASS |
| 35 | `readonly_sqlite_connection_rejects_writes_and_preserves_wal_bytes` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:719` | PASS |
| 36 | `single_image_attachment_keeps_file_identity_without_fake_folder` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:740` | PASS |
| 37 | `orphaned_node_is_an_error_not_zero_orphans_success` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:772` | PASS |
| 38 | `page_limit_rejects_a_large_book_without_partial_report` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:790` | PASS |
| 39 | `drive_root_and_unicode_paths_remain_literal` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:803` | PASS |
| 40 | `legacy_txt_text_encoding_fallback_matches_production_book_select` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:811` | PASS |
| 41 | `exact_current_production_owned_sql_and_browse_predicate_match_shadow_views` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:825` | PASS |
| 42 | `detail_sampling_keeps_hidden_root_even_when_its_id_is_later` | `tools/smart-mixed-shadow/tests/adapter_shadow.rs:887` | PASS |

## 4. 共用的不变量与测量保护

`tests/adapter_shadow.rs:54` 的 `invariant` 对成功结果统一断言：真实来源集合等于 proposed + fallback 的不交并集、真实 ID 不重复、没有凭空来源、mutation_policy 固定只读。分组成员唯一性由 `compare()` 在报告返回前再次校验。失败结果必须 Err，不能输出半份成功报告。

`logical_digest` 对合成数据库所有用户表内容生成摘要；`disk_digest` 对合成 DB 主文件生成 SHA。WAL 测试另外读取合成 WAL 文件字节。Metadata 测试有真实 schema 的多表状态，不只是检查源 media 文件没被碰。

SQLite 中断测试使用 progress handler 使正在执行的读真正被打断，随后在同一个 reader 上重新成功读取；不是只在输入传入一个 cancelled bool。并发测试在 snapshot 固定之后用另一连接提交，确认首次旧结果一致、次次版本变化、旧 expected_version 拒绝。

## 5. CLI 与基准覆盖

`scripts/verify.py` 实际执行 SQLite cases：03、10、11、12、20、22、28、36、42、45、49。每例跑两次 summary 相同，missing/duplicate 为 0，DB SHA 不变。没有把 Phase 1 的内存期望直接要求为当前索引输出；图片 verified 缺口属于有解释的差异。

- 03：两个系列 + 大量独立文件。
- 10/11/45：作者、画师与电子书分类。
- 12/28：设定素材、父子图片书，保留物理 fallback。
- 20/22/42：版本、合集、多格式容器。
- 36：当前 MOBI/AZW3 已索引格式。
- 49：混合系列完整组合。
- 非法 Root 999：stderr `ROOT_NOT_FOUND`、exit 2，无成功输出。
- 基准：1000/10000/100000 个单页、Root 直属的合成 PDF 索引；数据库建立在另一个进程，不计入 Shadow 耗时/峰值。详情投影包含 Root，完整差异报告在内存生成。

## 6. 覆盖边界与未执行

- 图片二进制、PDF/EPUB/TXT/Kindle 真实解码、CRC/句柄安全仍由原生产实现负责，本轮未重验媒体文件。
- schema 未持久保存图片实际格式验证；本轮图片相关测试验证**不造证据、原 ID/页序回退**，不声称所有图片已归组。
- 只比较 Root + 最多 64 个首页子节点的完整旧 detail；所有来源映射完整，但不是所有深层旧详情视图的全对全展开对照。
- Unicode 测试保留原字符串/NFC/NFD/大小写风险，不等于模拟了全部 Windows 文件系统大小写规则或 real handle 行为。
- 默认特性无工厂；Clippy 对默认与 all-features 都通过。生产应用没有导入独立 crate，正式扫描器/UI 完全未参与。
- 未运行真实个人 DB、正式 GUI、全应用 npm/Cargo gate、更新器/打包/发布。没有“因格式未实现而跳过”的测试，也没有任何 ignored test。
