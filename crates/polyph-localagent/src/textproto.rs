//! Tool calls for models without native tool calling. The model writes
//! blocks like:
//!
//! ```text
//! <tool name="read">{"path": "src/lib.rs"}</tool>
//! ```

use crate::llm::ToolCallOut;

pub const INSTRUCTIONS: &str = "To use a tool, write a block exactly like this, with JSON arguments:\n\
<tool name=\"read\">{\"path\": \"src/lib.rs\"}</tool>\n\
Tools: read {path}, write {path, content}, edit {path, old, new}, list {path}, glob {pattern}, grep {pattern, path}, shell {command}.\n\
After writing tool blocks, stop and wait for the results. When you are done, answer without any tool block.";

pub fn parse(text: &str) -> Vec<ToolCallOut> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(start) = rest.find("<tool name=\"") {
        let after = &rest[start + 12..];
        let Some(q) = after.find('"') else { break };
        let name = after[..q].to_string();
        let Some(gt) = after[q..].find('>') else { break };
        let body_start = q + gt + 1;
        let Some(end) = after[body_start..].find("</tool>") else { break };
        let args = after[body_start..body_start + end].trim().to_string();
        out.push(ToolCallOut { id: format!("text_{}", out.len()), name, arguments: args });
        rest = &after[body_start + end + 7..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks() {
        let c = parse(
            "I will look.\n<tool name=\"read\">{\"path\": \"a.rs\"}</tool>\n<tool name=\"shell\">\n{\"command\": \"ls\"}\n</tool>",
        );
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].name, "read");
        assert_eq!(c[1].arguments, "{\"command\": \"ls\"}");
        assert!(parse("no tools here").is_empty());
    }
}
