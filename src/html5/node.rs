
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FontFamily {
    SansSerif,
    Serif,
    Monospace,
    Custom(String),
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
    pub href:       String,
    pub attrs_raw:  String,
    pub children:   Vec<Node>,
    /// JavaScript event listeners registered via `addEventListener`.
    /// Stores the event type (e.g. "click") and the function definition.
    pub event_listeners: Vec<(String, crate::js::types::JsFunction)>,
}

#[derive(Debug)]
pub struct TextNode {
    pub text:  String,
}

impl Node {

    pub fn dump(&self, depth: usize) {
        let indent = "  ".repeat(depth);
        match self {
            Node::Element(e) => {
                println!("{}<{}> id={:?} class={:?} href={:?}", indent, e.tag, e.id, e.class_name, e.href);
                for child in &e.children { child.dump(depth + 1); }
            }
            Node::Text(t) => println!("{}[TEXT] {:?}", indent, t.text),
        }
    }
}
