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

    // Compute display dimensions
    let (disp_w, disp_h) = match (explicit_w, explicit_h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None)    => {
            let h = (nat_h as f32 * w as f32 / nat_w as f32) as i32;
            (w, h.max(1))
        }
        (None, Some(h))    => {
            let w = (nat_w as f32 * h as f32 / nat_h as f32) as i32;
            (w.max(1), h)
        }
        (None, None) => {
            // Default: fit within available width, no upscaling
            let avail = (max_w - state.cursor_x).max(1);
            if nat_w > avail {
                let h = (nat_h as f32 * avail as f32 / nat_w as f32) as i32;
                (avail, h.max(1))
            } else {
                (nat_w, nat_h)
            }
        }
    };

    if state.paint {
        let texture = tc.create_texture_from_surface(&surface).ok()?;
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
) {
    let tag = el.tag.to_lowercase();
    if matches!(tag.as_str(), "head" | "style" | "title" | "meta" | "link") {
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
        let src = get_attr(&el.attrs_raw, "src").unwrap_or("").to_owned();
        if !src.is_empty() {
            // Parse explicit width / height from attributes then inline style
            let attr_w = get_attr(&el.attrs_raw, "width")
                .and_then(|v| parse_dim_attr(v, max_w - state.cursor_x));
            let attr_h = get_attr(&el.attrs_raw, "height")
                .and_then(|v| parse_dim_attr(v, state.ctx.viewport_height));

            let mut style_w = attr_w;
            let mut style_h = attr_h;
            if let Some(style_raw) = get_attr(&el.attrs_raw, "style") {
                for part in style_raw.split(';') {
                    if let Some(colon) = part.find(':') {
                        let k = part[..colon].trim();
                        let v = part[colon + 1..].trim();
                        match k {
                            "width"  => style_w = parse_dim_attr(v, max_w - state.cursor_x),
                            "height" => style_h = parse_dim_attr(v, state.ctx.viewport_height),
                            _ => {}
                        }
                    }
                }
            }

            // If the image is wider than remaining space, wrap to next line first
            let need_w = style_w.unwrap_or(0);
            if need_w > 0 && state.cursor_x + need_w > max_w && state.cursor_x > state.line_start_x {
                state.cursor_y += state.line_height;
                state.cursor_x = state.line_start_x;
            }

            if let Some(bytes) = images.get_bytes(&src, base_url).map(|b| b.to_vec()) {
                if let Some((dw, dh)) = paint_image(state, canvas, tc, &bytes, style_w, style_h, max_w) {
                    state.cursor_x += dw;
                    if dh > state.line_height {
                        state.line_height = dh;
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
    // The parser captures the full <svg>…</svg> markup as a single TextNode
    // child.  Extract it and render as a raster image via SDL2_image + nanosvg.
    if tag == "svg" {
        let svg_markup: Option<String> = el.children.iter().find_map(|c| {
            if let crate::html5::node::Node::Text(t) = c {
                if t.text.contains("<svg") || t.text.trim_start().starts_with('<') {
                    return Some(t.text.clone());
                }
            }
            None
        });

        if let Some(markup) = svg_markup {
            let bytes = markup.into_bytes();

            let attr_w = get_attr(&el.attrs_raw, "width")
                .and_then(|v| parse_dim_attr(v, max_w - state.cursor_x));
            let attr_h = get_attr(&el.attrs_raw, "height")
                .and_then(|v| parse_dim_attr(v, state.ctx.viewport_height));

            let mut style_w = attr_w;
            let mut style_h = attr_h;
            if let Some(style_raw) = get_attr(&el.attrs_raw, "style") {
                for part in style_raw.split(';') {
                    if let Some(colon) = part.find(':') {
                        let k = part[..colon].trim();
                        let v = part[colon + 1..].trim();
                        match k {
                            "width"  => style_w = parse_dim_attr(v, max_w - state.cursor_x),
                            "height" => style_h = parse_dim_attr(v, state.ctx.viewport_height),
                            _ => {}
                        }
                    }
                }
            }

            // SVG is block-level: start on its own line
            if state.cursor_x > state.line_start_x {
                state.cursor_y += state.line_height;
                state.cursor_x = state.line_start_x;
            }

            if let Some((_dw, dh)) = paint_image(state, canvas, tc, &bytes, style_w, style_h, max_w) {
                state.cursor_y += dh;
                state.cursor_x = state.line_start_x;
                state.last_margin_bottom = 0;
            }
        }
        return;
    }

    let is_block_tag = matches!(tag.as_str(), 
        "div" | "p" | "h1" | "h2" | "h3" | "ul" | "li" | "ol" | "dl" | "dt" | "dd" | 
        "body" | "html" | "header" | "footer" | "section" | "nav" | "article" | "aside" | "main" | 
        "figure" | "figcaption" | "blockquote" | "pre"
    );
    let is_inline_block_tag = matches!(tag.as_str(), "button");

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
    let old_line_height = state.line_height;
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

    // Tag-specific defaults for "exact look as chrome"
    match tag.as_str() {
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
        "button" => {
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
    let mut current_chain = ancestors.to_vec();
    current_chain.push(el);

    let mut props_to_apply = Vec::new();
    for sheet in &state.stylesheets {
        for rule in &sheet.rules {
            if matches_selector(&rule.selector, &current_chain) {
                for (prop, val) in &rule.properties {
                    props_to_apply.push((prop.clone(), val.clone()));
                }
            }
        }
    }
    for (prop, val) in props_to_apply {
        apply_style_prop(state, &prop, &val);
    }

    state.line_height = (state.current_font_size as f32 * 1.2) as i32;
    let is_header = matches!(tag.as_str(), "h1" | "h2" | "h3");

    // 2. Apply inline styles (overwrites global)
    if let Some(style_raw) = get_attr(&el.attrs_raw, "style") {
        for part in style_raw.split(';') {
            // Split on first ':' only — allows values like oklch(...) which have no ':'
            if let Some(colon_pos) = part.find(':') {
                let k = part[..colon_pos].trim();
                let v = part[colon_pos + 1..].trim();
                if !k.is_empty() && !v.is_empty() {
                    apply_style_prop(state, k, v);
                }
            }
        }
    }

    if tag == "html" {
        state.root_font_size = state.current_font_size as f32;
    }

    let is_block = state.current_display == crate::render::layout::state::Display::Block;
    let is_inline_block = state.current_display == crate::render::layout::state::Display::InlineBlock;
    let is_hidden = state.current_display == crate::render::layout::state::Display::None;

    if is_hidden {
        return;
    }

    // Inline elements must not carry a fixed_width inherited from an ancestor block.
    // Reset it so child_max_w and background-width calculations are correct.
    if !is_block && !is_inline_block {
        state.fixed_width = None;
    }

    // Ensure blocks start on a new line (inline-block also starts new line if following text)
    if (is_block || is_inline_block) && state.cursor_x > state.line_start_x {
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
        return;
    }

    if is_header {
        state.cursor_y += state.line_height / 2;
    }

    let initial_paint = state.paint;
    
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
    }

    let bg_start_y = state.cursor_y;
    let bg_start_x = state.cursor_x;

    // Apply Padding
    if is_block || is_inline_block {
        state.cursor_y += state.padding_top;
        state.cursor_x += state.padding_left;
    }

    let inner_start_y = state.cursor_y;
    let inner_start_x = state.cursor_x;
    let inner_line_height = state.line_height;
    if is_block || is_inline_block {
        state.line_start_x = inner_start_x;
    }

    let pr = if is_block || is_inline_block { state.padding_right } else { 0 };
    let child_max_w = if (is_block || is_inline_block) {
        if let Some(w) = state.fixed_width {
            bg_start_x + w - pr
        } else {
            max_w - mr - pr
        }
    } else {
        // Inline elements pass max_w through unchanged; their fixed_width
        // is from an ancestor block and must not influence child_max_w here.
        max_w
    };

    if (is_block || is_inline_block) && state.current_bg_color.is_some() {
        // Pass 1: Measure height (paint = false)
        let initial_y = state.cursor_y;
        state.paint = false;
        let mut content_max_x = state.line_start_x;
        for child in &el.children {
            state.layout_node(canvas, tc, fonts, images, base_url, child, child_max_w, &current_chain);
            content_max_x = content_max_x.max(state.cursor_x);
        }
        
        if state.cursor_x > state.line_start_x {
            state.cursor_y += state.line_height;
            state.cursor_x = state.line_start_x;
        }
        state.cursor_y += state.padding_bottom;
        
        let mut bg_h = state.cursor_y - bg_start_y;

        if state.fixed_width == Some(200) {
            eprintln!("200px box: bg_start_y={} cursor_y={} bg_h={} padding_top={} padding_bottom={} line_height={}", 
                bg_start_y, state.cursor_y, bg_h, state.padding_top, state.padding_bottom, state.line_height);
        }
        if tag == "body" || tag == "html" {
            bg_h = bg_h.max(state.ctx.viewport_height + state.ctx.scroll_y);
        }

        if initial_paint {
            if let Some(bg) = state.current_bg_color {
                let alpha = (bg[3] as f32 * state.current_opacity) as u8;
                let (rect_x, rect_w) = if tag == "body" || tag == "html" {
                    (0, max_w)
                } else {
                    let w = if is_inline_block {
                        content_max_x - bg_start_x + state.padding_right
                    } else {
                        state.fixed_width.unwrap_or(max_w - mr - bg_start_x)
                    };
                    (bg_start_x, w)
                };
                let rect = sdl2::rect::Rect::new(rect_x, bg_start_y - state.ctx.scroll_y, rect_w as u32, bg_h as u32);
                fill_rounded_rect(canvas, rect, state.current_border_radius, Color::RGBA(bg[0], bg[1], bg[2], alpha));
                
                if tag == "button" {
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
            }
        }
        
        // Pass 2: Paint (paint = initial_paint)
        state.paint = initial_paint;
        state.cursor_y = inner_start_y;
        state.cursor_x = inner_start_x;
        state.line_start_x = inner_start_x;
        state.line_height = inner_line_height;
        
        for child in &el.children {
            state.layout_node(canvas, tc, fonts, images, base_url, child, child_max_w, &current_chain);
        }
        
        // Re-calculate final Y to include everything
        if state.cursor_x > state.line_start_x {
            state.cursor_y += state.line_height;
            state.cursor_x = state.line_start_x;
        }
        state.cursor_y += state.padding_bottom;

    } else if is_block || is_inline_block {
        // Blocks or inline-blocks without background still need padding handled
        for child in &el.children {
            state.layout_node(canvas, tc, fonts, images, base_url, child, child_max_w, &current_chain);
        }
        if state.cursor_x > state.line_start_x {
            state.cursor_y += state.line_height;
            state.cursor_x = state.line_start_x;
        }
        state.cursor_y += state.padding_bottom;
    } else {
        // Pure inline elements (like span without inline-block)
        for child in &el.children {
            state.layout_node(canvas, tc, fonts, images, base_url, child, child_max_w, &current_chain);
        }
        // Inline elements do not contribute block-axis padding to cursor_y
        if is_inline_block {
            state.cursor_x += state.padding_right + mr;
        }
    }

    if is_block {
        if is_header {
            state.cursor_y += state.line_height / 2;
        }
        state.cursor_x = old_line_start_x;
        state.last_margin_bottom = state.margin_bottom;
    }
    
    if is_inline_block {
        state.line_start_x = old_line_start_x;
    }
    
    state.current_display = old_display;
    
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
}

fn parse_unit(val: &str, current_font_size: u16, root_font_size: f32) -> Option<f32> {
    let val = val.trim();
    if let Some(v) = val.strip_suffix("px") {
        v.parse::<f32>().ok()
    } else if let Some(v) = val.strip_suffix("em") {
        v.parse::<f32>().ok().map(|e| e * current_font_size as f32)
    } else if let Some(v) = val.strip_suffix("rem") {
        v.parse::<f32>().ok().map(|r| r * root_font_size)
    } else {
        // Fallback for unitless numbers if needed, or just return None
        val.parse::<f32>().ok()
    }
}

fn apply_style_prop(state: &mut LayoutState, prop: &str, val: &str) {
    match prop {
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
        "font-size" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.current_font_size = v as u16;
            }
        }
        "font-weight" => {
            state.current_bold = matches!(val, "bold" | "700" | "800" | "900");
        }
        "border-radius" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.current_border_radius = v as i32;
            }
        }
        "font-style" => {
            state.current_italic = val.eq_ignore_ascii_case("italic");
        }
        "opacity" => {
            if let Some(v) = val.parse::<f32>().ok() {
                state.current_opacity = (state.current_opacity * v).clamp(0.0, 1.0);
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
            // CSS shorthand: 1 value | 2 values (vertical horizontal) | 3 values (top h bottom) | 4 values (top right bottom left)
            let parts: Vec<&str> = val.split_whitespace().collect();
            match parts.len() {
                1 => {
                    if let Some(v) = parse_unit(parts[0], state.current_font_size, state.root_font_size) {
                        let v = v as i32;
                        state.padding_top = v; state.padding_bottom = v;
                        state.padding_left = v; state.padding_right = v;
                    }
                }
                2 => {
                    // vertical | horizontal
                    let vv = parse_unit(parts[0], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    let vh = parse_unit(parts[1], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    state.padding_top = vv; state.padding_bottom = vv;
                    state.padding_left = vh; state.padding_right = vh;
                }
                3 => {
                    // top | horizontal | bottom
                    let vt = parse_unit(parts[0], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    let vh = parse_unit(parts[1], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    let vb = parse_unit(parts[2], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    state.padding_top = vt; state.padding_bottom = vb;
                    state.padding_left = vh; state.padding_right = vh;
                }
                4 => {
                    // top | right | bottom | left
                    state.padding_top    = parse_unit(parts[0], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    state.padding_right  = parse_unit(parts[1], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    state.padding_bottom = parse_unit(parts[2], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    state.padding_left   = parse_unit(parts[3], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                }
                _ => {}
            }
        }
        "padding-inline" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                let v = v as i32;
                state.padding_left = v;
                state.padding_right = v;
            }
        }
        "padding-block" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                let v = v as i32;
                state.padding_top = v;
                state.padding_bottom = v;
            }
        }
        "padding-top" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.padding_top = v as i32;
            }
        }
        "padding-bottom" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.padding_bottom = v as i32;
            }
        }
        "padding-left" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.padding_left = v as i32;
            }
        }
        "padding-right" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.padding_right = v as i32;
            }
        }
        "margin" => {
            use crate::render::layout::state::Margin;
            let parts: Vec<&str> = val.split_whitespace().collect();
            let parse_m = |s: &str, fs: u16, rfs: f32| -> Margin {
                if s == "auto" { Margin::Auto }
                else { Margin::Px(parse_unit(s, fs, rfs).unwrap_or(0.0) as i32) }
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
                    let vv = parse_unit(parts[0], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    state.margin_top    = vv;
                    state.margin_bottom = vv;
                    state.margin_left   = parse_m(parts[1], state.current_font_size, state.root_font_size);
                    state.margin_right  = parse_m(parts[1], state.current_font_size, state.root_font_size);
                }
                3 => {
                    // top | horizontal | bottom
                    state.margin_top    = parse_unit(parts[0], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    state.margin_left   = parse_m(parts[1], state.current_font_size, state.root_font_size);
                    state.margin_right  = parse_m(parts[1], state.current_font_size, state.root_font_size);
                    state.margin_bottom = parse_unit(parts[2], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                }
                4 => {
                    // top | right | bottom | left
                    state.margin_top    = parse_unit(parts[0], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    state.margin_right  = parse_m(parts[1], state.current_font_size, state.root_font_size);
                    state.margin_bottom = parse_unit(parts[2], state.current_font_size, state.root_font_size).unwrap_or(0.0) as i32;
                    state.margin_left   = parse_m(parts[3], state.current_font_size, state.root_font_size);
                }
                _ => {}
            }
        }
        "margin-inline" => {
            if val == "auto" {
                state.margin_left = crate::render::layout::state::Margin::Auto;
                state.margin_right = crate::render::layout::state::Margin::Auto;
            } else if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                let v = v as i32;
                state.margin_left = crate::render::layout::state::Margin::Px(v);
                state.margin_right = crate::render::layout::state::Margin::Px(v);
            }
        }
        "margin-block" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                let v = v as i32;
                state.margin_top = v;
                state.margin_bottom = v;
            }
        }
        "margin-top" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.margin_top = v as i32;
            }
        }
        "margin-bottom" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.margin_bottom = v as i32;
            }
        }
        "margin-left" => {
            if val == "auto" {
                state.margin_left = crate::render::layout::state::Margin::Auto;
            } else if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.margin_left = crate::render::layout::state::Margin::Px(v as i32);
            }
        }
        "margin-right" => {
            if val == "auto" {
                state.margin_right = crate::render::layout::state::Margin::Auto;
            } else if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.margin_right = crate::render::layout::state::Margin::Px(v as i32);
            }
        }
        "width" => {
            if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
                state.fixed_width = Some(v as i32);
            }
        }
        "display" => {
            state.current_display = match val {
                "block" => crate::render::layout::state::Display::Block,
                "inline" => crate::render::layout::state::Display::Inline,
                "inline-block" => crate::render::layout::state::Display::InlineBlock,
                "none" => crate::render::layout::state::Display::None,
                _ => state.current_display,
            };
        }
        "line-height" => {
            if let Ok(v) = val.parse::<f32>() {
                state.line_height = (state.current_font_size as f32 * v) as i32;
            } else if let Some(v) = parse_unit(val, state.current_font_size, state.root_font_size) {
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
            | "roboto" | "ubuntu" | "cantarell" | "noto sans" => {
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
            if matches_selector(&rule.selector, &table_chain) {
                for (p, v) in &rule.properties {
                    props.push((p.clone(), v.clone()));
                }
            }
        }
    }
    for (p, v) in props {
        apply_style_prop(state, &p, &v);
    }
    // Apply inline style on <table>
    if let Some(style_raw) = crate::html5::parser::get_attr(&table.attrs_raw, "style") {
        for part in style_raw.split(';') {
            if let Some(cp) = part.find(':') {
                let k = part[..cp].trim();
                let v = part[cp+1..].trim();
                if !k.is_empty() && !v.is_empty() { apply_style_prop(state, k, v); }
            }
        }
    }

    // Determine table width
    let table_start_x = state.cursor_x;
    let table_w = state.fixed_width.unwrap_or(max_w - table_start_x);

    // Margin top
    state.cursor_y += state.margin_top;
    let table_start_y = state.cursor_y;

    let rows = get_rows(table);
    if rows.is_empty() {
        state.fixed_width = old_fixed_width;
        state.margin_top  = old_margin_top;
        state.margin_bottom = old_margin_bottom;
        return;
    }

    // Determine column count (max across all rows)
    let col_count = rows.iter().map(|r| get_cells(r).len()).max().unwrap_or(1).max(1);
    let col_w = table_w / col_count as i32;

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
            let cell_x = table_start_x + ci as i32 * col_w;
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
            let mut cprops = Vec::new();
            for sheet in &state.stylesheets {
                for rule in &sheet.rules {
                    if matches_selector(&rule.selector, &cell_chain) {
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
            let cell_x = table_start_x + ci as i32 * col_w;
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
            let mut cprops = Vec::new();
            for sheet in &state.stylesheets {
                for rule in &sheet.rules {
                    if matches_selector(&rule.selector, &cell_chain) {
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
            let cell_pad_b = state.padding_bottom;
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

fn matches_selector(selector: &str, chain: &[&Element]) -> bool {
    let parts: Vec<&str> = selector.split_whitespace().collect();
    if parts.is_empty() { return false; }
    
    let mut chain_idx = chain.len() as i32 - 1;
    for part in parts.iter().rev() {
        let mut found = false;
        while chain_idx >= 0 {
            if element_matches_part(chain[chain_idx as usize], part) {
                found = true;
                chain_idx -= 1;
                break;
            }
            // For the VERY LAST part (the target), it MUST match the current element
            if chain_idx == chain.len() as i32 - 1 { break; }
            chain_idx -= 1;
        }
        if !found { return false; }
    }
    true
}

fn element_matches_part(el: &Element, part: &str) -> bool {
    if part == "*" { return true; }
    
    let mut tag_part = part;
    let mut id_part = None;
    let mut class_part = None;
    
    if let Some(idx) = part.find('#') {
        tag_part = &part[..idx];
        let rest = &part[idx+1..];
        if let Some(c_idx) = rest.find('.') {
            id_part = Some(&rest[..c_idx]);
            class_part = Some(&rest[c_idx+1..]);
        } else {
            id_part = Some(rest);
        }
    } else if let Some(idx) = part.find('.') {
        tag_part = &part[..idx];
        class_part = Some(&part[idx+1..]);
    }
    
    if !tag_part.is_empty() && tag_part != el.tag { return false; }
    if let Some(id) = id_part { if id != el.id { return false; } }
    if let Some(class) = class_part { if class != el.class_name { return false; } }
    
    true
}
