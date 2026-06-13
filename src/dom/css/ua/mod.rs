mod display;
mod forms;
mod lists;
mod media;
mod structure;
mod tables;
mod text;

use structure::SIZED_TAGS;

/// Apply browser UA-stylesheet defaults for the given element's tag.
pub fn apply_tag_defaults(el: &mut crate::dom::node::Element) {
    let t = el.tag.clone();
    let tag = t.as_str();

    display::apply_display(el);

    if tag == "a" {
        if let Some(href) = crate::dom::parser::get_attr(&el.attrs_raw, "href") {
            el.style.href = Some(href.to_owned());
        }
    }

    if SIZED_TAGS.contains(&tag) {
        if let Some(w) = crate::dom::parser::get_attr(&el.attrs_raw, "width") {
            if let Ok(n) = w.trim_end_matches("px").parse::<i32>() {
                el.style.size.width = Some(n);
            }
        }
        if let Some(h) = crate::dom::parser::get_attr(&el.attrs_raw, "height") {
            if let Ok(n) = h.trim_end_matches("px").parse::<i32>() {
                el.style.size.height = Some(n);
            }
        }
    }

    {
        let s = &mut el.style;
        text::apply_text_defaults(tag, s);
        lists::apply_list_defaults(tag, s);
        tables::apply_table_defaults(tag, s);
        media::apply_media_defaults(tag, s);
        structure::apply_structure_defaults(tag, s);
    }

    forms::apply_form_defaults(tag, el);
}
