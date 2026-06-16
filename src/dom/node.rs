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
    Flex,
    Grid,
    Hidden,
}

/// CSS `position`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    #[default] Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// CSS `flex-direction`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default] Row,
    Column,
    RowReverse,
    ColumnReverse,
}

impl FlexDirection {
    pub fn from_css(val: &str) -> Self {
        match val.to_ascii_lowercase().as_str() {
            "column"         => FlexDirection::Column,
            "row-reverse"    => FlexDirection::RowReverse,
            "column-reverse" => FlexDirection::ColumnReverse,
            _                => FlexDirection::Row,
        }
    }
    pub fn is_row(self) -> bool {
        matches!(self, FlexDirection::Row | FlexDirection::RowReverse)
    }
}

/// CSS `flex-wrap`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexWrap {
    #[default] NoWrap,
    Wrap,
    WrapReverse,
}

impl FlexWrap {
    pub fn from_css(val: &str) -> Self {
        match val.to_ascii_lowercase().as_str() {
            "wrap"         => FlexWrap::Wrap,
            "wrap-reverse" => FlexWrap::WrapReverse,
            _              => FlexWrap::NoWrap,
        }
    }
}

/// CSS `justify-content`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyContent {
    #[default] FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl JustifyContent {
    pub fn from_css(val: &str) -> Self {
        match val.to_ascii_lowercase().as_str() {
            "flex-end"     | "end"   => JustifyContent::FlexEnd,
            "center"                 => JustifyContent::Center,
            "space-between"          => JustifyContent::SpaceBetween,
            "space-around"           => JustifyContent::SpaceAround,
            "space-evenly"           => JustifyContent::SpaceEvenly,
            _                        => JustifyContent::FlexStart,
        }
    }
}

/// CSS `align-items`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignItems {
    #[default] Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

impl AlignItems {
    pub fn from_css(val: &str) -> Self {
        match val.to_ascii_lowercase().as_str() {
            "flex-start" | "start" => AlignItems::FlexStart,
            "flex-end"   | "end"   => AlignItems::FlexEnd,
            "center"               => AlignItems::Center,
            "baseline"             => AlignItems::Baseline,
            _                      => AlignItems::Stretch,
        }
    }
}
/// A single CSS grid track (column or row) sizing value.
#[derive(Debug, Clone, PartialEq)]
pub enum GridTrackSize {
    /// Fixed pixel size.
    Px(i32),
    /// Fraction of free space.
    Fr(f32),
    /// Percentage of the container.
    Percent(f32),
    /// `auto` — shrink to content.
    Auto,
    /// `min-content` / `max-content` (both treated as auto for now).
    MinContent,
    MaxContent,
    /// `minmax(min, max)` — resolved to min value for simplicity.
    Minmax(Box<GridTrackSize>, Box<GridTrackSize>),
}

impl Default for GridTrackSize {
    fn default() -> Self { GridTrackSize::Auto }
}

/// CSS `grid-auto-flow`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridAutoFlow {
    #[default] Row,
    Column,
    RowDense,
    ColumnDense,
}

impl GridAutoFlow {
    pub fn from_css(val: &str) -> Self {
        let lv = val.to_ascii_lowercase();
        let dense = lv.contains("dense");
        if lv.contains("column") {
            if dense { GridAutoFlow::ColumnDense } else { GridAutoFlow::Column }
        } else {
            if dense { GridAutoFlow::RowDense } else { GridAutoFlow::Row }
        }
    }
    pub fn is_column(self) -> bool {
        matches!(self, GridAutoFlow::Column | GridAutoFlow::ColumnDense)
    }
}

/// CSS `justify-items` / `justify-self`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyItems {
    #[default] Stretch,
    Start,
    End,
    Center,
}

impl JustifyItems {
    pub fn from_css(val: &str) -> Self {
        match val.to_ascii_lowercase().as_str() {
            "start" | "flex-start" => JustifyItems::Start,
            "end"   | "flex-end"   => JustifyItems::End,
            "center"               => JustifyItems::Center,
            _                      => JustifyItems::Stretch,
        }
    }
}

/// CSS `align-content` / `justify-content` for grid
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignContent {
    #[default] Stretch,
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl AlignContent {
    pub fn from_css(val: &str) -> Self {
        match val.to_ascii_lowercase().as_str() {
            "start" | "flex-start" => AlignContent::Start,
            "end"   | "flex-end"   => AlignContent::End,
            "center"               => AlignContent::Center,
            "space-between"        => AlignContent::SpaceBetween,
            "space-around"         => AlignContent::SpaceAround,
            "space-evenly"         => AlignContent::SpaceEvenly,
            _                      => AlignContent::Stretch,
        }
    }
}

/// CSS `text-transform`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextTransform { #[default] None, Uppercase, Lowercase, Capitalize }

/// CSS `visibility`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default] Visible,
    Hidden,
    Collapse,
}

