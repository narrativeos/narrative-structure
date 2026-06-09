#!/bin/bash
# 数据加载性能基准测试脚本
# 直接使用 sqlite3 测试数据库查询性能

DB_PATH="${HOME}/.narrativeos/narrative-structure/Projects/00560806_025936_3018/narrative.db"

echo "=========================================="
echo "  NarrativeStructure 数据加载性能测试"
echo "=========================================="
echo "数据库: $DB_PATH"
echo ""

if [ ! -f "$DB_PATH" ]; then
    echo "错误: 数据库文件不存在!"
    exit 1
fi

# 数据库基本信息
echo "--- 数据库基本信息 ---"
DB_SIZE=$(du -m "$DB_PATH" | cut -f1)
echo "数据库大小: ${DB_SIZE} MB"

TOTAL_BLOCKS=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM blocks;")
echo "总块数: $TOTAL_BLOCKS"

BLOCK_TYPES=$(sqlite3 "$DB_PATH" "SELECT block_type, COUNT(*) as cnt FROM blocks GROUP BY block_type ORDER BY cnt DESC;")
echo "块类型分布:"
echo "$BLOCK_TYPES" | while IFS='|' read -r type cnt; do
    echo "  $type: $cnt"
done
echo ""

AVG_CONTENT_SIZE=$(sqlite3 "$DB_PATH" "SELECT CAST(SUM(length(content)) AS REAL) / COUNT(*) FROM blocks;")
echo "平均内容大小: ${AVG_CONTENT_SIZE} bytes"
echo ""

# 测试 1: 数据库打开时间
echo "--- 测试 1: 数据库打开时间 (10 次平均) ---"
TOTAL_TIME=0
for i in $(seq 1 10); do
    START=$(date +%s%N)
    sqlite3 "$DB_PATH" "SELECT 1;" > /dev/null
    END=$(date +%s%N)
    ELAPSED=$(( (END - START) / 1000000 ))
    echo "  第 $i 次: ${ELAPSED}ms"
    TOTAL_TIME=$((TOTAL_TIME + ELAPSED))
done
AVG=$((TOTAL_TIME / 10))
echo "  平均: ${AVG}ms"
echo ""

# 测试 2: TOC 查询 (heading 块)
echo "--- 测试 2: TOC 查询 (heading 块) ---"
HEADING_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM blocks WHERE block_type = 'heading';")
echo "Heading 块数量: $HEADING_COUNT"

for i in $(seq 1 5); do
    START=$(date +%s%N)
    sqlite3 "$DB_PATH" "SELECT id, substr(content, 1, 80) FROM blocks WHERE block_type = 'heading' ORDER BY order_idx;" > /dev/null
    END=$(date +%s%N)
    ELAPSED=$(( (END - START) / 1000000 ))
    echo "  第 $i 次: ${ELAPSED}ms"
done
echo ""

# 测试 3: 分页加载性能
echo "--- 测试 3: 分页加载性能 ---"
for PAGE_SIZE in 10 50 100 500 1000; do
    START=$(date +%s%N)
    sqlite3 "$DB_PATH" "SELECT id, content FROM blocks ORDER BY order_idx LIMIT $PAGE_SIZE OFFSET 0;" > /dev/null
    END=$(date +%s%N)
    ELAPSED=$(( (END - START) / 1000000 ))
    DATA_SIZE=$(sqlite3 "$DB_PATH" "SELECT SUM(length(content)) FROM blocks ORDER BY order_idx LIMIT $PAGE_SIZE;" 2>/dev/null)
    echo "  分页大小 $PAGE_SIZE: ${ELAPSED}ms (数据大小: $((DATA_SIZE / 1024)) KB)"
done
echo ""

# 测试 4: 按页码范围加载
echo "--- 测试 4: 按页码范围加载 (json_extract) ---"
MAX_PAGE=$(sqlite3 "$DB_PATH" "SELECT MAX(CAST(json_extract(metadata, '\$.page') AS INTEGER)) FROM blocks;")
echo "最大页码: $MAX_PAGE"

for RANGE in "1,10" "1,50" "1,100"; do
    IFS=',' read -r START_PAGE END_PAGE <<< "$RANGE"
    START=$(date +%s%N)
    sqlite3 "$DB_PATH" "SELECT content FROM blocks WHERE CAST(json_extract(metadata, '\$.page') AS INTEGER) BETWEEN $START_PAGE AND $END_PAGE ORDER BY order_idx;" > /dev/null
    END=$(date +%s%N)
    ELAPSED=$(( (END - START) / 1000000 ))
    BLOCK_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM blocks WHERE CAST(json_extract(metadata, '\$.page') AS INTEGER) BETWEEN $START_PAGE AND $END_PAGE;")
    echo "  页码 $START_PAGE-$END_PAGE: ${ELAPSED}ms ($BLOCK_COUNT 块)"
done
echo ""

# 测试 5: 单块查询 (100 次平均)
echo "--- 测试 5: 单块查询 (100 次平均) ---"
FIRST_ID=$(sqlite3 "$DB_PATH" "SELECT id FROM blocks LIMIT 1;")
TOTAL_TIME=0
for i in $(seq 1 100); do
    START=$(date +%s%N)
    sqlite3 "$DB_PATH" "SELECT content FROM blocks WHERE id = '$FIRST_ID';" > /dev/null
    END=$(date +%s%N)
    ELAPSED=$(( (END - START) / 1000000 ))
    TOTAL_TIME=$((TOTAL_TIME + ELAPSED))
done
AVG=$((TOTAL_TIME / 100))
echo "  平均: ${AVG}ms"
echo ""

# 测试 6: 全文搜索
echo "--- 测试 6: 全文搜索 (FTS5) ---"
for i in $(seq 1 5); do
    START=$(date +%s%N)
    sqlite3 "$DB_PATH" "SELECT b.id, b.content FROM blocks b INNER JOIN blocks_fts fts ON b.rowid = fts.rowid WHERE blocks_fts MATCH '*' LIMIT 10;" > /dev/null
    END=$(date +%s%N)
    ELAPSED=$(( (END - START) / 1000000 ))
    echo "  第 $i 次: ${ELAPSED}ms"
done
echo ""

# 测试 7: PRAGMA 设置时间
echo "--- 测试 7: PRAGMA 设置时间 ---"
for i in $(seq 1 5); do
    START=$(date +%s%N)
    sqlite3 "$DB_PATH" "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;" > /dev/null
    END=$(date +%s%N)
    ELAPSED=$(( (END - START) / 1000000 ))
    echo "  第 $i 次: ${ELAPSED}ms"
done
echo ""

# 索引分析
echo "--- 索引分析 ---"
sqlite3 "$DB_PATH" ".indices blocks"
echo ""
sqlite3 "$DB_PATH" "SELECT sql FROM sqlite_master WHERE type='index' AND tbl_name='blocks';"
echo ""

echo "=========================================="
echo "  性能测试完成"
echo "=========================================="