use crate::*;

const API: &str = "https://export.arxiv.org/api/query";

/// Metadata parsed from an arXiv Atom entry.
pub(crate) struct Metadata {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) authors: Vec<String>,
    pub(crate) published: String,
    pub(crate) updated: String,
    pub(crate) primary_category: String,
    pub(crate) categories: Vec<String>,
    pub(crate) doi: String,
    pub(crate) comment: String,
    pub(crate) journal_ref: String,
    pub(crate) abs_url: String,
    pub(crate) pdf_url: String,
}

/// The e-print endpoint gives either a source archive or nothing (PDF-only).
pub(crate) enum Source {
    Archive(Vec<u8>),
    PdfOnly,
}

/// Blocking arXiv HTTP client. A one-shot CLI has no need for async.
pub(crate) struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    pub(crate) fn new() -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .user_agent("arxivmd (sourcetrait grammar)")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ArxivmdError::Network { message: e.to_string() })?;
        Ok(Self { http })
    }

    /// Title / abstract / authors and friends, from the Atom API.
    pub(crate) fn metadata(&self, id: &str) -> Result<Metadata> {
        let url = reqwest::Url::parse_with_params(API, &[("id_list", id)])
            .map_err(|e| ArxivmdError::Network { message: e.to_string() })?;
        let res = self
            .http
            .get(url)
            .header(reqwest::header::ACCEPT, "application/atom+xml")
            .send()
            .map_err(|e| ArxivmdError::Network { message: e.to_string() })?;
        if !res.status().is_success() {
            return Err(ArxivmdError::Network {
                message: format!("arXiv metadata HTTP {}", res.status()),
            });
        }
        let body = res
            .text()
            .map_err(|e| ArxivmdError::Network { message: e.to_string() })?;
        parse_atom_metadata(id, &body).ok_or_else(|| ArxivmdError::NotFound { id: id.to_string() })
    }

    /// The e-print source archive (tar/tar.gz), or PdfOnly when none is offered.
    pub(crate) fn source(&self, id: &str) -> Result<Source> {
        let url = format!("https://arxiv.org/e-print/{id}");
        let res = self
            .http
            .get(&url)
            .header(
                reqwest::header::ACCEPT,
                "application/x-eprint-tar, application/x-tar, application/octet-stream",
            )
            .send()
            .map_err(|e| ArxivmdError::Network { message: e.to_string() })?;
        let status = res.status();
        if status.is_success() {
            let content_type = res
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_ascii_lowercase();
            let bytes = res
                .bytes()
                .map_err(|e| ArxivmdError::Network { message: e.to_string() })?;
            if content_type.contains("application/pdf") || looks_like_pdf(&bytes) {
                return Ok(Source::PdfOnly);
            }
            if content_type.contains("text/html") || looks_like_html(&bytes) {
                return Err(ArxivmdError::Network {
                    message: "arXiv returned HTML for the e-print request".into(),
                });
            }
            return Ok(Source::Archive(bytes.to_vec()));
        }
        let code = status.as_u16();
        if code == 400 || code == 403 || code == 404 {
            return Ok(Source::PdfOnly);
        }
        Err(ArxivmdError::Network {
            message: format!("arXiv e-print HTTP {status}"),
        })
    }

    /// The rendered PDF (the fallback source).
    pub(crate) fn pdf(&self, id: &str) -> Result<Vec<u8>> {
        let url = format!("https://arxiv.org/pdf/{id}.pdf");
        let res = self
            .http
            .get(&url)
            .header(reqwest::header::ACCEPT, "application/pdf")
            .send()
            .map_err(|e| ArxivmdError::Network { message: e.to_string() })?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ArxivmdError::NotFound { id: id.to_string() });
        }
        if !res.status().is_success() {
            return Err(ArxivmdError::Network {
                message: format!("arXiv pdf HTTP {}", res.status()),
            });
        }
        let bytes = res
            .bytes()
            .map_err(|e| ArxivmdError::Network { message: e.to_string() })?;
        Ok(bytes.to_vec())
    }
}

/// The bare arXiv id from an id or a full arXiv URL. Handles new-style
/// (`1706.03762`, `1706.03762v5`) and old-style (`math/0211159`) ids, and the
/// `/abs/` and `/pdf/` URL forms (trailing `.pdf` dropped).
pub(crate) fn parse_id(input: &str) -> String {
    let s = input.trim();
    let tail = s
        .split_once("/abs/")
        .or_else(|| s.split_once("/pdf/"))
        .map(|(_, t)| t)
        .unwrap_or(s);
    let tail = tail.split(['?', '#']).next().unwrap_or(tail);
    let tail = tail.strip_suffix(".pdf").unwrap_or(tail);
    tail.trim_matches('/').to_string()
}

