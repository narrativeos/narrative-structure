/// 数据加载性能测试模块
/// 用于测量打开项目、TOC 加载、分页加载等关键路径的性能
use rusqlite::Connection;
use std::path::PathBuf;
use std::time::Instant;

/// 运行完整的性能基准测试
pub fn run_perf_benchmark(project_path: &PathBuf) {
    eprintln!("\n========================================");
    eprintln!("[PERF BENCHMARK] 数据加载性能测试");
    eprintln!("[PERF BENCHMARK] 项目路径: {}", project_path.display());
    eprintln!("========================================\n");
    
    // 1. 数据库打开性能
    benchmark_db_open(project_path);
    
    // 2. TOC 查询性能
    benchmark_toc_query(project_path);
    
    // 3. 分页加载性能 (不同分页大小)
    benchmark_paginated_load(project_path);
    
    // 4. 按页码范围加载性能
    benchmark_page_range_load(project_path);
    
    // 5. 单块查询性能
    benchmark_single_block_query(project_path);
    
    // 6. 数据库统计信息
    benchmark_db_stats(project_path);
    
    eprintln!("\n========================================");
    eprintln!("[PERF BENCHMARK] 测试完成");
    eprintln!("========================================\n");
}

/// 基准测试: 数据库打开
fn benchmark_db_open(project_path: &PathBuf) {
    eprintln!("--- 测试 1: 数据库打开 ---");
    let db_path = project_path.join("narrative.db");
    
    // 预热
    let _ = Connection::open(&db_path).unwrap();
    
    // 正式测试 (10 次取平均)
    let iterations = 10;
    let mut total = std::time::Duration::ZERO;
    
    for i in 0..iterations {
        let start = Instant::now();
        let conn = Connection::open(&db_path).unwrap();
        let elapsed = start.elapsed();
        total += elapsed;
        drop(conn);
        eprintln!("  第 {} 次: {:.3}ms", i + 1, elapsed.as_secs_f64() * 1000.0);
    }
    
    let avg = total / iterations;
    eprintln!("  平均: {:.3}ms ({} 次)\n", avg.as_secs_f64() * 1000.0, iterations);
}

/// 基准测试: TOC 查询
fn benchmark_toc_query(project_path: &PathBuf) {
    eprintln!("--- 测试 2: TOC 查询 (heading 块) ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    // 预热
    let _ = conn.query_scalar(
        "SELECT COUNT(*) FROM blocks WHERE block_type = 'heading'",
        [],
        |r| r.get::<_, i64>(0),
    ).unwrap();
    
    // 正式测试
    let iterations = 5;
    let mut total = std::time::Duration::ZERO;
    let mut node_count = 0;
    
    for i in 0..iterations {
        let start = Instant::now();
        let nodes: Vec<(String, String)> = conn
            .prepare(
                "SELECT id, substr(content, 1, 80) 
                 FROM blocks WHERE block_type = 'heading' ORDER BY order_idx"
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0).unwrap(), row.get(1).unwrap()))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        let elapsed = start.elapsed();
        total += elapsed;
        node_count = nodes.len();
        drop(nodes);
        eprintln!("  第 {} 次: {:.3}ms ({} 节点)", i + 1, elapsed.as_secs_f64() * 1000.0, node_count);
    }
    
    let avg = total / iterations;
    eprintln!("  平均: {:.3}ms ({} 次, {} 节点)\n", avg.as_secs_f64() * 1000.0, iterations, node_count);
}

