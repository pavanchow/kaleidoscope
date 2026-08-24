use std::collections::BTreeMap;

/// A single node in the DOM tree. Kaleidoscope keeps only the two node
/// kinds a rendering pipeline actually needs: elements (with a tag name,
/// an attribute map, and children) and text runs.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Element(ElementData),
    Text(String),
    Comment(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElementData {
    pub tag_name: String,
    pub attributes: BTreeMap<String, String>,
    pub children: Vec<Node>,
}

impl ElementData {
    pub fn new(tag_name: String, attributes: BTreeMap<String, String>) -> Self {
        ElementData {
            tag_name,
            attributes,
            children: Vec::new(),
        }
    }

    pub fn id(&self) -> Option<&str> {
        self.attributes.get("id").map(|s| s.as_str())
    }

    pub fn classes(&self) -> Vec<&str> {
        match self.attributes.get("class") {
            Some(s) => s.split_whitespace().collect(),
            None => Vec::new(),
        }
    }

    pub fn has_class(&self, name: &str) -> bool {
        self.classes().iter().any(|c| *c == name)
    }
}

impl Node {
    pub fn element(tag_name: &str, attributes: BTreeMap<String, String>, children: Vec<Node>) -> Node {
        let mut data = ElementData::new(tag_name.to_string(), attributes);
        data.children = children;
        Node::Element(data)
    }

    pub fn text(s: &str) -> Node {
        Node::Text(s.to_string())
    }

    pub fn as_element(&self) -> Option<&ElementData> {
        match self {
            Node::Element(e) => Some(e),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn children(&self) -> &[Node] {
        match self {
            Node::Element(e) => &e.children,
            _ => &[],
        }
    }
}

/// Void elements never have children or a closing tag, per the HTML spec.
pub fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}