fn parse_atom_metadata(id: &str, atom: &str) -> Option<Metadata> {
    let start = atom.find("<entry")?;
    let end_rel = atom[start..].find("</entry>")?;
    let entry = &atom[start..start + end_rel + "</entry>".len()];

    let title = collapse_ws(&extract_tag(entry, "title")?);
    let summary = collapse_ws(&extract_tag(entry, "summary").unwrap_or_default());
    let authors = extract_authors(entry);
    let published = extract_tag(entry, "published").unwrap_or_default().trim().to_string();
    let updated = extract_tag(entry, "updated").unwrap_or_default().trim().to_string();
    let primary_category = extract_attr(entry, "arxiv:primary_category", "term").unwrap_or_default();
    let categories = extract_all_attrs(entry, "category", "term");
    let doi = extract_tag(entry, "arxiv:doi").unwrap_or_default().trim().to_string();
    let comment = collapse_ws(&extract_tag(entry, "arxiv:comment").unwrap_or_default());
    let journal_ref = collapse_ws(&extract_tag(entry, "arxiv:journal_ref").unwrap_or_default());

    Some(Metadata {
        id: id.to_string(),
        title,
        summary,
        authors,
        published,
        updated,
        primary_category,
        categories,
        doi,
        comment,
        journal_ref,
        abs_url: format!("https://arxiv.org/abs/{id}"),
        pdf_url: format!("https://arxiv.org/pdf/{id}.pdf"),
    })
}

// The inner text of the first `<tag ...>...</tag>` (attributes on the open tag
// are tolerated).
fn extract_tag(s: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = s.find(&open)?;
    let after_open = &s[start..];
    let end_open = after_open.find('>')?;
    let after = &after_open[end_open + 1..];
    let close = format!("</{tag}>");
    let end = after.find(&close)?;
    Some(after[..end].to_string())
}

// The value of `attr="..."` on the first `<tag ...>` open tag.
fn extract_attr(s: &str, tag: &str, attr: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = s.find(&open)?;
    let after = &s[start..];
    let tag_end = after.find('>')?;
    let tag_body = &after[..tag_end];
    let needle = format!("{attr}=\"");
    let pos = tag_body.find(&needle)?;
    let rest = &tag_body[pos + needle.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

// The `attr` value of every `<tag ...>` open tag.
fn extract_all_attrs(s: &str, tag: &str, attr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let open = format!("<{tag}");
    let needle = format!("{attr}=\"");
    let mut remainder = s;
    while let Some(start) = remainder.find(&open) {
        let after = &remainder[start..];
        let Some(tag_end) = after.find('>') else { break };
        let tag_body = &after[..tag_end];
        if let Some(pos) = tag_body.find(&needle) {
            let rest = &tag_body[pos + needle.len()..];
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
            }
        }
        remainder = &remainder[start + tag_end..];
    }
    out
}

fn extract_authors(entry: &str) -> Vec<String> {
    let mut authors = Vec::new();
    let mut remainder = entry;
    while let Some(start) = remainder.find("<author") {
        let section = &remainder[start..];
        let Some(end_rel) = section.find("</author>") else { break };
        let end = start + end_rel + "</author>".len();
        let block = &remainder[start..end];
        if let Some(name) = extract_tag(block, "name") {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                authors.push(trimmed.to_string());
            }
        }
        remainder = &remainder[end..];
    }
    authors
}

// Collapse runs of whitespace to single spaces (Atom title/summary carry the
// document's line breaks and indentation).
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn looks_like_pdf(bytes: &[u8]) -> bool {
    bytes.len() >= 5 && &bytes[..5] == b"%PDF-"
}

fn looks_like_html(bytes: &[u8]) -> bool {
    let n = bytes.len().min(1024);
    let mut i = 0;
    while i < n && matches!(bytes[i], b'\t' | b'\n' | b'\r' | b' ') {
        i += 1;
    }
    if i >= n || bytes[i] != b'<' {
        return false;
    }
    let s = String::from_utf8_lossy(&bytes[i..n]).to_ascii_lowercase();
    s.starts_with("<!doctype html") || s.starts_with("<html")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_id_forms() {
        assert_eq!(parse_id("1706.03762"), "1706.03762");
        assert_eq!(parse_id("https://arxiv.org/abs/1706.03762"), "1706.03762");
        assert_eq!(parse_id("https://arxiv.org/abs/1706.03762v5"), "1706.03762v5");
        assert_eq!(parse_id("https://arxiv.org/pdf/1706.03762.pdf"), "1706.03762");
        assert_eq!(parse_id("https://arxiv.org/abs/math/0211159"), "math/0211159");
    }

    #[test]
    fn parse_atom_extracts_fields() {
        let atom = r#"<feed>
          <entry>
            <title>Attention Is
              All You Need</title>
            <summary> A summary. </summary>
            <published>2017-06-12T00:00:00Z</published>
            <author><name>Ashish Vaswani</name></author>
            <author><name> Noam Shazeer </name></author>
            <arxiv:primary_category term="cs.CL"/>
            <category term="cs.CL"/>
            <category term="cs.LG"/>
            <arxiv:doi>10.0/xyz</arxiv:doi>
          </entry>
        </feed>"#;
        let m = parse_atom_metadata("1706.03762", atom).expect("metadata");
        assert_eq!(m.title, "Attention Is All You Need");
        assert_eq!(m.summary, "A summary.");
        assert_eq!(m.authors, vec!["Ashish Vaswani", "Noam Shazeer"]);
        assert_eq!(m.primary_category, "cs.CL");
        assert_eq!(m.categories, vec!["cs.CL", "cs.LG"]);
        assert_eq!(m.doi, "10.0/xyz");
        assert_eq!(m.abs_url, "https://arxiv.org/abs/1706.03762");
    }
}
