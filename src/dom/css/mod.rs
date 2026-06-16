mod color;
mod length;
pub(crate) mod inline;
mod ua;
pub mod stylesheet;
pub mod cascade;

// Public surface
#[allow(unused_imports)]
pub use color::{parse_color, parse_color_alpha};
#[allow(unused_imports)]
pub use length::{parse_length, parse_length_ctx, parse_box_spacing, LengthContext};
pub use inline::apply_inline;
pub use ua::apply_tag_defaults;
pub use stylesheet::{StyleSheet, Rule, Selector, SimpleSelector, PseudoClass, Specificity,
                     parse_selector_list, parse_selector_group, parse_simple_selector,
                     parse_pseudo_class, parse_nth};
pub use cascade::{matches, matches_with_state, apply_cascade, apply_cascade_with_state, PseudoState};
