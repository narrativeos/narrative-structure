/// 独立性能基准测试二进制
/// 运行: cargo run --bin bench
use std::path::PathBuf;
use std::time::Instant;
use rusqlite::Connection;

fn main() {
    let project_path = PathBuf::from(std::env::var("HOME").unwrap())
        .join(".narrativeos")
        .join("narrative-structure")
        .join("Projects")
        .join("00560806_025936_3018");
    
    println!("\n========================================");
    println!("  NarrativeStructure 数据加载性能测试");
    println!("  项目: {}", project_path.display());
    println!("========================================\n");
    
    // 测试 1: 数据库打开
    bench_db_open(&project_path);
    
    // 测试 2: PRAGMA + 迁移 (模拟 open_project)
    bench_open_project(&project_path);
    
    // 测试 3: TOC 查询
    bench_get_toc(&project_path);
    
    // 测试 4: 分页加载
    bench_get_blocks_paginated(&project_path);
    
    // 测试 5: 按页码范围加载
    bench_get_blocks_by_page(&project_path);
    
    // 测试 6: 单块查询
    bench_get_block(&project_path);
    
    // 测试 7: 全文搜索
    bench_search(&project_path);
    
    // 测试 8: 页码统计
    bench_page_stats(&project_path);
    
    // 测试 9: 完整加载流程模拟
    bench_full_load_flow(&project_path);
    
    println!("\n========================================");
    println!("  测试完成");
    println!("========================================\n");
}

fn bench_db_open(project_path: &PathBuf) {
    println!("--- 测试 1: 数据库打开 ---");
    let db_path = project_path.join("narrative.db");
    
    // 预热
    let _ = Connection::open(&db_path).unwrap();
    
    let iterations = 10;
    let mut times = vec![];
    
    for i in 0..iterations {
        let start = Instant::now();
        let conn = Connection::open(&db_path).unwrap();
        let elapsed = start.elapsed();
        times.push(elapsed);
        println!("  第 {} 次: {:.3}ms", i + 1, elapsed.as_secs_f64() * 1000.0);
        drop(conn);
    }
    
    let avg: f64 = times.iter().map(|t| t.as_secs_f64() * 1000.0).sum::<f64>() / iterations as f64;
    println!("  平均: {:.3}ms\n", avg);
}

fn bench_open_project(project_path: &PathBuf) {
    println!("--- 测试 2: 模拟 open_project (PRAGMA + 迁移) ---");
    let db_path = project_path.join("narrative.db");
    
    let iterations = 5;
    let mut times = vec![];
    
    for i in 0..iterations {
        let start = Instant::now();
        
        let conn = Connection::open(&db_path).unwrap();
        
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;",
        ).unwrap();
        
        let _ = conn.execute_batch("ALTER TABLE blocks ADD COLUMN original_content TEXT DEFAULT ''");
        
        let elapsed = start.elapsed();
        times.push(elapsed);
        println!("  第 {} 次: {:.3}ms", i + 1, elapsed.as_secs_f64() * 1000.0);
        drop(conn);
    }
    
    let avg: f64 = times.iter().map(|t| t.as_secs_f64() * 1000.0).sum::<f64>() / iterations as f64;
    println!("  平均: {:.3}ms\n", avg);
}

#[derive(Clone)]
struct TocNode {
    id: String,
    parent_id: Option<String>,
    order_idx: i32,
    level: i32,
    block_type: String,
    content_preview: String,
    children: Vec<TocNode>,
}

