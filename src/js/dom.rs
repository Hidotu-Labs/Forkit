/// Read-only DOM view exposed to the JS engine.
///
/// Wraps the parsed `Node` tree and provides the subset of DOM read APIs
/// that scripts commonly use:
///
///   document.getElementById("id")
///   document.getElementsByTagName("tag")
///   document.getElementsByClassName("cls")
///   document.querySelector("tag" | "#id" | ".class")   — simple selectors only
///   document.querySelectorAll(...)                       — returns array-like
///   document.title
///
/// On any matched element the following properties/methods are available:
///   .id  .className  .tagName  .textContent  .innerHTML  .innerText
///   .getAttribute("name")
///   .children  (array-like: .length, [0], [1], …)

use crate::dom::node::{Node, Element};
use crate::dom::parser::get_attr;

// ---------------------------------------------------------------------------
// A "live" element handle — a snapshot of one DOM element
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct JsElement {
    pub tag:        String,
    pub id:         String,
    pub class_name: String,
    pub attrs_raw:  String,
    /// Concatenated text content of this element and all its descendants.
    pub text_content: String,
    /// Serialised inner HTML of this element's children.
    pub inner_html:   String,
    /// Direct child element snapshots.
    pub children:     Vec<JsElement>,
}

impl JsElement {
    fn from_element(el: &Element) -> Self {
        JsElement {
            tag:          el.tag.clone(),
            id:           el.id.clone(),
            class_name:   el.class_name.clone(),
            attrs_raw:    el.attrs_raw.clone(),
            text_content: collect_text(&el.children),
            inner_html:   serialize_inner(&el.children),
            children:     el.children.iter().filter_map(|n| {
                if let Node::Element(c) = n { Some(JsElement::from_element(c)) } else { None }
            }).collect(),
        }
    }

    /// Read a named attribute (case-insensitive key).
    pub fn get_attribute(&self, name: &str) -> Option<String> {
        // id / class / style are stored as dedicated fields
        match name.to_ascii_lowercase().as_str() {
            "id"    => if self.id.is_empty() { None } else { Some(self.id.clone()) },
            "class" => if self.class_name.is_empty() { None } else { Some(self.class_name.clone()) },
            other   => get_attr(&self.attrs_raw, other).map(|s| s.to_owned()),
        }
    }
}

// ---------------------------------------------------------------------------
// DOM context passed into the interpreter
// ---------------------------------------------------------------------------
pub struct JsDom<'a> {
    root:  &'a Node,
    pub title: String,
}

impl<'a> JsDom<'a> {
    pub fn new(root: &'a Node) -> Self {
        JsDom { root, title: String::new() }
    }

    pub fn with_title(root: &'a Node, title: String) -> Self {
        JsDom { root, title }
    }

    /// document.getElementById(id)  — returns first match or None
    pub fn get_element_by_id(&self, id: &str) -> Option<JsElement> {
        find_first(self.root, &|el| el.id == id)
    }

    /// document.getElementsByTagName(tag)  — returns all matches
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<JsElement> {
        let t = tag.to_ascii_lowercase();
        find_all(self.root, &|el| {
            t == "*" || el.tag.to_ascii_lowercase() == t
        })
    }

    /// document.getElementsByClassName(cls)  — space-separated classes, all must match
    pub fn get_elements_by_class_name(&self, cls: &str) -> Vec<JsElement> {
        let required: Vec<&str> = cls.split_ascii_whitespace().collect();
        find_all(self.root, &|el| {
            let classes: Vec<&str> = el.class_name.split_ascii_whitespace().collect();
            required.iter().all(|r| classes.iter().any(|c| c.eq_ignore_ascii_case(r)))
        })
    }

    /// document.querySelector — supports: tag, #id, .class, tag.class, tag#id
    /// Returns the first element that matches.
    pub fn query_selector(&self, sel: &str) -> Option<JsElement> {
        let sel = sel.trim();
        let pred = build_predicate(sel);
        find_first(self.root, &pred)
    }

    /// document.querySelectorAll — same selector, all matches
    pub fn query_selector_all(&self, sel: &str) -> Vec<JsElement> {
        let sel = sel.trim();
        let pred = build_predicate(sel);
        find_all(self.root, &pred)
    }

