use anyhow::Result;

/// Parsed prompt file.
#[derive(Debug, Clone)]
pub struct PromptFile {
    pub raw: String,
    pub prompts: Vec<String>,
}

pub fn parse_prompts_markdown(raw: String) -> PromptFile {
    // Heuristics:
    // - Prefer fenced blocks labelled ```prompt
    // - Else, split on headings (##/###)
    // - Else, split on blank-line paragraphs
    let fenced = extract_fenced_prompts(&raw);
    let prompts = if !fenced.is_empty() {
        fenced
    } else {
        let headed = split_on_headings(&raw);
        if headed.len() >= 2 {
            headed
        } else {
            split_on_blank_paragraphs(&raw)
        }
    };

    PromptFile { raw, prompts }
}

pub fn load_or_create_prompts_file(path: &str) -> Result<PromptFile> {
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(parse_prompts_markdown(raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let template = default_prompts_template();
            std::fs::write(path, &template)?;
            Ok(parse_prompts_markdown(template))
        }
        Err(e) => Err(e.into()),
    }
}

fn default_prompts_template() -> String {
    r#"# Prompts

This file is read by the TUI on startup.

## Prompt 1
What is Elasticsearch?

## Prompt 2
Explain what Kibana is, in one paragraph.
"#
    .to_string()
}

fn extract_fenced_prompts(raw: &str) -> Vec<String> {
    let mut prompts = Vec::new();
    let mut in_block = false;
    let mut buf = String::new();

    for line in raw.lines() {
        let trimmed = line.trim_end();
        if !in_block {
            if let Some(info) = trimmed.strip_prefix("```")
                && info.trim().eq_ignore_ascii_case("prompt") {
                    in_block = true;
                    buf.clear();
                }
        } else if trimmed == "```" {
            in_block = false;
            let p = buf.trim().to_string();
            if !p.is_empty() {
                prompts.push(p);
            }
            buf.clear();
        } else {
            buf.push_str(line);
            buf.push('\n');
        }
    }

    prompts
}

fn split_on_headings(raw: &str) -> Vec<String> {
    let mut prompts = Vec::new();
    let mut current = String::new();
    let mut saw_heading = false;

    for line in raw.lines() {
        let is_heading = line.starts_with("## ") || line.starts_with("### ");
        if is_heading {
            saw_heading = true;
            let p = current.trim().to_string();
            if !p.is_empty() {
                prompts.push(p);
            }
            current.clear();
            continue;
        }

        // Skip top-level title lines if we're going to parse prompts from headings.
        if saw_heading && line.starts_with("# ") {
            continue;
        }

        current.push_str(line);
        current.push('\n');
    }

    let p = current.trim().to_string();
    if !p.is_empty() {
        prompts.push(p);
    }

    prompts
}

fn split_on_blank_paragraphs(raw: &str) -> Vec<String> {
    raw.split("\n\n")
        .map(|chunk| chunk.trim())
        .filter(|chunk| !chunk.is_empty())
        .filter(|chunk| !chunk.starts_with('#'))
        .map(|chunk| chunk.to_string())
        .collect()
}
