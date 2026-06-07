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

/// 将 Markdown 文本按物理行分块
///
/// 每一行 = 一个块，order_idx = 行号（跳过 YAML frontmatter 后从0开始）。
/// 标题行（# 开头）标记为 `heading` 类型并维护 TOC 层级栈，
/// 空行标记为 `empty`，其余为 `text`。
pub fn parse_markdown(md_content: &str) -> Vec<ParsedBlock> {
    let lines: Vec<&str> = md_content.lines().collect();
    let mut blocks: Vec<ParsedBlock> = Vec::new();

    // 跳过 YAML frontmatter
    let start_idx = skip_frontmatter(&lines);

    // 层级栈: (heading_id, level)
    let mut heading_stack: Vec<(String, i32)> = Vec::new();

    for (line_no, line) in lines.iter().enumerate().skip(start_idx) {
        let order_idx = (line_no - start_idx) as i32;
        let trimmed = line.trim();

        let (block_type, level, parent_id, bid) = if trimmed.starts_with('#') {
            // ---- 标题行 ----
            let (mut level, _) = parse_heading(line);
            level = level.clamp(1, 6);

            while let Some((_, l)) = heading_stack.last() {
                if *l >= level {
                    heading_stack.pop();
                } else {
                    break;
                }
            }

            let parent_id = heading_stack.last().map(|(id, _)| id.clone());
            let id = Uuid::new_v4().to_string();
            heading_stack.push((id.clone(), level));

            ("heading".to_string(), level, parent_id, Some(id))
        } else if trimmed.is_empty() {
            // ---- 空行 ----
            let level = heading_stack.len() as i32;
            let parent_id = heading_stack.last().map(|(id, _)| id.clone());
            ("empty".to_string(), level, parent_id, None)
        } else {
            // ---- 普通文本行 ----
            let level = heading_stack.len() as i32;
            let parent_id = heading_stack.last().map(|(id, _)| id.clone());
            ("text".to_string(), level, parent_id, None)
        };

        let id = bid.unwrap_or_else(|| Uuid::new_v4().to_string());

        blocks.push(ParsedBlock {
            id,
            parent_id,
            order_idx,
            level,
            block_type,
            content: line.to_string(), // 保留原始行内容
            metadata: "{}".to_string(),
        });
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
    fn test_section_parsing() {
        let md = "# Ch1\n\nSome text.\n\n## Ch1.1\n\nMore text.\n\n# Ch2\n\nCh2 text.";
        let blocks = parse_markdown(md);
        // 11 行（含空行）
        assert_eq!(blocks.len(), 11);
        assert_eq!(blocks[0].block_type, "heading"); // # Ch1
        assert_eq!(blocks[0].level, 1);
        assert_eq!(blocks[4].block_type, "heading"); // ## Ch1.1
        assert_eq!(blocks[4].level, 2);
        assert_eq!(blocks[4].parent_id.as_ref(), Some(&blocks[0].id));
        assert_eq!(blocks[8].block_type, "heading"); // # Ch2
        assert_eq!(blocks[8].level, 1);
        assert!(blocks[8].parent_id.is_none());
    }

    #[test]
    fn test_section_hierarchy() {
        let md = "# H1\n\n## H2\n\n### H3\n\nText under H3.\n\n## H2.2\n\nMore.";
        let blocks = parse_markdown(md);
        assert_eq!(blocks[0].block_type, "heading"); // # H1
        assert_eq!(blocks[2].block_type, "heading"); // ## H2
        assert_eq!(blocks[2].parent_id.as_ref(), Some(&blocks[0].id));
        assert_eq!(blocks[4].block_type, "heading"); // ### H3
        assert_eq!(blocks[4].parent_id.as_ref(), Some(&blocks[2].id));
        assert_eq!(blocks[8].block_type, "heading"); // ## H2.2
        assert_eq!(blocks[8].parent_id.as_ref(), Some(&blocks[0].id));
    }

    #[test]
    fn test_preserves_html_tables() {
        let md = "# Catalog\n\n<table><tr><td>Chap 1</td></tr></table>\n\nSome text.";
        let blocks = parse_markdown(md);
        // HTML table 行原样保留
        let table_line = blocks.iter().find(|b| b.content.contains("<table>"));
        assert!(table_line.is_some());
    }
}
