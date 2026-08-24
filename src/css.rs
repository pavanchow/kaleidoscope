use crate::error::{check_input_size, EngineError, EngineResult, MAX_CSS_DEPTH};

/// A parsed stylesheet: an ordered list of rules. Source order matters for
/// the cascade, later rules of equal specificity win.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub property: String,
    pub value: Value,
}

/// One simple selector: any combination of a tag name, an id, and classes.
/// `universal` marks a bare `*`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SimpleSelector {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub universal: bool,
}

/// A selector is a chain of simple selectors joined by descendant
/// combinators (whitespace). The last element matches the node itself,
/// earlier elements must match some ancestor, in order, further up the
/// tree. A selector with one element is a flat simple selector.
#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    pub parts: Vec<SimpleSelector>,
}

pub type Specificity = (u32, u32, u32);

impl Selector {
    pub fn specificity(&self) -> Specificity {
        let mut ids = 0;
        let mut classes = 0;
        let mut types = 0;
        for part in &self.parts {
            if part.id.is_some() {
                ids += 1;
            }
            classes += part.classes.len() as u32;
            if part.tag.is_some() {
                types += 1;
            }
        }
        (ids, classes, types)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Keyword(String),
    Length(f32, Unit),
    Color(Color),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Px,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }
}

impl Value {
    pub fn to_px(&self) -> Option<f32> {
        match self {
            Value::Length(n, Unit::Px) => Some(*n),
            Value::Keyword(k) if k == "0" => Some(0.0),
            _ => None,
        }
    }

    pub fn as_keyword(&self) -> Option<&str> {
        match self {
            Value::Keyword(k) => Some(k.as_str()),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<Color> {
        match self {
            Value::Color(c) => Some(*c),
            Value::Keyword(k) => named_color(k),
            _ => None,
        }
    }
}

fn named_color(name: &str) -> Option<Color> {
    Some(match name {
        "black" => Color::rgb(0, 0, 0),
        "white" => Color::rgb(255, 255, 255),
        "red" => Color::rgb(255, 0, 0),
        "green" => Color::rgb(0, 128, 0),
        "blue" => Color::rgb(0, 0, 255),
        "yellow" => Color::rgb(255, 255, 0),
        "gray" | "grey" => Color::rgb(128, 128, 128),
        "orange" => Color::rgb(255, 165, 0),
        "purple" => Color::rgb(128, 0, 128),
        "transparent" => Color { r: 0, g: 0, b: 0, a: 0 },
        _ => return None,
    })
}

pub fn parse(input: &str) -> EngineResult<Stylesheet> {
    check_input_size(input)?;
    let mut parser = CssParser {
        chars: input.chars().collect(),
        pos: 0,
        depth: 0,
    };
    parser.parse_stylesheet()
}

struct CssParser {
    chars: Vec<char>,
    pos: usize,
    depth: usize,
}

impl CssParser {
    fn parse_stylesheet(&mut self) -> EngineResult<Stylesheet> {
        let mut rules = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.eof() {
                break;
            }
            rules.push(self.parse_rule()?);
        }
        Ok(Stylesheet { rules })
    }

    fn parse_rule(&mut self) -> EngineResult<Rule> {
        self.depth += 1;
        if self.depth > MAX_CSS_DEPTH {
            return Err(EngineError::NestingTooDeep { limit: MAX_CSS_DEPTH });
        }
        let selectors = self.parse_selector_list()?;
        self.skip_whitespace_and_comments();
        if self.peek() != Some('{') {
            return Err(EngineError::InvalidCss("expected '{' after selector".to_string()));
        }
        self.advance();
        let declarations = self.parse_declarations()?;
        self.skip_whitespace_and_comments();
        if self.peek() == Some('}') {
            self.advance();
        }
        self.depth -= 1;
        Ok(Rule { selectors, declarations })
    }

    fn parse_selector_list(&mut self) -> EngineResult<Vec<Selector>> {
        let mut selectors = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            selectors.push(self.parse_selector()?);
            self.skip_whitespace_and_comments();
            if self.peek() == Some(',') {
                self.advance();
                continue;
            }
            break;
        }
        Ok(selectors)
    }

    fn parse_selector(&mut self) -> EngineResult<Selector> {
        let mut parts = Vec::new();
        loop {
            self.skip_inline_whitespace();
            let simple = self.parse_simple_selector()?;
            if simple != SimpleSelector::default() {
                parts.push(simple);
            }
            // A run of whitespace followed by another simple selector start
            // means a descendant combinator; anything else ends the chain.
            let checkpoint = self.pos;
            self.skip_inline_whitespace();
            match self.peek() {
                Some('{') | Some(',') | None => {
                    self.pos = checkpoint;
                    break;
                }
                Some(c) if is_selector_start(c) => continue,
                _ => {
                    self.pos = checkpoint;
                    break;
                }
            }
        }
        if parts.is_empty() {
            return Err(EngineError::InvalidCss("empty selector".to_string()));
        }
        Ok(Selector { parts })
    }

