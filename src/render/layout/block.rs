use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::pixels::Color;
use crate::html5::node::Element;
use crate::render::font::FontCache;
use crate::render::image::{ImageCache, preprocess_svg, sniff_image_type};
use crate::html5::parser::get_attr;
use crate::css::{self};
use super::LayoutState;

// ──────────────────────────────────────────────────────────────────────────────
// Image / SVG rendering helper
// ──────────────────────────────────────────────────────────────────────────────

/// Render image bytes at the current cursor position.
///
/// `explicit_w` / `explicit_h` come from the element's `width` / `height`
/// attributes or CSS.  When only one dimension is given the other is scaled
/// proportionally.  When neither is given a sensible maximum width is used.
///
/// Returns the (drawn_w, drawn_h) on success so the caller can advance the
/// cursor.
fn paint_image(
    state:      &mut LayoutState,
    canvas:     &mut sdl2::render::Canvas<Window>,
    tc:         &sdl2::render::TextureCreator<WindowContext>,
    bytes:      &[u8],
    explicit_w: Option<i32>,
    explicit_h: Option<i32>,
    max_w_prop: Option<i32>,
    max_h_prop: Option<i32>,
    max_w:      i32,
) -> Option<(i32, i32)> {
    use sdl2::rwops::RWops;
    use sdl2::image::ImageRWops;

    // Detect format; SVG needs to be pre-processed before decoding
    let img_type = sniff_image_type(bytes);
    let owned_bytes: Vec<u8>;
    let bytes_to_load: &[u8] = if img_type == "SVG" {
        let (processed, _) = preprocess_svg(bytes);
        owned_bytes = processed;
        &owned_bytes
    } else {
        bytes
    };

    let rw = RWops::from_bytes(bytes_to_load).ok()?;
    let surface = rw.load_typed(img_type).ok()?;

    let nat_w = surface.width()  as i32;
    let nat_h = surface.height() as i32;
    if nat_w == 0 || nat_h == 0 {
        return None;
    }

    let (mut disp_w, mut disp_h) = match (explicit_w, explicit_h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None)    => {
            let h = (nat_h as f32 * w as f32 / nat_w as f32) as i32;
            (w, h.max(1))
        }
        (None, Some(h))    => {
            let w = (nat_w as f32 * h as f32 / nat_h as f32) as i32;
            (w.max(1), h)
        }
        (None, None) => (nat_w, nat_h),
    };

    // 2. Constrain by max_h_prop (preserve aspect ratio if explicit_w is None)
    if let Some(mh) = max_h_prop {
        if disp_h > mh {
            if explicit_w.is_none() {
                disp_w = (disp_w as f32 * mh as f32 / disp_h as f32) as i32;
                disp_w = disp_w.max(1);
            }
            disp_h = mh;
        }
    }

    // 3. Constrain by max_w_prop (preserve aspect ratio if explicit_h is None)
    if let Some(mw) = max_w_prop {
        if disp_w > mw {
            if explicit_h.is_none() && max_h_prop.is_none() {
                disp_h = (disp_h as f32 * mw as f32 / disp_w as f32) as i32;
                disp_h = disp_h.max(1);
            }
            disp_w = mw;
        }
    }

    // 4. Fit within available space (preserve aspect ratio)
    let avail = (max_w - state.cursor_x).max(1);
    if explicit_w.is_none() && max_w_prop.is_none() && disp_w > avail {
        if explicit_h.is_none() {
            disp_h = (disp_h as f32 * avail as f32 / disp_w as f32) as i32;
            disp_h = disp_h.max(1);
        }
        disp_w = avail;
    }

    if state.paint {
        let mut texture = tc.create_texture_from_surface(&surface).ok()?;
        if state.current_opacity < 1.0 {
            let alpha = (state.current_opacity * 255.0).round() as u8;
            texture.set_alpha_mod(alpha);
            texture.set_blend_mode(sdl2::render::BlendMode::Blend);
        }
        let dst = sdl2::rect::Rect::new(
            state.cursor_x,
            state.cursor_y - state.ctx.scroll_y,
            disp_w as u32,
            disp_h as u32,
        );
        let _ = canvas.copy(&texture, None, Some(dst));
    }

    Some((disp_w, disp_h))
}

/// Parse a dimension attribute value like "200", "200px", or "50%".
/// Percent is resolved against `reference`.
fn parse_dim_attr(val: &str, reference: i32) -> Option<i32> {
    let v = val.trim();
    if let Some(px) = v.strip_suffix("px") {
        return px.trim().parse::<f32>().ok().map(|f| f as i32);
    }
    if let Some(pct) = v.strip_suffix('%') {
        return pct.trim().parse::<f32>().ok().map(|f| (f / 100.0 * reference as f32) as i32);
    }
    v.parse::<f32>().ok().map(|f| f as i32)
}

fn layout_grid(
    state: &mut LayoutState,
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    fonts: &mut FontCache,
    images: &mut ImageCache,
    base_url: &str,
    el: &Element,
    max_w: i32,
    ancestors: &[&Element],
) {
    let cols = state.grid_columns.max(1);
    let gap = if state.grid_gap > 0 { state.grid_gap } else { 12 };
    let avail_grid = max_w - state.cursor_x;
    let actual_grid_w = if let Some(w) = state.fixed_width {
        w.min(avail_grid)
    } else {
        avail_grid
    };
    let grid_start_x = if state.margin_left == crate::render::layout::state::Margin::Auto
        && state.margin_right == crate::render::layout::state::Margin::Auto
    {
        state.cursor_x + (avail_grid - actual_grid_w).max(0) / 2
    } else {
        state.cursor_x
    };
    let total_w = actual_grid_w;
    let col_w = (total_w - gap * (cols as i32 - 1)).max(1) / cols as i32;

    let el_children: Vec<&crate::html5::node::Node> = el.children.iter().filter(|c| {
        match c {
            crate::html5::node::Node::Text(t) => !t.text.trim().is_empty(),
            crate::html5::node::Node::Element(_) => true,
        }
    }).collect();

    let mut current_chain = ancestors.to_vec();
    current_chain.push(el);

    for chunk in el_children.chunks(cols) {
        let row_top = state.cursor_y;
        let mut row_height = 0;

        for (col_idx, child) in chunk.iter().enumerate() {
            let col_x = grid_start_x + col_idx as i32 * (col_w + gap);
            let col_right = col_x + col_w;
            state.cursor_x = col_x;
            state.line_start_x = col_x;
            state.cursor_y = row_top;
            state.line_height = 16;

            let is_align_center = state.text_align == crate::render::layout::state::TextAlign::Center
                || (if let crate::html5::node::Node::Element(cel) = child {
                    cel.class_name.contains("justify-self-center")
                } else {
                    false
                });

            let is_align_right = if let crate::html5::node::Node::Element(cel) = child {
                cel.class_name.contains("align-right") || cel.class_name.contains("justify-self-end")
            } else {
                false
            };

            if is_align_center {
                let sv_paint = state.paint;
                state.paint = false;
                let start_mx = state.cursor_x;
                let sv_max_x = state.max_x;
                state.max_x = start_mx;
                state.layout_node_with_siblings(canvas, tc, fonts, images, base_url, child, col_right, &current_chain, &[]);
                let child_w = (state.max_x - start_mx).max(1);
                state.max_x = sv_max_x;
                state.paint = sv_paint;

                let push_x = (col_w - child_w).max(0) / 2;
                state.cursor_x = col_x + push_x;
                state.line_start_x = col_x + push_x;
                state.cursor_y = row_top;
                state.line_height = 16;
            } else if is_align_right {
                let sv_paint = state.paint;
                state.paint = false;
                let start_mx = state.cursor_x;
                let sv_max_x = state.max_x;
                state.max_x = start_mx;
                state.layout_node_with_siblings(canvas, tc, fonts, images, base_url, child, col_right, &current_chain, &[]);
                let child_w = (state.max_x - start_mx).max(1);
                state.max_x = sv_max_x;
                state.paint = sv_paint;

                let push_x = (col_w - child_w).max(0);
                state.cursor_x = col_x + push_x;
                state.line_start_x = col_x + push_x;
                state.cursor_y = row_top;
                state.line_height = 16;
            }

            let start_y = state.cursor_y;
            state.layout_node_with_siblings(canvas, tc, fonts, images, base_url, child, col_right, &current_chain, &[]);
            let cell_h = (state.cursor_y - start_y).max(state.line_height);
            row_height = row_height.max(cell_h);
        }

        state.cursor_y = row_top + row_height + gap;
    }

    state.cursor_x = grid_start_x;
    state.line_start_x = grid_start_x;
}