/// CSS `overflow`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overflow { #[default] Visible, Hidden, Auto, Scroll }

/// CSS `word-break` / `overflow-wrap`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordBreak { #[default] Normal, BreakAll, BreakWord }

/// Which font family to use for rendering.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum FontFamily {
    #[default]
    SansSerif,
    Monospace,
    Serif,
    Custom(String),
}

impl FontFamily {
    pub fn from_css(val: &str) -> Self {
        // Split by commas and take the first family (fallback logic would be better but requires more changes)
        let first = val.split(',').next().unwrap_or(val).trim();
        let v = first.to_ascii_lowercase();

        // Strip quotes
        let stripped = if (v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')) {
            v[1..v.len()-1].to_string()
        } else {
            v.to_string()
        };

        match stripped.as_str() {
            "monospace" => FontFamily::Monospace,
            "serif"     => FontFamily::Serif,
            "sans-serif" | "helvetica" | "arial" | "verdana" | "tahoma" => FontFamily::SansSerif,
            _ => {
                if stripped.contains("mono") || stripped.contains("courier") || stripped.contains("consolas")
                    || stripped.contains("code") || stripped.contains("terminal") || stripped.contains("vera")
                {
                    FontFamily::Monospace
                } else if stripped.contains("serif") && !stripped.contains("sans") {
                    FontFamily::Serif
                } else {
                    FontFamily::Custom(stripped)
                }
            }
        }
    }
}

/// CSS `border-style`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStyle {
    #[default] Solid,
    Dashed,
    Dotted,
    None,
}

/// A simple uniform border
#[derive(Debug, Clone, Copy, Default)]
pub struct Border {
    pub width: u8,
    pub color: [u8; 3],
    pub style: BorderStyle,
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
    Explicit(i32, i32), // explicit width × height in px
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

/// A single colour stop inside a `linear-gradient`.
#[derive(Debug, Clone, Copy)]
pub struct GradientStop {
    pub color: [u8; 3],
    pub alpha: u8,
    /// Position in the range [0.0, 1.0]. `None` means "evenly distribute".
    pub pos:   Option<f32>,
}

/// A parsed CSS `linear-gradient(…)` value.
#[derive(Debug, Clone)]
pub struct LinearGradient {
    /// Angle in degrees: 0 = bottom→top, 90 = left→right (CSS convention).
    pub angle_deg: f32,
    pub stops:     Vec<GradientStop>,
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
    pub bg_image_url:        Option<String>,       // CSS background-image: url(…)
    pub bg_gradient:         Option<LinearGradient>, // CSS linear-gradient(…)
    pub bg_size:             BgSize,
    pub bg_repeat:           BgRepeat,
    pub bg_position:         BgPosition,
    pub bg_attachment_fixed: bool,   // true when background-attachment: fixed
    pub display:             Display,
    pub display_block:       bool,   // kept for compat — mirrors display==Block
    pub visibility:          Visibility,
    pub borders:             Borders,
    pub padding:             BoxSpacing,
    pub margin:              BoxSpacing,
    pub size:                SizeConstraint,
    pub overflow:            Overflow,
    pub opacity:             u8,
    pub color_alpha:         u8,   // alpha for `color` (255 = fully opaque)
    pub bg_alpha:            u8,   // alpha for `bg_color` (255 = fully opaque)
    pub border_radius:       [u16; 4],   // [top-left, top-right, bottom-right, bottom-left] in px
    /// When `border-radius` is a percentage (e.g. `50%`), the px value can't be
    /// computed until layout time when the element's actual dimensions are known.
    /// Store the percentage (0–100) here; 0 means no deferred percentage radius.
    /// Individual corners all get the same value for simplicity (handles `50%` circles).
    pub border_radius_raw:   u8,        // percentage 0–100; 0 = not a percentage

    // --- flexbox ---
    pub flex_direction:      FlexDirection,
    pub flex_wrap:           FlexWrap,
    pub justify_content:     JustifyContent,
    pub align_items:         AlignItems,
    pub flex_grow:           f32,
    pub flex_shrink:         f32,
    pub flex_basis:          Option<i32>,
    pub gap:                 i32,

