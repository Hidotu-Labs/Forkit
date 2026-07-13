/// Tests driven by `assets/html-test.html`.
///
/// Each test parses a targeted excerpt of the HTML that exercises one
/// feature shown in the test file and then asserts on the resulting DOM
/// structure.  We keep external dependencies to zero: only the parser and
/// node types are involved.

use crate::html5::parser::{parse_dom, get_attr};
use crate::html5::node::Node;

// ── helpers ─────────────────────────────────────────────────────────────────

/// Walk the DOM depth-first and collect all Element nodes whose tag matches.
fn find_all<'a>(root: &'a Node, tag: &str) -> Vec<&'a crate::html5::node::Element> {
    let mut out = Vec::new();
    collect(root, tag, &mut out);
    out
}

fn collect<'a>(node: &'a Node, tag: &str, out: &mut Vec<&'a crate::html5::node::Element>) {
    if let Node::Element(el) = node {
        if el.tag.eq_ignore_ascii_case(tag) {
            out.push(el);
        }
        for child in &el.children {
            collect(child, tag, out);
        }
    }
}

/// Return the first element with the given tag.
fn first<'a>(root: &'a Node, tag: &str) -> Option<&'a crate::html5::node::Element> {
    find_all(root, tag).into_iter().next()
}

/// Collect all text content from a node (depth-first).
fn text_content(node: &Node) -> String {
    match node {
        Node::Text(t) => t.text.clone(),
        Node::Element(el) => el.children.iter().map(text_content).collect::<String>(),
    }
}

/// Collect text content from an Element directly.
fn el_text(el: &crate::html5::node::Element) -> String {
    el.children.iter().map(text_content).collect()
}

/// Load the real `assets/html-test.html` file relative to the workspace root.
fn load_html() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/html-test.html");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read html-test.html: {e}"))
}

// ── tests ────────────────────────────────────────────────────────────────────

/// The parser produces a well-formed tree: we get exactly one <html>, one
/// <head>, and one <body>.
#[test]
fn test_document_structure() {
    let dom = parse_dom(&load_html());
    assert_eq!(find_all(&dom, "html").len(), 1, "expected one <html> element");
    assert_eq!(find_all(&dom, "head").len(), 1, "expected one <head> element");
    assert_eq!(find_all(&dom, "body").len(), 1, "expected one <body> element");
}

/// <title> content is parsed correctly.
#[test]
fn test_title_text() {
    let dom = parse_dom(&load_html());
    let titles = find_all(&dom, "title");
    assert!(!titles.is_empty(), "<title> not found");
    let title_text = el_text(titles[0]);
    assert!(
        title_text.contains("Forkit"),
        "title should contain 'Forkit', got: {title_text:?}"
    );
}

/// <meta charset> attribute is accessible.
#[test]
fn test_meta_charset_attr() {
    let dom = parse_dom(&load_html());
    let metas = find_all(&dom, "meta");
    let charset_meta = metas.iter().find(|m| {
        get_attr(&m.attrs_raw, "charset").is_some()
    });
    assert!(charset_meta.is_some(), "<meta charset> not found");
    assert_eq!(
        get_attr(&charset_meta.unwrap().attrs_raw, "charset"),
        Some("UTF-8"),
        "charset should be UTF-8"
    );
}

/// <meta name=\"viewport\"> is parsed and its content attribute is present.
#[test]
fn test_meta_viewport_attr() {
    let dom = parse_dom(&load_html());
    let metas = find_all(&dom, "meta");
    let vp = metas.iter().find(|m| {
        get_attr(&m.attrs_raw, "name") == Some("viewport")
    });
    assert!(vp.is_some(), "<meta name=viewport> not found");
    let content = get_attr(&vp.unwrap().attrs_raw, "content").unwrap_or("");
    assert!(
        content.contains("width=device-width"),
        "viewport content should contain width=device-width, got: {content:?}"
    );
}

/// The <h1> element is present and contains the right text.
#[test]
fn test_h1_text() {
    let dom = parse_dom(&load_html());
    let h1s = find_all(&dom, "h1");
    assert!(!h1s.is_empty(), "<h1> not found");
    let text = el_text(h1s[0]);
    assert!(
        text.contains("Forkit"),
        "h1 should contain 'Forkit', got: {text:?}"
    );
}

/// All five <h2> section headings are parsed.
#[test]
fn test_h2_count() {
    let dom = parse_dom(&load_html());
    let h2s = find_all(&dom, "h2");
    assert!(
        h2s.len() >= 5,
        "expected at least 5 <h2> elements, found {}",
        h2s.len()
    );
}

