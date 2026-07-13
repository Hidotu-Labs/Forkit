pub const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input",
    "link", "meta", "param", "source", "track", "wbr",
];

pub const SKIP_TAGS: &[&str] = &[
    "script", "noscript", "template", "math", "title",
];

pub const STYLE_HARVEST_TAGS: &[&str] = &["head"];

pub const RAW_TEXT_TAGS: &[&str] = &["pre", "textarea", "style"];

pub fn is_void(tag: &str)          -> bool { VOID_TAGS.contains(&tag) }
pub fn is_skip(tag: &str)          -> bool { SKIP_TAGS.contains(&tag) }
pub fn is_style_harvest(tag: &str) -> bool { STYLE_HARVEST_TAGS.contains(&tag) }
pub fn is_raw_text(tag: &str)      -> bool { RAW_TEXT_TAGS.contains(&tag) }
