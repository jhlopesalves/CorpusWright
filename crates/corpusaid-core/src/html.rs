use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref NOSCRIPT_RE: Regex = Regex::new(r"(?is)<noscript[^>]*>.*?</noscript>").unwrap();
}

/// Extracts readable plain text from HTML content.
///
/// This handles:
/// - Stripping HTML tags.
/// - Decoding HTML entities.
/// - Ignoring `<script>`, `<style>`, and `<noscript>` elements.
/// - Formatting block elements with sensible spacing.
pub fn extract_html(html: &str) -> String {
    // html2text automatically ignores <script> and <style> but leaves <noscript>.
    // We strip <noscript> manually before extraction to strictly comply with requirements.
    let cleaned_html = NOSCRIPT_RE.replace_all(html, "");

    // Extract text wrapping at 100_000 to avoid hard-wrapping lines in normal paragraphs.
    // The library uses `html5ever` internally which gracefully handles malformed HTML.
    let extracted = html2text::config::with_decorator(html2text::render::TrivialDecorator::new())
        .string_from_read(cleaned_html.as_bytes(), 100_000);

    // Some versions of html2text can return Result depending on features, but
    // `html2text::from_read` in 0.13 returns a Result string, or just unwraps.
    // From our test, we know it returns a Result that we can unwrap, but let's be safe.
    match extracted {
        Ok(text) => text.trim().to_string(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tag_stripping() {
        let html = "<html><body><h1>Title</h1><p>Hello <strong>world</strong>.</p></body></html>";
        let text = extract_html(html);
        assert!(text.contains("Title"));
        assert!(text.contains("Hello world."));
        assert!(!text.contains("<h1>"));
    }

    #[test]
    fn test_entity_decoding() {
        let html = "<p>Tom &amp; Jerry &lt;classic&gt; &#169;</p>";
        let text = extract_html(html);
        assert!(text.contains("Tom & Jerry <classic> ©"));
    }

    #[test]
    fn test_script_style_noscript_removal() {
        let html = r#"
            <style>.hidden { display: none; }</style>
            <script>alert("bad");</script>
            <noscript>Enable JavaScript</noscript>
            <p>Visible text.</p>
        "#;
        let text = extract_html(html);
        assert!(text.contains("Visible text."));
        assert!(!text.contains("display"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("Enable JavaScript"));
    }

    #[test]
    fn test_malformed_html() {
        let html = "<html><body><p>Unclosed paragraph<div><b>bold</i>";
        let text = extract_html(html);
        assert!(text.contains("Unclosed paragraph"));
        assert!(text.contains("bold"));
    }
}
