/// DOM view exposed to the JS engine — supports both read and write operations.
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
///   document.createElement("tag")
///
/// On any matched element the following read properties/methods are available:
///   .id  .className  .tagName  .textContent  .innerHTML  .innerText
///   .getAttribute("name")
///   .children  (array-like: .length, [0], [1], …)
///
/// Write support (mutations collected and applied after JS runs):
///   el.textContent = "..."
///   el.innerHTML   = "..."
///   el.setAttribute("name", "value")
///   el.removeAttribute("name")
///   el.style.color = "red"  (and other CSS properties)
///   el.className   = "..."
///   el.id          = "..."
///   el.appendChild(child)
///   el.remove()

use std::sync::Mutex;
use crate::html5::node::{Node, Element, TextNode};
use crate::html5::parser::get_attr;

// ---------------------------------------------------------------------------
// Pending DOM write operations
// ---------------------------------------------------------------------------

/// A pending mutation produced during JS execution.
/// `path` is the sequence of child-indices from the document root to the target element.
#[derive(Debug, Clone)]
pub enum DomMutation {
    /// Set `textContent` — replaces all children with a single text node.
    SetTextContent { path: Vec<usize>, value: String },
    /// Set `innerHTML` — replaces all children with parsed HTML fragment nodes.
    SetInnerHtml    { path: Vec<usize>, value: String },
    /// Set or update a named attribute.
    SetAttribute    { path: Vec<usize>, name: String, value: String },
    /// Remove a named attribute.
    RemoveAttribute { path: Vec<usize>, name: String },
    /// Set `el.className`.
    SetClassName    { path: Vec<usize>, value: String },
    /// Set `el.id`.
    SetId           { path: Vec<usize>, value: String },
    /// Append a newly-created element as the last child.
    AppendChild     { path: Vec<usize>, child_tag: String, child_text: String },
    /// Remove the element from its parent.
    Remove          { path: Vec<usize> },
    /// Add an event listener.
    AddEventListener { path: Vec<usize>, event_type: String, callback: crate::js::types::JsFunction },
    /// Insert HTML fragment relative to the element (beforebegin, afterbegin, beforeend, afterend).
    InsertAdjacentHtml { path: Vec<usize>, position: String, html: String },
    /// Prepend a child node (identified by its path) to this element.
    PrependChild { parent_path: Vec<usize>, child_path: Vec<usize> },
    /// Set a timer to execute a callback after `delay_ms`.
    SetTimeout { callback: crate::js::types::JsFunction, delay_ms: u32 },
}

// ---------------------------------------------------------------------------
// A "live" element handle — a snapshot of one DOM element, extended with a
// node path so that write operations can locate it back in the tree.
// ---------------------------------------------------------------------------
#[derive(Debug, Clone)]
pub struct JsElement {
    pub tag:        String,
    pub id:         String,
    pub class_name: String,
    pub href:       String,
    pub attrs_raw:  String,
    /// Concatenated text content of this element and all its descendants.
    pub text_content: String,
    /// Serialised inner HTML of this element's children.
    pub inner_html:   String,
    /// Direct child element snapshots.
    pub children:     Vec<JsElement>,
    /// Path of child-indices from the document root to this element.
    /// Used to apply write mutations back to the live tree.
    pub path: Vec<usize>,
}

impl JsElement {
    pub(crate) fn from_element(el: &Element, path: Vec<usize>) -> Self {
        let children: Vec<JsElement> = el.children.iter().enumerate().filter_map(|(i, n)| {
            if let Node::Element(c) = n {
                let mut child_path = path.clone();
                child_path.push(i);
                Some(JsElement::from_element(c, child_path))
            } else {
                None
            }
        }).collect();
        JsElement {
            tag:          el.tag.clone(),
            id:           el.id.clone(),
            class_name:   el.class_name.clone(),
            href:         el.href.clone(),
            attrs_raw:    el.attrs_raw.clone(),
            text_content: collect_text(&el.children),
            inner_html:   serialize_inner(&el.children),
            children,
            path,
        }
    }

    /// Read a named attribute (case-insensitive key).
    pub fn get_attribute(&self, name: &str) -> Option<String> {
        // id / class / href / style are stored as dedicated fields
        match name.to_ascii_lowercase().as_str() {
            "id"    => if self.id.is_empty() { None } else { Some(self.id.clone()) },
            "class" => if self.class_name.is_empty() { None } else { Some(self.class_name.clone()) },
            "href"  => if self.href.is_empty() { None } else { Some(self.href.clone()) },
            other   => get_attr(&self.attrs_raw, other).map(|s| s.to_owned()),
        }
    }
}

