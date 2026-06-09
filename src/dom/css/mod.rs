mod color;
mod length;
mod inline;
mod ua;
mod stylesheet;
pub mod cascade;

// Public surface — some items are used only within this crate's sub-modules
// or will be called by future features (e.g. stylesheet parser).
#[allow(unused_imports)]
pub use color::{parse_color, parse_color_alpha};
#[allow(unused_imports)]
pub use length::{parse_length, parse_length_ctx, parse_box_spacing, LengthContext};
pub use inline::apply_inline;
pub use ua::apply_tag_defaults;
pub use stylesheet::{StyleSheet, Rule, Selector, SimpleSelector, Specificity,
                     parse_selector_list, parse_selector_group, parse_simple_selector};
pub use cascade::{matches, apply_cascade};
