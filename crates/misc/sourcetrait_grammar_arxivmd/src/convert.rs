use crate::*;

/// Extract the LaTeX source archive, pick the main `.tex`, run pandoc to GFM,
/// and sanitize the result. Ported from markxiv's pandoc converter.
pub(crate) fn latex_to_markdown(tar_bytes: &[u8]) -> Result<String> {
    let workdir = tempfile::TempDir::new()
        .map_err(|e| ArxivmdError::Io { context: "temp dir".into(), source: e })?;
    let root = workdir.path();

    let tar_path = root.join("source.tar");
    std::fs::write(&tar_path, tar_bytes)
        .map_err(|e| ArxivmdError::Io { context: "write tar".into(), source: e })?;

    // Try plain tar first, then gzip.
    if extract_tar(root, &tar_path, false).is_err() {
        extract_tar(root, &tar_path, true)?;
    }

    let tex_files = collect_tex_files(root)?;
    let main_tex = select_main_tex(&tex_files)
        .ok_or_else(|| ArxivmdError::Convert { message: "no .tex source found".into() })?;
    let parent = main_tex.parent().unwrap_or(root);
    let main_file = main_tex
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ArxivmdError::Convert { message: "invalid main .tex path".into() })?;

    let md = run_pandoc(parent, main_file)?;
    Ok(sanitize_markdown(&md))
}

/// Convert a PDF to text via `pdftotext -raw` (the fallback path).
pub(crate) fn pdf_to_markdown(pdf_bytes: &[u8]) -> Result<String> {
    let workdir = tempfile::TempDir::new()
        .map_err(|e| ArxivmdError::Io { context: "temp dir".into(), source: e })?;
    let pdf_path = workdir.path().join("source.pdf");
    std::fs::write(&pdf_path, pdf_bytes)
        .map_err(|e| ArxivmdError::Io { context: "write pdf".into(), source: e })?;

    let pdftotext = std::env::var("ARXIVMD_PDFTOTEXT").unwrap_or_else(|_| "pdftotext".into());
    let out = std::process::Command::new(&pdftotext)
        .arg("-raw")
        .arg(&pdf_path)
        .arg("-")
        .output()
        .map_err(|e| ArxivmdError::Convert { message: format!("pdftotext spawn: {e}") })?;
    if !out.status.success() {
        return Err(ArxivmdError::Convert {
            message: format!("pdftotext failed: {}", String::from_utf8_lossy(&out.stderr).trim()),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn extract_tar(workdir: &std::path::Path, tar_path: &std::path::Path, gzip: bool) -> Result<()> {
    let mut cmd = std::process::Command::new("tar");
    cmd.current_dir(workdir);
    if gzip {
        cmd.args(["-x", "-z", "-f"]).arg(tar_path).arg("-C").arg(workdir);
    } else {
        cmd.args(["-x", "-f"]).arg(tar_path).arg("-C").arg(workdir);
    }
    let out = cmd
        .output()
        .map_err(|e| ArxivmdError::Convert { message: format!("tar spawn: {e}") })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(ArxivmdError::Convert {
            message: format!("tar failed: {}", String::from_utf8_lossy(&out.stderr).trim()),
        })
    }
}

fn collect_tex_files(root: &std::path::Path) -> Result<Vec<(std::path::PathBuf, String)>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir)
            .map_err(|e| ArxivmdError::Io { context: format!("read dir {}", dir.display()), source: e })?;
        for entry in rd {
            let entry = entry
                .map_err(|e| ArxivmdError::Io { context: "read entry".into(), source: e })?;
            let path = entry.path();
            let ft = entry
                .file_type()
                .map_err(|e| ArxivmdError::Io { context: "file type".into(), source: e })?;
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() && path.extension().map(|e| e == "tex").unwrap_or(false) {
                if let Ok(s) = std::fs::read_to_string(&path) {
                    out.push((path, s));
                }
            }
        }
    }
    Ok(out)
}

fn run_pandoc(cwd: &std::path::Path, main_file: &str) -> Result<String> {
    let pandoc = std::env::var("ARXIVMD_PANDOC").unwrap_or_else(|_| "pandoc".into());
    let out = std::process::Command::new(&pandoc)
        .current_dir(cwd)
        .args(["-f", "latex", "-t", "gfm"])
        .arg(main_file)
        .output()
        .map_err(|e| ArxivmdError::Convert { message: format!("pandoc spawn: {e}") })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(ArxivmdError::Convert {
            message: format!("pandoc failed: {}", String::from_utf8_lossy(&out.stderr).trim()),
        })
    }
}

/// Pick the main `.tex`: prefer one with `\documentclass`, avoid supplementary
/// names, break ties toward the longer file. Ported from markxiv's tex_main.rs.
fn select_main_tex(files: &[(std::path::PathBuf, String)]) -> Option<std::path::PathBuf> {
    if files.is_empty() {
        return None;
    }
    if files.len() == 1 {
        return Some(files[0].0.clone());
    }
    let mut tex: Vec<&(std::path::PathBuf, String)> = files.iter().collect();
    tex.sort_by_key(|(p, c)| {
        let has_documentclass = c.contains("\\documentclass");
        let supplementary = is_supplementary(p);
        (!has_documentclass, supplementary, std::cmp::Reverse(c.len()))
    });
    tex.first().map(|(p, _)| p.clone())
}

fn is_supplementary(p: &std::path::Path) -> bool {
    let name = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ["supp", "supplement", "appendix", "si", "supplementary"]
        .iter()
        .any(|bad| name.contains(bad))
}

