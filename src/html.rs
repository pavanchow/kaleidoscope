use crate::dom::{is_void_element, ElementData, Node};
use crate::error::{check_input_size, EngineError, EngineResult, MAX_DOM_DEPTH};
use std::collections::BTreeMap;

/// A tolerant HTML parser. It does not implement the full HTML5 tree
/// construction algorithm, it implements the subset a rendering engine
/// needs: nested elements, attributes, self-closing and void tags, text,
/// and comments, while recovering from unclosed tags instead of failing.
pub struct HtmlParser<'a> {
    input: &'a [u8],
    pos: usize,
}

/// One frame of the open-element stack while building the tree.
struct OpenFrame {
    tag_name: String,
    attributes: BTreeMap<String, String>,
    children: Vec<Node>,
}

pub fn parse(input: &str) -> EngineResult<Node> {
    check_input_size(input)?;
    let mut parser = HtmlParser {
        input: input.as_bytes(),
        pos: 0,
    };
    parser.parse_document()
}

impl<'a> HtmlParser<'a> {
    fn parse_document(&mut self) -> EngineResult<Node> {
        let mut stack: Vec<OpenFrame> = vec![OpenFrame {
            tag_name: "html".to_string(),
            attributes: BTreeMap::new(),
            children: Vec::new(),
        }];

        while !self.eof() {
            if stack.len() > MAX_DOM_DEPTH {
                return Err(EngineError::NestingTooDeep { limit: MAX_DOM_DEPTH });
            }

            if self.starts_with("<!--") {
                let comment = self.consume_comment();
                self.push_child(&mut stack, Node::Comment(comment));
                continue;
            }

            if self.starts_with("<!") {
                // Doctype or bogus declaration, skip to '>'.
                self.consume_until_char(b'>');
                self.advance_one();
                continue;
            }

            if self.peek() == Some(b'<') {
                if self.starts_with("</") {
                    self.handle_close_tag(&mut stack);
                } else if self.looks_like_tag_start() {
                    self.handle_open_tag(&mut stack)?;
                } else {
                    // A lone '<' not followed by a tag name, treat as text.
                    let text = self.consume_literal_lt();
                    self.push_child(&mut stack, Node::Text(text));
                }
                continue;
            }

            let text = self.consume_text();
            if !text.is_empty() {
                self.push_child(&mut stack, Node::Text(text));
            }
        }

        // Recover from any tags left unclosed at EOF: fold them into their
        // parents in order, deepest first.
        while stack.len() > 1 {
            let frame = stack.pop().unwrap();
            let node = frame_to_node(frame);
            stack.last_mut().unwrap().children.push(node);
        }

        let root = stack.pop().unwrap();
        let root_node = frame_to_node(root);
        // If the document already declared its own <html> element as the
        // sole top-level node, use it directly instead of double-wrapping
        // it inside the synthetic root.
        if let Node::Element(root_elem) = &root_node {
            let element_children: Vec<&Node> = root_elem
                .children
                .iter()
                .filter(|c| c.as_element().is_some())
                .collect();
            if let [only] = element_children.as_slice() {
                if only.as_element().unwrap().tag_name == "html" {
                    return Ok((*only).clone());
                }
            }
        }
        Ok(root_node)
    }

    fn push_child(&mut self, stack: &mut [OpenFrame], node: Node) {
        stack.last_mut().unwrap().children.push(node);
    }

    fn handle_close_tag(&mut self, stack: &mut Vec<OpenFrame>) {
        self.pos += 2; // consume "</"
        let name = self.consume_tag_name();
        self.consume_until_char(b'>');
        self.advance_one(); // consume '>'
        let name = name.to_lowercase();
        if name.is_empty() {
            return;
        }

        // Find a matching open ancestor. If none exists, this close tag is
        // spurious, ignore it rather than corrupting the tree.
        if let Some(idx) = stack.iter().rposition(|f| f.tag_name == name) {
            if idx == 0 {
                return; // never close the synthetic root
            }
            while stack.len() > idx + 1 {
                let frame = stack.pop().unwrap();
                let node = frame_to_node(frame);
                stack.last_mut().unwrap().children.push(node);
            }
            let frame = stack.pop().unwrap();
            let node = frame_to_node(frame);
            stack.last_mut().unwrap().children.push(node);
        }
    }

