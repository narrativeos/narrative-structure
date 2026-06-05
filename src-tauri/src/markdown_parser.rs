use uuid::Uuid;

/// 解析后的语义块（中间表示，不含 DB 字段）
#[derive(Debug, Clone)]
pub struct ParsedBlock {
    pub id: String,
    pub parent_id: Option<String>,
    pub order_idx: i32,
    pub level: i32,
    pub block_type: String,
    pub content: String,
    pub metadata: String,
}

/// 将 Markdown 文本解析为一组 SemanticBlock
///
/// 规则：
/// - `# ~ ######` → heading，level = # 数量
/// - 空行分隔的段落 → text
/// - ```...``` → code
/// - |...|...| → table
/// - ![...](...) → image
/// - 连续的非空行合并为同一段落
/// - 层级关系通过栈维护：heading 的 parent 是上一级 heading
/// - text/code/table 的 parent 是最近的 heading（或根）
pub fn parse_markdown(md_content: &str) -> Vec<ParsedBlock> {
    let lines: Vec<&str> = md_content.lines().collect();
    let mut blocks: Vec<ParsedBlock> = Vec::new();
    let mut order_counter: i32 = 0;

    // 跳过 YAML frontmatter
    let start_idx = skip_frontmatter(&lines);

    // 层级栈: (block_id, level)，栈顶是当前最近的 heading
    let mut heading_stack: Vec<(String, i32)> = Vec::new();

    let mut i = start_idx;
    while i < lines.len() {
        let line = lines[i];

        // 跳过空行
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // ---- 代码块 ----
        if line.trim_start().starts_with("```") {
            let lang = line.trim_start().strip_prefix("```").unwrap_or("").trim();
            let mut code_lines: Vec<&str> = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            i += 1; // 跳过结束 ```
            let code_content = code_lines.join("\n");

            let parent_id = heading_stack.last().map(|(id, _)| id.clone());
            let meta = if lang.is_empty() {
                "{}".to_string()
            } else {
                format!("{{\"language\":\"{}\"}}", lang)
            };

            blocks.push(ParsedBlock {
                id: Uuid::new_v4().to_string(),
                parent_id,
                order_idx: order_counter,
                level: heading_stack.len() as i32,
                block_type: "code".to_string(),
                content: code_content,
                metadata: meta,
            });
            order_counter += 1;
            continue;
        }

        // ---- 表格 ----
        if line.contains('|') && has_table_structure(&lines, i) {
            let mut table_lines: Vec<&str> = Vec::new();
            while i < lines.len() && lines[i].contains('|') {
                table_lines.push(lines[i]);
                i += 1;
            }
            let table_content = table_lines.join("\n");
            let parent_id = heading_stack.last().map(|(id, _)| id.clone());

            blocks.push(ParsedBlock {
                id: Uuid::new_v4().to_string(),
                parent_id,
                order_idx: order_counter,
                level: heading_stack.len() as i32,
                block_type: "table".to_string(),
                content: table_content,
                metadata: "{}".to_string(),
            });
            order_counter += 1;
            continue;
        }

        // ---- 图片 ----
        if line.trim_start().starts_with("![") {
            // ![alt](path)
            let parent_id = heading_stack.last().map(|(id, _)| id.clone());
            let meta = extract_image_meta(line);

            blocks.push(ParsedBlock {
                id: Uuid::new_v4().to_string(),
                parent_id,
                order_idx: order_counter,
                level: heading_stack.len() as i32,
                block_type: "image".to_string(),
                content: line.to_string(),
                metadata: meta,
            });
            order_counter += 1;
            i += 1;
            continue;
        }

        // ---- 标题 ----
        if line.starts_with('#') {
            let (level, title) = parse_heading(line);
            // 弹出所有 >= 当前 level 的 heading
            while let Some((_, l)) = heading_stack.last() {
                if *l >= level {
                    heading_stack.pop();
                } else {
                    break;
                }
            }
            let parent_id = heading_stack.last().map(|(id, _)| id.clone());

            let block = ParsedBlock {
                id: Uuid::new_v4().to_string(),
                parent_id,
                order_idx: order_counter,
                level: level,
                block_type: "heading".to_string(),
                content: title.to_string(),
                metadata: format!("{{\"heading_level\":{}}}", level),
            };
            let bid = block.id.clone();
            blocks.push(block);
            heading_stack.push((bid, level));
            order_counter += 1;
            i += 1;
            continue;
        }

        // ---- 普通段落 ----
        // 收集连续非空行（直到遇到空行、标题、代码块、表格）
        let mut para_lines: Vec<&str> = Vec::new();
        while i < lines.len()
            && !lines[i].trim().is_empty()
            && !lines[i].starts_with('#')
            && !lines[i].trim_start().starts_with("```")
            && !(lines[i].contains('|') && has_table_structure(&lines, i))
            && !lines[i].trim_start().starts_with("![")
        {
            para_lines.push(lines[i]);
            i += 1;
        }

        let para_text = para_lines.join("\n").trim().to_string();
        if para_text.is_empty() {
            continue;
        }

        let parent_id = heading_stack.last().map(|(id, _)| id.clone());
        let block_type = if is_list_item(&para_text) {
            "list_item"
        } else {
            "text"
        };

        blocks.push(ParsedBlock {
            id: Uuid::new_v4().to_string(),
            parent_id,
            order_idx: order_counter,
            level: heading_stack.len() as i32,
            block_type: block_type.to_string(),
            content: para_text,
            metadata: "{}".to_string(),
        });
        order_counter += 1;
    }

    blocks
}

