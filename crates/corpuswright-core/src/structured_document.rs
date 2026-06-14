//! Page-aware document lines shared by extraction and scanning code.

/// Number of lines treated as the top or bottom edge of a page.
pub const PAGE_EDGE_LINE_COUNT: usize = 3;

/// A document represented as pages containing page-local lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuredDocument {
    pub pages: Vec<DocumentPage>,
}

/// One page in a structured document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentPage {
    pub page_index: usize,
    pub lines: Vec<DocumentLine>,
}

/// One line with page-local position metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentLine {
    pub text: String,
    pub page_index: usize,
    pub line_index_in_page: usize,
    pub line_count_in_page: usize,
    pub is_page_top: bool,
    pub is_page_bottom: bool,
}

impl StructuredDocument {
    /// Builds a structured document from page text, preserving page order.
    ///
    /// Lines are split with `str::lines`, matching the existing extraction
    /// paths that pass page-local text around as newline-separated strings.
    pub fn from_pages<I, S>(pages: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let pages = pages
            .into_iter()
            .enumerate()
            .map(|(page_index, page)| {
                let page = page.as_ref();
                let raw_lines = page.lines().collect::<Vec<_>>();
                let line_count_in_page = raw_lines.len();
                let lines = raw_lines
                    .into_iter()
                    .enumerate()
                    .map(|(line_index_in_page, text)| DocumentLine {
                        text: text.to_string(),
                        page_index,
                        line_index_in_page,
                        line_count_in_page,
                        is_page_top: is_page_top(line_index_in_page, line_count_in_page),
                        is_page_bottom: is_page_bottom(line_index_in_page, line_count_in_page),
                    })
                    .collect();

                DocumentPage { page_index, lines }
            })
            .collect();

        Self { pages }
    }

    /// Treats flat text as one page without guessing page breaks.
    pub fn from_flat_text_as_single_page(text: &str) -> Self {
        Self::from_pages(std::iter::once(text))
    }

    /// Flattens pages by joining page lines with `\n` and pages with `\n\n`.
    pub fn to_flat_text(&self) -> String {
        self.pages
            .iter()
            .map(DocumentPage::to_text)
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Iterates over all lines in page order.
    pub fn iter_lines(&self) -> impl Iterator<Item = &DocumentLine> {
        self.pages.iter().flat_map(|page| page.lines.iter())
    }
}

impl DocumentPage {
    /// Flattens this page by joining its lines with `\n`.
    pub fn to_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn is_page_top(line_index: usize, line_count: usize) -> bool {
    line_index < line_count && line_index < PAGE_EDGE_LINE_COUNT
}

fn is_page_bottom(line_index: usize, line_count: usize) -> bool {
    line_index < line_count && line_index >= line_count.saturating_sub(PAGE_EDGE_LINE_COUNT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_pages_sets_page_indexes() {
        let document = StructuredDocument::from_pages(["alpha\nbeta", "gamma"]);

        assert_eq!(document.pages.len(), 2);
        assert_eq!(document.pages[0].page_index, 0);
        assert_eq!(document.pages[1].page_index, 1);
        assert_eq!(document.pages[0].lines[0].page_index, 0);
        assert_eq!(document.pages[1].lines[0].page_index, 1);
    }

    #[test]
    fn from_pages_sets_line_indexes_and_counts() {
        let document = StructuredDocument::from_pages(["a\nb\nc", "d"]);

        let first_page = &document.pages[0];
        assert_eq!(first_page.lines[0].line_index_in_page, 0);
        assert_eq!(first_page.lines[1].line_index_in_page, 1);
        assert_eq!(first_page.lines[2].line_index_in_page, 2);
        assert!(
            first_page
                .lines
                .iter()
                .all(|line| line.line_count_in_page == 3)
        );

        let second_page = &document.pages[1];
        assert_eq!(second_page.lines[0].line_index_in_page, 0);
        assert_eq!(second_page.lines[0].line_count_in_page, 1);
    }

    #[test]
    fn page_edge_flags_use_first_and_last_three_lines() {
        let document = StructuredDocument::from_pages(["l1\nl2\nl3\nl4\nl5\nl6\nl7"]);
        let lines = &document.pages[0].lines;

        assert!(lines[0].is_page_top);
        assert!(lines[1].is_page_top);
        assert!(lines[2].is_page_top);
        assert!(!lines[3].is_page_top);
        assert!(!lines[3].is_page_bottom);
        assert!(lines[4].is_page_bottom);
        assert!(lines[5].is_page_bottom);
        assert!(lines[6].is_page_bottom);
    }

    #[test]
    fn short_pages_can_have_lines_that_are_both_top_and_bottom() {
        let document = StructuredDocument::from_pages(["one\ntwo"]);

        for line in &document.pages[0].lines {
            assert!(line.is_page_top);
            assert!(line.is_page_bottom);
        }
    }

    #[test]
    fn flat_text_creates_one_page() {
        let document = StructuredDocument::from_flat_text_as_single_page("a\n\nb");

        assert_eq!(document.pages.len(), 1);
        assert_eq!(document.pages[0].lines.len(), 3);
        assert_eq!(document.pages[0].lines[1].text, "");
    }

    #[test]
    fn to_flat_text_is_deterministic() {
        let document = StructuredDocument::from_pages(["a\nb", "c", "d\ne"]);

        assert_eq!(document.to_flat_text(), "a\nb\n\nc\n\nd\ne");
    }

    #[test]
    fn empty_pages_and_empty_text_are_handled() {
        let document = StructuredDocument::from_pages(["", "body"]);

        assert_eq!(document.pages.len(), 2);
        assert!(document.pages[0].lines.is_empty());
        assert_eq!(document.pages[1].lines[0].text, "body");
        assert_eq!(document.to_flat_text(), "\n\nbody");

        let empty_text = StructuredDocument::from_flat_text_as_single_page("");
        assert_eq!(empty_text.pages.len(), 1);
        assert!(empty_text.pages[0].lines.is_empty());
        assert_eq!(empty_text.to_flat_text(), "");
    }

    #[test]
    fn iter_lines_returns_lines_in_page_order() {
        let document = StructuredDocument::from_pages(["a\nb", "c"]);
        let texts = document
            .iter_lines()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>();

        assert_eq!(texts, vec!["a", "b", "c"]);
    }
}