    /// document.title — from the pre-extracted page title
    pub fn title(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        // fallback: walk the DOM (works if <title> isn't in SKIP_TAGS)
        find_first(self.root, &|el| el.tag == "title")
            .map(|e| e.text_content)
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Simple selector parser  (#id | .class | tag | tag#id | tag.class)
// ---------------------------------------------------------------------------
fn build_predicate(sel: &str) -> Box<dyn Fn(&Element) -> bool> {
    let part = sel.split_ascii_whitespace().next().unwrap_or("").to_owned();

    // #id
    if let Some(id) = part.strip_prefix('#') {
        let id = id.to_owned();
        return Box::new(move |el: &Element| el.id == id);
    }
    // .class
    if let Some(cls) = part.strip_prefix('.') {
        let cls = cls.to_ascii_lowercase();
        return Box::new(move |el: &Element| {
            el.class_name.split_ascii_whitespace()
              .any(|c| c.to_ascii_lowercase() == cls)
        });
    }
    // tag  /  tag#id  /  tag.class
    let (tag_part, qualifier) = if let Some(pos) = part.find(|c| c == '#' || c == '.') {
        (&part[..pos], Some(&part[pos..]))
    } else {
        (part.as_str(), None)
    };
    let tag_part  = tag_part.to_ascii_lowercase();
    let qualifier = qualifier.map(|s| s.to_owned());

    Box::new(move |el: &Element| {
        let tag_ok = tag_part.is_empty() || el.tag.to_ascii_lowercase() == tag_part;
        if !tag_ok { return false; }
        match &qualifier {
            None => true,
            Some(q) if q.starts_with('#') => el.id == &q[1..],
            Some(q) if q.starts_with('.') => {
                let cls = q[1..].to_ascii_lowercase();
                el.class_name.split_ascii_whitespace()
                  .any(|c| c.to_ascii_lowercase() == cls)
            }
            _ => true,
        }
    })
}

// ---------------------------------------------------------------------------
// Tree walkers
// ---------------------------------------------------------------------------
fn find_first(node: &Node, pred: &dyn Fn(&Element) -> bool) -> Option<JsElement> {
    if let Node::Element(el) = node {
        if pred(el) { return Some(JsElement::from_element(el)); }
        for child in &el.children {
            if let Some(found) = find_first(child, pred) { return Some(found); }
        }
    }
    None
}

fn find_all(node: &Node, pred: &dyn Fn(&Element) -> bool) -> Vec<JsElement> {
    let mut out = Vec::new();
    collect_all(node, pred, &mut out);
    out
}

fn collect_all(node: &Node, pred: &dyn Fn(&Element) -> bool, out: &mut Vec<JsElement>) {
    if let Node::Element(el) = node {
        if pred(el) { out.push(JsElement::from_element(el)); }
        for child in &el.children {
            collect_all(child, pred, out);
        }
    }
}

// ---------------------------------------------------------------------------
// Text / HTML serialisation helpers
// ---------------------------------------------------------------------------
fn collect_text(nodes: &[Node]) -> String {
    let mut s = String::new();
    for n in nodes {
        match n {
            Node::Text(t)    => s.push_str(&t.text),
            Node::Element(e) => s.push_str(&collect_text(&e.children)),
        }
    }
    s
}

fn serialize_inner(nodes: &[Node]) -> String {
    let mut s = String::new();
    for n in nodes {
        match n {
            Node::Text(t) => s.push_str(&t.text),
            Node::Element(e) => {
                s.push('<');
                s.push_str(&e.tag);
                if !e.id.is_empty() {
                    s.push_str(&format!(" id=\"{}\"", e.id));
                }
                if !e.class_name.is_empty() {
                    s.push_str(&format!(" class=\"{}\"", e.class_name));
                }
                if !e.attrs_raw.is_empty() {
                    s.push(' ');
                    s.push_str(&e.attrs_raw);
                }
                s.push('>');
                s.push_str(&serialize_inner(&e.children));
                s.push_str(&format!("</{}>", e.tag));
            }
        }
    }
    s
}