pub fn layout_element(
    state:    &mut LayoutState,
    canvas:   &mut Canvas<Window>,
    tc:       &TextureCreator<WindowContext>,
    fonts:    &mut FontCache,
    images:   &mut ImageCache,
    base_url: &str,
    el:       &Element,
    max_w:    i32,
    ancestors: &[&Element],
    prev_siblings: &[&Element],
) {
    let tag = el.tag.to_lowercase();
    if matches!(tag.as_str(), "head" | "style" | "title" | "meta" | "link") {
        return;
    }
    if tag == "input" && get_attr(&el.attrs_raw, "type").map(|t| t.eq_ignore_ascii_case("hidden")).unwrap_or(false) {
        return;
    }
    if el.class_name.split_whitespace().any(|c| c == "main-content" || c == "skip-link") {
        return;
    }

    // Table elements get their own layout path
    if tag == "table" {
        layout_table(state, canvas, tc, fonts, images, base_url, el, max_w, ancestors);
        return;
    }
    // thead/tbody/tfoot/tr/td/th are rendered by layout_table; skip if encountered standalone
    if matches!(tag.as_str(), "thead" | "tbody" | "tfoot" | "tr" | "td" | "th") {
        return;
    }

    // ── <img> ────────────────────────────────────────────────────────────────
    // Handle before any state mutation so cursor advances happen exactly once
    // regardless of whether the parent runs a two-pass (measure + paint) loop.
    if tag == "img" {
        let mut src = get_attr(&el.attrs_raw, "src").unwrap_or("").to_owned();
        let mut style_w = get_attr(&el.attrs_raw, "width")
            .and_then(|v| parse_dim_attr(v, max_w - state.cursor_x));
        let mut style_h = get_attr(&el.attrs_raw, "height")
            .and_then(|v| parse_dim_attr(v, state.ctx.viewport_height));
        let mut max_w_prop: Option<i32> = None;
        let mut max_h_prop: Option<i32> = None;
        let mut is_block_img = false;

        let mut current_chain = ancestors.to_vec();
        current_chain.push(el);
        for sheet in &state.stylesheets {
            for rule in &sheet.rules {
                if rule.matches_element(&current_chain, prev_siblings) {
                    for (prop, val) in &rule.properties {
                        let lookup = |var_name: &str| state.css_variables.get(var_name).cloned();
                        let resolved = css::resolve_var(val, &lookup);
                        match prop.as_str() {
                            "content" if src.is_empty() => {
                                let trimmed = resolved.trim();
                                if let Some(url_part) = trimmed.strip_prefix("url(") {
                                    let inner = url_part.trim_end_matches(')').trim().trim_matches(|c| c == '\'' || c == '"');
                                    if !inner.is_empty() {
                                        src = inner.to_string();
                                    }
                                }
                            }
                            "height" => {
                                if style_h.is_none() {
                                    style_h = parse_dim_attr(&resolved, state.ctx.viewport_height);
                                }
                            }
                            "max-height" => {
                                if max_h_prop.is_none() {
                                    max_h_prop = parse_dim_attr(&resolved, state.ctx.viewport_height);
                                }
                            }
                            "width" => {
                                if style_w.is_none() {
                                    style_w = parse_dim_attr(&resolved, max_w - state.cursor_x);
                                }
                            }
                            "max-width" => {
                                if max_w_prop.is_none() {
                                    max_w_prop = parse_dim_attr(&resolved, max_w - state.cursor_x);
                                }
                            }
                            "display" => {
                                if resolved == "block" {
                                    is_block_img = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if let Some(style_raw) = get_attr(&el.attrs_raw, "style") {
            for part in style_raw.split(';') {
                if let Some(colon) = part.find(':') {
                    let k = part[..colon].trim();
                    let v = part[colon + 1..].trim();
                    match k {
                        "width"      => style_w = parse_dim_attr(v, max_w - state.cursor_x),
                        "height"     => style_h = parse_dim_attr(v, state.ctx.viewport_height),
                        "max-width"  => max_w_prop = parse_dim_attr(v, max_w - state.cursor_x),
                        "max-height" => max_h_prop = parse_dim_attr(v, state.ctx.viewport_height),
                        _ => {}
                    }
                }
            }
        }

        if is_block_img && state.cursor_x > state.line_start_x {
            state.cursor_y += state.line_height;
            state.cursor_x = state.line_start_x;
        }

        if !src.is_empty() {
            // If the image is wider than remaining space, wrap to next line first
            let need_w = style_w.or(max_w_prop).unwrap_or(0);
            if need_w > 0 && state.cursor_x + need_w > max_w && state.cursor_x > state.line_start_x {
                state.cursor_y += state.line_height;
                state.cursor_x = state.line_start_x;
            }

            if let Some(bytes) = images.get_bytes(&src, base_url).map(|b| b.to_vec()) {
                if let Some((dw, dh)) = paint_image(state, canvas, tc, &bytes, style_w, style_h, max_w_prop, max_h_prop, max_w) {
                    if state.paint {
                        if let Some(ref l) = state.active_link {
                            state.link_areas.push(crate::render::layout::state::LinkArea {
                                x: state.cursor_x,
                                y: state.cursor_y,
                                w: dw,
                                h: dh,
                                href: l.clone(),
                            });
                        }
                    }
                    state.cursor_x += dw;
                    if dh > state.line_height {
                        state.line_height = dh;
                    }
                    if is_block_img {
                        state.cursor_y += dh;
                        state.cursor_x = state.line_start_x;
                        state.line_height = 16;
                    }
                }
            } else {
                // Fallback: render alt text
                let alt = get_attr(&el.attrs_raw, "alt").unwrap_or("[img]").to_owned();
                if !alt.is_empty() {
                    super::inline::paint_text(state, canvas, tc, fonts, &alt, max_w);
                }
            }
        }
        return;
    }

    // ── <svg> ────────────────────────────────────────────────────────────────
    // Render as a raster image via SDL2_image + nanosvg.
    if tag == "svg" {
        let svg_markup: Option<String> = el.children.iter().find_map(|c| {
            if let crate::html5::node::Node::Text(t) = c {
                if t.text.contains("<svg") || t.text.trim_start().starts_with('<') {
                    return Some(t.text.clone());
                }
            }
            None
        }).or_else(|| {
            // Reconstruct SVG markup from element children if parsed as elements
            let inner = serialize_svg_children(&el.children);
            Some(format!("<svg {}>{inner}</svg>", el.attrs_raw))
        });

        if let Some(markup) = svg_markup {
            let bytes = markup.into_bytes();

            let attr_w = get_attr(&el.attrs_raw, "width")
                .and_then(|v| parse_dim_attr(v, max_w - state.cursor_x));
            let attr_h = get_attr(&el.attrs_raw, "height")
                .and_then(|v| parse_dim_attr(v, state.ctx.viewport_height));

            let mut style_w = attr_w;
            let mut style_h = attr_h;

            if style_w.is_none() || style_h.is_none() {
                if let Some(parent) = ancestors.last() {
                    if let Some(parent_style) = get_attr(&parent.attrs_raw, "style") {
                        for part in parent_style.split(';') {
                            if let Some(colon) = part.find(':') {
                                let k = part[..colon].trim();
                                let v = part[colon + 1..].trim();
                                if k == "width" && style_w.is_none() {
                                    style_w = parse_dim_attr(v, max_w - state.cursor_x);
                                }
                                if k == "height" && style_h.is_none() {
                                    style_h = parse_dim_attr(v, state.ctx.viewport_height);
                                }
                            }
                        }
                    }
                    if style_w.is_none() {
                        if let Some(pw) = get_attr(&parent.attrs_raw, "width").and_then(|v| parse_dim_attr(v, max_w - state.cursor_x)) {
                            style_w = Some(pw);
                        }
                    }
                    if style_h.is_none() {
                        if let Some(ph) = get_attr(&parent.attrs_raw, "height").and_then(|v| parse_dim_attr(v, state.ctx.viewport_height)) {
                            style_h = Some(ph);
                        }
                    }
                }
            }

            if style_w.is_none() && style_h.is_none() {
                if let Some(parent) = ancestors.last() {
                    if matches!(parent.tag.as_str(), "span" | "a" | "button" | "div") {
                        style_w = Some(24);
                        style_h = Some(24);
                    }
                }
            }

            let old_display = state.current_display;
            let old_mt = state.margin_top;
            let old_mb = state.margin_bottom;
            let old_ml = state.margin_left;
            let old_mr = state.margin_right;
            let old_op = state.current_opacity;
            let old_fw = state.fixed_width;
            let old_vis = state.visibility;

            state.current_display = crate::render::layout::state::Display::Inline;
            state.margin_top = 0;
            state.margin_bottom = 0;
            state.margin_left = crate::render::layout::state::Margin::Px(0);
            state.margin_right = crate::render::layout::state::Margin::Px(0);
            state.fixed_width = None;

            // Apply global CSS rules for svg (e.g. svg { display: block; margin: auto; })
            let mut current_chain = ancestors.to_vec();
            current_chain.push(el);
            let mut svg_props = Vec::new();
            for sheet in &state.stylesheets {
                for rule in &sheet.rules {
                    if rule.matches_element(&current_chain, prev_siblings) {
                        for (prop, val) in &rule.properties {
                            svg_props.push((prop.clone(), val.clone()));
                        }
                    }
                }
            }
            for (prop, val) in &svg_props {
                let lookup = |var_name: &str| state.css_variables.get(var_name).cloned();
                let resolved = css::resolve_var(&val, &lookup);
                apply_style_prop(state, &prop, &resolved);
            }

            if let Some(style_raw) = get_attr(&el.attrs_raw, "style") {
                for part in style_raw.split(';') {
                    if let Some(colon) = part.find(':') {
                        let k = part[..colon].trim();
                        let v = part[colon + 1..].trim();
                        let lookup = |var_name: &str| state.css_variables.get(var_name).cloned();
                        let resolved = css::resolve_var(v, &lookup);
                        apply_style_prop(state, k, &resolved);
                        match k {
                            "width"  => style_w = parse_dim_attr(&resolved, max_w - state.cursor_x),
                            "height" => style_h = parse_dim_attr(&resolved, state.ctx.viewport_height),
                            _ => {}
                        }
                    }
                }
            }

            if let Some(w) = state.fixed_width {
                if style_w.is_none() && w < state.ctx.viewport_width {
                    style_w = Some(w);
                }
            }

            // SVG start on its own line if block
            if matches!(state.current_display, crate::render::layout::state::Display::Block | crate::render::layout::state::Display::FlowRoot) || state.cursor_x > state.line_start_x {
                if state.cursor_x > state.line_start_x {
                    state.cursor_y += state.line_height;
                }
                state.cursor_x = state.line_start_x;
            }

            // Margin top (can be negative, e.g. -2.75em, but do not push above window top 0)
            state.cursor_y = (state.cursor_y + state.margin_top).max(8);

            // Margin left / auto centering
            let actual_w = style_w.unwrap_or(44);
            let avail = (max_w - state.line_start_x).max(0);
            let mut ml = state.margin_left.get_px();
            match (state.margin_left, state.margin_right) {
                (crate::render::layout::state::Margin::Auto, crate::render::layout::state::Margin::Auto) => {
                    ml = (avail - actual_w).max(0) / 2;
                }
                _ if state.text_align == crate::render::layout::state::TextAlign::Center => {
                    ml = (avail - actual_w).max(0) / 2;
                }
                _ => {}
            }
            state.cursor_x += ml;

            if state.visibility && state.current_opacity > 0.001 {
                if let Some((_dw, dh)) = paint_image(state, canvas, tc, &bytes, style_w, style_h, None, None, max_w) {
                    state.cursor_y += dh + state.margin_bottom;
                    state.cursor_x = state.line_start_x;
                    state.last_margin_bottom = state.margin_bottom;
                }
            }

            state.current_display = old_display;
            state.margin_top = old_mt;
            state.margin_bottom = old_mb;
            state.margin_left = old_ml;
            state.margin_right = old_mr;
            state.current_opacity = old_op;
            state.fixed_width = old_fw;
            state.visibility = old_vis;
        }
        return;
    }

    let is_block_tag = matches!(tag.as_str(), 
        "div" | "p" | "h1" | "h2" | "h3" | "ul" | "li" | "ol" | "dl" | "dt" | "dd" | 
        "body" | "html" | "header" | "footer" | "section" | "nav" | "article" | "aside" | "main" | 
        "figure" | "figcaption" | "blockquote" | "pre"
    );
    let is_inline_block_tag = matches!(tag.as_str(), "button" | "input");

    let old_display = state.current_display;
    state.current_display = if is_block_tag { 
        crate::render::layout::state::Display::Block 
    } else if is_inline_block_tag {
        crate::render::layout::state::Display::InlineBlock
    } else { 
        crate::render::layout::state::Display::Inline 
    };

    let old_link = state.active_link.clone();
    let old_color = state.current_color;
    let old_bg = state.current_bg_color;
    let old_font_size = state.current_font_size;
    let old_bold = state.current_bold;
    let old_italic = state.current_italic;
    let mut old_line_height = state.line_height;
    let old_transform = state.current_text_transform;
    let old_opacity = state.current_opacity;
    let old_border_radius = state.current_border_radius;
    let old_font_family = state.current_font_family.clone();
    let old_padding_top = state.padding_top;
    let old_padding_bottom = state.padding_bottom;
    let old_padding_left = state.padding_left;
    let old_padding_right = state.padding_right;
    let old_margin_top = state.margin_top;
    let old_margin_bottom = state.margin_bottom;
    let old_margin_left = state.margin_left;
    let old_margin_right = state.margin_right;
    let old_fixed_width = state.fixed_width;
    let old_line_start_x = state.line_start_x;
    let old_text_align = state.text_align;
    let old_css_variables = state.css_variables.clone();
    let old_float = state.current_float;
    let old_list_style = state.list_style;
    let old_underline = state.text_decoration_underline;
    let old_visibility = state.visibility;
    let old_paint = state.paint;

    // Reset non-inherited box-model properties for the current element
    state.current_bg_color = None;
    state.current_border_radius = 0;
    state.padding_top = 0;
    state.padding_bottom = 0;
    state.padding_left = 0;
    state.padding_right = 0;
    state.margin_top = 0;
    state.margin_bottom = 0;
    state.margin_left = crate::render::layout::state::Margin::Px(0);
    state.margin_right = crate::render::layout::state::Margin::Px(0);
    state.fixed_width = None;
    state.current_float = crate::render::layout::state::Float::None;

    // Tag-specific defaults for "exact look as chrome"
    match tag.as_str() {
        "body" => {
            state.margin_top = 8;
            state.margin_bottom = 8;
            state.margin_left = crate::render::layout::state::Margin::Px(8);
            state.margin_right = crate::render::layout::state::Margin::Px(8);
        },
        "a" => {
            state.current_color = [0, 60, 200, 255];
            state.text_decoration_underline = true;
        },
        "h1" => {
            state.current_font_size = 24;
            state.current_bold = true;
            state.margin_top = 16;
            state.margin_bottom = 16;
        },
        "h2" => {
            state.current_font_size = 20;
            state.current_bold = true;
            state.margin_top = 14;
            state.margin_bottom = 14;
        },
        "h3" => {
            state.current_font_size = 16;
            state.current_bold = true;
            state.margin_top = 12;
            state.margin_bottom = 12;
        },
        "p" => {
            state.margin_top = 10;
            state.margin_bottom = 10;
        },
        "button" | "input" => {
            state.current_bg_color = Some([239, 239, 239, 255]);
            state.current_border_radius = 4;
            state.padding_left = 12;
            state.padding_right = 12;
            state.padding_top = 4;
            state.padding_bottom = 4;
            state.margin_left = crate::render::layout::state::Margin::Px(2);
            state.margin_right = crate::render::layout::state::Margin::Px(2);
        },
        "b" | "strong" => {
            state.current_bold = true;
        },
        "i" | "em" => {
            state.current_italic = true;
        },
        _ => {
            // font-size inherits from parent — do not reset it here
        }
    }

    // 1. Apply global styles
    state.line_height = (state.current_font_size as f32 * 1.2) as i32;
    let mut current_chain = ancestors.to_vec();
    current_chain.push(el);

    let mut props_to_apply = Vec::new();
    let mut before_content: Option<String> = None;
    for sheet in &state.stylesheets {
        for rule in &sheet.rules {
            if rule.is_before {
                if rule.matches_before(&current_chain, prev_siblings) {
                    for (prop, val) in &rule.properties {
                        if prop == "content" {
                            let text = val.trim().trim_matches(|c| c == '"' || c == '\'');
                            if !text.is_empty() {
                                before_content = Some(text.to_string());
                            }
                        }
                    }
                }
            } else if rule.matches_element(&current_chain, prev_siblings) {
                for (prop, val) in &rule.properties {
                    if prop.starts_with("--") {
                        state.css_variables.insert(prop.clone(), val.clone());
                    } else {
                        props_to_apply.push((prop.clone(), val.clone()));
                    }
                }
            }
        }
    }

    // 2. Process inline styles (including custom properties --var)
    let mut inline_props = Vec::new();
    if let Some(style_raw) = get_attr(&el.attrs_raw, "style") {
        for part in style_raw.split(';') {
            // Split on first ':' only — allows values like oklch(...) or urls
            if let Some(colon_pos) = part.find(':') {
                let k = part[..colon_pos].trim();
                let v = part[colon_pos + 1..].trim();
                if !k.is_empty() && !v.is_empty() {
                    if k.starts_with("--") {
                        state.css_variables.insert(k.to_string(), v.to_string());
                    } else {
                        inline_props.push((k.to_lowercase(), v.to_string()));
                    }
                }
            }
        }
    }

    // Apply global properties resolved with current CSS variables
    for (prop, val) in props_to_apply {
        let lookup = |var_name: &str| state.css_variables.get(var_name).cloned();
        let resolved = css::resolve_var(&val, &lookup);
        apply_style_prop(state, &prop, &resolved);
    }

    // Apply inline properties resolved with current CSS variables
    for (prop, val) in inline_props {
        let lookup = |var_name: &str| state.css_variables.get(var_name).cloned();
        let resolved = css::resolve_var(&val, &lookup);
        apply_style_prop(state, &prop, &resolved);
    }

    if let Some(align) = get_attr(&el.attrs_raw, "align") {
        match align.to_ascii_lowercase().as_str() {
            "center" => state.text_align = crate::render::layout::state::TextAlign::Center,
            "right" => state.text_align = crate::render::layout::state::TextAlign::Right,
            "left" => state.text_align = crate::render::layout::state::TextAlign::Left,
            _ => {}
        }
    }

    if tag == "html" {
        state.root_font_size = state.current_font_size as f32;
    }

    let is_block = matches!(state.current_display, crate::render::layout::state::Display::Block | crate::render::layout::state::Display::FlowRoot);
    let is_inline_block = state.current_display == crate::render::layout::state::Display::InlineBlock;
    let is_grid = state.current_display == crate::render::layout::state::Display::Grid;
    let is_hidden = state.current_display == crate::render::layout::state::Display::None;

    if is_hidden {
        state.current_display = old_display;
        state.current_float = old_float;
        state.list_style = old_list_style;
        state.active_link = old_link;
        state.current_color = old_color;
        state.current_bg_color = old_bg;
        state.current_font_size = old_font_size;
        state.current_bold = old_bold;
        state.current_italic = old_italic;
        state.line_height = old_line_height;
        state.current_text_transform = old_transform;
        state.current_opacity = old_opacity;
        state.current_border_radius = old_border_radius;
        state.current_font_family = old_font_family;
        state.padding_top = old_padding_top;
        state.padding_bottom = old_padding_bottom;
        state.padding_left = old_padding_left;
        state.padding_right = old_padding_right;
        state.margin_top = old_margin_top;
        state.margin_bottom = old_margin_bottom;
        state.margin_left = old_margin_left;
        state.margin_right = old_margin_right;
        state.fixed_width = old_fixed_width;
        state.line_start_x = old_line_start_x;
        state.text_align = old_text_align;
        state.css_variables = old_css_variables;
        state.text_decoration_underline = old_underline;
        state.visibility = old_visibility;
        state.paint = old_paint;
        return;
    }

    if is_grid {
        let sv_paint = state.paint;
        if !state.visibility || state.current_opacity <= 0.001 {
            state.paint = false;
        }
        layout_grid(state, canvas, tc, fonts, images, base_url, el, max_w, ancestors);
        state.paint = sv_paint;
        state.current_display = old_display;
        state.current_float = old_float;
        state.list_style = old_list_style;
        state.active_link = old_link;
        state.current_color = old_color;
        state.current_bg_color = old_bg;
        state.current_font_size = old_font_size;
        state.current_bold = old_bold;
        state.current_italic = old_italic;
        state.line_height = old_line_height;
        state.current_text_transform = old_transform;
        state.current_opacity = old_opacity;
        state.current_border_radius = old_border_radius;
        state.current_font_family = old_font_family;
        state.padding_top = old_padding_top;
        state.padding_bottom = old_padding_bottom;
        state.padding_left = old_padding_left;
        state.padding_right = old_padding_right;
        state.margin_top = old_margin_top;
        state.margin_bottom = old_margin_bottom;
        state.margin_left = old_margin_left;
        state.margin_right = old_margin_right;
        state.fixed_width = old_fixed_width;
        state.line_start_x = old_line_start_x;
        state.text_align = old_text_align;
        state.css_variables = old_css_variables;
        state.text_decoration_underline = old_underline;
        state.visibility = old_visibility;
        state.paint = old_paint;
        return;
    }

    let is_inline_box = !is_block && (is_inline_block || state.current_bg_color.is_some());

    // Inline elements must not carry a fixed_width inherited from an ancestor block.
    // Reset it so child_max_w and background-width calculations are correct.
    if !is_block && !is_inline_block {
        state.fixed_width = None;
    }

    if matches!(tag.as_str(), "h1" | "h2" | "h3" | "footer") {
        if state.cursor_y < state.float_bottom {
            state.cursor_y = state.float_bottom;
        }
        state.float_bottom = 0;
        state.float_right_w = 0;
    }

    let is_float_right = state.current_float == crate::render::layout::state::Float::Right;
    let is_float_left = state.current_float == crate::render::layout::state::Float::Left;

    let is_bfc = (is_block || is_inline_block)
        && !is_float_right
        && !is_float_left
        && (state.current_display == crate::render::layout::state::Display::FlowRoot
            || matches!(tag.as_str(), "header" | "footer")
            || (state.current_bg_color.is_some() && !matches!(tag.as_str(), "body" | "html")));

    let old_float_bottom = state.float_bottom;
    let old_float_right_w = state.float_right_w;
    if is_bfc {
        state.float_bottom = 0;
        state.float_right_w = 0;
    }

    // Ensure blocks start on a new line (inline-block only wraps if exceeding max_w)
    if is_block && state.current_float == crate::render::layout::state::Float::None && state.cursor_x > state.line_start_x {
        state.cursor_y += state.line_height;
        state.cursor_x = state.line_start_x;
    }

    if tag == "a" && !el.href.is_empty() {
        state.active_link = Some(el.href.clone());
    }

    if tag == "br" {
        state.cursor_y += state.line_height;
        state.cursor_x = state.line_start_x;
        state.current_display = old_display;
        state.current_float = old_float;
        state.list_style = old_list_style;
        state.active_link = old_link;
        state.current_color = old_color;
        state.current_bg_color = old_bg;
        state.current_font_size = old_font_size;
        state.current_bold = old_bold;
        state.current_italic = old_italic;
        state.line_height = old_line_height;
        state.current_text_transform = old_transform;
        state.current_opacity = old_opacity;
        state.current_border_radius = old_border_radius;
        state.current_font_family = old_font_family;
        state.padding_top = old_padding_top;
        state.padding_bottom = old_padding_bottom;
        state.padding_left = old_padding_left;
        state.padding_right = old_padding_right;
        state.margin_top = old_margin_top;
        state.margin_bottom = old_margin_bottom;
        state.margin_left = old_margin_left;
        state.margin_right = old_margin_right;
        state.fixed_width = old_fixed_width;
        state.line_start_x = old_line_start_x;
        state.text_align = old_text_align;
        state.css_variables = old_css_variables;
        state.text_decoration_underline = old_underline;
        state.visibility = old_visibility;
        state.paint = old_paint;
        return;
    }

    let is_empty_block = is_block
        && before_content.is_none()
        && state.current_bg_color.is_none()
        && state.padding_top == 0
        && state.padding_bottom == 0
        && el.children.iter().all(|child| match child {
            crate::html5::node::Node::Text(t) => t.text.trim().is_empty(),
            crate::html5::node::Node::Element(_) => false,
        });

    if is_empty_block {
        if is_bfc {
            state.float_bottom = old_float_bottom;
            state.float_right_w = old_float_right_w;
        }
        state.current_display = old_display;
        state.current_float = old_float;
        state.list_style = old_list_style;
        state.active_link = old_link;
        state.current_color = old_color;
        state.current_bg_color = old_bg;
        state.current_font_size = old_font_size;
        state.current_bold = old_bold;
        state.current_italic = old_italic;
        state.line_height = old_line_height;
        state.current_text_transform = old_transform;
        state.current_opacity = old_opacity;
        state.current_border_radius = old_border_radius;
        state.current_font_family = old_font_family;
        state.fixed_width = old_fixed_width;
        state.padding_top = old_padding_top;
        state.padding_bottom = old_padding_bottom;
        state.padding_left = old_padding_left;
        state.padding_right = old_padding_right;
        state.margin_top = old_margin_top;
        state.margin_bottom = old_margin_bottom;
        state.margin_left = old_margin_left;
        state.margin_right = old_margin_right;
        state.line_start_x = old_line_start_x;
        state.text_align = old_text_align;
        state.css_variables = old_css_variables;
        state.text_decoration_underline = old_underline;
        state.visibility = old_visibility;
        state.paint = old_paint;
        return;
    }

    let initial_paint = state.paint && state.visibility && state.current_opacity > 0.001;
    
    // Calculate actual pixel values for margins (supporting 'auto')
    let mut ml = state.margin_left.get_px();
    let mut mr = state.margin_right.get_px();

    if is_block || is_inline_block {
        if let Some(w) = state.fixed_width {
            let avail = max_w - state.line_start_x;
            match (state.margin_left, state.margin_right) {
                (crate::render::layout::state::Margin::Auto, crate::render::layout::state::Margin::Auto) => {
                    ml = (avail - w).max(0) / 2;
                    mr = ml;
                }
                (crate::render::layout::state::Margin::Auto, _) => {
                    ml = (avail - w - mr).max(0);
                }
                (_, crate::render::layout::state::Margin::Auto) => {
                    mr = (avail - w - ml).max(0);
                }
                _ => {}
            }
        }
    }

    // Margin Collapsing (simple, only for blocks)
    if is_block {
        let gap = std::cmp::max(state.last_margin_bottom, state.margin_top);
        state.cursor_y += gap;
        state.last_margin_bottom = 0; // Reset after use
        state.cursor_x += ml;
    } else if is_inline_block {
        state.cursor_x += ml;
        state.cursor_y += state.margin_top;
    } else {
        state.cursor_x += ml + state.padding_left;
    }

    let bg_start_y = state.cursor_y;
    let bg_start_x = state.cursor_x;

    // Apply Padding
    if is_block || is_inline_block || is_inline_box {
        state.cursor_y += state.padding_top;
        state.cursor_x += state.padding_left;
    }

    let inner_start_y = state.cursor_y;
    let inner_start_x = state.cursor_x;
    let inner_line_height = state.line_height;
    if is_block || is_inline_block || is_inline_box {
        state.line_start_x = inner_start_x;
    }

    let pr = if is_block || is_inline_block || is_inline_box { state.padding_right } else { 0 };
    let mut child_max_w = if is_block || is_inline_block || is_inline_box {
        if let Some(w) = state.fixed_width {
            (bg_start_x + w - pr).min(max_w - mr - pr)
        } else {
            max_w - mr - pr
        }
    } else {
        max_w
    };
    if state.cursor_y < state.float_bottom && state.float_right_w > 0 && state.current_float == crate::render::layout::state::Float::None {
        child_max_w = (child_max_w - state.float_right_w).max(100);
    } else if state.cursor_y >= state.float_bottom {
        state.float_right_w = 0;
    }

    // Render list bullet for <li> elements
    if tag == "li" && state.list_style != crate::render::layout::state::ListStyle::None && state.current_display != crate::render::layout::state::Display::Inline {
        if state.paint {
            let bullet_x = state.cursor_x - 14;
            let bullet_y = state.cursor_y + (state.line_height - 6) / 2;
            let col = Color::RGBA(state.current_color[0], state.current_color[1], state.current_color[2], (state.current_color[3] as f32 * state.current_opacity) as u8);
            canvas.set_draw_color(col);
            match state.list_style {
                crate::render::layout::state::ListStyle::Square => {
                    let _ = canvas.fill_rect(sdl2::rect::Rect::new(bullet_x, bullet_y - state.ctx.scroll_y, 5, 5));
                }
                _ => {
                    let _ = canvas.fill_rect(sdl2::rect::Rect::new(bullet_x, bullet_y - state.ctx.scroll_y, 4, 4));
                }
            }
        }
    }

    if (is_block || is_inline_box) && state.current_bg_color.is_some() {
        // Pass 1: Measure height (paint = false)
        let _initial_y = state.cursor_y;
        state.paint = false;
        let sv_max_x = state.max_x;
        state.max_x = inner_start_x;
        let mut content_max_x = state.line_start_x;
        let is_float_right = state.current_float == crate::render::layout::state::Float::Right;
        let is_float_left = state.current_float == crate::render::layout::state::Float::Left;
        let is_shrink_wrap = is_float_right || is_float_left || is_inline_box;
        let sv_text_align = state.text_align;
        if is_shrink_wrap {
            state.text_align = crate::render::layout::state::TextAlign::Left;
        }
        let is_button_like = tag == "button" || (tag == "input" && matches!(get_attr(&el.attrs_raw, "type").unwrap_or("text"), "submit" | "button" | "reset"));
        let input_value_text = if tag == "input" {
            let raw_val = get_attr(&el.attrs_raw, "value").unwrap_or("");
            let placeholder = get_attr(&el.attrs_raw, "placeholder").unwrap_or("");
            if !raw_val.is_empty() {
                raw_val.to_string()
            } else if !placeholder.is_empty() {
                placeholder.to_string()
            } else if is_button_like {
                "Submit".to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if tag == "input" && !input_value_text.is_empty() {
            super::inline::paint_text(state, canvas, tc, fonts, &input_value_text, child_max_w);
            content_max_x = content_max_x.max(state.max_x.max(state.cursor_x));
        } else {
            if let Some(ref bc) = before_content {
                super::inline::paint_text(state, canvas, tc, fonts, bc, child_max_w);
            }
            let mut prev_siblings: Vec<&Element> = Vec::new();
            for child in &el.children {
                state.layout_node_with_siblings(canvas, tc, fonts, images, base_url, child, child_max_w, &current_chain, &prev_siblings);
                content_max_x = content_max_x.max(state.max_x.max(state.cursor_x));
                if let crate::html5::node::Node::Element(cel) = child {
                    prev_siblings.push(cel);
                }
            }
        }
        content_max_x = content_max_x.max(state.max_x);
        state.max_x = sv_max_x.max(state.max_x);
        state.text_align = sv_text_align;
        
        if is_bfc {
            state.cursor_y = state.cursor_y.max(state.float_bottom);
        }
        if state.cursor_x > state.line_start_x {
            state.cursor_y += state.line_height;
            state.cursor_x = state.line_start_x;
        }
        state.cursor_y += state.padding_bottom;
        
        let mut bg_h = state.cursor_y - bg_start_y;

        if tag == "body" || tag == "html" {
            bg_h = bg_h.max(state.ctx.viewport_height + state.ctx.scroll_y);
        } else if tag == "footer" {
            let rem_viewport = (state.ctx.viewport_height + state.ctx.scroll_y) - bg_start_y;
            bg_h = bg_h.max(rem_viewport);
        }

        let (rect_x, rect_w) = if tag == "body" || tag == "html" || tag == "footer" {
            (0, max_w)
        } else if is_float_right {
            let w = (content_max_x - bg_start_x + state.padding_right).max(1);
            (max_w - mr - w, w)
        } else {
            let w = if is_inline_box {
                (content_max_x - bg_start_x + state.padding_right).max(1)
            } else {
                state.fixed_width.unwrap_or(max_w - mr - bg_start_x)
            };
            (bg_start_x, w)
        };

        if initial_paint {
            if let Some(bg) = state.current_bg_color {
                let alpha = (bg[3] as f32 * state.current_opacity) as u8;
                let rect = sdl2::rect::Rect::new(rect_x, bg_start_y - state.ctx.scroll_y, rect_w as u32, bg_h as u32);
                if state.current_border_radius > 10 { println!("PILL_RECT: rect={:?}", rect); }
                fill_rounded_rect(canvas, rect, state.current_border_radius, Color::RGBA(bg[0], bg[1], bg[2], alpha));
                
                // Add a subtle border for buttons if not explicitly styled
                if is_button_like && tag == "button" {
                    let border_col = Color::RGBA(180, 180, 180, alpha);
                    canvas.set_draw_color(border_col);
                    let _ = canvas.draw_rect(rect);
                }
            }

            if tag == "a" && !el.href.is_empty() {
                state.link_areas.push(crate::render::layout::state::LinkArea {
                    x: rect_x,
                    y: bg_start_y,
                    w: rect_w,
                    h: bg_h,
                    href: el.href.clone(),
                });
            }

            if is_button_like {
                let action = match get_attr(&el.attrs_raw, "type").unwrap_or("submit") {
                    "submit" => crate::render::layout::state::ButtonAction::Submit(String::new()),
                    "reset"  => crate::render::layout::state::ButtonAction::Reset,
                    _        => crate::render::layout::state::ButtonAction::None,
                };
                state.button_areas.push(crate::render::layout::state::ButtonArea {
                    x: rect_x,
                    y: bg_start_y,
                    w: rect_w,
                    h: bg_h,
                    action,
                });
            }

            // Register click listener event area for any element with JS click listeners
            for (etype, _) in &el.event_listeners {
                state.event_areas.push(crate::render::layout::state::EventArea {
                    x: rect_x,
                    y: bg_start_y,
                    w: rect_w,
                    h: bg_h,
                    element_ptr: el as *const Element as usize,
                    event_type: etype.clone(),
                });
            }
        }
        
        // Pass 2: Paint (paint = initial_paint)
        state.paint = initial_paint;
        if is_bfc {
            state.float_bottom = 0;
            state.float_right_w = 0;
        }
        let pass2_child_max_w = if is_float_right {
            rect_x + rect_w - state.padding_right
        } else if is_float_left {
            state.line_start_x + rect_w - state.padding_right
        } else {
            child_max_w
        };
        if is_float_right {
            state.cursor_x = rect_x + state.padding_left;
            state.line_start_x = state.cursor_x;
            state.cursor_y = inner_start_y;
        } else if is_inline_box && state.current_border_radius > 0 {
            state.cursor_y = bg_start_y + (bg_h - inner_line_height).max(0) / 2 - 1;
            state.cursor_x = inner_start_x;
            state.line_start_x = inner_start_x;
        } else {
            state.cursor_y = inner_start_y;
            state.cursor_x = inner_start_x;
            state.line_start_x = inner_start_x;
        }
        state.line_height = inner_line_height;
        
        if tag == "input" && !input_value_text.is_empty() {
            let transformed_text = match state.current_text_transform {
                crate::render::layout::state::TextTransform::Capitalize => {
                    let mut c = input_value_text.chars();
                    match c.next() {
                        None => String::new(),
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    }
                }
                crate::render::layout::state::TextTransform::Uppercase => input_value_text.to_uppercase(),
                crate::render::layout::state::TextTransform::Lowercase => input_value_text.to_lowercase(),
                crate::render::layout::state::TextTransform::None => input_value_text.clone(),
            };
            super::inline::paint_text(state, canvas, tc, fonts, &transformed_text, pass2_child_max_w);
        } else {
            if let Some(ref bc) = before_content {
                super::inline::paint_text(state, canvas, tc, fonts, bc, pass2_child_max_w);
            }
            let mut prev_siblings: Vec<&Element> = Vec::new();
            for child in &el.children {
                state.layout_node_with_siblings(canvas, tc, fonts, images, base_url, child, pass2_child_max_w, &current_chain, &prev_siblings);
                if let crate::html5::node::Node::Element(cel) = child {
                    prev_siblings.push(cel);
                }
            }
        }
        
        // Re-calculate final Y to include everything
        if is_float_right {
            state.float_bottom = old_float_bottom.max(bg_start_y + bg_h);
            state.float_right_w = rect_w + mr + 16;
            state.cursor_y = bg_start_y;
            state.cursor_x = old_line_start_x;
            state.line_start_x = old_line_start_x;
        } else if is_inline_box {
            state.cursor_x = rect_x + rect_w + mr + 6;
            state.cursor_y = bg_start_y;
            state.line_height = state.line_height.max(bg_h);
        } else {
            if is_bfc {
                state.cursor_y = state.cursor_y.max(state.float_bottom);
                state.float_bottom = old_float_bottom;
                state.float_right_w = old_float_right_w;
            }
            if state.cursor_x > state.line_start_x {
                state.cursor_y += state.line_height;
                state.cursor_x = state.line_start_x;
            }
            state.cursor_y += state.padding_bottom;
        }

    } else if is_block || is_inline_block {
        let is_float_right = state.current_float == crate::render::layout::state::Float::Right;
        let is_float_left = state.current_float == crate::render::layout::state::Float::Left;
        if is_float_right {
            // Measure width first
            let sv_paint = state.paint;
            state.paint = false;
            let start_x = state.cursor_x;
            let sv_max_x = state.max_x;
            state.max_x = start_x;
            let mut prev_siblings: Vec<&Element> = Vec::new();
            for child in &el.children {
                state.layout_node_with_siblings(canvas, tc, fonts, images, base_url, child, child_max_w, &current_chain, &prev_siblings);
                if let crate::html5::node::Node::Element(cel) = child {
                    prev_siblings.push(cel);
                }
            }
            let measured_w = (state.max_x - start_x).max(1);
            state.max_x = sv_max_x.max(state.max_x);
            state.paint = sv_paint;

            state.cursor_x = child_max_w - measured_w - mr;
            state.line_start_x = state.cursor_x;
            state.cursor_y = bg_start_y;

            if let Some(ref bc) = before_content {
                super::inline::paint_text(state, canvas, tc, fonts, bc, child_max_w);
            }
            let mut prev_siblings: Vec<&Element> = Vec::new();
            for child in &el.children {
                state.layout_node_with_siblings(canvas, tc, fonts, images, base_url, child, child_max_w, &current_chain, &prev_siblings);
                if let crate::html5::node::Node::Element(cel) = child {
                    prev_siblings.push(cel);
                }
            }
            let float_end_y = if state.cursor_x > state.line_start_x {
                state.cursor_y + state.line_height
            } else {
                state.cursor_y
            };
            state.float_bottom = old_float_bottom.max(float_end_y);
            state.float_right_w = measured_w + mr + 16;
            state.cursor_y = bg_start_y;
            state.cursor_x = old_line_start_x;
            state.line_start_x = old_line_start_x;
        } else if is_float_left {
            if let Some(ref bc) = before_content {
                super::inline::paint_text(state, canvas, tc, fonts, bc, child_max_w);
            }
            let mut prev_siblings: Vec<&Element> = Vec::new();
            for child in &el.children {
                state.layout_node_with_siblings(canvas, tc, fonts, images, base_url, child, child_max_w, &current_chain, &prev_siblings);
                if let crate::html5::node::Node::Element(cel) = child {
                    prev_siblings.push(cel);
                }
            }
            let float_end_y = if state.cursor_x > state.line_start_x {
                state.cursor_y + state.line_height
            } else {
                state.cursor_y
            };
            state.float_bottom = old_float_bottom.max(float_end_y);
            state.cursor_y = bg_start_y;
        } else {
            let mut content_max_x = state.cursor_x;
            if let Some(ref bc) = before_content {
                super::inline::paint_text(state, canvas, tc, fonts, bc, child_max_w);
                content_max_x = content_max_x.max(state.cursor_x);
            }
            let mut prev_siblings: Vec<&Element> = Vec::new();
            for child in &el.children {
                state.layout_node_with_siblings(canvas, tc, fonts, images, base_url, child, child_max_w, &current_chain, &prev_siblings);
                content_max_x = content_max_x.max(state.cursor_x);
                if let crate::html5::node::Node::Element(cel) = child {
                    prev_siblings.push(cel);
                }
            }
            if is_inline_block {
                let box_w = (content_max_x - bg_start_x).max(0);
                state.cursor_x = bg_start_x + box_w + state.padding_right + mr;
                state.cursor_y = bg_start_y;
            } else {
                if is_bfc {
                    state.cursor_y = state.cursor_y.max(state.float_bottom);
                    state.float_bottom = old_float_bottom;
                    state.float_right_w = old_float_right_w;
                }
                if state.cursor_x > state.line_start_x {
                    state.cursor_y += state.line_height;
                    state.cursor_x = state.line_start_x;
                }
                state.cursor_y += state.padding_bottom;
            }
        }
    } else {
        if let Some(ref bc) = before_content {
            super::inline::paint_text(state, canvas, tc, fonts, bc, child_max_w);
        }
        // Pure inline elements (like span without inline-block)
        let mut prev_siblings: Vec<&Element> = Vec::new();
        for child in &el.children {
            state.layout_node_with_siblings(canvas, tc, fonts, images, base_url, child, child_max_w, &current_chain, &prev_siblings);
            if let crate::html5::node::Node::Element(cel) = child {
                prev_siblings.push(cel);
            }
        }
        if is_inline_block {
            state.cursor_x += state.padding_right + mr;
        } else {
            state.cursor_x += mr + state.padding_right;
        }
    }

    if is_block {
        state.cursor_x = old_line_start_x;
        state.last_margin_bottom = state.margin_bottom;
    }
    
    if is_inline_block {
        state.line_start_x = old_line_start_x;
    }
    
    state.current_display = old_display;
    state.current_float = old_float;
    state.list_style = old_list_style;
    
    state.active_link = old_link;
    state.current_color = old_color;
    state.current_bg_color = old_bg;
    state.current_font_size = old_font_size;
    state.current_bold = old_bold;
    state.current_italic = old_italic;
    if !is_block {
        old_line_height = old_line_height.max(state.line_height);
    }
    state.line_height = old_line_height;
    state.current_text_transform = old_transform;
    state.current_opacity = old_opacity;
    state.current_border_radius = old_border_radius;
    state.current_font_family = old_font_family;
    state.fixed_width = old_fixed_width;
    state.padding_top = old_padding_top;
    state.padding_bottom = old_padding_bottom;
    state.padding_left = old_padding_left;
    state.padding_right = old_padding_right;
    state.margin_top = old_margin_top;
    state.margin_bottom = old_margin_bottom;
    state.margin_left = old_margin_left;
    state.margin_right = old_margin_right;
    state.line_start_x = old_line_start_x;
    state.text_align = old_text_align;
    state.css_variables = old_css_variables;
    state.text_decoration_underline = old_underline;
    state.visibility = old_visibility;
    state.paint = old_paint;
}

fn parse_simple_unit(val: &str, current_font_size: u16, root_font_size: f32, vh: i32, vw: i32) -> Option<f32> {
    let val = val.trim();
    if let Some(v) = val.strip_suffix("px") {
        v.parse::<f32>().ok()
    } else if let Some(v) = val.strip_suffix("em") {
        v.parse::<f32>().ok().map(|e| e * current_font_size as f32)
    } else if let Some(v) = val.strip_suffix("rem") {
        v.parse::<f32>().ok().map(|r| r * root_font_size)
    } else if let Some(v) = val.strip_suffix("vh") {
        v.parse::<f32>().ok().map(|pct| pct * vh as f32 / 100.0)
    } else if let Some(v) = val.strip_suffix("vw") {
        v.parse::<f32>().ok().map(|pct| pct * vw as f32 / 100.0)
    } else if let Some(v) = val.strip_suffix('%') {
        v.parse::<f32>().ok().map(|pct| pct * vw as f32 / 100.0)
    } else {
        val.parse::<f32>().ok()
    }
}

fn eval_calc(expr: &str, current_font_size: u16, root_font_size: f32, vh: i32, vw: i32) -> Option<f32> {
    let s = expr.trim();
    let inner = if s.starts_with("calc(") && s.ends_with(')') {
        &s[5..s.len() - 1].trim()
    } else {
        s
    };

    #[derive(Debug, PartialEq, Clone)]
    enum Tok {
        Num(f32),
        Plus,
        Minus,
        Mul,
        Div,
        LParen,
        RParen,
    }

    let mut tokens = Vec::new();
    let chars: Vec<char> = inner.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '+' => { tokens.push(Tok::Plus); i += 1; }
            '*' => { tokens.push(Tok::Mul); i += 1; }
            '/' => { tokens.push(Tok::Div); i += 1; }
            '(' => { tokens.push(Tok::LParen); i += 1; }
            ')' => { tokens.push(Tok::RParen); i += 1; }
            '-' => {
                let prev_is_op = tokens.is_empty() || matches!(tokens.last(), Some(Tok::Plus | Tok::Minus | Tok::Mul | Tok::Div | Tok::LParen));
                if prev_is_op && i + 1 < len && (chars[i + 1].is_ascii_digit() || chars[i + 1] == '.') {
                    let start = i;
                    i += 1;
                    while i < len && !chars[i].is_whitespace() && chars[i] != '+' && chars[i] != '-' && chars[i] != '*' && chars[i] != '/' && chars[i] != ')' {
                        i += 1;
                    }
                    let tok_str: String = chars[start..i].iter().collect();
                    let val = parse_simple_unit(&tok_str, current_font_size, root_font_size, vh, vw)?;
                    tokens.push(Tok::Num(val));
                } else {
                    tokens.push(Tok::Minus);
                    i += 1;
                }
            }
            _ => {
                let start = i;
                while i < len && !chars[i].is_whitespace() && chars[i] != '+' && chars[i] != '-' && chars[i] != '*' && chars[i] != '/' && chars[i] != '(' && chars[i] != ')' {
                    i += 1;
                }
                let tok_str: String = chars[start..i].iter().collect();
                let val = parse_simple_unit(&tok_str, current_font_size, root_font_size, vh, vw)?;
                tokens.push(Tok::Num(val));
            }
        }
    }

    struct Parser {
        tokens: Vec<Tok>,
        pos: usize,
    }

    impl Parser {
        fn parse_expr(&mut self) -> Option<f32> {
            let mut left = self.parse_term()?;
            while self.pos < self.tokens.len() {
                match self.tokens[self.pos] {
                    Tok::Plus => {
                        self.pos += 1;
                        let right = self.parse_term()?;
                        left += right;
                    }
                    Tok::Minus => {
                        self.pos += 1;
                        let right = self.parse_term()?;
                        left -= right;
                    }
                    _ => break,
                }
            }
            Some(left)
        }

        fn parse_term(&mut self) -> Option<f32> {
            let mut left = self.parse_factor()?;
            while self.pos < self.tokens.len() {
                match self.tokens[self.pos] {
                    Tok::Mul => {
                        self.pos += 1;
                        let right = self.parse_factor()?;
                        left *= right;
                    }
                    Tok::Div => {
                        self.pos += 1;
                        let right = self.parse_factor()?;
                        if right != 0.0 { left /= right; }
                    }
                    _ => break,
                }
            }
            Some(left)
        }

        fn parse_factor(&mut self) -> Option<f32> {
            if self.pos >= self.tokens.len() { return None; }
            match self.tokens[self.pos] {
                Tok::Num(n) => {
                    self.pos += 1;
                    Some(n)
                }
                Tok::LParen => {
                    self.pos += 1;
                    let val = self.parse_expr()?;
                    if self.pos < self.tokens.len() && self.tokens[self.pos] == Tok::RParen {
                        self.pos += 1;
                    }
                    Some(val)
                }
                Tok::Minus => {
                    self.pos += 1;
                    let val = self.parse_factor()?;
                    Some(-val)
                }
                _ => None,
            }
        }
    }

    let mut p = Parser { tokens, pos: 0 };
    p.parse_expr()
}

fn parse_unit(val: &str, current_font_size: u16, root_font_size: f32, vh: i32, vw: i32) -> Option<f32> {
    let val = val.trim();
    if val.starts_with("calc(") {
        eval_calc(val, current_font_size, root_font_size, vh, vw)
    } else {
        parse_simple_unit(val, current_font_size, root_font_size, vh, vw)
    }
}

fn apply_style_prop(state: &mut LayoutState, prop: &str, val: &str) {
    match prop {
        "text-decoration" | "text-decoration-line" => {
            let lower = val.to_ascii_lowercase();
            if lower.contains("none") {
                state.text_decoration_underline = false;
            } else if lower.contains("underline") {
                state.text_decoration_underline = true;
            }
        }
        "color" => {
            if let Some(css_color) = css::color::CssColor::parse(val) {
                let (r, g, b, a) = css_color.to_rgba8();
                state.current_color = [r, g, b, a];
            }
        }
        "background-color" | "background" => {
            if let Some(css_color) = css::color::CssColor::parse(val) {
                state.current_bg_color = Some(css_color.to_rgba8().into());
            }
        }
        "font" => {
            // e.g. "16px/1.6 system-ui,sans-serif" or "bold 14px Arial"
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            let mut parts = val.split_whitespace().peekable();
            while let Some(part) = parts.next() {
                if part == "bold" {
                    state.current_bold = true;
                } else if part == "italic" {
                    state.current_italic = true;
                } else if let Some(slash_idx) = part.find('/') {
                    let size_str = &part[..slash_idx];
                    let lh_str = &part[slash_idx + 1..];
                    if let Some(pct) = size_str.trim().strip_suffix('%') {
                        if let Ok(p) = pct.trim().parse::<f32>() {
                            state.current_font_size = (state.current_font_size as f32 * p / 100.0).round().max(1.0) as u16;
                        }
                    } else if let Some(sz) = parse_unit(size_str, state.current_font_size, state.root_font_size, vh, vw) {
                        state.current_font_size = sz as u16;
                    }
                    if let Some(pct) = lh_str.trim().strip_suffix('%') {
                        if let Ok(p) = pct.trim().parse::<f32>() {
                            state.line_height = (state.current_font_size as f32 * p / 100.0) as i32;
                        }
                    } else if let Ok(multiplier) = lh_str.parse::<f32>() {
                        state.line_height = (state.current_font_size as f32 * multiplier) as i32;
                    } else if let Some(lh) = parse_unit(lh_str, state.current_font_size, state.root_font_size, vh, vw) {
                        state.line_height = lh as i32;
                    }
                    let rest: Vec<&str> = parts.collect();
                    if !rest.is_empty() {
                        let fam = rest.join(" ");
                        state.current_font_family = parse_font_family(&fam);
                    }
                    break;
                } else if let Some(pct) = part.trim().strip_suffix('%') {
                    if let Ok(p) = pct.trim().parse::<f32>() {
                        state.current_font_size = (state.current_font_size as f32 * p / 100.0).round().max(1.0) as u16;
                    }
                    let rest: Vec<&str> = parts.collect();
                    if !rest.is_empty() {
                        let fam = rest.join(" ");
                        state.current_font_family = parse_font_family(&fam);
                    }
                    break;
                } else if let Some(sz) = parse_unit(part, state.current_font_size, state.root_font_size, vh, vw) {
                    state.current_font_size = sz as u16;
                    let rest: Vec<&str> = parts.collect();
                    if !rest.is_empty() {
                        let fam = rest.join(" ");
                        state.current_font_family = parse_font_family(&fam);
                    }
                    break;
                }
            }
        }
        "font-size" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(pct) = val.trim().strip_suffix('%') {
                if let Ok(p) = pct.trim().parse::<f32>() {
                    state.current_font_size = (state.current_font_size as f32 * p / 100.0).round().max(1.0) as u16;
                }
            } else if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.current_font_size = v as u16;
            }
        }
        "font-weight" => {
            state.current_bold = matches!(val, "bold" | "700" | "800" | "900");
        }
        "border-radius" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.current_border_radius = v as i32;
            }
        }
        "font-style" => {
            state.current_italic = val.eq_ignore_ascii_case("italic");
        }
        "text-align" => {
            state.text_align = match val.trim().to_ascii_lowercase().as_str() {
                "center" => crate::render::layout::state::TextAlign::Center,
                "right" => crate::render::layout::state::TextAlign::Right,
                _ => crate::render::layout::state::TextAlign::Left,
            };
        }
        "justify-items" => {
            if val.trim().eq_ignore_ascii_case("center") {
                state.text_align = crate::render::layout::state::TextAlign::Center;
            }
        }
        "opacity" => {
            if let Some(v) = val.parse::<f32>().ok() {
                state.current_opacity = (state.current_opacity * v).clamp(0.0, 1.0);
            }
        }
        "visibility" => {
            match val.trim().to_ascii_lowercase().as_str() {
                "hidden" | "collapse" => state.visibility = false,
                "visible" => state.visibility = true,
                _ => {}
            }
        }
        "text-transform" => {
            state.current_text_transform = match val {
                "uppercase"  => crate::render::layout::state::TextTransform::Uppercase,
                "lowercase"  => crate::render::layout::state::TextTransform::Lowercase,
                "capitalize" => crate::render::layout::state::TextTransform::Capitalize,
                _ => crate::render::layout::state::TextTransform::None,
            };
        }
        "padding" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            // CSS shorthand: 1 value | 2 values (vertical horizontal) | 3 values (top h bottom) | 4 values (top right bottom left)
            let parts: Vec<&str> = val.split_whitespace().collect();
            match parts.len() {
                1 => {
                    if let Some(v) = parse_unit(parts[0], state.current_font_size, state.root_font_size, vh, vw) {
                        let v = v as i32;
                        state.padding_top = v; state.padding_bottom = v;
                        state.padding_left = v; state.padding_right = v;
                    }
                }
                2 => {
                    // vertical | horizontal
                    let vv = parse_unit(parts[0], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    let vh_val = parse_unit(parts[1], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    state.padding_top = vv; state.padding_bottom = vv;
                    state.padding_left = vh_val; state.padding_right = vh_val;
                }
                3 => {
                    // top | horizontal | bottom
                    let vt = parse_unit(parts[0], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    let vh_val = parse_unit(parts[1], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    let vb = parse_unit(parts[2], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    state.padding_top = vt; state.padding_bottom = vb;
                    state.padding_left = vh_val; state.padding_right = vh_val;
                }
                4 => {
                    // top | right | bottom | left
                    state.padding_top    = parse_unit(parts[0], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    state.padding_right  = parse_unit(parts[1], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    state.padding_bottom = parse_unit(parts[2], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    state.padding_left   = parse_unit(parts[3], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                }
                _ => {}
            }
        }
        "padding-inline" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                let v = v as i32;
                state.padding_left = v;
                state.padding_right = v;
            }
        }
        "padding-block" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                let v = v as i32;
                state.padding_top = v;
                state.padding_bottom = v;
            }
        }
        "padding-top" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.padding_top = v as i32;
            }
        }
        "padding-bottom" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.padding_bottom = v as i32;
            }
        }
        "padding-left" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.padding_left = v as i32;
            }
        }
        "padding-right" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.padding_right = v as i32;
            }
        }
        "margin" => {
            use crate::render::layout::state::Margin;
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            let parts: Vec<&str> = val.split_whitespace().collect();
            let parse_m = |s: &str, fs: u16, rfs: f32| -> Margin {
                if s == "auto" { Margin::Auto }
                else { Margin::Px(parse_unit(s, fs, rfs, vh, vw).unwrap_or(0.0) as i32) }
            };
            match parts.len() {
                1 => {
                    let m = parse_m(parts[0], state.current_font_size, state.root_font_size);
                    state.margin_top    = m.get_px();
                    state.margin_bottom = m.get_px();
                    state.margin_left   = m;
                    state.margin_right  = parse_m(parts[0], state.current_font_size, state.root_font_size);
                }
                2 => {
                    // vertical | horizontal
                    let vv = parse_unit(parts[0], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    state.margin_top    = vv;
                    state.margin_bottom = vv;
                    state.margin_left   = parse_m(parts[1], state.current_font_size, state.root_font_size);
                    state.margin_right  = parse_m(parts[1], state.current_font_size, state.root_font_size);
                }
                3 => {
                    // top | horizontal | bottom
                    state.margin_top    = parse_unit(parts[0], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    state.margin_left   = parse_m(parts[1], state.current_font_size, state.root_font_size);
                    state.margin_right  = parse_m(parts[1], state.current_font_size, state.root_font_size);
                    state.margin_bottom = parse_unit(parts[2], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                }
                4 => {
                    // top | right | bottom | left
                    state.margin_top    = parse_unit(parts[0], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    state.margin_right  = parse_m(parts[1], state.current_font_size, state.root_font_size);
                    state.margin_bottom = parse_unit(parts[2], state.current_font_size, state.root_font_size, vh, vw).unwrap_or(0.0) as i32;
                    state.margin_left   = parse_m(parts[3], state.current_font_size, state.root_font_size);
                }
                _ => {}
            }
        }
        "margin-inline" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if val == "auto" {
                state.margin_left = crate::render::layout::state::Margin::Auto;
                state.margin_right = crate::render::layout::state::Margin::Auto;
            } else if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                let v = v as i32;
                state.margin_left = crate::render::layout::state::Margin::Px(v);
                state.margin_right = crate::render::layout::state::Margin::Px(v);
            }
        }
        "margin-block" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                let v = v as i32;
                state.margin_top = v;
                state.margin_bottom = v;
            }
        }
        "margin-top" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.margin_top = v as i32;
            }
        }
        "margin-bottom" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.margin_bottom = v as i32;
            }
        }
        "margin-left" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if val == "auto" {
                state.margin_left = crate::render::layout::state::Margin::Auto;
            } else if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.margin_left = crate::render::layout::state::Margin::Px(v as i32);
            }
        }
        "margin-right" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if val == "auto" {
                state.margin_right = crate::render::layout::state::Margin::Auto;
            } else if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.margin_right = crate::render::layout::state::Margin::Px(v as i32);
            }
        }
        "width" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.fixed_width = Some(v as i32);
            }
        }
        "max-width" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                let max_w_val = v as i32;
                state.fixed_width = Some(state.fixed_width.map(|cur| cur.min(max_w_val)).unwrap_or(max_w_val));
            }
        }
        "transform" => {
            if val.contains("translateY(-") || val.contains("translate(-") || (val.contains("translate3d(") && val.contains("-")) {
                state.current_display = crate::render::layout::state::Display::None;
            }
        }
        "display" => {
            state.current_display = match val.trim().to_ascii_lowercase().as_str() {
                "flow-root" => crate::render::layout::state::Display::FlowRoot,
                "block" | "flex" => crate::render::layout::state::Display::Block,
                "grid" => crate::render::layout::state::Display::Grid,
                "inline" => crate::render::layout::state::Display::Inline,
                "inline-block" | "inline-flex" => crate::render::layout::state::Display::InlineBlock,
                "none" => crate::render::layout::state::Display::None,
                _ => state.current_display,
            };
        }
        "grid-template-columns" => {
            let lower = val.to_ascii_lowercase();
            if lower.contains("repeat(2") {
                state.grid_columns = 2;
            } else {
                let count = val.split_whitespace().count();
                if count >= 2 {
                    state.grid_columns = count;
                }
            }
        }
        "gap" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.grid_gap = v as i32;
            }
        }
        "float" => {
            state.current_float = match val.trim().to_ascii_lowercase().as_str() {
                "left" => crate::render::layout::state::Float::Left,
                "right" => crate::render::layout::state::Float::Right,
                _ => crate::render::layout::state::Float::None,
            };
        }
        "list-style" | "list-style-type" => {
            let lower = val.trim().to_ascii_lowercase();
            if lower.contains("none") {
                state.list_style = crate::render::layout::state::ListStyle::None;
            } else if lower.contains("square") {
                state.list_style = crate::render::layout::state::ListStyle::Square;
            } else if lower.contains("disc") {
                state.list_style = crate::render::layout::state::ListStyle::Disc;
            }
        }
        "line-height" => {
            let vh = state.ctx.viewport_height;
            let vw = state.ctx.viewport_width;
            if let Some(pct) = val.trim().strip_suffix('%') {
                if let Ok(p) = pct.trim().parse::<f32>() {
                    state.line_height = (state.current_font_size as f32 * p / 100.0) as i32;
                }
            } else if let Ok(v) = val.parse::<f32>() {
                state.line_height = (state.current_font_size as f32 * v) as i32;
            } else if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size, vh, vw) {
                state.line_height = v as i32;
            }
        }
        "font-family" => {
            state.current_font_family = parse_font_family(val);
        }
        _ => {}
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Font family parsing
// ──────────────────────────────────────────────────────────────────────────────

