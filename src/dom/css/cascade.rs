/// CSS selector matching against DOM nodes.
///
/// The public entry points are:
/// - [`matches`] — tests whether an `Element` satisfies a `Selector`.
/// - [`apply_cascade`] — walks the DOM and applies all stylesheet rules with
///   correct specificity ordering, `inherit`/`initial` keywords, and property
///   inheritance from parent to child.
/// - [`apply_cascade_with_state`] — like `apply_cascade` but accepts a
///   [`PseudoState`] for dynamic pseudo-class matching (`:hover`, `:focus`, etc.).

use crate::dom::node::{Element, Node, Style, Visibility};
use crate::dom::parser::get_attr;
use super::stylesheet::{PseudoClass, Selector, SimpleSelector, StyleSheet, Specificity};
use super::inline::apply_property;

// ─────────────────────────────────────────────────────────────────────────────
// PseudoState — runtime pseudo-class context
// ─────────────────────────────────────────────────────────────────────────────

/// Runtime pseudo-class state passed during cascade evaluation.
/// Determines which elements match :hover, :focus, :checked, etc.
pub struct PseudoState {
    /// Path of element IDs/tags that are currently hovered (from root to leaf).
    pub hovered_path: Vec<String>,
    /// Raw pointer address of the hovered element (set by browser hit-test).
    pub hovered_ptr: Option<usize>,
    /// Raw pointer address of the focused element.
    pub focused_ptr: Option<usize>,
    /// Raw pointer addresses of checked inputs.
    pub checked_ptrs: Vec<usize>,
}