    fn handle_open_tag(&mut self, stack: &mut Vec<OpenFrame>) -> EngineResult<()> {
        self.pos += 1; // consume '<'
        let name = self.consume_tag_name().to_lowercase();
        if name.is_empty() {
            // Not actually a tag, treat the '<' as literal text.
            self.push_child(stack, Node::Text("<".to_string()));
            return Ok(());
        }

        let attributes = self.consume_attributes();
        self.skip_whitespace();
        let mut self_closing = false;
        if self.peek() == Some(b'/') {
            self_closing = true;
            self.advance_one();
        }
        if self.peek() == Some(b'>') {
            self.advance_one();
        }

        if self_closing || is_void_element(&name) {
            self.push_child(stack, Node::element(&name, attributes, Vec::new()));
            return Ok(());
        }

        // Raw text elements: their content is not parsed as markup.
        if name == "script" || name == "style" {
            let raw = self.consume_raw_text_until_close(&name);
            let node = Node::element(&name, attributes, vec![Node::text(&raw)]);
            self.push_child(stack, node);
            return Ok(());
        }

        stack.push(OpenFrame {
            tag_name: name,
            attributes,
            children: Vec::new(),
        });
        Ok(())
    }

    fn consume_raw_text_until_close(&mut self, tag: &str) -> String {
        let closer = format!("</{tag}");
        let start = self.pos;
        loop {
            if self.eof() {
                break;
            }
            if self.rest_lower_starts_with(&closer) {
                break;
            }
            self.pos += 1;
        }
        let text = String::from_utf8_lossy(&self.input[start..self.pos]).to_string();
        // consume the closing tag itself
        if self.rest_lower_starts_with(&closer) {
            self.consume_until_char(b'>');
            self.advance_one();
        }
        text
    }

    fn rest_lower_starts_with(&self, pat: &str) -> bool {
        let end = (self.pos + pat.len()).min(self.input.len());
        let slice = &self.input[self.pos..end];
        if slice.len() < pat.len() {
            return false;
        }
        String::from_utf8_lossy(slice).to_lowercase() == pat.to_lowercase()
    }