/// Parse a CSS `font-family` value into a `FontFamily`.
///
/// The value is a comma-separated priority list; we walk left-to-right and
/// return the first token we recognise.  Generic keywords (`serif`,
/// `sans-serif`, `monospace`) are mapped directly; everything else becomes
/// `FontFamily::Custom(name)` so the font cache can look it up in its
/// `@font-face` registry or the system font search path.
fn parse_font_family(val: &str) -> crate::html5::node::FontFamily {
    use crate::html5::node::FontFamily;

    for token in val.split(',') {
        // Strip surrounding whitespace and quotes (' or ")
        let name = token.trim().trim_matches(|c| c == '\'' || c == '"').trim();
        if name.is_empty() { continue; }
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "sans-serif" | "arial" | "helvetica" | "verdana"
            | "-apple-system" | "system-ui" | "blinkmacsystemfont"
            | "segoe ui" | "open sans" | "helvetica neue"
            | "roboto" | "ubuntu" | "cantarell" | "noto sans" | "noto" => {
                return FontFamily::SansSerif;
            }
            "serif" | "times" | "times new roman" | "georgia"
            | "garamond" | "palatino" | "book antiqua" => {
                return FontFamily::Serif;
            }
            "monospace" | "courier" | "courier new" | "lucida console"
            | "consolas" | "monaco" | "menlo" | "source code pro"
            | "fira code" | "jetbrains mono" | "inconsolata"
            | "noto sans mono" => {
                return FontFamily::Monospace;
            }
            _ => {
                // Unknown name — return as Custom so the font cache can try
                // to resolve it via @font-face or system paths.
                return FontFamily::Custom(name.to_string());
            }
        }
    }

    // Empty or unparseable — fall back to sans-serif
    crate::html5::node::FontFamily::SansSerif
}