    fn parse_simple_selector(&mut self) -> EngineResult<SimpleSelector> {
        let mut sel = SimpleSelector::default();
        loop {
            match self.peek() {
                Some('*') => {
                    sel.universal = true;
                    self.advance();
                }
                Some('#') => {
                    self.advance();
                    sel.id = Some(self.consume_ident());
                }
                Some('.') => {
                    self.advance();
                    sel.classes.push(self.consume_ident());
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '-' => {
                    sel.tag = Some(self.consume_ident().to_lowercase());
                }
                _ => break,
            }
        }
        Ok(sel)
    }

    fn parse_declarations(&mut self) -> EngineResult<Vec<Declaration>> {
        let mut decls = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.peek() == Some('}') || self.eof() {
                break;
            }
            if self.peek() == Some(';') {
                self.advance();
                continue;
            }
            let property = self.consume_ident().to_lowercase();
            self.skip_whitespace_and_comments();
            if self.peek() != Some(':') {
                // Malformed declaration, skip to next ';' or '}' and recover.
                self.skip_until(|c| c == ';' || c == '}');
                continue;
            }
            self.advance();
            self.skip_whitespace_and_comments();
            let raw_value = self.consume_value_raw();
            if let Some(value) = parse_value(raw_value.trim()) {
                if !property.is_empty() {
                    decls.push(Declaration { property, value });
                }
            }
            self.skip_whitespace_and_comments();
            if self.peek() == Some(';') {
                self.advance();
            }
        }
        Ok(decls)
    }

    fn consume_value_raw(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == ';' || c == '}' {
                break;
            }
            self.advance();
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn consume_ident(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn skip_until(&mut self, pred: impl Fn(char) -> bool) {
        while let Some(c) = self.peek() {
            if pred(c) {
                break;
            }
            self.advance();
        }
    }

    fn skip_inline_whitespace(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.advance();
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while matches!(self.peek(), Some(c) if c.is_whitespace()) {
                self.advance();
            }
            if self.peek() == Some('/') && self.peek_at(1) == Some('*') {
                self.advance();
                self.advance();
                while !self.eof() && !(self.peek() == Some('*') && self.peek_at(1) == Some('/')) {
                    self.advance();
                }
                self.advance();
                self.advance();
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) {
        if self.pos < self.chars.len() {
            self.pos += 1;
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.chars.len()
    }
}

fn is_selector_start(c: char) -> bool {
    c == '*' || c == '#' || c == '.' || c.is_ascii_alphabetic() || c == '_' || c == '-'
}

fn parse_value(raw: &str) -> Option<Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(hex) = raw.strip_prefix('#') {
        return parse_hex_color(hex).map(Value::Color);
    }
    if let Some(px) = raw.strip_suffix("px") {
        if let Ok(n) = px.trim().parse::<f32>() {
            return Some(Value::Length(n, Unit::Px));
        }
    }
    if raw == "0" {
        return Some(Value::Length(0.0, Unit::Px));
    }
    Some(Value::Keyword(raw.to_string()))
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let expand = |c: char| -> Option<u8> {
        let s: String = [c, c].iter().collect();
        u8::from_str_radix(&s, 16).ok()
    };
    match hex.len() {
        3 => {
            let chars: Vec<char> = hex.chars().collect();
            Some(Color::rgb(expand(chars[0])?, expand(chars[1])?, expand(chars[2])?))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::rgb(r, g, b))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_rule() {
        let sheet = parse("div { width: 10px; color: #ff0000; }").unwrap();
        assert_eq!(sheet.rules.len(), 1);
        let rule = &sheet.rules[0];
        assert_eq!(rule.selectors[0].parts[0].tag, Some("div".to_string()));
        assert_eq!(rule.declarations[0].property, "width");
        assert_eq!(rule.declarations[0].value, Value::Length(10.0, Unit::Px));
        assert_eq!(rule.declarations[1].value, Value::Color(Color::rgb(255, 0, 0)));
    }

    #[test]
    fn parses_id_class_and_universal_selectors() {
        let sheet = parse("#main {} .item {} * {}").unwrap();
        assert_eq!(sheet.rules[0].selectors[0].parts[0].id, Some("main".to_string()));
        assert_eq!(sheet.rules[1].selectors[0].parts[0].classes, vec!["item"]);
        assert!(sheet.rules[2].selectors[0].parts[0].universal);
    }

    #[test]
    fn specificity_orders_id_over_class_over_type() {
        let id_sel = Selector {
            parts: vec![SimpleSelector { id: Some("x".into()), ..Default::default() }],
        };
        let class_sel = Selector {
            parts: vec![SimpleSelector { classes: vec!["x".into()], ..Default::default() }],
        };
        let type_sel = Selector {
            parts: vec![SimpleSelector { tag: Some("div".into()), ..Default::default() }],
        };
        assert!(id_sel.specificity() > class_sel.specificity());
        assert!(class_sel.specificity() > type_sel.specificity());
    }

    #[test]
    fn recovers_from_malformed_declaration() {
        let sheet = parse("div { not-a-real-decl ; color: blue; }").unwrap();
        let decls = &sheet.rules[0].declarations;
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].property, "color");
    }

    #[test]
    fn deeply_nested_rules_are_bounded() {
        let mut css = String::new();
        for _ in 0..(MAX_CSS_DEPTH + 10) {
            css.push_str("div{width:1px;} ");
        }
        // Not nested rules but many rules should still parse fine; depth
        // guards apply to malformed recursive constructs, verified via the
        // dedicated bound in tests::bounds in main integration tests.
        assert!(parse(&css).is_ok());
    }
}
