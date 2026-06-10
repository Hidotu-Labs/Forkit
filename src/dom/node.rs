#![allow(dead_code)]

/// Text alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign { #[default] Left, Center, Right }

/// List bullet style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListStyleType { #[default] Disc, Circle, Square, Decimal, None }

/// CSS `display` value (subset)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    #[default] Inline,
    Block,
    InlineBlock,
    Hidden,   // display:none
}
/// CSS `text-transform`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextTransform { #[default] None, Uppercase, Lowercase, Capitalize }

/// CSS `overflow`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow { #[default] Visible, Hidden, Auto, Scroll }

/// CSS `word-break` / `overflow-wrap`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordBreak { #[default] Normal, BreakAll, BreakWord }

/// Which font family to use for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontFamilyHint { #[default] SansSerif, Monospace, Serif }

/// A simple uniform border
#[derive(Debug, Clone, Copy, Default)]
pub struct Border {
    pub width: u8,
    pub color: [u8; 3],
}

/// Per-side borders
#[derive(Debug, Clone, Copy, Default)]
pub struct Borders {
    pub top:    Border,
    pub right:  Border,
    pub bottom: Border,
    pub left:   Border,
}

impl Borders {
    /// Set all four sides to the same border.
    pub fn uniform(b: Border) -> Self {
        Borders { top: b, right: b, bottom: b, left: b }
    }
}

/// Box spacing (top, right, bottom, left)
#[derive(Debug, Clone, Copy, Default)]
pub struct BoxSpacing {
    pub top:    i32,
    pub right:  i32,
    pub bottom: i32,
    pub left:   i32,
}

/// Optional size constraint.
///
/// `*_raw` fields hold the original CSS value string for any dimension whose
/// value uses viewport-relative (`vw`/`vh`) or percentage (`%`) units.  Those
/// need to be resolved at layout time when the real containing-block width and
/// viewport dimensions are known.  Absolute pixel/em values are resolved at
/// cascade time and stored directly in the `Option<i32>` fields; the
/// corresponding `*_raw` field is `None` in that case.
#[derive(Debug, Clone, Default)]
pub struct SizeConstraint {
    pub width:          Option<i32>,
    pub height:         Option<i32>,
    pub max_width:      Option<i32>,
    pub min_width:      Option<i32>,
    pub max_height:     Option<i32>,
    pub min_height:     Option<i32>,
    /// Raw CSS value for `width` when it uses `%`/`vw`/`vh`.
    pub width_raw:      Option<String>,
    /// Raw CSS value for `height` when it uses `%`/`vw`/`vh`.
    pub height_raw:     Option<String>,
    /// Raw CSS value for `max-width` when it uses `%`/`vw`/`vh`.
    pub max_width_raw:  Option<String>,
    /// Raw CSS value for `min-width` when it uses `%`/`vw`/`vh`.
    pub min_width_raw:  Option<String>,
    /// Raw CSS value for `max-height` when it uses `%`/`vw`/`vh`.
    pub max_height_raw: Option<String>,
    /// Raw CSS value for `min-height` when it uses `%`/`vw`/`vh`.
    pub min_height_raw: Option<String>,
}

/// CSS `background-size`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BgSize {
    #[default] Auto,   // use the image's natural size
    Cover,             // scale to cover the box (may crop)
    Contain,           // scale to fit inside the box (may letterbox)
}

/// CSS `background-repeat`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BgRepeat {
    #[default] Repeat,  // tile in both axes
    RepeatX,            // tile horizontally only
    RepeatY,            // tile vertically only
    NoRepeat,           // single image, no tiling
}

/// CSS `background-position` — resolved to pixel offsets relative to the box.
/// Percentages are stored as-is and resolved at paint time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BgPosition {
    /// Horizontal offset in px (0 = left edge)
    pub x: i32,
    /// Vertical offset in px (0 = top edge)
    pub y: i32,
}

/// A simple box-shadow descriptor (offset-x, offset-y, blur, color).
#[derive(Debug, Clone, Copy, Default)]
pub struct BoxShadow {
    pub offset_x: i32,
    pub offset_y: i32,
    pub blur:     i32,
    pub color:    [u8; 3],
    pub alpha:    u8,
}