/// The sanitize pipeline (ported from markxiv): figures -> caption blockquotes,
/// KaTeX fixups, display-math isolation, then HTML stripped while math survives.
fn sanitize_markdown(input: &str) -> String {
    let out = extract_figure_captions(input);
    let out = fix_katex_commands(&out);
    let out = normalize_display_math(&out);
    strip_html_preserve_math(out.trim_start())
}

fn extract_figure_captions(input: &str) -> String {
    let mut out = input.to_string();
    let mut figure_num = 0u32;
    while let Some(start) = out.find("<figure") {
        if let Some(rel_end) = out[start..].find("</figure>") {
            let end = start + rel_end + "</figure>".len();
            let block = out[start..end].to_string();
            figure_num += 1;
            let caption = figcaption_text(&block);
            let replacement = match caption {
                Some(cap) => format!("\n\n> **Figure {figure_num}:** {cap}\n\n"),
                None => format!("\n\n> **Figure {figure_num}**\n\n"),
            };
            out.replace_range(start..end, &replacement);
        } else if let Some(rel_end) = out[start..].find("\n\n") {
            let end = start + rel_end;
            out.replace_range(start..end, "");
        } else {
            out.truncate(start);
            break;
        }
    }
    out
}

fn figcaption_text(block: &str) -> Option<String> {
    let fc_start = block.find("<figcaption")?;
    let content_start = block[fc_start..].find('>').map(|i| fc_start + i + 1)?;
    let fc_end = block.find("</figcaption>")?;
    if content_start >= fc_end {
        return None;
    }
    let text = block[content_start..fc_end].trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn fix_katex_commands(input: &str) -> String {
    static RE_MATHCAL: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(\\mathcal\{[^}]*\})\{").unwrap());
    static RE_TEXTSC: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\\textsc\{([^}]*)\}").unwrap());
    static RE_CALL: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\\Call\{([^}]*)\}\{([^}]*)\}").unwrap());
    static RE_MATHBBM: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\\mathbbm\{([^}]*)\}").unwrap());

    let s = RE_MATHCAL.replace_all(input, "${1}_{");
    let s = RE_TEXTSC.replace_all(&s, r"\textbf{$1}");
    let s = RE_CALL.replace_all(&s, r"\textbf{$1}($2)");
    let s = RE_MATHBBM.replace_all(&s, r"\mathbb{$1}");
    s.into_owned()
}

fn normalize_display_math(input: &str) -> String {
    static RE_DISPLAY: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?s)\$\$(.+?)\$\$").unwrap());

    let mut result = input.to_string();
    let matches: Vec<_> = RE_DISPLAY.find_iter(input).collect();
    for m in matches.into_iter().rev() {
        let start = m.start();
        let end = m.end();
        let matched = &input[start..end];
        let inner = &matched[2..matched.len() - 2];

        let before = &input[..start];
        let after = &input[end..];
        let line_start_ok = before.is_empty()
            || before.ends_with('\n')
            || before.trim_end().ends_with('\n')
            || before.trim_end().is_empty();
        let line_end_ok =
            after.is_empty() || after.starts_with('\n') || after.trim_start().starts_with('\n');
        if line_start_ok && line_end_ok {
            continue;
        }

        let mut replacement = String::new();
        if !before.is_empty() && !before.ends_with('\n') {
            replacement.push('\n');
        }
        replacement.push_str("$$");
        replacement.push_str(inner);
        replacement.push_str("$$");
        if !after.is_empty() && !after.starts_with('\n') {
            replacement.push('\n');
        }
        result.replace_range(start..end, &replacement);
    }
    result
}

// Strip HTML tags while copying `$...$` / `$$...$$` math verbatim, so `<` / `>`
// inside math survive.
fn strip_html_preserve_math(input: &str) -> String {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;
    while i < len {
        if i + 1 < len && bytes[i] == b'$' && bytes[i + 1] == b'$' {
            out.push_str("$$");
            i += 2;
            while i + 1 < len && !(bytes[i] == b'$' && bytes[i + 1] == b'$') {
                let ch = input[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
            if i + 1 < len && bytes[i] == b'$' && bytes[i + 1] == b'$' {
                out.push_str("$$");
                i += 2;
            }
        } else if bytes[i] == b'$' {
            out.push('$');
            i += 1;
            while i < len && bytes[i] != b'$' {
                let ch = input[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
            if i < len && bytes[i] == b'$' {
                out.push('$');
                i += 1;
            }
        } else if bytes[i] == b'<' {
            i += 1;
            while i < len && bytes[i] != b'>' {
                i += 1;
            }
            if i < len {
                i += 1;
            }
        } else {
            let ch = input[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn figure_block_becomes_caption() {
        let s = "<figure><figcaption>Overview</figcaption></figure>\n\n# Title";
        let out = sanitize_markdown(s);
        assert!(!out.contains("<figure"));
        assert!(out.contains("> **Figure 1:** Overview"));
        assert!(out.contains("# Title"));
    }

    #[test]
    fn strips_html_keeps_math() {
        let s = "text <em>bold</em> and $a < b > c$ end <p>para</p>";
        assert_eq!(sanitize_markdown(s), "text bold and $a < b > c$ end para");
    }

    #[test]
    fn fixes_katex() {
        assert_eq!(fix_katex_commands(r"$\textsc{Adam}$"), r"$\textbf{Adam}$");
        assert_eq!(fix_katex_commands(r"$\mathbbm{1}$"), r"$\mathbb{1}$");
    }

    #[test]
    fn picks_documentclass_over_supplement() {
        let files = vec![
            (std::path::PathBuf::from("supp.tex"), "appendix".to_string()),
            (std::path::PathBuf::from("paper.tex"), "\\documentclass{article}".to_string()),
        ];
        assert_eq!(select_main_tex(&files), Some(std::path::PathBuf::from("paper.tex")));
    }
}
