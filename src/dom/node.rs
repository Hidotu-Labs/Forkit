#![allow(dead_code)]

/// Computed style for a DOM node.
#[derive(Debug, Clone)]
pub struct Style {
    pub color:        [u8; 3],       // RGB foreground
    pub bg_color:     Option<[u8; 3]>, // None = transparent
    pub font_size:    u16,
    pub bold:         bool,
    pub italic:       bool,
    pub display_block: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            color:         [0, 0, 0],
            bg_color:      None,
            font_size:     16,
            bold:          false,
            italic:        false,
            display_block: false,
        }
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
    pub style:      Style,
    pub children:   Vec<Node>,
}

#[derive(Debug)]
pub struct TextNode {
    pub text:  String,
    pub style: Style, // inherited from parent at parse time
}

impl Node {
    pub fn style(&self) -> &Style {
        match self {
            Node::Element(e) => &e.style,
            Node::Text(t)    => &t.style,
        }
    }

    /// Recursively print the tree (debug helper).
    pub fn dump(&self, depth: usize) {
        let indent = " ".repeat(depth * 2);
        match self {
            Node::Element(e) => {
                println!("{}<{}> id=\"{}\" class=\"{}\"",
                         indent, e.tag, e.id, e.class_name);
                for child in &e.children {
                    child.dump(depth + 1);
                }
            }
            Node::Text(t) => {
                println!("{}[TEXT] {:?}", indent, t.text);
            }
        }
    }
}