// ---------------------------------------------------------------------------
// DOM context passed into the interpreter
// ---------------------------------------------------------------------------
pub struct JsDom<'a> {
    root:       &'a Node,
    pub title:  String,
    /// Pending mutations collected during JS execution.
    /// Applied to the live DOM after all scripts finish via `take_mutations()`.
    pub mutations: Mutex<Vec<DomMutation>>,
}

impl<'a> JsDom<'a> {
    pub fn new(root: &'a Node) -> Self {
        JsDom { root, title: String::new(), mutations: Mutex::new(Vec::new()) }
    }

    pub fn with_title(root: &'a Node, title: String) -> Self {
        JsDom { root, title, mutations: Mutex::new(Vec::new()) }
    }

    pub fn root(&self) -> &'a Node {
        self.root
    }

    /// Consume and return all pending mutations.
    pub fn take_mutations(&self) -> Vec<DomMutation> {
        self.mutations.lock().unwrap().drain(..).collect()
    }

    /// Queue a write mutation.
    pub fn push_mutation(&self, m: DomMutation) {
        self.mutations.lock().unwrap().push(m);
    }

    /// document.getElementById(id)  — returns first match or None
    pub fn get_element_by_id(&self, id: &str) -> Option<JsElement> {
        find_first(self.root, &|el| el.id == id, &[])
    }

    /// document.getElementsByTagName(tag)  — returns all matches
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<JsElement> {
        let t = tag.to_ascii_lowercase();
        find_all(self.root, &|el| {
            t == "*" || el.tag.to_ascii_lowercase() == t
        }, &[])
    }

    /// document.getElementsByClassName(cls)  — space-separated classes, all must match
    pub fn get_elements_by_class_name(&self, cls: &str) -> Vec<JsElement> {
        let required: Vec<&str> = cls.split_ascii_whitespace().collect();
        find_all(self.root, &|el| {
            let classes: Vec<&str> = el.class_name.split_ascii_whitespace().collect();
            required.iter().all(|r| classes.iter().any(|c| c.eq_ignore_ascii_case(r)))
        }, &[])
    }

    /// document.querySelector — supports: tag, #id, .class, tag.class, tag#id
    /// Returns the first element that matches.
    pub fn query_selector(&self, sel: &str) -> Option<JsElement> {
        let sel = sel.trim();
        let pred = build_predicate(sel);
        find_first(self.root, &pred, &[])
    }

    /// document.querySelectorAll — same selector, all matches
    pub fn query_selector_all(&self, sel: &str) -> Vec<JsElement> {
        let sel = sel.trim();
        let pred = build_predicate(sel);
        find_all(self.root, &pred, &[])
    }

    /// element.querySelectorAll — search inside an element by path
    pub fn query_selector_all_on(&self, path: &[usize], sel: &str) -> Vec<JsElement> {
        let sel = sel.trim();
        let pred = build_predicate(sel);
        fn navigate_ref<'a>(node: &'a Node, path: &[usize]) -> Option<&'a Node> {
            if path.is_empty() { return Some(node); }
            if let Node::Element(el) = node {
                let idx = path[0];
                if idx < el.children.len() {
                    return navigate_ref(&el.children[idx], &path[1..]);
                }
            }
            None
        }
        if let Some(target) = navigate_ref(self.root, path) {
            find_all(target, &pred, path)
        } else {
            Vec::new()
        }
    }

    /// document.createElement("tag") — creates a detached element snapshot
    /// that can be appended via el.appendChild(child).
    pub fn create_element(&self, tag: &str) -> JsElement {
        JsElement {
            tag:          tag.to_ascii_lowercase(),
            id:           String::new(),
            class_name:   String::new(),
            href:         String::new(),
            attrs_raw:    String::new(),
            text_content: String::new(),
            inner_html:   String::new(),
            children:     Vec::new(),
            // Detached — path is empty; will be ignored in encoding
            path:         vec![],
        }
    }

    /// document.addEventListener(type, callback)
    pub fn add_event_listener(&self, path: Vec<usize>, event_type: &str, callback: crate::js::types::JsFunction) {
        self.push_mutation(DomMutation::AddEventListener {
            path,
            event_type: event_type.to_owned(),
            callback,
        });
    }

    pub fn insert_adjacent_html(&self, path: Vec<usize>, position: &str, html: &str) {
        self.push_mutation(DomMutation::InsertAdjacentHtml {
            path,
            position: position.to_owned(),
            html: html.to_owned(),
        });
    }

    pub fn prepend_child(&self, parent_path: Vec<usize>, child_path: Vec<usize>) {
        self.push_mutation(DomMutation::PrependChild {
            parent_path,
            child_path,
        });
    }

    pub fn set_timeout(&self, callback: crate::js::types::JsFunction, delay_ms: u32) {
        self.push_mutation(DomMutation::SetTimeout {
            callback,
            delay_ms,
        });
    }

    /// document.title — from the pre-extracted page title
    pub fn title(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        // fallback: walk the DOM (works if <title> isn't in SKIP_TAGS)
        find_first(self.root, &|el| el.tag == "title", &[])
            .map(|e| e.text_content)
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Mutation application — walks the mutable DOM tree by path and applies writes
// ---------------------------------------------------------------------------

/// Apply a list of `DomMutation`s to the live DOM tree.
/// Call this after all JS scripts have finished executing.
pub fn apply_mutations(root: &mut Node, mutations: Vec<DomMutation>) {
    for mutation in mutations {
        apply_one(root, mutation);
    }
}

pub fn apply_one(root: &mut Node, mutation: DomMutation) {
    match mutation {
        DomMutation::SetTextContent { path, value } => {
            if path.is_empty() { return; } // detached element — no-op
            if let Some(el) = navigate_mut(root, &path) {
                el.children.clear();
                if !value.is_empty() {
                    el.children.push(Node::Text(TextNode {
                        text:  value,
                    }));
                }
            }
        }
        DomMutation::SetInnerHtml { path, value } => {
            if path.is_empty() { return; }
            if let Some(el) = navigate_mut(root, &path) {
                // Parse a simple HTML fragment and replace children.
                el.children = parse_html_fragment(&value);
            }
        }
        DomMutation::SetAttribute { path, name, value } => {
            if path.is_empty() { return; }
            if let Some(el) = navigate_mut(root, &path) {
                match name.to_ascii_lowercase().as_str() {
                    "id"    => { el.id = value; }
                    "class" => { el.class_name = value; }
                    "href"  => { el.href = value; }
                    other   => {
                        set_attr_raw(&mut el.attrs_raw, other, &value);
                    }
                }
            }
        }
        DomMutation::RemoveAttribute { path, name } => {
            if path.is_empty() { return; }
            if let Some(el) = navigate_mut(root, &path) {
                match name.to_ascii_lowercase().as_str() {
                    "id"    => { el.id.clear(); }
                    "class" => { el.class_name.clear(); }
                    "href"  => { el.href.clear(); }
                    other   => { remove_attr_raw(&mut el.attrs_raw, other); }
                }
            }
        }
        DomMutation::SetClassName { path, value } => {
            if path.is_empty() { return; }
            if let Some(el) = navigate_mut(root, &path) {
                el.class_name = value;
            }
        }
        DomMutation::SetId { path, value } => {
            if path.is_empty() { return; }
            if let Some(el) = navigate_mut(root, &path) {
                el.id = value;
            }
        }
        DomMutation::AppendChild { path, child_tag, child_text } => {
            if path.is_empty() { return; }
            if let Some(el) = navigate_mut(root, &path) {
                let mut child_el = Element {
                    tag:        child_tag.clone(),
                    id:         String::new(),
                    class_name: String::new(),
                    href:       String::new(),
                    attrs_raw:  String::new(),
                    children:   Vec::new(),
                    event_listeners: Vec::new(),
                };
                if !child_text.is_empty() {
                    child_el.children.push(Node::Text(TextNode {
                        text:  child_text,
                    }));
                }
                el.children.push(Node::Element(child_el));
            }
        }
        DomMutation::Remove { path } => {
            if path.is_empty() { return; }
            let parent_path = &path[..path.len() - 1];
            let child_idx   = path[path.len() - 1];
            if let Some(parent) = navigate_mut(root, parent_path) {
                if child_idx < parent.children.len() {
                    parent.children.remove(child_idx);
                }
            }
        }
        DomMutation::AddEventListener { path, event_type, callback } => {
            if let Some(el) = navigate_mut(root, &path) {
                el.event_listeners.push((event_type, callback));
            }
        }
        DomMutation::InsertAdjacentHtml { path, position, html } => {
            let new_nodes = parse_html_fragment(&html);
            if new_nodes.is_empty() { return; }
            match position.to_ascii_lowercase().as_str() {
                "beforebegin" => {
                    if path.is_empty() { return; }
                    let parent_path = &path[..path.len() - 1];
                    let child_idx = path[path.len() - 1];
                    if let Some(parent) = navigate_mut(root, parent_path) {
                        for (offset, node) in new_nodes.into_iter().enumerate() {
                            parent.children.insert((child_idx + offset).min(parent.children.len()), node);
                        }
                    }
                }
                "afterbegin" => {
                    if let Some(el) = navigate_mut(root, &path) {
                        for (offset, node) in new_nodes.into_iter().enumerate() {
                            el.children.insert(offset.min(el.children.len()), node);
                        }
                    }
                }
                "beforeend" => {
                    if let Some(el) = navigate_mut(root, &path) {
                        el.children.extend(new_nodes);
                    }
                }
                "afterend" => {
                    if path.is_empty() { return; }
                    let parent_path = &path[..path.len() - 1];
                    let child_idx = path[path.len() - 1];
                    if let Some(parent) = navigate_mut(root, parent_path) {
                        for (offset, node) in new_nodes.into_iter().enumerate() {
                            parent.children.insert((child_idx + 1 + offset).min(parent.children.len()), node);
                        }
                    }
                }
                _ => {}
            }
        }
        DomMutation::PrependChild { parent_path, child_path } => {
            // Find and remove the child from its current location
            if child_path.is_empty() { return; }
            let old_parent_path = &child_path[..child_path.len() - 1];
            let old_idx = child_path[child_path.len() - 1];
            let removed_node = if let Some(old_parent) = navigate_mut(root, old_parent_path) {
                if old_idx < old_parent.children.len() {
                    Some(old_parent.children.remove(old_idx))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(node) = removed_node {
                if let Some(new_parent) = navigate_mut(root, &parent_path) {
                    new_parent.children.insert(0, node);
                }
            }
        }
        DomMutation::SetTimeout { .. } => {}
    }
}

/// Walk the DOM tree following `path` (each value is a child index) and
/// return a mutable reference to the target `Element`, or `None` if the
/// path is invalid.
fn navigate_mut<'a>(node: &'a mut Node, path: &[usize]) -> Option<&'a mut Element> {
    match node {
        Node::Text(_) => None,
        Node::Element(el) => {
            if path.is_empty() {
                return Some(el);
            }
            let idx = path[0];
            if idx < el.children.len() {
                navigate_mut(&mut el.children[idx], &path[1..])
            } else {
                None
            }
        }
    }
}

/// Set or update a key="value" pair inside a raw attrs string.
fn set_attr_raw(attrs_raw: &mut String, name: &str, value: &str) {
    // Remove old occurrence first, then append.
    remove_attr_raw(attrs_raw, name);
    if !attrs_raw.is_empty() { attrs_raw.push(' '); }
    attrs_raw.push_str(&format!("{}=\"{}\"", name, value.replace('"', "&quot;")));
}

/// Remove a key="value" pair from a raw attrs string.
fn remove_attr_raw(attrs_raw: &mut String, name: &str) {
    // Build new string without the key.
    let mut result = String::new();
    let mut rest = attrs_raw.as_str();
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() { break; }
        // Find the attribute name (up to `=` or whitespace).
        let key_end = rest.find(|c: char| c == '=' || c.is_ascii_whitespace())
            .unwrap_or(rest.len());
        let key = &rest[..key_end];
        rest = &rest[key_end..];
        // Skip `=` and optional value.
        let value_part;
        if rest.starts_with('=') {
            rest = &rest[1..]; // skip '='
            if rest.starts_with('"') {
                // Quoted value.
                let end = rest[1..].find('"').map(|p| p + 2).unwrap_or(rest.len());
                value_part = &rest[..end];
                rest = &rest[end..];
            } else if rest.starts_with('\'') {
                let end = rest[1..].find('\'').map(|p| p + 2).unwrap_or(rest.len());
                value_part = &rest[..end];
                rest = &rest[end..];
            } else {
                let end = rest.find(|c: char| c.is_ascii_whitespace()).unwrap_or(rest.len());
                value_part = &rest[..end];
                rest = &rest[end..];
            }
        } else {
            value_part = "";
        }
        // Only keep this attr if its key doesn't match.
        if !key.eq_ignore_ascii_case(name) {
            if !result.is_empty() { result.push(' '); }
            result.push_str(key);
            if !value_part.is_empty() {
                result.push('=');
                result.push_str(value_part);
            }
        }
    }
    *attrs_raw = result;
}