/// 跳过 YAML frontmatter (--- ... ---)
fn skip_frontmatter(lines: &[&str]) -> usize {
    if lines.first().map(|l| l.trim()) == Some("---") {
        for (i, line) in lines.iter().enumerate().skip(1) {
            if line.trim() == "---" {
                return i + 1;
            }
        }
    }
    0
}

/// 解析标题: "# 标题" → (1, "标题")
fn parse_heading(line: &str) -> (i32, &str) {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|c| *c == '#').count() as i32;
    let level_usize = level as usize;
    let title = trimmed[level_usize..].trim();
    (level.min(6), title)
}

/// 检测后续行是否构成表格（至少有一行分隔线 `|---|`）
fn has_table_structure(lines: &[&str], start: usize) -> bool {
    if start + 1 >= lines.len() {
        return false;
    }
    let next = lines[start + 1].trim();
    // 表格分隔线: |---|:---| 等
    let is_separator = next.contains('|')
        && next.chars().all(|c| {
            c == '|' || c == '-' || c == ':' || c == ' ' || c == '\t'
        });
    is_separator
}

/// 提取图片元数据: ![alt](path)
fn extract_image_meta(line: &str) -> String {
    let alt_start = line.find("![");
    let alt_end = line.find("](");
    let path_end = line.rfind(')');

    match (alt_start, alt_end, path_end) {
        (Some(s), Some(e), Some(p)) if s < e && e < p => {
            let alt = &line[s + 2..e];
            let path = &line[e + 2..p];
            format!(
                "{{\"alt\":\"{}\",\"path\":\"{}\"}}",
                alt.replace('"', "\\\""),
                path.replace('"', "\\\"")
            )
        }
        _ => "{}".to_string(),
    }
}

/// 检测是否为列表项
fn is_list_item(text: &str) -> bool {
    let trimmed = text.trim_start();
    // 无序列表: - /* / +
    // 有序列表: 1. 2) 等
    trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
        || trimmed
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .count()
            > 0
            && trimmed
                .chars()
                .skip_while(|c| c.is_ascii_digit())
                .next()
                .map_or(false, |c| c == '.' || c == ')')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heading() {
        assert_eq!(parse_heading("# Title"), (1, "Title"));
        assert_eq!(parse_heading("## Section"), (2, "Section"));
        assert_eq!(parse_heading("###   Sub  "), (3, "Sub"));
    }

    #[test]
    fn test_skip_frontmatter() {
        let lines = vec!["---", "title: Test", "---", "# Real Content"];
        assert_eq!(skip_frontmatter(&lines), 3);
    }

    #[test]
    fn test_basic_markdown() {
        let md = "# Ch1\n\nSome text.\n\n## Ch1.1\n\nMore text.";
        let blocks = parse_markdown(md);
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].block_type, "heading");
        assert_eq!(blocks[0].content, "Ch1");
        assert_eq!(blocks[1].block_type, "text");
        assert_eq!(blocks[3].block_type, "text");
    }

    #[test]
    fn test_hierarchy() {
        let md = "# H1\n\n## H2\n\n### H3\n\nText under H3.";
        let blocks = parse_markdown(md);
        // H2 的 parent 应该是 H1
        assert_eq!(blocks[1].parent_id.as_ref(), Some(&blocks[0].id));
        // H3 的 parent 应该是 H2
        assert_eq!(blocks[2].parent_id.as_ref(), Some(&blocks[1].id));
        // Text 的 parent 应该是 H3
        assert_eq!(blocks[3].parent_id.as_ref(), Some(&blocks[2].id));
    }
}