impl PseudoState {
    /// A no-op pseudo state — no element is hovered, focused, or checked.
    pub fn none() -> Self {
        PseudoState {
            hovered_path: vec![],
            hovered_ptr:  None,
            focused_ptr:  None,
            checked_ptrs: vec![],
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SiblingCtx — sibling context for structural pseudo-classes
// ─────────────────────────────────────────────────────────────────────────────

/// Sibling context for structural pseudo-class evaluation
/// (`:first-child`, `:nth-child`, etc.).
struct SiblingCtx<'a> {
    /// All children of the parent element (element + text nodes), in order.
    siblings: &'a [Node],
    /// Index of the current element within `siblings`.
    index: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// Cascade engine
// ─────────────────────────────────────────────────────────────────────────────

/// The inheritable CSS properties, by canonical lowercase name.
#[allow(dead_code)]
const INHERITABLE: &[&str] = &[
    "color",
    "font-size",
    "font-weight",
    "font-style",
    "font-family",
    "text-align",
    "line-height",
    "letter-spacing",
    "word-spacing",
    "white-space",
    "text-transform",
    "font-variant-caps",
    "word-break",
    "overflow-wrap",
    "visibility",
];

/// Walk the entire DOM tree rooted at `root`, apply every rule from `sheets` to
/// each element, then propagate inheritable properties.
///
/// This is equivalent to `apply_cascade_with_state(root, sheets, &PseudoState::none())`.
pub fn apply_cascade(root: &mut Node, sheets: &[StyleSheet]) {
    apply_cascade_with_state(root, sheets, &PseudoState::none());
}

/// Like [`apply_cascade`] but uses the given [`PseudoState`] for dynamic
/// pseudo-class matching (`:hover`, `:focus`, `:checked`, etc.).
pub fn apply_cascade_with_state(root: &mut Node, sheets: &[StyleSheet], state: &PseudoState) {
    reset_styles(root);

    // ── Collect CSS custom properties from :root ──────────────────────────
    // Any declaration whose property name starts with `--` on the :root
    // element is a custom property.  We gather them here so `var()` can be
    // resolved for every element in the tree.
    let css_vars = collect_root_vars(root, sheets);

    let parent_style = Style::default();
    cascade_node_inner(root, sheets, &[], &parent_style, 1.0, state, None, &css_vars);
}

/// Walk the DOM and reset every element's `style` field to UA defaults +
/// inline style. Text nodes get `Style::default()`. This undoes any
/// previously applied cascade rules so each frame starts clean.
fn reset_styles(node: &mut Node) {
    match node {
        Node::Text(t) => {
            t.style = Style::default();
        }
        Node::Element(el) => {
            // Reset to a clean Style, then re-apply UA tag defaults and inline
            // style so structural defaults (display:block, font sizes, etc.)
            // are preserved while author rules from previous frames are gone.
            el.style = Style::default();
            super::ua::apply_tag_defaults(el);
            if !el.style_attr.is_empty() {
                let inline = el.style_attr.clone();
                super::inline::apply_inline(&inline, &mut el.style);
            }
            for child in &mut el.children {
                reset_styles(child);
            }
        }
    }
}

/// Collect all CSS custom properties (`--name: value`) declared on the
/// `:root` element by walking every sheet.  Returns a map of
/// `property_name → value` (without the leading `--`).
fn collect_root_vars(root: &Node, sheets: &[StyleSheet]) -> std::collections::HashMap<String, String> {
    let mut vars = std::collections::HashMap::new();

    // We need the root element to test :root matching (ancestors = []).
    let root_el = match root {
        Node::Element(el) => el,
        _ => return vars,
    };

    for sheet in sheets {
        for rule in &sheet.rules {
            let matches_root = rule.selectors.iter().any(|sel| {
                matches_full(root_el, sel, &[], &PseudoState::none(), None)
            });
            if matches_root {
                for (prop, val) in &rule.declarations {
                    if prop.starts_with("--") {
                        // Store without the `--` prefix for easy lookup.
                        vars.insert(prop.clone(), val.clone());
                    }
                }
            }
        }
    }

    vars
}

/// Resolve all `var(--name)` and `var(--name, fallback)` references in `val`
/// using the provided custom-property map.
fn resolve_vars(val: &str, vars: &std::collections::HashMap<String, String>) -> String {
    if !val.contains("var(") {
        return val.to_owned();
    }

    let mut result = String::new();
    let chars: Vec<char> = val.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Look for "var("
        if i + 4 <= len && chars[i..i+4].iter().collect::<String>().eq_ignore_ascii_case("var(") {
            i += 4; // skip "var("
            // Collect the argument, tracking nested parens
            let mut depth = 1usize;
            let mut arg = String::new();
            while i < len && depth > 0 {
                match chars[i] {
                    '(' => { depth += 1; arg.push(chars[i]); }
                    ')' => {
                        depth -= 1;
                        if depth > 0 { arg.push(chars[i]); }
                    }
                    c => { arg.push(c); }
                }
                i += 1;
            }
            // arg is now the content inside var(…)
            // Split on first comma to get name and optional fallback
            let (name_part, fallback_part) = if let Some(comma) = find_comma_outside_parens(&arg) {
                (&arg[..comma], Some(arg[comma+1..].trim().to_owned()))
            } else {
                (arg.as_str(), None)
            };
            let name = name_part.trim();
            if let Some(resolved) = vars.get(name) {
                // Recursively resolve in case the value itself uses vars
                let resolved = resolve_vars(resolved.trim(), vars);
                result.push_str(&resolved);
            } else if let Some(fallback) = fallback_part {
                let fallback = resolve_vars(&fallback, vars);
                result.push_str(&fallback);
            }
            // If neither found and no fallback, nothing is emitted (property will be ignored)
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Find the position of the first comma that is not nested inside parentheses.
fn find_comma_outside_parens(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => { if depth > 0 { depth -= 1; } }
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

fn cascade_node_inner(
    node:             &mut Node,
    sheets:           &[StyleSheet],
    ancestors:        &[&Element],
    parent_style:     &Style,
    inherited_opacity: f32,
    state:            &PseudoState,
    sib:              Option<&SiblingCtx<'_>>,
    css_vars:         &std::collections::HashMap<String, String>,
) {
    match node {
        Node::Text(t) => {
            inherit_from(&mut t.style, parent_style);
        }
        Node::Element(el) => {
            // ── 1. Collect matching rules ─────────────────────────────────
            let mut matched: Vec<(usize, usize, Specificity, &[(String, String)])> = Vec::new();
            for (sheet_idx, sheet) in sheets.iter().enumerate() {
                for (rule_idx, rule) in sheet.rules.iter().enumerate() {
                    if rule.selectors.iter().any(|sel| matches_full(el, sel, ancestors, state, sib)) {
                        let spec = rule.selectors
                            .iter()
                            .filter(|sel| matches_full(el, sel, ancestors, state, sib))
                            .map(|sel| sel.specificity())
                            .max()
                            .unwrap_or(Specificity(0, 0, 0));
                        matched.push((sheet_idx, rule_idx, spec, &rule.declarations));
                    }
                }
            }

            // ── 2. Sort by (specificity, source_order) so last write wins ─
            matched.sort_by_key(|(si, ri, spec, _)| (*spec, *si, *ri));

            // ── 3. Inherit, then apply sheet rules ────────────────────────
            inherit_from(&mut el.style, parent_style);

            let base_font = el.style.font_size;
            for (_, _, _, decls) in &matched {
                for (prop, val) in *decls {
                    // Skip custom property declarations (--name) — they are
                    // consumed by collect_root_vars, not applied to Style.
                    if prop.starts_with("--") {
                        continue;
                    }
                    // Resolve any var() references before applying.
                    let resolved_val = resolve_vars(val.trim(), css_vars);
                    let val = resolved_val.trim();
                    let prop_lc = prop.to_ascii_lowercase();
                    if val.eq_ignore_ascii_case("inherit") {
                        apply_inherit_keyword(&prop_lc, &mut el.style, parent_style);
                    } else if val.eq_ignore_ascii_case("initial") {
                        apply_initial_keyword(&prop_lc, &mut el.style);
                    } else {
                        apply_property(&prop_lc, val, base_font, &mut el.style);
                    }
                }
            }

            // ── 4. Inline style overrides everything ──────────────────────
            if !el.style_attr.is_empty() {
                let inline_attr = if el.style_attr.contains("var(") {
                    resolve_vars(&el.style_attr, css_vars)
                } else {
                    el.style_attr.clone()
                };
                super::inline::apply_inline(&inline_attr, &mut el.style);
            }

            // ── 4b. Bake CSS `opacity` into alpha channels ────────────────
            let own_opacity = el.style.opacity as f32 / 255.0;
            let effective_opacity = inherited_opacity * own_opacity;
            if effective_opacity < 1.0 {
                el.style.bg_alpha = (el.style.bg_alpha as f32 * effective_opacity).round() as u8;
                el.style.opacity  = (effective_opacity * 255.0).round() as u8;
            }

            // ── 5. Recurse into children ──────────────────────────────────
            let el_ptr: *const Element = el as *const Element;
            let child_ancestors: Vec<&Element> = {
                let mut v: Vec<&Element> = ancestors.to_vec();
                // SAFETY: el is valid for the entire duration of the child walk.
                v.push(unsafe { &*el_ptr });
                v
            };
            let child_parent_style = el.style.clone();

            // We need a raw pointer to el.children to iterate mutably while
            // building sibling contexts that borrow the immutable children slice.
            // SAFETY: We only read from the immutable slice for SiblingCtx and
            // mutate each element in turn — never aliasing.
            let children_ptr: *const Vec<Node> = &el.children as *const Vec<Node>;

            for (idx, child) in el.children.iter_mut().enumerate() {
                let sibling_ctx = SiblingCtx {
                    // SAFETY: children_ptr points to el.children which is alive.
                    siblings: unsafe { &*children_ptr },
                    index:    idx,
                };
                cascade_node_inner(
                    child,
                    sheets,
                    &child_ancestors,
                    &child_parent_style,
                    effective_opacity,
                    state,
                    Some(&sibling_ctx),
                    css_vars,
                );
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Return `true` if `el` matches `sel` given the ordered ancestor slice.
///
/// `ancestors` must be ordered from the **root** down to (but not including)
/// `el`'s direct parent, i.e. `ancestors.last()` is the direct parent.
///
/// This function never panics; unrecognised selector variants return `false`.
///
/// Uses a no-op `PseudoState` — for dynamic pseudo-classes use
/// [`matches_with_state`] instead.
pub fn matches(el: &Element, sel: &Selector, ancestors: &[&Element]) -> bool {
    matches_full(el, sel, ancestors, &PseudoState::none(), None)
}

/// Like [`matches`] but accepts a `PseudoState` and sibling context for
/// complete pseudo-class support.
pub fn matches_with_state(
    el:        &Element,
    sel:       &Selector,
    ancestors: &[&Element],
    state:     &PseudoState,
) -> bool {
    matches_full(el, sel, ancestors, state, None)
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal matching
// ─────────────────────────────────────────────────────────────────────────────

fn matches_full(
    el:        &Element,
    sel:       &Selector,
    ancestors: &[&Element],
    state:     &PseudoState,
    sib:       Option<&SiblingCtx<'_>>,
) -> bool {
    match sel {
        Selector::Tag(t)       => el.tag.eq_ignore_ascii_case(t),
        Selector::Class(c)     => has_class(el, c),
        Selector::Id(id)       => el.id == *id,
        Selector::Universal    => true,

        Selector::Compound(parts) => {
            parts.iter().all(|ss| matches_simple_with_ctx(el, ss, ancestors, state, sib))
        }

        Selector::Descendant(a, b) => {
            if !matches_full(el, b, ancestors, state, sib) { return false; }
            // Try matching 'a' against any ancestor, providing its own context
            for (i, anc) in ancestors.iter().enumerate() {
                if matches_full(anc, a, &ancestors[..i], state, None) {
                    return true;
                }
            }
            false
        }

        Selector::Child(a, b) => {
            if !matches_full(el, b, ancestors, state, sib) { return false; }
            match ancestors.last() {
                Some(parent) => matches_full(
                    parent, a,
                    &ancestors[..ancestors.len() - 1],
                    state, None,
                ),
                None => false,
            }
        }

        Selector::AdjacentSibling(a, b) => {
            if !matches_full(el, b, ancestors, state, sib) { return false; }
            if let Some(s) = sib {
                // Find the direct previous element-node sibling
                for i in (0..s.index).rev() {
                    match &s.siblings[i] {
                        Node::Element(prev) => {
                            return matches_full(prev, a, ancestors, state, None);
                        }
                        Node::Text(t) if !t.text.trim().is_empty() => {
                            // Non-empty text node prevents adjacency
                            return false;
                        }
                        _ => { /* Skip comment/empty-text */ }
                    }
                }
            }
            false
        }

        Selector::GeneralSibling(a, b) => {
            if !matches_full(el, b, ancestors, state, sib) { return false; }
            if let Some(s) = sib {
                s.siblings[..s.index].iter().any(|sibling| {
                    if let Node::Element(prev) = sibling {
                        matches_full(prev, a, ancestors, state, None)
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        }
    }
}

/// Match a single `SimpleSelector` component against `el`, with full pseudo-class
/// and sibling context support.
fn matches_simple_with_ctx(
    el:        &Element,
    ss:        &SimpleSelector,
    ancestors: &[&Element],
    state:     &PseudoState,
    sib:       Option<&SiblingCtx<'_>>,
) -> bool {
    match ss {
        SimpleSelector::Tag(t)       => el.tag.eq_ignore_ascii_case(t),
        SimpleSelector::Class(c)     => has_class(el, c),
        SimpleSelector::Id(id)       => el.id == *id,
        SimpleSelector::Universal    => true,
        SimpleSelector::AttrPresence(attr) => {
            get_attr(&el.attrs_raw, attr).is_some()
        }
        SimpleSelector::AttrEquality(attr, expected) => {
            get_attr(&el.attrs_raw, attr)
                .map(|v| v == expected.as_str())
                .unwrap_or(false)
        }
        SimpleSelector::AttrContainsWord(attr, word) => {
            get_attr(&el.attrs_raw, attr)
                .map(|v| v.split_ascii_whitespace().any(|w| w == word.as_str()))
                .unwrap_or(false)
        }
        SimpleSelector::AttrStartsWith(attr, prefix) => {
            get_attr(&el.attrs_raw, attr)
                .map(|v| v.starts_with(prefix.as_str()))
                .unwrap_or(false)
        }
        SimpleSelector::AttrEndsWith(attr, suffix) => {
            get_attr(&el.attrs_raw, attr)
                .map(|v| v.ends_with(suffix.as_str()))
                .unwrap_or(false)
        }
        SimpleSelector::AttrContains(attr, sub) => {
            get_attr(&el.attrs_raw, attr)
                .map(|v| v.contains(sub.as_str()))
                .unwrap_or(false)
        }
        SimpleSelector::Pseudo(pc) => {
            matches_pseudo(el, pc, ancestors, state, sib)
        }
    }
}

/// Evaluate a pseudo-class against the element.
fn matches_pseudo(
    el:        &Element,
    pc:        &PseudoClass,
    ancestors: &[&Element],
    state:     &PseudoState,
    sib:       Option<&SiblingCtx<'_>>,
) -> bool {
    match pc {
        PseudoClass::Hover  => state.hovered_ptr == Some(el as *const Element as usize),
        PseudoClass::Focus  => state.focused_ptr == Some(el as *const Element as usize),
        PseudoClass::Active => false, // not tracked yet

        PseudoClass::Link    => el.tag == "a" && get_attr(&el.attrs_raw, "href").is_some(),
        PseudoClass::Visited => el.tag == "a" && get_attr(&el.attrs_raw, "href").is_some(),

        PseudoClass::Checked  => {
            // Either explicitly in attrs_raw OR in checked_ptrs
            get_attr(&el.attrs_raw, "checked").is_some()
                || state.checked_ptrs.contains(&(el as *const Element as usize))
        }
        PseudoClass::Disabled => get_attr(&el.attrs_raw, "disabled").is_some(),
        PseudoClass::Enabled  => {
            get_attr(&el.attrs_raw, "disabled").is_none()
                && matches!(
                    el.tag.as_str(),
                    "input" | "button" | "select" | "textarea"
                )
        }

        PseudoClass::Empty => {
            el.children.iter().all(|c| {
                matches!(c, Node::Text(t) if t.text.trim().is_empty())
            })
        }

        PseudoClass::Root => {
            // :root matches the document root — the element with no ancestors.
            ancestors.is_empty()
        }

        // ── Structural pseudo-classes — require sibling context ───────────

        PseudoClass::FirstChild => sib.map(|s| {
            // First element-node sibling
            s.siblings.iter().take(s.index + 1)
                .filter(|n| matches!(n, Node::Element(_)))
                .count() == 1
        }).unwrap_or(false),

        PseudoClass::LastChild => sib.map(|s| {
            let elem_count = s.siblings.iter()
                .filter(|n| matches!(n, Node::Element(_)))
                .count();
            let elem_idx = s.siblings[..=s.index].iter()
                .filter(|n| matches!(n, Node::Element(_)))
                .count();
            elem_idx == elem_count
        }).unwrap_or(false),

        PseudoClass::OnlyChild => sib.map(|s| {
            s.siblings.iter()
                .filter(|n| matches!(n, Node::Element(_)))
                .count() == 1
        }).unwrap_or(false),

        PseudoClass::FirstOfType => sib.map(|s| {
            let my_tag = &el.tag;
            s.siblings[..=s.index].iter()
                .filter(|n| matches!(n, Node::Element(e) if e.tag.eq_ignore_ascii_case(my_tag)))
                .count() == 1
        }).unwrap_or(false),

        PseudoClass::LastOfType => sib.map(|s| {
            let my_tag = &el.tag;
            let total = s.siblings.iter()
                .filter(|n| matches!(n, Node::Element(e) if e.tag.eq_ignore_ascii_case(my_tag)))
                .count();
            let pos = s.siblings[..=s.index].iter()
                .filter(|n| matches!(n, Node::Element(e) if e.tag.eq_ignore_ascii_case(my_tag)))
                .count();
            pos == total
        }).unwrap_or(false),

        PseudoClass::OnlyOfType => sib.map(|s| {
            let my_tag = &el.tag;
            s.siblings.iter()
                .filter(|n| matches!(n, Node::Element(e) if e.tag.eq_ignore_ascii_case(my_tag)))
                .count() == 1
        }).unwrap_or(false),

        PseudoClass::NthChild(a, b) => sib.map(|s| {
            let pos = s.siblings[..=s.index].iter()
                .filter(|n| matches!(n, Node::Element(_)))
                .count() as i32;
            nth_matches(*a, *b, pos)
        }).unwrap_or(false),

        PseudoClass::NthLastChild(a, b) => sib.map(|s| {
            let total = s.siblings.iter()
                .filter(|n| matches!(n, Node::Element(_)))
                .count() as i32;
            let pos = s.siblings[..=s.index].iter()
                .filter(|n| matches!(n, Node::Element(_)))
                .count() as i32;
            nth_matches(*a, *b, total - pos + 1)
        }).unwrap_or(false),

        PseudoClass::NthOfType(a, b) => sib.map(|s| {
            let my_tag = &el.tag;
            let pos = s.siblings[..=s.index].iter()
                .filter(|n| matches!(n, Node::Element(e) if e.tag.eq_ignore_ascii_case(my_tag)))
                .count() as i32;
            nth_matches(*a, *b, pos)
        }).unwrap_or(false),

        PseudoClass::Not(inner_ss) => {
            !matches_simple_with_ctx(el, inner_ss, ancestors, state, sib)
        }

        PseudoClass::Unknown(_) => false,
    }
}

/// An+B matching: returns true if `pos` satisfies the An+B expression.
///
/// `pos` is 1-based.  `a == 0` reduces to `pos == b`.
fn nth_matches(a: i32, b: i32, pos: i32) -> bool {
    if a == 0 {
        return pos == b;
    }
    let n = pos - b;
    n >= 0 && n % a == 0
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Return `true` if any whitespace-separated token in `el.class_name` equals `cls`.
fn has_class(el: &Element, cls: &str) -> bool {
    el.class_name.split_ascii_whitespace().any(|c| c == cls)
}

// ─────────────────────────────────────────────────────────────────────────────
// Inheritance helpers
// ─────────────────────────────────────────────────────────────────────────────

fn inherit_from(child: &mut Style, parent: &Style) {
    let def = Style::default();

    // ── Regular CSS inheritance ──────────────────────────────────────────────
    if child.color == def.color {
        child.color       = parent.color;
        child.color_alpha = parent.color_alpha;
    }
    
    // Forkit extension: inline backgrounds (not standard CSS, but needed for 
    // Forkit's inline-layout model where text nodes paint the backgrounds).
    if child.bg_color.is_none() && parent.display == crate::dom::node::Display::Inline {
        child.bg_color = parent.bg_color;
        child.bg_alpha = parent.bg_alpha;
    }

    if child.font_size == def.font_size {
        child.font_size = parent.font_size;
        if child.font_size_raw.is_none() {
            child.font_size_raw = parent.font_size_raw.clone();
        }
    }
    if child.bold == def.bold {
        child.bold = parent.bold;
    }
    if child.italic == def.italic {
        child.italic = parent.italic;
    }
    if child.text_align == def.text_align {
        child.text_align = parent.text_align;
    }
    #[allow(clippy::float_cmp)]
    if child.line_height_mul == def.line_height_mul {
        child.line_height_mul = parent.line_height_mul;
    }
    if child.letter_spacing == def.letter_spacing {
        child.letter_spacing = parent.letter_spacing;
    }
    if child.word_spacing == def.word_spacing {
        child.word_spacing = parent.word_spacing;
    }
    if child.white_space_pre == def.white_space_pre {
        child.white_space_pre = parent.white_space_pre;
    }
    if child.text_transform == def.text_transform {
        child.text_transform = parent.text_transform;
    }
    if child.font_variant_caps == def.font_variant_caps {
        child.font_variant_caps = parent.font_variant_caps;
    }
    if child.font_family == def.font_family {
        child.font_family = parent.font_family;
    }
    if child.word_break == def.word_break {
        child.word_break = parent.word_break;
    }
    if child.visibility == def.visibility {
        child.visibility = parent.visibility;
    }
    if child.underline == def.underline {
        child.underline = parent.underline;
    }
    if child.strikethrough == def.strikethrough {
        child.strikethrough = parent.strikethrough;
    }
    // list-style-type is inherited — li gets its bullet style from the parent ul/ol
    if child.list_style_type == def.list_style_type {
        child.list_style_type = parent.list_style_type;
    }
}

fn apply_inherit_keyword(prop: &str, child: &mut Style, parent: &Style) {
    match prop {
        "color"             => { child.color = parent.color; child.color_alpha = parent.color_alpha; }
        "font-size"         => { child.font_size = parent.font_size; }
        "font-weight"       => { child.bold = parent.bold; }
        "font-style"        => { child.italic = parent.italic; }
        "text-align"        => { child.text_align = parent.text_align; }
        "line-height"       => { child.line_height_mul = parent.line_height_mul; }
        "letter-spacing"    => { child.letter_spacing = parent.letter_spacing; }
        "word-spacing"      => { child.word_spacing = parent.word_spacing; }
        "white-space"       => { child.white_space_pre = parent.white_space_pre; }
        "text-transform"    => { child.text_transform = parent.text_transform; }
        "font-variant-caps" => { child.font_variant_caps = parent.font_variant_caps; }
        "font-family"       => { child.font_family = parent.font_family; }
        "word-break" | "overflow-wrap" => { child.word_break = parent.word_break; }
        "visibility"        => { child.visibility = parent.visibility; }
        _ => {}
    }
}

fn apply_initial_keyword(prop: &str, style: &mut Style) {
    let def = Style::default();
    match prop {
        "color"               => { style.color = def.color; style.color_alpha = def.color_alpha; }
        "font-size"           => { style.font_size = def.font_size; }
        "font-weight"         => { style.bold = def.bold; }
        "font-style"          => { style.italic = def.italic; }
        "text-align"          => { style.text_align = def.text_align; }
        "line-height"         => { style.line_height_mul = def.line_height_mul; }
        "letter-spacing"      => { style.letter_spacing = def.letter_spacing; }
        "word-spacing"        => { style.word_spacing = def.word_spacing; }
        "white-space"         => { style.white_space_pre = def.white_space_pre; }
        "text-transform"      => { style.text_transform = def.text_transform; }
        "font-variant-caps"   => { style.font_variant_caps = def.font_variant_caps; }
        "background-color"    => { style.bg_color = def.bg_color; style.bg_alpha = def.bg_alpha; }
        "background-image"    => { style.bg_image_url = None; style.bg_gradient = None; }
        "background-size"     => { style.bg_size = def.bg_size; }
        "background-repeat"   => { style.bg_repeat = def.bg_repeat; }
        "background-position" => { style.bg_position = def.bg_position; }
        "border-radius"       => { style.border_radius = def.border_radius; }
        "display"             => { style.display = def.display; style.display_block = def.display_block; }
        "visibility"          => { style.visibility = Visibility::default(); }
        "opacity"             => { style.opacity = def.opacity; }
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::node::{Element, Style, Node, TextNode};
    use crate::dom::css::stylesheet::{Selector, SimpleSelector, StyleSheet};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_el(tag: &str, id: &str, class: &str) -> Element {
        Element {
            tag:        tag.to_string(),
            id:         id.to_string(),
            class_name: class.to_string(),
            style_attr: String::new(),
            attrs_raw:  String::new(),
            style:      Style::default(),
            children:   vec![],
        }
    }

    fn make_el_inline(tag: &str, inline: &str) -> Element {
        Element {
            tag:        tag.to_string(),
            id:         String::new(),
            class_name: String::new(),
            style_attr: inline.to_string(),
            attrs_raw:  String::new(),
            style:      Style::default(),
            children:   vec![],
        }
    }

    fn el(tag: &str, id: &str, class: &str) -> Element {
        make_el(tag, id, class)
    }

    fn el_with_attrs(tag: &str, id: &str, class: &str, attrs: &str) -> Element {
        Element {
            tag:        tag.to_string(),
            id:         id.to_string(),
            class_name: class.to_string(),
            style_attr: String::new(),
            attrs_raw:  attrs.to_string(),
            style:      Style::default(),
            children:   vec![],
        }
    }

    #[test]
    fn tag_match() {
        let e = el("div", "", "");
        assert!(matches(&e, &Selector::Tag("div".into()), &[]));
    }

    #[test]
    fn tag_no_match() {
        let e = el("span", "", "");
        assert!(!matches(&e, &Selector::Tag("div".into()), &[]));
    }

    #[test]
    fn tag_case_insensitive() {
        let e = el("DIV", "", "");
        assert!(matches(&e, &Selector::Tag("div".into()), &[]));
    }

    #[test]
    fn class_match() {
        let e = el("p", "", "foo bar");
        assert!(matches(&e, &Selector::Class("foo".into()), &[]));
        assert!(matches(&e, &Selector::Class("bar".into()), &[]));
    }

    #[test]
    fn class_no_match() {
        let e = el("p", "", "foo");
        assert!(!matches(&e, &Selector::Class("baz".into()), &[]));
    }

    #[test]
    fn id_match() {
        let e = el("div", "main", "");
        assert!(matches(&e, &Selector::Id("main".into()), &[]));
    }

    #[test]
    fn id_no_match() {
        let e = el("div", "main", "");
        assert!(!matches(&e, &Selector::Id("other".into()), &[]));
    }

    #[test]
    fn universal_match() {
        let e = el("anything", "", "");
        assert!(matches(&e, &Selector::Universal, &[]));
    }

    #[test]
    fn compound_all_match() {
        let e = el("div", "hero", "box");
        let sel = Selector::Compound(vec![
            SimpleSelector::Tag("div".into()),
            SimpleSelector::Class("box".into()),
            SimpleSelector::Id("hero".into()),
        ]);
        assert!(matches(&e, &sel, &[]));
    }

    #[test]
    fn compound_partial_miss() {
        let e = el("div", "", "box");
        let sel = Selector::Compound(vec![
            SimpleSelector::Tag("span".into()),
            SimpleSelector::Class("box".into()),
        ]);
        assert!(!matches(&e, &sel, &[]));
    }

    #[test]
    fn descendant_match() {
        let ancestor = el("div", "", "");
        let child = el("p", "", "");
        assert!(matches(
            &child,
            &Selector::Descendant(
                Box::new(Selector::Tag("div".into())),
                Box::new(Selector::Tag("p".into())),
            ),
            &[&ancestor],
        ));
    }

    #[test]
    fn descendant_miss_when_not_a_descendant() {
        let ancestor = el("span", "", "");
        let child = el("p", "", "");
        assert!(!matches(
            &child,
            &Selector::Descendant(
                Box::new(Selector::Tag("div".into())),
                Box::new(Selector::Tag("p".into())),
            ),
            &[&ancestor],
        ));
    }

    #[test]
    fn descendant_deep_match() {
        let grandparent = el("div", "", "");
        let parent = el("section", "", "");
        let child = el("p", "", "");
        assert!(matches(
            &child,
            &Selector::Descendant(
                Box::new(Selector::Tag("div".into())),
                Box::new(Selector::Tag("p".into())),
            ),
            &[&grandparent, &parent],
        ));
    }

    #[test]
    fn child_match() {
        let parent = el("ul", "", "");
        let child = el("li", "", "");
        assert!(matches(
            &child,
            &Selector::Child(
                Box::new(Selector::Tag("ul".into())),
                Box::new(Selector::Tag("li".into())),
            ),
            &[&parent],
        ));
    }

    #[test]
    fn child_no_match_wrong_parent() {
        let parent = el("ol", "", "");
        let child = el("li", "", "");
        assert!(!matches(
            &child,
            &Selector::Child(
                Box::new(Selector::Tag("ul".into())),
                Box::new(Selector::Tag("li".into())),
            ),
            &[&parent],
        ));
    }

    #[test]
    fn child_no_match_grandparent_only() {
        let grandparent = el("div", "", "");
        let parent = el("section", "", "");
        let child = el("p", "", "");
        assert!(!matches(
            &child,
            &Selector::Child(
                Box::new(Selector::Tag("div".into())),
                Box::new(Selector::Tag("p".into())),
            ),
            &[&grandparent, &parent],
        ));
    }

    #[test]
    fn attr_presence_match() {
        let e = el_with_attrs("a", "", "", r#"href="https://example.com""#);
        let sel = Selector::Compound(vec![SimpleSelector::AttrPresence("href".into())]);
        assert!(matches(&e, &sel, &[]));
    }

    #[test]
    fn attr_presence_miss() {
        let e = el_with_attrs("a", "", "", r#"class="link""#);
        let sel = Selector::Compound(vec![SimpleSelector::AttrPresence("href".into())]);
        assert!(!matches(&e, &sel, &[]));
    }

    #[test]
    fn attr_equality_match() {
        let e = el_with_attrs("input", "", "", r#"type="text""#);
        let sel = Selector::Compound(vec![
            SimpleSelector::AttrEquality("type".into(), "text".into()),
        ]);
        assert!(matches(&e, &sel, &[]));
    }

    #[test]
    fn attr_equality_miss() {
        let e = el_with_attrs("input", "", "", r#"type="checkbox""#);
        let sel = Selector::Compound(vec![
            SimpleSelector::AttrEquality("type".into(), "text".into()),
        ]);
        assert!(!matches(&e, &sel, &[]));
    }

    // ── Cascade engine tests ──────────────────────────────────────────────────

    #[test]
    fn cascade_author_overrides_ua() {
        let mut root_el = make_el("p", "", "");
        root_el.style.bold = true;
        let sheet = StyleSheet::parse("p { font-weight: normal; }");
        let mut root = Node::Element(root_el);
        apply_cascade(&mut root, &[sheet]);
        if let Node::Element(el) = &root {
            assert!(!el.style.bold, "author rule should override UA bold");
        }
    }

    #[test]
    fn cascade_inline_overrides_author() {
        let mut root_el = make_el_inline("p", "color: rgb(255,0,0)");
        root_el.style.color = [0, 0, 0];
        let sheet = StyleSheet::parse("p { color: rgb(0,0,255); }");
        let mut root = Node::Element(root_el);
        apply_cascade(&mut root, &[sheet]);
        if let Node::Element(el) = &root {
            assert_eq!(el.style.color, [255, 0, 0],
                "inline style should override author rule");
        }
    }

    #[test]
    fn cascade_specificity_ordering() {
        let root_el = make_el("p", "", "highlight");
        let sheet = StyleSheet::parse("p { color: rgb(0,0,255); } .highlight { color: rgb(255,165,0); }");
        let mut root = Node::Element(root_el);
        apply_cascade(&mut root, &[sheet]);
        if let Node::Element(el) = &root {
            assert_eq!(el.style.color, [255, 165, 0],
                ".highlight rule (higher specificity) should win over p rule");
        }
    }

    #[test]
    fn cascade_child_inherits_color() {
        let child_el = make_el("span", "", "");
        let mut parent_el = make_el("p", "", "");
        parent_el.children.push(Node::Element(child_el));
        let sheet = StyleSheet::parse("p { color: rgb(200,100,50); }");
        let mut root = Node::Element(parent_el);
        apply_cascade(&mut root, &[sheet]);
        if let Node::Element(parent) = &root {
            assert_eq!(parent.style.color, [200, 100, 50]);
            if let Some(Node::Element(child)) = parent.children.first() {
                assert_eq!(child.style.color, [200, 100, 50],
                    "child should inherit color from parent");
            } else {
                panic!("expected child element");
            }
        }
    }

    #[test]
    fn cascade_inherit_keyword() {
        let child_el = make_el("span", "", "");
        let mut parent_el = make_el("p", "", "");
        parent_el.children.push(Node::Element(child_el));
        let sheet = StyleSheet::parse(
            "p { font-weight: bold; } span { font-weight: inherit; }"
        );
        let mut root = Node::Element(parent_el);
        apply_cascade(&mut root, &[sheet]);
        if let Node::Element(parent) = &root {
            assert!(parent.style.bold, "parent should be bold");
            if let Some(Node::Element(child)) = parent.children.first() {
                assert!(child.style.bold, "child with `inherit` should be bold");
            } else {
                panic!("expected child element");
            }
        }
    }

    #[test]
    fn cascade_initial_keyword() {
        let mut root_el = make_el("p", "", "");
        root_el.style.bold = true;
        let sheet = StyleSheet::parse("p { font-weight: initial; }");
        let mut root = Node::Element(root_el);
        apply_cascade(&mut root, &[sheet]);
        if let Node::Element(el) = &root {
            assert!(!el.style.bold, "`initial` should reset bold to false");
        }
    }

    #[test]
    fn cascade_empty_sheets() {
        let root_el = make_el("div", "", "");
        let default_color = root_el.style.color;
        let mut root = Node::Element(root_el);
        apply_cascade(&mut root, &[]);
        if let Node::Element(el) = &root {
            assert_eq!(el.style.color, default_color);
        }
    }

    #[test]
    fn cascade_bg_alpha_preserved() {
        let root_el = make_el("div", "", "pass");
        let sheet = StyleSheet::parse(".pass { background-color: rgba(30, 160, 80, 0.1); }");
        let mut root = Node::Element(root_el);
        apply_cascade(&mut root, &[sheet]);
        if let Node::Element(el) = &root {
            assert_eq!(el.style.bg_color, Some([30, 160, 80]),
                "bg_color should be set to the rgba rgb components");
            assert_eq!(el.style.bg_alpha, 26,
                "bg_alpha should be 26 (0.1 * 255 rounded), not 255");
        }
    }

    #[test]
    fn inline_bg_alpha_preserved() {
        let root_el = make_el_inline("div", "background-color: rgba(255, 165, 0, 0.25)");
        let mut root = Node::Element(root_el);
        apply_cascade(&mut root, &[]);
        if let Node::Element(el) = &root {
            assert_eq!(el.style.bg_color, Some([255, 165, 0]));
            assert_eq!(el.style.bg_alpha, 64,
                "bg_alpha should be 64 (0.25 * 255 rounded), not 255");
        }
    }

    // ── New pseudo-class tests ────────────────────────────────────────────────

    #[test]
    fn pseudo_hover_matches_when_hovered() {
        let e = el("a", "", "");
        let ptr = &e as *const Element as usize;
        let state = PseudoState {
            hovered_path: vec![],
            hovered_ptr:  Some(ptr),
            focused_ptr:  None,
            checked_ptrs: vec![],
        };
        let sel = Selector::Compound(vec![
            SimpleSelector::Tag("a".into()),
            SimpleSelector::Pseudo(PseudoClass::Hover),
        ]);
        assert!(matches_with_state(&e, &sel, &[], &state));
    }

    #[test]
    fn pseudo_hover_no_match_when_not_hovered() {
        let e = el("a", "", "");
        let state = PseudoState::none();
        let sel = Selector::Compound(vec![
            SimpleSelector::Tag("a".into()),
            SimpleSelector::Pseudo(PseudoClass::Hover),
        ]);
        assert!(!matches_with_state(&e, &sel, &[], &state));
    }

    #[test]
    fn pseudo_first_child_matches() {
        let e1 = make_el("li", "", "");
        let e2 = make_el("li", "", "");
        let children = vec![Node::Element(e1), Node::Element(e2)];
        if let Node::Element(first_li) = &children[0] {
            let sib = SiblingCtx { siblings: &children, index: 0 };
            assert!(matches_pseudo(first_li, &PseudoClass::FirstChild, &[], &PseudoState::none(), Some(&sib)));
        }
    }

    #[test]
    fn pseudo_first_child_no_match_second() {
        let e1 = make_el("li", "", "");
        let e2 = make_el("li", "", "");
        let children = vec![Node::Element(e1), Node::Element(e2)];
        if let Node::Element(second_li) = &children[1] {
            let sib = SiblingCtx { siblings: &children, index: 1 };
            assert!(!matches_pseudo(second_li, &PseudoClass::FirstChild, &[], &PseudoState::none(), Some(&sib)));
        }
    }

    #[test]
    fn pseudo_nth_child_odd() {
        let e1 = make_el("li", "", "");
        let e2 = make_el("li", "", "");
        let e3 = make_el("li", "", "");
        let children = vec![
            Node::Element(e1),
            Node::Element(e2),
            Node::Element(e3),
        ];
        // :nth-child(odd) = (2, 1) — positions 1 and 3
        if let Node::Element(li1) = &children[0] {
            let sib = SiblingCtx { siblings: &children, index: 0 };
            assert!(matches_pseudo(li1, &PseudoClass::NthChild(2, 1), &[], &PseudoState::none(), Some(&sib)));
        }
        if let Node::Element(li2) = &children[1] {
            let sib = SiblingCtx { siblings: &children, index: 1 };
            assert!(!matches_pseudo(li2, &PseudoClass::NthChild(2, 1), &[], &PseudoState::none(), Some(&sib)));
        }
        if let Node::Element(li3) = &children[2] {
            let sib = SiblingCtx { siblings: &children, index: 2 };
            assert!(matches_pseudo(li3, &PseudoClass::NthChild(2, 1), &[], &PseudoState::none(), Some(&sib)));
        }
    }

    #[test]
    fn pseudo_not() {
        let e = el("p", "", "foo");
        // :not(.foo) — p.foo should NOT match :not(.foo)
        let pc = PseudoClass::Not(Box::new(SimpleSelector::Class("foo".into())));
        assert!(!matches_pseudo(&e, &pc, &[], &PseudoState::none(), None));

        // :not(.bar) — p.foo SHOULD match :not(.bar)
        let pc2 = PseudoClass::Not(Box::new(SimpleSelector::Class("bar".into())));
        assert!(matches_pseudo(&e, &pc2, &[], &PseudoState::none(), None));
    }

    #[test]
    fn attr_contains_word_match() {
        let e = el_with_attrs("div", "", "", r#"class="foo bar baz""#);
        let sel = Selector::Compound(vec![
            SimpleSelector::AttrContainsWord("class".into(), "bar".into()),
        ]);
        assert!(matches(&e, &sel, &[]));
    }

    #[test]
    fn attr_starts_with_match() {
        let e = el_with_attrs("a", "", "", r#"href="https://example.com""#);
        let sel = Selector::Compound(vec![
            SimpleSelector::AttrStartsWith("href".into(), "https".into()),
        ]);
        assert!(matches(&e, &sel, &[]));
    }

    #[test]
    fn attr_ends_with_match() {
        let e = el_with_attrs("a", "", "", r#"href="document.pdf""#);
        let sel = Selector::Compound(vec![
            SimpleSelector::AttrEndsWith("href".into(), ".pdf".into()),
        ]);
        assert!(matches(&e, &sel, &[]));
    }

    #[test]
    fn attr_contains_sub_match() {
        let e = el_with_attrs("a", "", "", r#"href="https://example.com/page""#);
        let sel = Selector::Compound(vec![
            SimpleSelector::AttrContains("href".into(), "example".into()),
        ]);
        assert!(matches(&e, &sel, &[]));
    }

    #[test]
    fn nth_matches_pure_integer() {
        // :nth-child(3) matches only position 3
        assert!( nth_matches(0, 3, 3));
        assert!(!nth_matches(0, 3, 1));
        assert!(!nth_matches(0, 3, 2));
    }

    #[test]
    fn nth_matches_even() {
        // :nth-child(even) = (2, 0)
        assert!(!nth_matches(2, 0, 1));
        assert!( nth_matches(2, 0, 2));
        assert!(!nth_matches(2, 0, 3));
        assert!( nth_matches(2, 0, 4));
    }
}
