use crate::css::{Declaration, SimpleSelector, Specificity, Stylesheet, Value};
use crate::dom::{ElementData, Node};
use std::collections::BTreeMap;

/// A DOM node paired with the computed values won by the cascade. Text
/// nodes carry no properties of their own, they inherit visually through
/// layout instead.
pub struct StyledNode<'a> {
    pub node: &'a Node,
    pub specified: BTreeMap<String, Value>,
    pub children: Vec<StyledNode<'a>>,
}

impl<'a> StyledNode<'a> {
    pub fn tag_name(&self) -> &str {
        match self.node {
            Node::Element(e) => e.tag_name.as_str(),
            Node::Text(_) => "#text",
            Node::Comment(_) => "#comment",
        }
    }

    pub fn value(&self, prop: &str) -> Option<&Value> {
        self.specified.get(prop)
    }

    pub fn display(&self) -> Display {
        match self.value("display").and_then(|v| v.as_keyword()) {
            Some("inline") => Display::Inline,
            Some("none") => Display::None,
            _ => Display::Block,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Block,
    Inline,
    None,
}

/// Build a styled tree by matching the stylesheet against every element,
/// respecting specificity, source order, and inline style overrides.
pub fn style_tree<'a>(root: &'a Node, stylesheet: &Stylesheet) -> StyledNode<'a> {
    style_node(root, stylesheet, &[])
}

fn style_node<'a>(node: &'a Node, stylesheet: &Stylesheet, ancestors: &[&'a ElementData]) -> StyledNode<'a> {
    match node {
        Node::Element(elem) => {
            let specified = specified_values(elem, stylesheet, ancestors);
            let mut next_ancestors = ancestors.to_vec();
            next_ancestors.push(elem);
            let children = elem
                .children
                .iter()
                .map(|child| style_node(child, stylesheet, &next_ancestors))
                .collect();
            StyledNode { node, specified, children }
        }
        Node::Text(_) | Node::Comment(_) => StyledNode {
            node,
            specified: BTreeMap::new(),
            children: Vec::new(),
        },
    }
}

fn specified_values(
    elem: &ElementData,
    stylesheet: &Stylesheet,
    ancestors: &[&ElementData],
) -> BTreeMap<String, Value> {
    let mut matched: Vec<(Specificity, usize, &Declaration)> = Vec::new();

    for (rule_order, rule) in stylesheet.rules.iter().enumerate() {
        for selector in &rule.selectors {
            if selector_matches(selector.parts.as_slice(), ancestors, elem) {
                let spec = selector.specificity();
                for decl in &rule.declarations {
                    matched.push((spec, rule_order, decl));
                }
                break; // one match per rule is enough to include its declarations
            }
        }
    }

    // Stable ordering by (specificity, source order) so later, more
    // specific declarations win, matching the CSS cascade.
    matched.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut result = BTreeMap::new();
    for (_, _, decl) in matched {
        result.insert(decl.property.clone(), decl.value.clone());
    }

    // Inline style wins over every stylesheet rule regardless of
    // specificity, matching real cascade behavior.
    if let Some(inline) = elem.attributes.get("style") {
        for decl in parse_inline_declarations(inline) {
            result.insert(decl.property, decl.value);
        }
    }

    result
}

fn selector_matches(parts: &[SimpleSelector], ancestors: &[&ElementData], elem: &ElementData) -> bool {
    let Some((last, rest)) = parts.split_last() else {
        return false;
    };
    if !simple_matches(last, elem) {
        return false;
    }
    if rest.is_empty() {
        return true;
    }
    // Walk ancestors from closest to furthest, matching remaining parts
    // back to front, each one against some ancestor further up than the
    // previous match.
    let mut remaining = rest;
    let mut idx = ancestors.len();
    while let Some((needle, shorter)) = remaining.split_last() {
        let mut found = false;
        while idx > 0 {
            idx -= 1;
            if simple_matches(needle, ancestors[idx]) {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
        remaining = shorter;
    }
    true
}

fn simple_matches(sel: &SimpleSelector, elem: &ElementData) -> bool {
    if let Some(tag) = &sel.tag {
        if tag != &elem.tag_name {
            return false;
        }
    }
    if let Some(id) = &sel.id {
        if Some(id.as_str()) != elem.id() {
            return false;
        }
    }
    for class in &sel.classes {
        if !elem.has_class(class) {
            return false;
        }
    }
    if !sel.universal && sel.tag.is_none() && sel.id.is_none() && sel.classes.is_empty() {
        return false;
    }
    true
}

fn parse_inline_declarations(style_attr: &str) -> Vec<Declaration> {
    let wrapped = format!("x{{{style_attr}}}");
    match crate::css::parse(&wrapped) {
        Ok(sheet) => sheet
            .rules
            .into_iter()
            .flat_map(|r| r.declarations)
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css;
    use crate::html;

    #[test]
    fn attaches_computed_values_to_matching_nodes() {
        let dom = html::parse("<div class=\"box\"></div>").unwrap();
        let sheet = css::parse(".box { width: 50px; color: #112233; }").unwrap();
        let styled = style_tree(&dom, &sheet);
        let div = &styled.children[0];
        assert_eq!(div.value("width").unwrap().to_px(), Some(50.0));
        assert_eq!(
            div.value("color").unwrap().as_color(),
            Some(css::Color::rgb(0x11, 0x22, 0x33))
        );
    }

    #[test]
    fn inline_style_overrides_stylesheet() {
        let dom = html::parse("<div class=\"box\" style=\"width: 5px;\"></div>").unwrap();
        let sheet = css::parse(".box { width: 50px; }").unwrap();
        let styled = style_tree(&dom, &sheet);
        let div = &styled.children[0];
        assert_eq!(div.value("width").unwrap().to_px(), Some(5.0));
    }

    #[test]
    fn id_beats_class_beats_type_in_cascade() {
        let dom = html::parse("<div id=\"x\" class=\"y\"></div>").unwrap();
        let sheet = css::parse("div { color: red; } .y { color: green; } #x { color: blue; }").unwrap();
        let styled = style_tree(&dom, &sheet);
        let div = &styled.children[0];
        assert_eq!(
            div.value("color").unwrap().as_color(),
            Some(css::Color::rgb(0, 0, 255))
        );
    }

    #[test]
    fn later_rule_of_equal_specificity_wins() {
        let dom = html::parse("<div class=\"a\"></div>").unwrap();
        let sheet = css::parse(".a { color: red; } .a { color: blue; }").unwrap();
        let styled = style_tree(&dom, &sheet);
        let div = &styled.children[0];
        assert_eq!(
            div.value("color").unwrap().as_color(),
            Some(css::Color::rgb(0, 0, 255))
        );
    }
}
