mod entities;
mod lexer;
mod builder;

pub use builder::{parse, get_attr, extract_page_meta};
pub use entities::decode_entities;