    // --- css grid ---
    /// Parsed `grid-template-columns` track sizes.
    pub grid_template_columns: Vec<GridTrackSize>,
    /// Parsed `grid-template-rows` track sizes.
    pub grid_template_rows:    Vec<GridTrackSize>,
    /// `column-gap` (also set by the `gap` shorthand's second value).
    pub column_gap:            i32,
    /// `row-gap` (also set by the `gap` shorthand's first value).
    pub row_gap:               i32,
    /// `grid-auto-flow`
    pub grid_auto_flow:        GridAutoFlow,
    /// `grid-auto-rows` — size for implicitly created rows.
    pub grid_auto_rows:        GridTrackSize,
    /// `grid-auto-columns` — size for implicitly created columns.
    pub grid_auto_columns:     GridTrackSize,
    /// `align-content` for grid containers.
    pub align_content:         AlignContent,
    /// `justify-items` — default alignment for all grid items along inline axis.
    pub justify_items:         JustifyItems,
    /// Per-item: `grid-column-start` (1-based, 0 = auto).
    pub grid_column_start:     i32,
    /// Per-item: `grid-column-end` (1-based exclusive, 0 = auto).
    pub grid_column_end:       i32,
    /// Per-item: `grid-row-start` (1-based, 0 = auto).
    pub grid_row_start:        i32,
    /// Per-item: `grid-row-end` (1-based exclusive, 0 = auto).
    pub grid_row_end:          i32,

    // --- list ---
    pub list_style_type:     ListStyleType,

    // --- margin auto ---
    /// `margin-left: auto` — used for horizontal centering / right-flush layout
    pub margin_auto_left:    bool,
    /// `margin-right: auto` — used for horizontal centering
    pub margin_auto_right:   bool,

    // --- link ---
    pub href:                Option<String>,

    // --- font family ---
    pub font_family:         FontFamily,

    // --- box shadow (single shadow) ---
    pub box_shadow:          Option<BoxShadow>,

    // --- word break ---
    pub word_break:          WordBreak,

    /// Raw CSS value for `font-size` when it uses viewport-relative units
    /// (`vw`, `vh`) or `calc()`. These cannot be resolved at cascade time
    /// because the real viewport dimensions aren't known yet; they are
    /// re-resolved at layout time in block.rs.
    pub font_size_raw:       Option<String>,

    // --- positioning ---
    pub position:            Position,
    pub top:                 Option<i32>,
    pub bottom:              Option<i32>,
    pub left:                Option<i32>,
    pub right:               Option<i32>,
    pub top_raw:             Option<String>,
    pub bottom_raw:          Option<String>,
    pub left_raw:            Option<String>,
    pub right_raw:           Option<String>,
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
            bg_gradient:       None,
            bg_size:           BgSize::Auto,
            bg_repeat:         BgRepeat::Repeat,
            bg_position:       BgPosition::default(),
            bg_attachment_fixed: false,
            display:           Display::Inline,
            display_block:     false,
            visibility:        Visibility::Visible,
            borders:           Borders::default(),
            padding:           BoxSpacing::default(),
            margin:            BoxSpacing::default(),
            size:              SizeConstraint::default(),
            overflow:          Overflow::Visible,
            opacity:           255,
            color_alpha:       255,
            bg_alpha:          255,
            border_radius:     [0, 0, 0, 0],
            border_radius_raw: 0,

            list_style_type:   ListStyleType::Disc,
            href:              None,
            font_family:       FontFamily::SansSerif,
            box_shadow:        None,
            word_break:        WordBreak::Normal,
            margin_auto_left:  false,
            margin_auto_right: false,
            font_size_raw:     None,

            flex_direction:    FlexDirection::Row,
            flex_wrap:         FlexWrap::NoWrap,
            justify_content:   JustifyContent::FlexStart,
            align_items:       AlignItems::Stretch,
            flex_grow:         0.0,
            flex_shrink:       1.0,
            flex_basis:        None,
            gap:               0,

            grid_template_columns: Vec::new(),
            grid_template_rows:    Vec::new(),
            column_gap:            0,
            row_gap:               0,
            grid_auto_flow:        GridAutoFlow::Row,
            grid_auto_rows:        GridTrackSize::Auto,
            grid_auto_columns:     GridTrackSize::Auto,
            align_content:         AlignContent::Stretch,
            justify_items:         JustifyItems::Stretch,
            grid_column_start:     0,
            grid_column_end:       0,
            grid_row_start:        0,
            grid_row_end:          0,

            position:          Position::Static,
            top:               None,
            bottom:            None,
            left:              None,
            right:             None,
            top_raw:           None,
            bottom_raw:        None,
            left_raw:          None,
            right_raw:         None,
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