/// The feature-matrix <table> is parsed with <thead>, <tbody>, <tr>, <th>,
/// and <td> elements.
#[test]
fn test_table_structure() {
    let dom = parse_dom(&load_html());
    assert!(!find_all(&dom, "table").is_empty(),  "<table> not found");
    assert!(!find_all(&dom, "thead").is_empty(),  "<thead> not found");
    assert!(!find_all(&dom, "tbody").is_empty(),  "<tbody> not found");
    let rows = find_all(&dom, "tr");
    assert!(rows.len() >= 2, "expected at least 2 <tr> elements");
    assert!(!find_all(&dom, "th").is_empty(), "<th> not found");
    assert!(!find_all(&dom, "td").is_empty(), "<td> not found");
}

/// The table contains a row for "Buttons" (verifying inner text of <td>s).
#[test]
fn test_table_contains_buttons_row() {
    let dom = parse_dom(&load_html());
    let tds = find_all(&dom, "td");
    let found = tds.iter().any(|td| el_text(td).contains("Buttons"));
    assert!(found, "expected a <td> containing 'Buttons' in the feature matrix");
}

/// Inline elements <b>, <strong>, <i>, <em> are all parsed.
#[test]
fn test_inline_text_elements() {
    let dom = parse_dom(&load_html());
    assert!(!find_all(&dom, "b").is_empty(),      "<b> not found");
    assert!(!find_all(&dom, "strong").is_empty(), "<strong> not found");
    assert!(!find_all(&dom, "i").is_empty(),      "<i> not found");
    assert!(!find_all(&dom, "em").is_empty(),     "<em> not found");
}

/// The hyperlink is parsed with the correct href.
#[test]
fn test_anchor_href() {
    let dom = parse_dom(&load_html());
    let anchors = find_all(&dom, "a");
    assert!(!anchors.is_empty(), "<a> not found");
    assert_eq!(
        anchors[0].href,
        "https://example.com",
        "anchor href mismatch"
    );
}

/// <span> elements with inline styles are parsed and their style attribute
/// is accessible.
#[test]
fn test_span_inline_style() {
    let dom = parse_dom(&load_html());
    let spans = find_all(&dom, "span");
    let uppercase_span = spans.iter().find(|s| {
        get_attr(&s.attrs_raw, "style")
            .map(|v| v.contains("uppercase"))
            .unwrap_or(false)
    });
    assert!(
        uppercase_span.is_some(),
        "expected a <span> with text-transform: uppercase"
    );
}

/// <br> void elements are parsed (they appear as self-closing elements in
/// the DOM with no children).
#[test]
fn test_br_elements() {
    let dom = parse_dom(&load_html());
    let brs = find_all(&dom, "br");
    assert!(!brs.is_empty(), "<br> elements not found");
    for br in &brs {
        assert!(br.children.is_empty(), "<br> should have no children");
    }
}

/// The div with `style=\"width: 200px; margin: 10px auto\"` is parsed and its
/// inline style is accessible.
#[test]
fn test_div_margin_auto_style() {
    let dom = parse_dom(&load_html());
    let divs = find_all(&dom, "div");
    let centered = divs.iter().find(|d| {
        get_attr(&d.attrs_raw, "style")
            .map(|s| s.contains("200px") && s.contains("auto"))
            .unwrap_or(false)
    });
    assert!(
        centered.is_some(),
        "expected a <div> with width:200px and margin:auto"
    );
}

/// inline-block divs are present (three of them in the layout section).
#[test]
fn test_inline_block_divs() {
    let dom = parse_dom(&load_html());
    let divs = find_all(&dom, "div");
    let ib_divs: Vec<_> = divs.iter().filter(|d| {
        get_attr(&d.attrs_raw, "style")
            .map(|s| s.contains("inline-block"))
            .unwrap_or(false)
    }).collect();
    assert!(
        ib_divs.len() >= 3,
        "expected at least 3 inline-block divs, found {}",
        ib_divs.len()
    );
}

/// Semantic block elements are parsed.
#[test]
fn test_semantic_elements() {
    let dom = parse_dom(&load_html());
    for tag in &["header", "nav", "main", "footer"] {
        assert!(
            !find_all(&dom, tag).is_empty(),
            "<{tag}> not found"
        );
    }
}

