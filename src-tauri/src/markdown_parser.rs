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

/// 将 Markdown 文本解析为章节级语义块
///
/// 每个 `#` 标题定义一个 section，其 content 是从标题行到下一个同级/上级标题之间的
/// **原始 Markdown**（保留所有格式：HTML 表格、代码块、图片等）。
///
/// 层级关系通过栈维护：每个 section 的 parent 是上一级标题对应的 section。
pub fn parse_markdown(md_content: &str) -> Vec<ParsedBlock> {
    let lines: Vec<&str> = md_content.lines().collect();
    let mut blocks: Vec<ParsedBlock> = Vec::new();
    let mut order_counter: i32 = 0;

    // 跳过 YAML frontmatter
    let start_idx = skip_frontmatter(&lines);

    // 层级栈: (section_id, level)
    let mut heading_stack: Vec<(String, i32)> = Vec::new();

    let mut i = start_idx;

    // ---- 处理标题之前的内容（如有）→ 作为根 section (level=0) ----
    if i < lines.len() && !lines[i].starts_with('#') {
        let mut root_lines: Vec<&str> = Vec::new();
        while i < lines.len() && !lines[i].starts_with('#') {
            root_lines.push(lines[i]);
            i += 1;
        }
        let content = root_lines.join("\n").trim().to_string();
        if !content.is_empty() {
            blocks.push(ParsedBlock {
                id: Uuid::new_v4().to_string(),
                parent_id: None,
                order_idx: order_counter,
                level: 0,
                block_type: "section".to_string(),
                content,
                metadata: "{}".to_string(),
            });
            order_counter += 1;
        }
    }

    // ---- 按标题拆分 section ----
    while i < lines.len() {
        // 跳过空行
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        // 遇到标题 → 开始新 section
        if lines[i].starts_with('#') {
            let (level, _title) = parse_heading(lines[i]);

            // 弹出所有 >= 当前 level 的 section
            while let Some((_, l)) = heading_stack.last() {
                if *l >= level {
                    heading_stack.pop();
                } else {
                    break;
                }
            }

            let parent_id = heading_stack.last().map(|(id, _)| id.clone());

            // 收集从当前标题到下一个标题之间的所有行
            let mut section_lines: Vec<&str> = Vec::new();
            section_lines.push(lines[i]); // 包含标题行本身
            i += 1;

            while i < lines.len() && !lines[i].starts_with('#') {
                section_lines.push(lines[i]);
                i += 1;
            }

            let content = section_lines.join("\n");

            let block = ParsedBlock {
                id: Uuid::new_v4().to_string(),
                parent_id,
                order_idx: order_counter,
                level,
                block_type: "section".to_string(),
                content,
                metadata: format!("{{\"heading_level\":{}}}", level),
            };

            let bid = block.id.clone();
            blocks.push(block);
            heading_stack.push((bid, level));
            order_counter += 1;
            // i 已指向下一个 section 或末尾
        } else {
            // 非标题行（理论上不会到这里，但作为兜底）
            i += 1;
        }
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
        // 3 sections: Ch1, Ch1.1(child), Ch2
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].level, 1);
        assert!(blocks[0].content.contains("Ch1"));
        assert!(blocks[0].content.contains("Some text"));
        assert_eq!(blocks[1].level, 2);
        assert_eq!(blocks[1].parent_id.as_ref(), Some(&blocks[0].id));
        assert_eq!(blocks[2].level, 1);
        assert!(blocks[2].content.contains("Ch2"));
    }

    #[test]
    fn test_section_hierarchy() {
        let md = "# H1\n\n## H2\n\n### H3\n\nText under H3.\n\n## H2.2\n\nMore.";
        let blocks = parse_markdown(md);
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[1].parent_id.as_ref(), Some(&blocks[0].id));
        assert_eq!(blocks[2].parent_id.as_ref(), Some(&blocks[1].id));
        assert_eq!(blocks[3].parent_id.as_ref(), Some(&blocks[0].id));
    }

    #[test]
    fn test_preserves_html_tables() {
        let md = "# Catalog\n\n<table><tr><td>Chap 1</td></tr></table>\n\nSome text.";
        let blocks = parse_markdown(md);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].content.contains("<table>"));
        assert!(blocks[0].content.contains("</table>"));
    }
}