// ──────────────────────────────────────────────────────────────────────────────
// Table layout
// ──────────────────────────────────────────────────────────────────────────────

/// Collect all `<tr>` elements from a table (looking through thead/tbody/tfoot).
fn get_rows(table: &Element) -> Vec<&Element> {
    let mut rows = Vec::new();
    for node in &table.children {
        if let crate::html5::node::Node::Element(child) = node {
            let t = child.tag.to_lowercase();
            if t == "tr" {
                rows.push(child);
            } else if matches!(t.as_str(), "thead" | "tbody" | "tfoot") {
                for subnode in &child.children {
                    if let crate::html5::node::Node::Element(subel) = subnode {
                        if subel.tag.to_lowercase() == "tr" {
                            rows.push(subel);
                        }
                    }
                }
            }
        }
    }
    rows
}

/// Collect `<td>` / `<th>` children of a `<tr>`.
fn get_cells(tr: &Element) -> Vec<&Element> {
    tr.children.iter().filter_map(|n| {
        if let crate::html5::node::Node::Element(e) = n {
            if matches!(e.tag.to_lowercase().as_str(), "td" | "th") {
                return Some(e);
            }
        }
        None
    }).collect()
}

fn layout_table(
    state:    &mut LayoutState,
    canvas:   &mut Canvas<Window>,
    tc:       &TextureCreator<WindowContext>,
    fonts:    &mut FontCache,
    images:   &mut ImageCache,
    base_url: &str,
    table:    &Element,
    max_w:    i32,
    ancestors: &[&Element],
) {
    // Build ancestor chain including the table itself
    let mut table_chain = ancestors.to_vec();
    table_chain.push(table);

    // Apply table-level CSS (width:100% etc.) so fixed_width may be set
    let old_fixed_width = state.fixed_width;
    let old_margin_top  = state.margin_top;
    let old_margin_bottom = state.margin_bottom;
    // Reset table-specific state
    state.fixed_width = None;
    state.margin_top  = 0;
    state.margin_bottom = 0;

    // Apply global styles for <table>
    let mut props = Vec::new();
    for sheet in &state.stylesheets {
        for rule in &sheet.rules {
            if rule.matches_element(&table_chain, &[]) {
                for (p, v) in &rule.properties {
                    if p.starts_with("--") {
                        state.css_variables.insert(p.clone(), v.clone());
                    } else {
                        props.push((p.clone(), v.clone()));
                    }
                }
            }
        }
    }
    for (p, v) in props {
        let lookup = |var_name: &str| state.css_variables.get(var_name).cloned();
        let resolved = css::resolve_var(&v, &lookup);
        apply_style_prop(state, &p, &resolved);
    }
    // Apply inline style on <table>
    if let Some(style_raw) = crate::html5::parser::get_attr(&table.attrs_raw, "style") {
        for part in style_raw.split(';') {
            if let Some(cp) = part.find(':') {
                let k = part[..cp].trim();
                let v = part[cp+1..].trim();
                if !k.is_empty() && !v.is_empty() {
                    if k.starts_with("--") {
                        state.css_variables.insert(k.to_string(), v.to_string());
                    } else {
                        let lookup = |var_name: &str| state.css_variables.get(var_name).cloned();
                        let resolved = css::resolve_var(v, &lookup);
                        apply_style_prop(state, k, &resolved);
                    }
                }
            }
        }
    }

    // Determine table width
    let table_start_x = state.cursor_x;
    let table_w = state.fixed_width.unwrap_or(max_w - table_start_x);

    // Margin top
    state.cursor_y += state.margin_top;
    let _table_start_y = state.cursor_y;

    let rows = get_rows(table);
    if rows.is_empty() {
        state.fixed_width = old_fixed_width;
        state.margin_top  = old_margin_top;
        state.margin_bottom = old_margin_bottom;
        return;
    }

    // Determine column count (max across all rows)
    let col_count = rows.iter().map(|r| get_cells(r).len()).max().unwrap_or(1).max(1);
    let mut col_widths = vec![table_w / col_count as i32; col_count];
    if col_count == 2 {
        let mut max_col0_chars = 0;
        for row in &rows {
            let cells = get_cells(row);
            if let Some(c0) = cells.first() {
                fn get_text_len(node: &crate::html5::node::Node) -> usize {
                    match node {
                        crate::html5::node::Node::Text(t) => t.text.trim().len(),
                        crate::html5::node::Node::Element(el) => el.children.iter().map(get_text_len).sum(),
                    }
                }
                let text_len: usize = c0.children.iter().map(get_text_len).sum();
                max_col0_chars = max_col0_chars.max(text_len);
            }
        }
        if max_col0_chars > 0 && max_col0_chars < 40 {
            let c0_w = ((max_col0_chars as i32 * 10) + 25).min(table_w / 2).max(120);
            col_widths[0] = c0_w;
            col_widths[1] = (table_w - c0_w).max(50);
        }
    }

    // ── Pass over each row ──────────────────────────────────────────────────
    for row in &rows {
        let cells = get_cells(row);

        // Build ancestor chain for cells
        let mut row_chain = table_chain.clone();
        row_chain.push(*row);

        // Measure the tallest cell in this row (paint=false pass)
        let row_start_y = state.cursor_y;
        let mut row_height = 0i32;

        let saved_paint = state.paint;
        state.paint = false;

        for (ci, cell) in cells.iter().enumerate() {
            let col_w = col_widths.get(ci).copied().unwrap_or(col_widths[0]);
            let cell_x = table_start_x + col_widths[..ci].iter().sum::<i32>();
            let cell_end_x = cell_x + col_w;

            // Save & restore full layout state around each cell measurement
            let sv_cx = state.cursor_x;
            let sv_cy = state.cursor_y;
            let sv_lsx = state.line_start_x;
            let sv_fw  = state.fixed_width;
            let sv_pt  = state.padding_top;
            let sv_pb  = state.padding_bottom;
            let sv_pl  = state.padding_left;
            let sv_pr  = state.padding_right;
            let sv_mt  = state.margin_top;
            let sv_mb  = state.margin_bottom;
            let sv_ml  = state.margin_left;
            let sv_mr  = state.margin_right;
            let sv_bg  = state.current_bg_color;
            let sv_col = state.current_color;
            let sv_fs  = state.current_font_size;
            let sv_bold= state.current_bold;
            let sv_it  = state.current_italic;
            let sv_lh  = state.line_height;
            let sv_br  = state.current_border_radius;
            let sv_op  = state.current_opacity;
            let sv_dis = state.current_display;
            let sv_tt  = state.current_text_transform;
            let sv_lmb = state.last_margin_bottom;

            // Set up cell position
            state.cursor_x    = cell_x;
            state.cursor_y    = row_start_y;
            state.line_start_x = cell_x;
            state.fixed_width = Some(col_w);
            state.margin_top  = 0;
            state.margin_bottom = 0;
            state.margin_left  = crate::render::layout::state::Margin::Px(0);
            state.margin_right = crate::render::layout::state::Margin::Px(0);
            state.last_margin_bottom = 0;

            // Apply cell-level styles
            let mut cell_chain = row_chain.clone();
            cell_chain.push(*cell);
            let cell_siblings: Vec<&Element> = cells[..ci].to_vec();
            let mut cprops = Vec::new();
            for sheet in &state.stylesheets {
                for rule in &sheet.rules {
                    if rule.matches_element(&cell_chain, &cell_siblings) {
                        for (p, v) in &rule.properties {
                            cprops.push((p.clone(), v.clone()));
                        }
                    }
                }
            }
            for (p, v) in cprops {
                apply_style_prop(state, &p, &v);
            }
            if let Some(s) = crate::html5::parser::get_attr(&cell.attrs_raw, "style") {
                for part in s.split(';') {
                    if let Some(cp2) = part.find(':') {
                        let k2 = part[..cp2].trim();
                        let v2 = part[cp2+1..].trim();
                        if !k2.is_empty() && !v2.is_empty() { apply_style_prop(state, k2, v2); }
                    }
                }
            }

            let cell_pad_t = state.padding_top;
            let cell_pad_b = state.padding_bottom;
            let cell_pad_l = state.padding_left;

            state.cursor_y    += cell_pad_t;
            state.cursor_x    += cell_pad_l;
            state.line_start_x = state.cursor_x;
            state.fixed_width = Some(col_w);

            // Layout children
            for child in &cell.children {
                state.layout_node(canvas, tc, fonts, images, base_url, child, cell_end_x, &cell_chain);
            }
            if state.cursor_x > state.line_start_x {
                state.cursor_y += state.line_height;
            }
            state.cursor_y += cell_pad_b;

            let cell_h = state.cursor_y - row_start_y;
            if cell_h > row_height { row_height = cell_h; }

            // Restore
            state.cursor_x    = sv_cx;
            state.cursor_y    = sv_cy;
            state.line_start_x = sv_lsx;
            state.fixed_width = sv_fw;
            state.padding_top  = sv_pt;
            state.padding_bottom = sv_pb;
            state.padding_left = sv_pl;
            state.padding_right = sv_pr;
            state.margin_top   = sv_mt;
            state.margin_bottom = sv_mb;
            state.margin_left  = sv_ml;
            state.margin_right = sv_mr;
            state.current_bg_color = sv_bg;
            state.current_color    = sv_col;
            state.current_font_size = sv_fs;
            state.current_bold     = sv_bold;
            state.current_italic   = sv_it;
            state.line_height      = sv_lh;
            state.current_border_radius = sv_br;
            state.current_opacity  = sv_op;
            state.current_display  = sv_dis;
            state.current_text_transform = sv_tt;
            state.last_margin_bottom = sv_lmb;
        }

        state.paint = saved_paint;

        // ── Paint pass: render each cell at the correct position ──────────
        for (ci, cell) in cells.iter().enumerate() {
            let col_w = col_widths.get(ci).copied().unwrap_or(col_widths[0]);
            let cell_x = table_start_x + col_widths[..ci].iter().sum::<i32>();
            let cell_end_x = cell_x + col_w;

            let sv_cx = state.cursor_x;
            let sv_cy = state.cursor_y;
            let sv_lsx = state.line_start_x;
            let sv_fw  = state.fixed_width;
            let sv_pt  = state.padding_top;
            let sv_pb  = state.padding_bottom;
            let sv_pl  = state.padding_left;
            let sv_pr  = state.padding_right;
            let sv_mt  = state.margin_top;
            let sv_mb  = state.margin_bottom;
            let sv_ml  = state.margin_left;
            let sv_mr  = state.margin_right;
            let sv_bg  = state.current_bg_color;
            let sv_col = state.current_color;
            let sv_fs  = state.current_font_size;
            let sv_bold= state.current_bold;
            let sv_it  = state.current_italic;
            let sv_lh  = state.line_height;
            let sv_br  = state.current_border_radius;
            let sv_op  = state.current_opacity;
            let sv_dis = state.current_display;
            let sv_tt  = state.current_text_transform;
            let sv_lmb = state.last_margin_bottom;
            let sv_und = state.text_decoration_underline;

            state.cursor_x    = cell_x;
            state.cursor_y    = row_start_y;
            state.line_start_x = cell_x;
            state.fixed_width = Some(col_w);
            state.margin_top  = 0;
            state.margin_bottom = 0;
            state.margin_left  = crate::render::layout::state::Margin::Px(0);
            state.margin_right = crate::render::layout::state::Margin::Px(0);
            state.last_margin_bottom = 0;

            let mut cell_chain = row_chain.clone();
            cell_chain.push(*cell);
            let cell_siblings: Vec<&Element> = cells[..ci].to_vec();
            let mut cprops = Vec::new();
            for sheet in &state.stylesheets {
                for rule in &sheet.rules {
                    if rule.matches_element(&cell_chain, &cell_siblings) {
                        for (p, v) in &rule.properties {
                            cprops.push((p.clone(), v.clone()));
                        }
                    }
                }
            }
            for (p, v) in cprops {
                let lookup = |var_name: &str| state.css_variables.get(var_name).cloned();
                let resolved = css::resolve_var(&v, &lookup);
                apply_style_prop(state, &p, &resolved);
            }
            if let Some(s) = crate::html5::parser::get_attr(&cell.attrs_raw, "style") {
                for part in s.split(';') {
                    if let Some(cp2) = part.find(':') {
                        let k2 = part[..cp2].trim();
                        let v2 = part[cp2+1..].trim();
                        if !k2.is_empty() && !v2.is_empty() { apply_style_prop(state, k2, v2); }
                    }
                }
            }

            // Draw cell background across full row_height
            if state.paint {
                if let Some(bg) = state.current_bg_color {
                    let alpha = (bg[3] as f32 * state.current_opacity) as u8;
                    let rect = sdl2::rect::Rect::new(
                        cell_x, row_start_y - state.ctx.scroll_y,
                        col_w as u32, row_height as u32,
                    );
                    fill_rounded_rect(canvas, rect, state.current_border_radius,
                        Color::RGBA(bg[0], bg[1], bg[2], alpha));
                }
            }

            let cell_pad_t = state.padding_top;
            let _cell_pad_b = state.padding_bottom;
            let cell_pad_l = state.padding_left;

            state.cursor_y    = row_start_y + cell_pad_t;
            state.cursor_x    = cell_x + cell_pad_l;
            state.line_start_x = state.cursor_x;
            state.fixed_width = Some(col_w);

            for child in &cell.children {
                state.layout_node(canvas, tc, fonts, images, base_url, child, cell_end_x, &cell_chain);
            }

            // Restore
            state.cursor_x    = sv_cx;
            state.cursor_y    = sv_cy;
            state.line_start_x = sv_lsx;
            state.fixed_width = sv_fw;
            state.padding_top  = sv_pt;
            state.padding_bottom = sv_pb;
            state.padding_left = sv_pl;
            state.padding_right = sv_pr;
            state.margin_top   = sv_mt;
            state.margin_bottom = sv_mb;
            state.margin_left  = sv_ml;
            state.margin_right = sv_mr;
            state.current_bg_color = sv_bg;
            state.current_color    = sv_col;
            state.current_font_size = sv_fs;
            state.current_bold     = sv_bold;
            state.current_italic   = sv_it;
            state.line_height      = sv_lh;
            state.current_border_radius = sv_br;
            state.current_opacity  = sv_op;
            state.current_display  = sv_dis;
            state.current_text_transform = sv_tt;
            state.last_margin_bottom = sv_lmb;
            state.text_decoration_underline = sv_und;
        }

        // Advance Y past this row
        state.cursor_y = row_start_y + row_height;
        state.cursor_x = table_start_x;
        state.last_margin_bottom = 0;
    }

    // Margin bottom
    state.cursor_y += state.margin_bottom;
    state.cursor_x  = table_start_x;
    state.line_start_x = table_start_x;
    state.last_margin_bottom = state.margin_bottom;

    // Restore table-level state
    state.fixed_width   = old_fixed_width;
    state.margin_top    = old_margin_top;
    state.margin_bottom = old_margin_bottom;
}