/// <button> elements are parsed; the file has at least 5.
#[test]
fn test_button_count() {
    let dom = parse_dom(&load_html());
    let buttons = find_all(&dom, "button");
    assert!(
        buttons.len() >= 5,
        "expected at least 5 <button> elements, found {}",
        buttons.len()
    );
}

/// The reset and submit buttons have the correct `type` attributes.
#[test]
fn test_button_types() {
    let dom = parse_dom(&load_html());
    let buttons = find_all(&dom, "button");
    let has_reset = buttons.iter().any(|b| {
        get_attr(&b.attrs_raw, "type") == Some("reset")
    });
    let has_submit = buttons.iter().any(|b| {
        get_attr(&b.attrs_raw, "type") == Some("submit")
    });
    assert!(has_reset,  "no <button type=reset> found");
    assert!(has_submit, "no <button type=submit> found");
}

/// CSS class names are stored on elements.
#[test]
fn test_class_names() {
    let dom = parse_dom(&load_html());
    // The subtitle paragraph has class="subtitle"
    let paras = find_all(&dom, "p");
    let subtitle = paras.iter().find(|p| p.class_name == "subtitle");
    assert!(subtitle.is_some(), "no <p class=subtitle> found");

    // Several divs should have class="card"
    let divs = find_all(&dom, "div");
    let cards: Vec<_> = divs.iter().filter(|d| d.class_name == "card").collect();
    assert!(
        cards.len() >= 4,
        "expected at least 4 .card divs, found {}",
        cards.len()
    );
}

/// Elements with an `id` attribute expose it correctly.  The HTML doesn't
/// assign any `id` attributes, so this tests the inverse: no spurious IDs.
#[test]
fn test_no_spurious_ids() {
    let dom = parse_dom(&load_html());
    let all_elements: Vec<&crate::html5::node::Element> = {
        fn collect_all<'a>(node: &'a Node, out: &mut Vec<&'a crate::html5::node::Element>) {
            if let Node::Element(el) = node {
                out.push(el);
                for child in &el.children {
                    collect_all(child, out);
                }
            }
        }
        let mut v = Vec::new();
        collect_all(&dom, &mut v);
        v
    };
    for el in all_elements {
        assert!(
            el.id.is_empty(),
            "<{}> should have no id, but got {:?}",
            el.tag,
            el.id
        );
    }
}

/// The <style> block in <head> is parsed as a style element whose single
/// text child contains the CSS rules.
#[test]
fn test_style_block_parsed() {
    let dom = parse_dom(&load_html());
    let styles = find_all(&dom, "style");
    assert!(!styles.is_empty(), "<style> not found");
    let css_text = el_text(styles[0]);
    assert!(
        css_text.contains("background-color"),
        "style block should contain CSS rules with background-color"
    );
    assert!(
        css_text.contains("border-radius"),
        "style block should contain border-radius rules"
    );
}

/// The feature-matrix table's header row has exactly two columns ("Feature"
/// and "Status").
#[test]
fn test_table_header_columns() {
    let dom = parse_dom(&load_html());
    let ths = find_all(&dom, "th");
    assert!(ths.len() >= 2, "expected at least 2 <th> elements");
    let texts: Vec<String> = ths.iter()
        .map(|th| el_text(th).trim().to_string())
        .collect();
    assert!(texts.contains(&"Feature".to_string()), "missing 'Feature' header");
    assert!(texts.contains(&"Status".to_string()),  "missing 'Status' header");
}

/// <td> cells with `class=\"done\"` are present for every supported feature row.
#[test]
fn test_done_class_cells() {
    let dom = parse_dom(&load_html());
    let tds = find_all(&dom, "td");
    let done_cells: Vec<_> = tds.iter().filter(|td| td.class_name == "done").collect();
    // There are 13 feature rows in the HTML
    assert!(
        done_cells.len() >= 13,
        "expected at least 13 .done cells, found {}",
        done_cells.len()
    );
}

/// get_attr handles both quoted and unquoted attribute values correctly.
#[test]
fn test_get_attr_variants() {
    assert_eq!(get_attr(r#"href="https://example.com""#, "href"), Some("https://example.com"));
    assert_eq!(get_attr(r#"href='https://example.com'"#, "href"), Some("https://example.com"));
    assert_eq!(get_attr(r#"type=submit"#, "type"), Some("submit"));
    assert_eq!(get_attr(r#"disabled"#, "disabled"), Some(""));
    assert_eq!(get_attr(r#"href="x""#, "src"), None);
}