/// 基准测试: 分页加载
fn benchmark_paginated_load(project_path: &PathBuf) {
    eprintln!("--- 测试 3: 分页加载 ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    let page_sizes = [10, 50, 100, 500, 1000];
    
    for page_size in &page_sizes {
        let start = Instant::now();
        let blocks: Vec<(String, String)> = conn
            .prepare(
                "SELECT id, content 
                 FROM blocks ORDER BY order_idx LIMIT ?1 OFFSET ?2"
            )
            .unwrap()
            .query_map(rusqlite::params![*page_size, 0], |row| {
                Ok((row.get(0).unwrap(), row.get(1).unwrap()))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        let elapsed = start.elapsed();
        eprintln!("  分页大小 {}: {:.3}ms ({} 行, {:.2} KB/行)", 
            page_size, 
            elapsed.as_secs_f64() * 1000.0,
            blocks.len(),
            if blocks.len() > 0 {
                let total_bytes: usize = blocks.iter().map(|(_, c)| c.len()).sum();
                (total_bytes as f64 / blocks.len() as f64 / 1024.0)
            } else { 0.0 }
        );
        drop(blocks);
    }
    eprintln!();
}

/// 基准测试: 按页码范围加载
fn benchmark_page_range_load(project_path: &PathBuf) {
    eprintln!("--- 测试 4: 按页码范围加载 ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    // 获取最大页码
    let max_page: i32 = conn
        .query_scalar(
            "SELECT MAX(CAST(json_extract(metadata, '$.page') AS INTEGER)) FROM blocks",
            [],
            |r| r.get(0),
        )
        .unwrap();
    
    eprintln!("  最大页码: {}", max_page);
    
    let ranges = [(1, 10), (1, 50), (1, 100)];
    
    for (start, end) in &ranges {
        let start_time = Instant::now();
        let blocks: Vec<String> = conn
            .prepare(
                "SELECT content FROM blocks 
                 WHERE CAST(json_extract(metadata, '$.page') AS INTEGER) BETWEEN ?1 AND ?2
                 ORDER BY order_idx"
            )
            .unwrap()
            .query_map(rusqlite::params![*start, *end], |row| {
                Ok(row.get(0).unwrap())
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        let elapsed = start_time.elapsed();
        eprintln!("  页码范围 {}-{}: {:.3}ms ({} 块)", 
            start, end, 
            elapsed.as_secs_f64() * 1000.0,
            blocks.len()
        );
        drop(blocks);
    }
    eprintln!();
}

/// 基准测试: 单块查询
fn benchmark_single_block_query(project_path: &PathBuf) {
    eprintln!("--- 测试 5: 单块查询 ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    // 获取一个块 ID
    let block_id: String = conn
        .query_scalar("SELECT id FROM blocks LIMIT 1", [], |r| r.get(0))
        .unwrap();
    
    let iterations = 100;
    let mut total = std::time::Duration::ZERO;
    
    for i in 0..iterations {
        let start = Instant::now();
        let content: String = conn
            .query_row(
                "SELECT content FROM blocks WHERE id = ?1",
                [&block_id],
                |row| row.get(0),
            )
            .unwrap();
        let elapsed = start.elapsed();
        total += elapsed;
        drop(content);
    }
    
    let avg = total / iterations;
    eprintln!("  平均: {:.3}ms ({} 次)\n", avg.as_secs_f64() * 1000.0, iterations);
}

/// 基准测试: 数据库统计
fn benchmark_db_stats(project_path: &PathBuf) {
    eprintln!("--- 测试 6: 数据库统计 ---");
    let db_path = project_path.join("narrative.db");
    let conn = Connection::open(&db_path).unwrap();
    
    let total_blocks: i64 = conn
        .query_scalar("SELECT COUNT(*) FROM blocks", [], |r| r.get(0))
        .unwrap();
    
    let db_size = std::fs::metadata(&db_path).unwrap().len();
    
    eprintln!("  总块数: {}", total_blocks);
    eprintln!("  数据库大小: {:.2} MB", db_size as f64 / 1_048_576.0);
    eprintln!("  平均每块大小: {:.2} bytes", 
        conn.query_scalar(
            "SELECT SUM(length(content)) FROM blocks", [], |r| r.get::<_, i64>(0)
        ).unwrap_or(0) as f64 / total_blocks as f64
    );
    eprintln!();
}