fn bench_get_toc(project_path: &PathBuf) {
    println!("--- 测试 3: get_toc (TOC 查询 + 树构建) ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    let iterations = 5;
    let mut times = vec![];
    let mut node_count = 0;
    
    for i in 0..iterations {
        let start = Instant::now();
        
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, order_idx, level, block_type,
                    substr(content, 1, 80) as content_preview
             FROM blocks
             WHERE block_type = 'heading'
             ORDER BY level, order_idx"
        ).unwrap();
        
        let flat_nodes: Vec<TocNode> = stmt
            .query_map([], |row| {
                Ok(TocNode {
                    id: row.get(0).unwrap(),
                    parent_id: row.get(1).unwrap(),
                    order_idx: row.get(2).unwrap(),
                    level: row.get(3).unwrap(),
                    block_type: row.get(4).unwrap(),
                    content_preview: row.get(5).unwrap(),
                    children: vec![],
                })
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        
        let tree = build_toc_tree(flat_nodes);
        let elapsed = start.elapsed();
        times.push(elapsed);
        node_count = tree.len();
        
        println!("  第 {} 次: {:.3}ms ({} 根节点)", i + 1, elapsed.as_secs_f64() * 1000.0, node_count);
    }
    
    let avg: f64 = times.iter().map(|t| t.as_secs_f64() * 1000.0).sum::<f64>() / iterations as f64;
    println!("  平均: {:.3}ms ({} 根节点)\n", avg, node_count);
    drop(conn);
}

fn build_toc_tree(flat_nodes: Vec<TocNode>) -> Vec<TocNode> {
    let mut roots: Vec<TocNode> = vec![];
    let mut children_map: std::collections::HashMap<String, Vec<TocNode>> = std::collections::HashMap::new();
    
    for node in flat_nodes {
        match &node.parent_id {
            Some(pid) => {
                children_map.entry(pid.clone()).or_default().push(node);
            }
            None => {
                roots.push(node);
            }
        }
    }
    
    fn attach_children(nodes: &mut Vec<TocNode>, children_map: &std::collections::HashMap<String, Vec<TocNode>>) {
        for node in nodes.iter_mut() {
            if let Some(children) = children_map.get(&node.id) {
                let mut child_nodes = children.clone();
                attach_children(&mut child_nodes, children_map);
                node.children = child_nodes;
            }
        }
    }
    
    attach_children(&mut roots, &children_map);
    roots
}

struct Block {
    id: String,
    content: String,
    metadata: String,
}

fn bench_get_blocks_paginated(project_path: &PathBuf) {
    println!("--- 测试 4: get_blocks_paginated ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    let page_sizes = [10, 50, 100, 500, 1000];
    
    for limit in &page_sizes {
        let start = Instant::now();
        
        let mut stmt = conn.prepare(
            "SELECT id, content, metadata FROM blocks ORDER BY order_idx LIMIT ?1 OFFSET ?2"
        ).unwrap();
        
        let blocks: Vec<Block> = stmt
            .query_map(rusqlite::params![*limit, 0], |row| {
                Ok(Block {
                    id: row.get(0).unwrap(),
                    content: row.get(1).unwrap(),
                    metadata: row.get(2).unwrap(),
                })
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        
        let elapsed = start.elapsed();
        let total_bytes: usize = blocks.iter().map(|b| b.content.len()).sum();
        
        println!("  limit={:4}: {:.3}ms ({} 行, {:.2} KB)", 
            limit, elapsed.as_secs_f64() * 1000.0, blocks.len(), total_bytes as f64 / 1024.0);
        
        drop(blocks);
    }
    println!();
    drop(conn);
}

fn bench_get_blocks_by_page(project_path: &PathBuf) {
    println!("--- 测试 5: get_blocks_by_page (按页码范围) ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    // 获取实际页码分布
    let page_dist: Vec<(i32, i32)> = conn
        .prepare(
            "SELECT CAST(json_extract(metadata, '$.page') AS INTEGER) as page, COUNT(*) as cnt
             FROM blocks WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) > 0
             GROUP BY page ORDER BY cnt DESC LIMIT 10"
        ).unwrap()
        .query_map([], |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    
    println!("  页码分布 (Top 10):");
    for (page, cnt) in &page_dist {
        println!("    p{}: {} 块", page, cnt);
    }
    
    // 测试实际有意义的页码范围
    let ranges = [(1, 1), (2, 2)];
    
    for (start, end) in &ranges {
        let start_time = Instant::now();
        
        let mut stmt = conn.prepare(
            "SELECT id, content, metadata FROM blocks
             WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) BETWEEN ?1 AND ?2
             ORDER BY order_idx"
        ).unwrap();
        
        let blocks: Vec<Block> = stmt
            .query_map(rusqlite::params![*start, *end], |row| {
                Ok(Block {
                    id: row.get(0).unwrap(),
                    content: row.get(1).unwrap(),
                    metadata: row.get(2).unwrap(),
                })
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        
        let elapsed = start_time.elapsed();
        let total_bytes: usize = blocks.iter().map(|b| b.content.len()).sum();
        
        println!("  p{}-p{}: {:.3}ms ({} 块, {:.2} KB)", 
            start, end, elapsed.as_secs_f64() * 1000.0, blocks.len(), total_bytes as f64 / 1024.0);
        
        drop(blocks);
    }
    println!();
    drop(conn);
}

fn bench_get_block(project_path: &PathBuf) {
    println!("--- 测试 6: get_block (单块查询) ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    let block_id: String = conn
        .query_row("SELECT id FROM blocks LIMIT 1", [], |row| row.get(0)).unwrap();
    
    let iterations = 100;
    let mut times = vec![];
    
    for i in 0..iterations {
        let start = Instant::now();
        
        let content: String = conn
            .query_row(
                "SELECT content FROM blocks WHERE id = ?1",
                [&block_id],
                |row| row.get(0),
            ).unwrap();
        
        let elapsed = start.elapsed();
        times.push(elapsed);
        drop(content);
    }
    
    let avg: f64 = times.iter().map(|t| t.as_secs_f64() * 1000.0).sum::<f64>() / iterations as f64;
    println!("  平均: {:.3}ms ({} 次)\n", avg, iterations);
    drop(conn);
}

fn bench_search(project_path: &PathBuf) {
    println!("--- 测试 7: search_blocks (FTS5 全文搜索) ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    let iterations = 5;
    let mut times = vec![];
    
    for i in 0..iterations {
        let start = Instant::now();
        
        let mut stmt = conn.prepare(
            "SELECT b.id, b.content FROM blocks b
             INNER JOIN blocks_fts fts ON b.rowid = fts.rowid
             WHERE blocks_fts MATCH 'the'
             ORDER BY rank LIMIT 10"
        ).unwrap();
        
        let results: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        
        let elapsed = start.elapsed();
        times.push(elapsed);
        println!("  第 {} 次: {:.3}ms ({} 结果)", i + 1, elapsed.as_secs_f64() * 1000.0, results.len());
        drop(results);
    }
    
    let avg: f64 = times.iter().map(|t| t.as_secs_f64() * 1000.0).sum::<f64>() / iterations as f64;
    println!("  平均: {:.3}ms\n", avg);
    drop(conn);
}

fn bench_page_stats(project_path: &PathBuf) {
    println!("--- 测试 8: get_page_stats ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    let start = Instant::now();
    
    let stats: Vec<(i32, i32)> = conn
        .prepare(
            "SELECT CAST(json_extract(metadata, '$.page') AS INTEGER) as page, COUNT(*) as cnt
             FROM blocks WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) > 0
             GROUP BY page ORDER BY page"
        ).unwrap()
        .query_map([], |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    
    let elapsed = start.elapsed();
    println!("  {:.3}ms ({} 个不同页码)", elapsed.as_secs_f64() * 1000.0, stats.len());
    
    let total_blocks: i32 = stats.iter().map(|(_, c)| *c).sum();
    println!("  覆盖 {} 个块", total_blocks);
    println!();
    drop(conn);
}

fn bench_full_load_flow(project_path: &PathBuf) {
    println!("--- 测试 9: 完整加载流程模拟 ---");
    let db_path = project_path.join("narrative.db");
    
    let total_start = Instant::now();
    
    // Step 1: Open DB
    let s = Instant::now();
    let conn = Connection::open(&db_path).unwrap();
    println!("  1. DB 打开: {:.3}ms", s.elapsed().as_secs_f64() * 1000.0);
    
    // Step 2: PRAGMA
    let s = Instant::now();
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
    ).unwrap();
    println!("  2. PRAGMA: {:.3}ms", s.elapsed().as_secs_f64() * 1000.0);
    
    // Step 3: Get TOC
    let s = Instant::now();
    let mut toc_stmt = conn.prepare(
        "SELECT id, parent_id, order_idx, level, block_type, substr(content, 1, 80)
         FROM blocks WHERE block_type = 'heading' ORDER BY level, order_idx"
    ).unwrap();
    let toc_nodes: Vec<TocNode> = toc_stmt
        .query_map([], |row| {
            Ok(TocNode {
                id: row.get(0).unwrap(),
                parent_id: row.get(1).unwrap(),
                order_idx: row.get(2).unwrap(),
                level: row.get(3).unwrap(),
                block_type: row.get(4).unwrap(),
                content_preview: row.get(5).unwrap(),
                children: vec![],
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let toc_tree = build_toc_tree(toc_nodes);
    println!("  3. TOC 查询+构建: {:.3}ms ({} 根节点)", s.elapsed().as_secs_f64() * 1000.0, toc_tree.len());
    
    // Step 4: Page stats
    let s = Instant::now();
    let stats: Vec<(i32, i32)> = conn
        .prepare(
            "SELECT CAST(json_extract(metadata, '$.page') AS INTEGER), COUNT(*)
             FROM blocks WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) > 0
             GROUP BY 1 ORDER BY 1"
        ).unwrap()
        .query_map([], |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    println!("  4. 页码统计: {:.3}ms ({} 页)", s.elapsed().as_secs_f64() * 1000.0, stats.len());
    
    // Step 5: Load first page (page=1)
    let s = Instant::now();
    let mut page_stmt = conn.prepare(
        "SELECT id, content, metadata FROM blocks
         WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) = 1
         ORDER BY order_idx"
    ).unwrap();
    let page1_blocks: Vec<Block> = page_stmt
        .query_map([], |row| {
            Ok(Block {
                id: row.get(0).unwrap(),
                content: row.get(1).unwrap(),
                metadata: row.get(2).unwrap(),
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let page1_bytes: usize = page1_blocks.iter().map(|b| b.content.len()).sum();
    println!("  5. 加载 p1: {:.3}ms ({} 块, {:.2} KB)", 
        s.elapsed().as_secs_f64() * 1000.0, page1_blocks.len(), page1_bytes as f64 / 1024.0);
    
    // Step 6: Paginated load (first 100 blocks)
    let s = Instant::now();
    let mut paginated_stmt = conn.prepare(
        "SELECT id, content, metadata FROM blocks ORDER BY order_idx LIMIT 100 OFFSET 0"
    ).unwrap();
    let paginated_blocks: Vec<Block> = paginated_stmt
        .query_map([], |row| {
            Ok(Block {
                id: row.get(0).unwrap(),
                content: row.get(1).unwrap(),
                metadata: row.get(2).unwrap(),
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    let paginated_bytes: usize = paginated_blocks.iter().map(|b| b.content.len()).sum();
    println!("  6. 分页 100 块: {:.3}ms ({:.2} KB)", 
        s.elapsed().as_secs_f64() * 1000.0, paginated_bytes as f64 / 1024.0);
    
    let total = total_start.elapsed();
    println!("\n  完整加载流程总耗时: {:.3}ms", total.as_secs_f64() * 1000.0);
    println!();
}
