pub fn format_source(source: &str) -> String {
    let mut output = String::new();
    let mut blank_pending = false;
    let mut indent = 0usize;

    for raw_line in source.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            blank_pending = true;
            continue;
        }

        if !output.is_empty() && blank_pending {
            output.push('\n');
        }
        blank_pending = false;

        if trimmed.starts_with('}') {
            indent = indent.saturating_sub(1);
        }

        output.push_str(&"  ".repeat(indent));
        output.push_str(&normalize_line(trimmed));
        output.push('\n');

        if trimmed.ends_with('{') {
            indent += 1;
        }
    }

    output
}

fn normalize_line(line: &str) -> String {
    if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
        return line.to_string();
    }

    let mut normalized = String::new();
    let mut in_string = false;
    let mut previous_space = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if !in_string
                    && !normalized.is_empty()
                    && !normalized.ends_with(' ')
                    && !normalized.ends_with('=')
                    && !normalized.ends_with('(')
                {
                    normalized.push(' ');
                }
                in_string = !in_string;
                previous_space = false;
                normalized.push(ch);
            }
            c if c.is_whitespace() && !in_string => {
                previous_space = true;
            }
            '{' | '}' | '=' if !in_string => {
                trim_trailing_space(&mut normalized);
                normalized.push(' ');
                normalized.push(ch);
                normalized.push(' ');
                previous_space = false;
            }
            '-' if !in_string && chars.peek() == Some(&'>') => {
                chars.next();
                trim_trailing_space(&mut normalized);
                normalized.push_str(" -> ");
                previous_space = false;
            }
            _ => {
                if previous_space && !normalized.ends_with(' ') {
                    normalized.push(' ');
                }
                previous_space = false;
                normalized.push(ch);
            }
        }
    }

    normalized.trim().to_string()
}

fn trim_trailing_space(value: &mut String) {
    while value.ends_with(' ') {
        value.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_blocks_and_preserves_comments() {
        let source = r#"language   0.1

working "Fmt Test"{
// keep me
tempo   120
meter 4/4
seed "has spaces"
rite main bars 1{
invoke kick with hits every 1/16
raise tension 0.1->0.9
}
}
"#;

        insta::assert_snapshot!("formatter_basic", format_source(source));
    }
}
