#!/usr/bin/env python3
"""NarrativeStructure 项目数据库初始化脚本。

用法:
    python scripts/init_db.py <project_dir>

示例:
    python scripts/init_db.py Projects/MyDocument_A

功能:
    1. 在指定项目目录下创建 narrative.db (SQLite)
    2. 创建 blocks 表（语义块存储）
    3. 创建 blocks_fts 全文索引（FTS5）
    4. 启用 WAL 模式与外键约束
"""

import sqlite3
import sys
import os
from datetime import datetime


SCHEMA_SQL = """
-- 项目数据库: narrative.db
-- 语义块核心表: blocks
CREATE TABLE IF NOT EXISTS blocks (
    id          TEXT PRIMARY KEY,          -- UUID，全局唯一标识
    parent_id   TEXT,                      -- 父块 UUID (NULL = 根块)
    order_idx   INTEGER NOT NULL DEFAULT 0, -- 同级排序序号
    level       INTEGER NOT NULL DEFAULT 0, -- 语义层级 (0: 根, 1: 标题1, 2: 标题2...)
    block_type  TEXT NOT NULL DEFAULT 'text', -- 'heading' | 'text' | 'table' | 'image'
    content     TEXT DEFAULT '',           -- 文本内容 / 图片路径
    metadata    TEXT DEFAULT '{}',         -- JSON: {bbox, confidence, ...}
    version     INTEGER NOT NULL DEFAULT 1, -- 乐观锁版本号
    created_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (parent_id) REFERENCES blocks(id) ON DELETE CASCADE
);

-- 索引：按层级 + 排序快速查询目录树
CREATE INDEX IF NOT EXISTS idx_blocks_tree
    ON blocks(level, order_idx);

-- 索引：按父块查询子块
CREATE INDEX IF NOT EXISTS idx_blocks_parent
    ON blocks(parent_id);

-- 索引：按类型过滤
CREATE INDEX IF NOT EXISTS idx_blocks_type
    ON blocks(block_type);

-- 全文搜索索引 (FTS5)，内容同步自 blocks 表
CREATE VIRTUAL TABLE IF NOT EXISTS blocks_fts
    USING fts5(
        content,
        content='blocks',
        content_rowid='rowid'
    );

-- 触发器：插入时自动同步 FTS 索引
CREATE TRIGGER IF NOT EXISTS blocks_ai AFTER INSERT ON blocks BEGIN
    INSERT INTO blocks_fts(rowid, content) VALUES (new.rowid, new.content);
END;

-- 触发器：删除时自动同步 FTS 索引
CREATE TRIGGER IF NOT EXISTS blocks_ad AFTER DELETE ON blocks BEGIN
    INSERT INTO blocks_fts(blocks_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;

-- 触发器：更新时自动同步 FTS 索引
CREATE TRIGGER IF NOT EXISTS blocks_au AFTER UPDATE ON blocks BEGIN
    INSERT INTO blocks_fts(blocks_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
    INSERT INTO blocks_fts(rowid, content) VALUES (new.rowid, new.content);
END;
"""


def init_project_db(project_dir: str) -> str:
    """初始化项目数据库。

    Args:
        project_dir: 项目根目录路径

    Returns:
        narrative.db 的完整路径

    Raises:
        FileNotFoundError: 项目目录不存在
        sqlite3.Error: 数据库操作失败
    """
    project_path = os.path.abspath(project_dir)

    if not os.path.isdir(project_path):
        raise FileNotFoundError(f"项目目录不存在: {project_path}")

    # 确保子目录存在
    for subdir in ("assets", "nodes", "prompts"):
        os.makedirs(os.path.join(project_path, subdir), exist_ok=True)

    db_path = os.path.join(project_path, "narrative.db")
    conn = sqlite3.connect(db_path)

    try:
        # 性能与安全配置
        conn.execute("PRAGMA journal_mode=WAL;")
        conn.execute("PRAGMA foreign_keys=ON;")
        conn.execute("PRAGMA busy_timeout=5000;")

        # 执行建表脚本
        conn.executescript(SCHEMA_SQL)
        conn.commit()

        print(f"✅ 数据库已初始化: {db_path}")
        print(f"   - blocks 表已创建")
        print(f"   - blocks_fts 全文索引已创建")
        print(f"   - WAL 模式已启用")
        print(f"   - 外键约束已启用")

    except sqlite3.Error as e:
        print(f"❌ 数据库初始化失败: {e}", file=sys.stderr)
        raise
    finally:
        conn.close()

    return db_path


def main():
    if len(sys.argv) < 2:
        print(f"用法: python {sys.argv[0]} <project_dir>", file=sys.stderr)
        print(f"示例: python {sys.argv[0]} Projects/MyDocument_A", file=sys.stderr)
        sys.exit(1)

    project_dir = sys.argv[1]
    try:
        init_project_db(project_dir)
    except Exception as e:
        print(f"❌ 初始化失败: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