/// Computed style for a DOM node.
#[derive(Debug, Clone)]
pub struct Style {
    // --- text ---
    pub color:               [u8; 3],
    pub font_size:           u16,
    pub bold:                bool,
    pub italic:              bool,
    pub underline:           bool,
    pub strikethrough:       bool,
    pub text_align:          TextAlign,
    pub line_height_mul:     f32,
    pub letter_spacing:      i32,
    pub word_spacing:        i32,
    pub white_space_pre:     bool,
    pub text_transform:      TextTransform,
    pub font_variant_caps:   bool,   // true → small-caps

    // --- box ---
    pub bg_color:            Option<[u8; 3]>,
    pub bg_image_url:        Option<String>,   // CSS background-image: url(…)
    pub bg_size:             BgSize,
    pub bg_repeat:           BgRepeat,
    pub bg_position:         BgPosition,
    pub display:             Display,
    pub display_block:       bool,   // kept for compat — mirrors display==Block
    pub borders:             Borders,
    pub padding:             BoxSpacing,
    pub margin:              BoxSpacing,
    pub size:                SizeConstraint,
    pub overflow:            Overflow,
    pub opacity:             u8,
    pub color_alpha:         u8,   // alpha for `color` (255 = fully opaque)
    pub bg_alpha:            u8,   // alpha for `bg_color` (255 = fully opaque)
    pub border_radius:       [u16; 4],   // [top-left, top-right, bottom-right, bottom-left] in px

    // --- list ---
    pub list_style_type:     ListStyleType,

    // --- link ---
    pub href:                Option<String>,

    // --- font family ---
    pub font_family:         FontFamilyHint,

    // --- box shadow (single shadow) ---
    pub box_shadow:          Option<BoxShadow>,

    // --- word break ---
    pub word_break:          WordBreak,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            color:             [0, 0, 0],
            font_size:         16,
            bold:              false,
            italic:            false,
            underline:         false,
            strikethrough:     false,
            text_align:        TextAlign::Left,
            line_height_mul:   1.2,
            letter_spacing:    0,
            word_spacing:      0,
            white_space_pre:   false,
            text_transform:    TextTransform::None,
            font_variant_caps: false,

            bg_color:          None,
            bg_image_url:      None,
            bg_size:           BgSize::Auto,
            bg_repeat:         BgRepeat::Repeat,
            bg_position:       BgPosition::default(),
            display:           Display::Inline,
            display_block:     false,
            borders:           Borders::default(),
            padding:           BoxSpacing::default(),
            margin:            BoxSpacing::default(),
            size:              SizeConstraint::default(),
            overflow:          Overflow::Visible,
            opacity:           255,
            color_alpha:       255,
            bg_alpha:          255,
            border_radius:     [0, 0, 0, 0],

            list_style_type:   ListStyleType::Disc,
            href:              None,
            font_family:       FontFamilyHint::SansSerif,
            box_shadow:        None,
            word_break:        WordBreak::Normal,
        }
    }
}

// Convenience: uniform border shorthand (kept for UA stylesheet compat)
impl Style {
    pub fn set_border_all(&mut self, b: Border) {
        self.borders = Borders::uniform(b);
    }
    /// Returns the "dominant" border for drawing purposes (left side, or top if left is 0).
    pub fn dominant_border(&self) -> Border {
        if self.borders.left.width > 0 { self.borders.left }
        else if self.borders.top.width > 0 { self.borders.top }
        else { self.borders.right }
    }
}

/// A single node in the DOM tree.
#[derive(Debug)]
pub enum Node {
    Element(Element),
    Text(TextNode),
}

#[derive(Debug)]
pub struct Element {
    pub tag:        String,
    pub id:         String,
    pub class_name: String,
    pub style_attr: String,
    pub attrs_raw:  String,
    pub style:      Style,
    pub children:   Vec<Node>,
}

#[derive(Debug)]
pub struct TextNode {
    pub text:  String,
    pub style: Style,
}

impl Node {
    pub fn style(&self) -> &Style {
        match self {
            Node::Element(e) => &e.style,
            Node::Text(t)    => &t.style,
        }
    }

    pub fn dump(&self, depth: usize) {
        let indent = "  ".repeat(depth);
        match self {
            Node::Element(e) => {
                println!("{}<{}> id={:?} class={:?}", indent, e.tag, e.id, e.class_name);
                for child in &e.children { child.dump(depth + 1); }
            }
            Node::Text(t) => println!("{}[TEXT] {:?}", indent, t.text),
        }
    }
}
