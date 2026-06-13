use crate::dom::node::{Element, Display};

pub const BLOCK_TAGS: &[&str] = &[
    "div", "p", "h1", "h2", "h3", "h4", "h5", "h6",
    "ul", "ol", "li", "dl", "dt", "dd",
    "nav", "header", "footer", "main",
    "section", "article", "aside", "figure", "figcaption",
    "body", "html", "blockquote", "pre", "details", "summary",
    "table", "thead", "tbody", "tfoot", "tr", "th", "td", "caption",
    "form", "fieldset", "legend",
    "address", "dialog",
    "video", "audio", "canvas",
];

pub fn apply_display(el: &mut Element) {
    let t = el.tag.as_str();
    let s = &mut el.style;

    if BLOCK_TAGS.contains(&t) {
        s.display_block = true;
        s.display       = Display::Block;
    } else if t != "#document" {
        s.display_block = false;
        if s.display == Display::Block { s.display = Display::Inline; }
    }
}