/// Parse a very simple HTML fragment into a list of `Node`s.
/// Supports plain text and `<tag>text</tag>` elements (one level deep).
fn parse_html_fragment(html: &str) -> Vec<Node> {
    crate::html5::parser::parse_fragment(html)
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
fn find_first(node: &Node, pred: &dyn Fn(&Element) -> bool, path: &[usize]) -> Option<JsElement> {
    if let Node::Element(el) = node {
        if pred(el) { return Some(JsElement::from_element(el, path.to_vec())); }
        for (i, child) in el.children.iter().enumerate() {
            let mut child_path = path.to_vec();
            child_path.push(i);
            if let Some(found) = find_first(child, pred, &child_path) { return Some(found); }
        }
    }
    None
}

fn find_all(node: &Node, pred: &dyn Fn(&Element) -> bool, path: &[usize]) -> Vec<JsElement> {
    let mut out = Vec::new();
    collect_all(node, pred, path, &mut out);
    out
}

fn collect_all(node: &Node, pred: &dyn Fn(&Element) -> bool, path: &[usize], out: &mut Vec<JsElement>) {
    if let Node::Element(el) = node {
        if pred(el) { out.push(JsElement::from_element(el, path.to_vec())); }
        for (i, child) in el.children.iter().enumerate() {
            let mut child_path = path.to_vec();
            child_path.push(i);
            collect_all(child, pred, &child_path, out);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html5::parser::parse_dom;

    #[test]
    fn test_example_script_execution() {
        let html = "<!doctype html><html><body><p>Initial English text</p></body></html>";
        let mut dom = parse_dom(html);
        let js_dom = JsDom::with_title(&dom, "Example Domain".into());
        let mut js_scope = crate::js::scope::Scope::new();

        let script = r#"
            var B=document.body,P,p;
            B.children[0].insertAdjacentHTML("afterend","<p lang=fr>Texte en francais</p><a href=https://iana.org>Learn more</a>");
            P=[...B.querySelectorAll("p")];
            P[0].lang="en";
            navigator.languages.some(l=>p=P.find(p=>p.lang==l.split("-")[0]));
            p=p||P[0];
            B.prepend(p);
            B.insertAdjacentHTML("afterbegin", '<style>svg{display:block}</style><svg width=44 height=44></svg>');
        "#;

        let _ = crate::js::interpreter::execute_with_dom_and_scope(script, &js_dom, &mut js_scope);
        let mutations = js_dom.take_mutations();
        assert!(!mutations.is_empty(), "Script should generate mutations");

        for muta in mutations {
            apply_one(&mut dom, muta);
        }

        // Verify that <style> and <svg> were inserted into <body>
        fn find_tag(node: &Node, tag: &str) -> bool {
            match node {
                Node::Element(el) => {
                    if el.tag == tag { return true; }
                    el.children.iter().any(|c| find_tag(c, tag))
                }
                _ => false,
            }
        }

        assert!(find_tag(&dom, "style"), "Injected <style> block must exist in DOM");
        assert!(find_tag(&dom, "svg"), "Injected <svg> block must exist in DOM");
        assert!(find_tag(&dom, "a"), "Injected <a> link must exist in DOM");
    }

    #[test]
    fn test_example_svg_rasterization() {
        use sdl2::rwops::RWops;
        use sdl2::image::ImageRWops;

        let raw_svg = r#"<svg viewBox=0,0,20,20 width=44 height=44 fill=currentColor aria-hidden=true><path fill=none stroke=currentColor stroke-width=1.3 stroke-linejoin=round d="M6 4H3v12q4 0 7 1.5-1-3.5-4-4.5V2q3.5 1 4 3v12.5q3-1.5 7-1.5V4q-4.5 0-7 1"/><g transform=rotate(-6,13.6,8.3)><path id=q d="M11.5 6.6h1.8v1.8l-1 1.6-.6-.3.8-1.3h-1z"/><use href=#q x=2.4 /></g></svg>"#;
        let (processed, _) = crate::render::image::preprocess_svg(raw_svg.as_bytes());
        let rw = RWops::from_bytes(&processed).expect("Failed to create RWops from processed SVG");
        let surface = rw.load_typed("SVG").expect("Failed to rasterize SVG via SDL2_image");
        assert_eq!(surface.width(), 44);
        assert_eq!(surface.height(), 44);
    }
}
