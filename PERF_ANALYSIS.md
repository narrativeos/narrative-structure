# NarrativeStructure 数据加载性能分析报告

## 测试环境
- **项目**: `~/.narrativeos/narrative-structure/Projects/00560806_025936_3018`
- **数据库大小**: 16 MB
- **总块数**: 25,511
- **块类型分布**: text: 15,201 (59.6%) / empty: 9,231 (36.2%) / heading: 1,079 (4.2%)
- **平均内容大小**: 48.88 bytes/块
- **测试工具**: Rust rusqlite 直接基准测试 + sqlite3 CLI 验证

---

## Rust 层性能测试结果（项目 00560806_025936_3018）

### 1. 数据库打开
| 指标 | 值 |
|------|-----|
| **平均** | **0.094ms** |
| 最小 | 0.089ms |
| 最大 | 0.109ms |
| 评级 | ✅ 优秀 |

### 2. open_project 完整流程 (DB + PRAGMA + 迁移)
| 指标 | 值 |
|------|-----|
| **平均** | **1.208ms** |
| 最小 | 0.833ms |
| 最大 | 1.810ms |
| 评级 | ✅ 优秀 |

### 3. TOC 查询 + 树构建 (1079 heading 块)
| 指标 | 值 |
|------|-----|
| **平均** | **5.178ms** |
| 最小 | 4.103ms |
| 最大 | 7.675ms |
| 根节点数 | 1079 |
| 评级 | ✅ 优秀 |

### 4. 分页加载性能
| Limit | 时间 | 数据大小 |
|-------|------|---------|
| 10 | 7.356ms | 0.23 KB |
| 50 | 7.489ms | 2.45 KB |
| 100 | 7.051ms | 9.84 KB |
| 500 | 6.699ms | 31.40 KB |
| 1000 | 7.355ms | 43.02 KB |

**评级**: ✅ 优秀。所有分页大小稳定在 6-7ms，数据量随分页大小线性增长（正确）。

### 5. 按页码范围加载
| 页码范围 | 时间 | 块数 | 数据大小 |
|---------|------|------|---------|
| p1-p1 | 10.579ms | 12 | 0.27 KB |
| p2-p2 | 42.907ms | 25,499 | 2,404.81 KB |

**⚠️ 严重问题**: 99.95% 的块被标记为 `page=2`，导致加载任意包含 p2 的页码范围时返回全部数据。

### 6. 单块查询 (PRIMARY KEY)
| 指标 | 值 |
|------|-----|
| **平均** | **0.007ms** (7μs) |
| 测试次数 | 100 |
| 评级 | ✅ 极佳 |

### 7. FTS5 全文搜索
| 指标 | 值 |
|------|-----|
| **平均** | **0.245ms** |
| 暖机后 | 0.112ms |
| 评级 | ✅ 优秀 |

### 8. 页码统计 (全表扫描 json_extract)
| 指标 | 值 |
|------|-----|
| 时间 | 19.828ms |
| 不同页码数 | 2 |
| 覆盖块数 | 25,511 |
| 评级 | ⚠️ 可接受但有优化空间 |

### 9. 完整加载流程模拟
| 步骤 | 时间 |
|------|------|
| 1. DB 打开 | 0.086ms |
| 2. PRAGMA | 0.355ms |
| 3. TOC 查询+构建 | 3.350ms |
| 4. 页码统计 | 19.660ms |
| 5. 加载 p1 | 9.466ms |
| 6. 分页 100 块 | 5.006ms |
| **总耗时** | **37.954ms** |

---

## 🔴 发现的关键问题

### 问题 1: 页码元数据分布异常（P0）
```
页码 1:   12 块 (0.05%)
页码 2: 25,499 块 (99.95%)
```

**根因**: `page_mapper::apply_bbox_page_mapping` 的 bbox→page 文本匹配算法未能正确将 MD 行映射到对应 PDF 页码。绝大多数块被分配到 page=2。

**影响**:
1. `get_blocks_by_page` 查询失效 — 加载 p2 范围时返回全部 25,499 块 (2.4 MB)
2. 前端按页浏览功能无法正常分页
3. 页码统计查询需全表扫描 `json_extract` (19.8ms)

### 问题 2: json_extract 全表扫描（P1）
`get_page_stats` 和 `get_blocks_by_page` 使用 `CAST(json_extract(metadata, '$.page') AS INTEGER)` 进行 WHERE/GROUP BY，无法利用索引，导致全表扫描。

---

## 性能基线总结

| 操作 | Rust 层性能 | sqlite3 CLI | 评级 |
|------|-----------|-------------|------|
| DB 打开 | 0.094ms | 4ms | ✅ 优秀 |
| open_project 完整 | 1.208ms | - | ✅ 优秀 |
| TOC (1079 headings) | 5.178ms | 7ms | ✅ 优秀 |
| 分页 100 块 | 7.051ms | 8ms | ✅ 优秀 |
| 分页 1000 块 | 7.355ms | 9ms | ✅ 优秀 |
| 单块查询 | 0.007ms | 4ms | ✅ 极佳 |
| FTS5 搜索 | 0.245ms | 4-5ms | ✅ 优秀 |
| 页码范围查询 | 42.9ms (全量) | 15ms (全量) | ❌ 失效 |
| **完整加载流程** | **37.954ms** | - | ✅ 优秀 |

> **注意**: sqlite3 CLI 的 4-7ms 包含进程启动开销。Rust rusqlite 直接连接的实际性能快 40-50 倍。

---

## 优化建议

### 高优先级 (P0)
1. **修复页码映射算法**: 检查 `page_mapper.rs` 中的 bbox span 文本 → MD 行匹配逻辑
2. **添加 page 独立列**: 避免每次查询都执行 `json_extract` 全表扫描
   ```sql
   ALTER TABLE blocks ADD COLUMN page INTEGER DEFAULT 0;
   CREATE INDEX idx_blocks_page ON blocks(page);
   ```
3. **批量更新 page 列**: 导入后从 metadata 提取 page 值到独立列

### 中优先级 (P1)
4. **优化 get_page_stats**: 使用 page 列替代 json_extract
5. **验证前端分页参数**: 确认 `handlePageChange` 的 9 页窗口策略是否合理

### 低优先级 (P2)
6. **TOC 缓存**: 项目打开后缓存 TOC 树，避免重复查询
7. **连接池**: 当前单连接 + Mutex 方案在并发场景下可能成为瓶颈

---

## Rust 性能计时器集成

已在以下函数添加 `[PERF]` 日志（stderr 输出）:

| 函数 | 日志格式 |
|------|---------|
| `open_project` | `[PERF] open_project: 路径验证/Xms DB打开/Xms PRAGMA/Xms 迁移/Xms 总/Xms` |
| `get_toc` | `[PERF] get_toc: prepare/Xms query/Xms build_tree/Xms total/Xms nodes=X` |
| `get_blocks_paginated` | `[PERF] get_blocks_paginated: limit=X offset=X prepare/Xms query/Xms total/Xms rows=X` |

运行 `npx tauri dev` 后在终端查看 `[PERF]` 前缀输出即可实时监控。

---

## 基准测试工具

- **Rust 独立基准**: `cargo run --bin bench` （在 `src-tauri/` 目录下运行）
- **Shell 脚本**: `bash scripts/bench_perf.sh`
- **测试数据**: `~/.narrativeos/narrative-structure/Projects/00560806_025936_3018/narrative.db`

---

*报告生成时间: 2026-06-08*
*测试项目: 00560806_025936_3018 (25,511 块 / 16MB)*