    fn consume_tag_name(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'-' || c == b':' {
                self.pos += 1;
            } else {
                break;
            }
        }
        String::from_utf8_lossy(&self.input[start..self.pos]).to_string()
    }

    fn consume_attributes(&mut self) -> BTreeMap<String, String> {
        let mut attrs = BTreeMap::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                None => break,
                Some(b'>') | Some(b'/') => break,
                _ => {}
            }
            let name_start = self.pos;
            while let Some(c) = self.peek() {
                if c == b'=' || c == b'>' || c == b'/' || c.is_ascii_whitespace() {
                    break;
                }
                self.pos += 1;
            }
            if self.pos == name_start {
                // Could not make progress, avoid an infinite loop on junk.
                self.pos += 1;
                continue;
            }
            let name = String::from_utf8_lossy(&self.input[name_start..self.pos]).to_lowercase();
            self.skip_whitespace();
            let value = if self.peek() == Some(b'=') {
                self.advance_one();
                self.skip_whitespace();
                self.consume_attr_value()
            } else {
                String::new()
            };
            if !name.is_empty() {
                attrs.insert(name, value);
            }
        }
        attrs
    }

    fn consume_attr_value(&mut self) -> String {
        match self.peek() {
            Some(q @ b'"') | Some(q @ b'\'') => {
                self.advance_one();
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if c == q {
                        break;
                    }
                    self.pos += 1;
                }
                let value = String::from_utf8_lossy(&self.input[start..self.pos]).to_string();
                if self.peek() == Some(q) {
                    self.advance_one();
                }
                value
            }
            _ => {
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if c.is_ascii_whitespace() || c == b'>' {
                        break;
                    }
                    self.pos += 1;
                }
                String::from_utf8_lossy(&self.input[start..self.pos]).to_string()
            }
        }
    }

    fn consume_comment(&mut self) -> String {
        self.pos += 4; // consume "<!--"
        let start = self.pos;
        while !self.eof() && !self.starts_with("-->") {
            self.pos += 1;
        }
        let text = String::from_utf8_lossy(&self.input[start..self.pos]).to_string();
        if self.starts_with("-->") {
            self.pos += 3;
        }
        text
    }

    fn consume_text(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b'<' {
                break;
            }
            self.pos += 1;
        }
        decode_entities(&String::from_utf8_lossy(&self.input[start..self.pos]))
    }

    fn consume_literal_lt(&mut self) -> String {
        self.pos += 1;
        "<".to_string()
    }

    fn looks_like_tag_start(&self) -> bool {
        match self.input.get(self.pos + 1) {
            Some(c) => c.is_ascii_alphabetic(),
            None => false,
        }
    }

    fn consume_until_char(&mut self, target: u8) {
        while let Some(c) = self.peek() {
            if c == target {
                return;
            }
            self.pos += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn advance_one(&mut self) {
        if !self.eof() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn starts_with(&self, pat: &str) -> bool {
        let bytes = pat.as_bytes();
        let end = self.pos + bytes.len();
        end <= self.input.len() && &self.input[self.pos..end] == bytes
    }
}

fn frame_to_node(frame: OpenFrame) -> Node {
    let mut data = ElementData::new(frame.tag_name, frame.attributes);
    data.children = frame.children;
    Node::Element(data)
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&nbsp;", "\u{a0}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_children(html: &str) -> Vec<Node> {
        let doc = parse(html).unwrap();
        doc.as_element().unwrap().children.clone()
    }

    #[test]
    fn parses_nested_elements() {
        let doc = parse("<div><p>hi</p></div>").unwrap();
        let root = doc.as_element().unwrap();
        let div = root.children[0].as_element().unwrap();
        assert_eq!(div.tag_name, "div");
        let p = div.children[0].as_element().unwrap();
        assert_eq!(p.tag_name, "p");
        assert_eq!(p.children[0], Node::text("hi"));
    }

    #[test]
    fn parses_attributes() {
        let doc = parse("<div id=\"main\" class=\"a b\"></div>").unwrap();
        let root = doc.as_element().unwrap();
        let div = root.children[0].as_element().unwrap();
        assert_eq!(div.id(), Some("main"));
        assert_eq!(div.classes(), vec!["a", "b"]);
    }

    #[test]
    fn handles_self_closing_and_void_tags() {
        let children = root_children("<br><img src=\"x.png\"/><input type=\"text\">");
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].as_element().unwrap().tag_name, "br");
        assert!(children[0].as_element().unwrap().children.is_empty());
        assert_eq!(children[1].as_element().unwrap().tag_name, "img");
        assert_eq!(children[2].as_element().unwrap().tag_name, "input");
    }

    #[test]
    fn recovers_from_unclosed_tag_without_panicking() {
        let doc = parse("<div><p>unclosed paragraph<div>next</div>").unwrap();
        let root = doc.as_element().unwrap();
        assert!(!root.children.is_empty());
    }

    #[test]
    fn parses_comments_and_text() {
        let children = root_children("hello <!-- a comment --> world");
        assert_eq!(children.len(), 3);
        assert_eq!(children[0], Node::text("hello "));
        assert_eq!(children[1], Node::Comment(" a comment ".to_string()));
        assert_eq!(children[2], Node::text(" world"));
    }
}