fn fill_rounded_rect(canvas: &mut Canvas<Window>, rect: sdl2::rect::Rect, radius: i32, color: Color) {
    if radius <= 0 {
        canvas.set_draw_color(color);
        let _ = canvas.fill_rect(rect);
        return;
    }

    let r = radius.min(rect.width() as i32 / 2).min(rect.height() as i32 / 2);
    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    canvas.set_draw_color(color);

    // Central body
    let center = sdl2::rect::Rect::new(rect.x() + r, rect.y(), (rect.width() as i32 - 2 * r) as u32, rect.height());
    let _ = canvas.fill_rect(center);

    // Side bars
    let left = sdl2::rect::Rect::new(rect.x(), rect.y() + r, r as u32, (rect.height() as i32 - 2 * r) as u32);
    let _ = canvas.fill_rect(left);
    let right = sdl2::rect::Rect::new(rect.x() + rect.width() as i32 - r, rect.y() + r, r as u32, (rect.height() as i32 - 2 * r) as u32);
    let _ = canvas.fill_rect(right);

    // Corner quadrants
    draw_corner(canvas, rect.x() + r, rect.y() + r, r, -1, -1); // Top-left
    draw_corner(canvas, rect.x() + rect.width() as i32 - r - 1, rect.y() + r, r, 1, -1); // Top-right
    draw_corner(canvas, rect.x() + r, rect.y() + rect.height() as i32 - r - 1, r, -1, 1); // Bottom-left
    draw_corner(canvas, rect.x() + rect.width() as i32 - r - 1, rect.y() + rect.height() as i32 - r - 1, r, 1, 1); // Bottom-right
}

fn draw_corner(canvas: &mut Canvas<Window>, cx: i32, cy: i32, r: i32, dx: i32, dy: i32) {
    for y in 0..r {
        for x in 0..r {
            if x * x + y * y <= r * r {
                let px = cx + x * dx;
                let py = cy + y * dy;
                let _ = canvas.draw_point(sdl2::rect::Point::new(px, py));
            }
        }
    }
}


fn serialize_svg_children(nodes: &[crate::html5::node::Node]) -> String {
    let mut out = String::new();
    for n in nodes {
        match n {
            crate::html5::node::Node::Text(t) => out.push_str(&t.text),
            crate::html5::node::Node::Element(el) => {
                out.push('<');
                out.push_str(&el.tag);
                if !el.attrs_raw.is_empty() {
                    out.push(' ');
                    out.push_str(&el.attrs_raw);
                }
                out.push('>');
                out.push_str(&serialize_svg_children(&el.children));
                out.push_str(&format!("</{}>", el.tag));
            }
        }
    }
    out
}
