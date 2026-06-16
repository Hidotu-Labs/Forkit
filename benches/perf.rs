/// Benchmarks for Forkit's parser, CSS cascade, and layout measurement.
///
/// Run with:  cargo bench
/// Or for a quick text report without HTML:
///           cargo bench -- --output-format bencher
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

use forkit::dom::css::StyleSheet;
use forkit::dom::css::cascade::apply_cascade;
use forkit::dom::parser::parse_with_sheets;

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

/// A small representative HTML page (~50 elements, minimal CSS).
const SMALL_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
  <style>
    body { font-family: sans-serif; margin: 0; padding: 16px; color: #222; }
    h1 { font-size: 2em; margin-bottom: 0.5em; }
    p  { line-height: 1.6; margin-bottom: 1em; }
    a  { color: #0066cc; text-decoration: underline; }
    .highlight { background-color: #fffde7; padding: 4px 8px; border-radius: 4px; }
    ul li { margin-bottom: 0.25em; }
  </style>
</head>
<body>
  <h1>Welcome to Forkit</h1>
  <p>This is a <a href="https://example.com">small test page</a> used for benchmarking.</p>
  <p class="highlight">Highlighted paragraph with extra styling applied.</p>
  <ul>
    <li>Item one</li><li>Item two</li><li>Item three</li>
    <li>Item four</li><li>Item five</li>
  </ul>
  <p>Another paragraph with <strong>bold</strong> and <em>italic</em> text.</p>
</body>
</html>"#;

/// A large HTML page (~500 elements, many CSS rules) that stresses the cascade.
fn large_html() -> String {
    let mut out = String::with_capacity(64 * 1024);
    out.push_str("<!DOCTYPE html><html><head><style>\n");
    // Generate 60 CSS rules with a mix of tag, class, and descendant selectors.
    for i in 0..20usize {
        out.push_str(&format!(
            ".row-{i} {{ background: rgb({r1},{g1},{b1}); padding: 4px; }}\n",
            r1 = (i * 13) % 256, g1 = (i * 37) % 256, b1 = (i * 71) % 256,
        ));
        out.push_str(&format!(
            ".row-{i} p {{ color: rgb({r2},{g2},{b2}); font-size: {fs}px; }}\n",
            r2 = (i * 19) % 256, g2 = (i * 43) % 256, b2 = (i * 67) % 256,
            fs = 12 + (i % 6),
        ));
        out.push_str(&format!(
            ".row-{i} span.label {{ font-weight: bold; margin-right: 8px; }}\n"
        ));
    }
    out.push_str("</style></head><body>\n");
    // Generate 100 block sections, each with a heading + 3 paragraphs.
    for i in 0..100usize {
        let cls = i % 20;
        out.push_str(&format!("<div class=\"row-{cls}\">\n"));
        out.push_str(&format!("  <h2>Section {i}</h2>\n"));
        for j in 0..3usize {
            out.push_str(&format!(
                "  <p>Paragraph {j} in section {i}. It contains some \
                 <span class=\"label\">Label {j}</span> and a fair amount of \
                 text to exercise word-wrapping during layout measurement. \
                 Extra filler words to make the line longer than a typical \
                 viewport width so wrapping actually occurs.</p>\n"
            ));
        }
        out.push_str("</div>\n");
    }
    out.push_str("</body></html>");
    out
}

/// A CSS-heavy string (~200 rules) exercising the CSS parser.
fn large_css() -> String {
    let mut out = String::with_capacity(32 * 1024);
    for i in 0..100usize {
        out.push_str(&format!(
            ".cls-{i} {{ color: rgb({r},{g},{b}); font-size: {fs}px; \
             padding: {p}px; margin: {m}px; }}\n",
            r = (i * 13) % 256, g = (i * 29) % 256, b = (i * 53) % 256,
            fs = 12 + (i % 8), p = 4 + (i % 16), m = 2 + (i % 8),
        ));
        out.push_str(&format!(
            "div.wrapper > .cls-{i}:hover {{ background-color: rgba({r},{g},{b},0.5); }}\n",
            r = (i * 7) % 256, g = (i * 41) % 256, b = (i * 61) % 256,
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Benchmark groups
// ---------------------------------------------------------------------------

fn bench_css_parse(c: &mut Criterion) {
    let small_css = r#"
        body { font-size: 16px; color: #333; background: #fff; }
        h1, h2, h3 { font-weight: bold; }
        a:hover { color: red; text-decoration: none; }
        .container { max-width: 1200px; margin: 0 auto; padding: 0 16px; }
        @media screen { p { line-height: 1.5; } }
    "#;
    let big_css = large_css();

    let mut group = c.benchmark_group("css_parse");
    group.bench_function("small_5_rules", |b| {
        b.iter(|| StyleSheet::parse(black_box(small_css)))
    });
    group.bench_function("large_200_rules", |b| {
        b.iter(|| StyleSheet::parse(black_box(big_css.as_str())))
    });
    group.finish();
}

fn bench_html_parse(c: &mut Criterion) {
    let big_html = large_html();

    let mut group = c.benchmark_group("html_parse");
    group.bench_function("small_50_elements", |b| {
        b.iter(|| parse_with_sheets(black_box(SMALL_HTML), black_box("file://test.html")))
    });
    group.bench_function("large_500_elements", |b| {
        b.iter(|| {
            parse_with_sheets(black_box(big_html.as_str()), black_box("file://test.html"))
        })
    });
    group.finish();
}

fn bench_cascade(c: &mut Criterion) {
    let big_html = large_html();

    let mut group = c.benchmark_group("css_cascade");

    group.bench_function("small_page", |b| {
        let (mut dom, sheets) = parse_with_sheets(SMALL_HTML, "file://test.html");
        b.iter(|| {
            apply_cascade(black_box(&mut dom), black_box(&sheets));
        });
    });

    group.bench_function("large_page_500_elements", |b| {
        let (mut dom, sheets) = parse_with_sheets(big_html.as_str(), "file://test.html");
        b.iter(|| {
            apply_cascade(black_box(&mut dom), black_box(&sheets));
        });
    });

    group.finish();
}

fn bench_full_parse_and_cascade(c: &mut Criterion) {
    let big_html = large_html();

    let mut group = c.benchmark_group("full_parse_cascade");

    group.bench_with_input(
        BenchmarkId::new("small", "50_elements"),
        &SMALL_HTML,
        |b, html| {
            b.iter(|| {
                let (mut dom, sheets) =
                    parse_with_sheets(black_box(html), "file://test.html");
                apply_cascade(&mut dom, &sheets);
                black_box(dom)
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("large", "500_elements"),
        big_html.as_str(),
        |b, html| {
            b.iter(|| {
                let (mut dom, sheets) =
                    parse_with_sheets(black_box(html), "file://test.html");
                apply_cascade(&mut dom, &sheets);
                black_box(dom)
            });
        },
    );

    group.finish();
}

fn bench_selector_matching(c: &mut Criterion) {
    use forkit::dom::node::{Element, Style, Node};
    use forkit::dom::css::stylesheet::StyleSheet as SS;

    // Build a realistic tree: 5 levels × 5 children ≈ 3905 nodes.
    fn make_tree(tag: &str, id: &str, class: &str, depth: usize) -> Node {
        let mut el = Element {
            tag:        tag.to_string(),
            id:         id.to_string(),
            class_name: class.to_string(),
            style_attr: String::new(),
            attrs_raw:  String::new(),
            style:      Style::default(),
            children:   vec![],
        };
        if depth > 0 {
            for i in 0..5usize {
                el.children.push(make_tree(
                    if i % 2 == 0 { "div" } else { "span" },
                    "",
                    &format!("child-{}", i % 3),
                    depth - 1,
                ));
            }
        }
        Node::Element(el)
    }

    let mut root = make_tree("div", "root", "container", 5);

    // CSS with descendant, class, and compound selectors — exercises the
    // O(N×M) cascade loop with specificity sorting.
    let sheet = SS::parse(
        "div { color: #333; } \
         .container .child-0 { color: red; } \
         div > span.child-1 { font-weight: bold; } \
         #root .child-2 { background: blue; } \
         span:first-child { text-decoration: underline; }",
    );

    c.bench_function("cascade_deep_tree_3905_nodes", |b| {
        b.iter(|| apply_cascade(black_box(&mut root), black_box(&[sheet.clone()])));
    });
}

criterion_group!(
    benches,
    bench_css_parse,
    bench_html_parse,
    bench_cascade,
    bench_full_parse_and_cascade,
    bench_selector_matching,
);
criterion_main!(benches);
