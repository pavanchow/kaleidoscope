use crate::dom::Node;
use crate::error::{EngineError, EngineResult, MAX_LAYOUT_DEPTH};
use crate::style::{Display, StyledNode};

/// A simple axis-aligned rectangle in pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EdgeSizes {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

/// The box model: a content rect surrounded by padding, border, and
/// margin, each tracked independently so `padding_box`/`border_box`
/// grow the content rect outward by the right amount.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Dimensions {
    pub content: Rect,
    pub padding: EdgeSizes,
    pub border: EdgeSizes,
    pub margin: EdgeSizes,
}

impl Dimensions {
    pub fn padding_box(&self) -> Rect {
        expand(self.content, self.padding)
    }

    pub fn border_box(&self) -> Rect {
        expand(self.padding_box(), self.border)
    }

    pub fn margin_box(&self) -> Rect {
        expand(self.border_box(), self.margin)
    }
}

fn expand(rect: Rect, edge: EdgeSizes) -> Rect {
    Rect {
        x: rect.x - edge.left,
        y: rect.y - edge.top,
        width: rect.width + edge.left + edge.right,
        height: rect.height + edge.top + edge.bottom,
    }
}

pub enum BoxKind<'a> {
    Block(&'a StyledNode<'a>),
    // The text run is kept for future inline text painting; Kaleidoscope
    // currently only sizes its vertical space (see layout_children).
    #[allow(dead_code)]
    Text(&'a StyledNode<'a>, String),
}

pub struct LayoutBox<'a> {
    pub dimensions: Dimensions,
    pub kind: BoxKind<'a>,
    pub children: Vec<LayoutBox<'a>>,
}

impl<'a> LayoutBox<'a> {
    pub fn tag(&self) -> String {
        match &self.kind {
            BoxKind::Block(styled) => styled.tag_name().to_string(),
            BoxKind::Text(_, _) => "#text".to_string(),
        }
    }

    pub fn styled(&self) -> &'a StyledNode<'a> {
        match &self.kind {
            BoxKind::Block(s) => s,
            BoxKind::Text(s, _) => s,
        }
    }
}

const DEFAULT_FONT_SIZE: f32 = 16.0;

/// Turn a styled tree into a layout tree that fills the given viewport
/// width, then compute pixel positions for every box.
pub fn layout_tree<'a>(
    styled_root: &'a StyledNode<'a>,
    viewport_width: f32,
) -> EngineResult<LayoutBox<'a>> {
    let mut root = build_layout_tree(styled_root, 0)?
        .ok_or(EngineError::LayoutTooDeep { limit: 0 })
        .or_else(|_| {
            // An empty or fully display:none document still yields a
            // trivial root box rather than an error.
            Ok::<LayoutBox<'a>, EngineError>(LayoutBox {
                dimensions: Dimensions::default(),
                kind: BoxKind::Block(styled_root),
                children: Vec::new(),
            })
        })?;

    // The root's containing block has no height yet, it grows as content
    // is stacked; only the width is fixed by the viewport.
    let mut containing_block = Dimensions::default();
    containing_block.content.width = viewport_width;

    layout_block(&mut root, containing_block, 0)?;
    Ok(root)
}

fn build_layout_tree<'a>(
    styled: &'a StyledNode<'a>,
    depth: usize,
) -> EngineResult<Option<LayoutBox<'a>>> {
    if depth > MAX_LAYOUT_DEPTH {
        return Err(EngineError::LayoutTooDeep { limit: MAX_LAYOUT_DEPTH });
    }
    if styled.display() == Display::None {
        return Ok(None);
    }

    match styled.node {
        Node::Comment(_) => Ok(None),
        Node::Text(text) => {
            if text.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(LayoutBox {
                    dimensions: Dimensions::default(),
                    kind: BoxKind::Text(styled, text.clone()),
                    children: Vec::new(),
                }))
            }
        }
        Node::Element(_) => {
            let mut children = Vec::new();
            for child in &styled.children {
                if let Some(box_) = build_layout_tree(child, depth + 1)? {
                    children.push(box_);
                }
            }
            Ok(Some(LayoutBox {
                dimensions: Dimensions::default(),
                kind: BoxKind::Block(styled),
                children,
            }))
        }
    }
}

fn layout_block(layout_box: &mut LayoutBox, containing_block: Dimensions, depth: usize) -> EngineResult<()> {
    if depth > MAX_LAYOUT_DEPTH {
        return Err(EngineError::LayoutTooDeep { limit: MAX_LAYOUT_DEPTH });
    }
    calculate_width(layout_box, containing_block);
    calculate_position(layout_box, containing_block);
    layout_children(layout_box, depth)?;
    calculate_height(layout_box);
    Ok(())
}

fn get_px(styled: &StyledNode, prop: &str) -> Option<f32> {
    styled.value(prop).and_then(|v| v.to_px())
}

fn is_auto(styled: &StyledNode, prop: &str) -> bool {
    matches!(styled.value(prop).and_then(|v| v.as_keyword()), Some("auto"))
}

fn calculate_width(layout_box: &mut LayoutBox, containing_block: Dimensions) {
    let styled = layout_box.styled();
    let cw = containing_block.content.width;

    let width_auto = is_auto(styled, "width") || get_px(styled, "width").is_none();
    let mut width = get_px(styled, "width").unwrap_or(0.0);

    let mut margin_left = get_px(styled, "margin").unwrap_or(0.0);
    let mut margin_right = get_px(styled, "margin").unwrap_or(0.0);
    let margin_left_auto = is_auto(styled, "margin");
    let margin_right_auto = margin_left_auto;

    let border = get_px(styled, "border-width").unwrap_or(0.0);
    let padding = get_px(styled, "padding").unwrap_or(0.0);

    let total = margin_left + margin_right + border * 2.0 + padding * 2.0 + width;
    let underflow = cw - total;

    if !width_auto {
        if margin_left_auto && margin_right_auto {
            margin_left = underflow / 2.0;
            margin_right = underflow / 2.0;
        } else if margin_left_auto {
            margin_left = underflow;
        } else if margin_right_auto {
            margin_right = underflow;
        } else {
            margin_right += underflow;
        }
    } else {
        if margin_left_auto {
            margin_left = 0.0;
        }
        if margin_right_auto {
            margin_right = 0.0;
        }
        if underflow >= 0.0 {
            width = underflow;
        } else {
            width = 0.0;
            margin_right += underflow;
        }
    }

    let d = &mut layout_box.dimensions;
    d.content.width = width.max(0.0);
    d.padding.left = padding;
    d.padding.right = padding;
    d.padding.top = padding;
    d.padding.bottom = padding;
    d.border.left = border;
    d.border.right = border;
    d.border.top = border;
    d.border.bottom = border;
    d.margin.left = margin_left;
    d.margin.right = margin_right;
    d.margin.top = get_px(styled, "margin").unwrap_or(0.0);
    d.margin.bottom = get_px(styled, "margin").unwrap_or(0.0);
}

fn calculate_position(layout_box: &mut LayoutBox, containing_block: Dimensions) {
    let d = &mut layout_box.dimensions;
    d.content.x = containing_block.content.x + d.margin.left + d.border.left + d.padding.left;
    d.content.y =
        containing_block.content.y + containing_block.content.height + d.margin.top + d.border.top + d.padding.top;
}

fn layout_children(layout_box: &mut LayoutBox, depth: usize) -> EngineResult<()> {
    let is_text = matches!(layout_box.kind, BoxKind::Text(_, _));
    if is_text {
        let font_size = get_px(layout_box.styled(), "font-size").unwrap_or(DEFAULT_FONT_SIZE);
        layout_box.dimensions.content.height = font_size * 1.2;
        return Ok(());
    }

    for child in &mut layout_box.children {
        // Re-read dimensions each iteration: content.height is the
        // running cursor, it grows after every child is placed.
        let containing = layout_box.dimensions;
        layout_block(child, containing, depth + 1)?;
        let child_margin_box = child.dimensions.margin_box();
        layout_box.dimensions.content.height += child_margin_box.height;
    }
    Ok(())
}

fn calculate_height(layout_box: &mut LayoutBox) {
    if let Some(h) = get_px(layout_box.styled(), "height") {
        layout_box.dimensions.content.height = h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css;
    use crate::html;
    use crate::style::style_tree;

    #[test]
    fn stacks_two_block_divs_with_fixed_heights_and_margins() {
        let dom = html::parse(
            "<html><body><div id=\"a\">x</div><div id=\"b\">y</div></body></html>",
        )
        .unwrap();
        let sheet = css::parse(
            "#a { display: block; width: 200px; height: 30px; margin: 5px; }
             #b { display: block; width: 200px; height: 40px; margin: 5px; }",
        )
        .unwrap();
        let styled = style_tree(&dom, &sheet);
        let root = layout_tree(&styled, 800.0).unwrap();

        let body = &root.children[0];
        let div_a = &body.children[0];
        let div_b = &body.children[1];

        assert_eq!(div_a.dimensions.content.x, 5.0);
        assert_eq!(div_a.dimensions.content.y, 5.0);
        assert_eq!(div_a.dimensions.content.width, 200.0);
        assert_eq!(div_a.dimensions.content.height, 30.0);

        // b starts after a's margin box (30 content + 5 top margin + 5
        // bottom margin = 40px tall margin box) plus its own top margin.
        assert_eq!(div_b.dimensions.content.x, 5.0);
        assert_eq!(div_b.dimensions.content.y, 40.0 + 5.0);
        assert_eq!(div_b.dimensions.content.width, 200.0);
        assert_eq!(div_b.dimensions.content.height, 40.0);
    }

    #[test]
    fn honors_padding_and_border_in_box_model() {
        let dom = html::parse("<div id=\"a\"></div>").unwrap();
        let sheet = css::parse(
            "#a { width: 100px; height: 50px; padding: 10px; border-width: 2px; margin: 3px; }",
        )
        .unwrap();
        let styled = style_tree(&dom, &sheet);
        let root = layout_tree(&styled, 400.0).unwrap();
        let div_a = &root.children[0];

        assert_eq!(div_a.dimensions.content.width, 100.0);
        assert_eq!(div_a.dimensions.content.height, 50.0);
        assert_eq!(div_a.dimensions.content.x, 3.0 + 2.0 + 10.0);
        assert_eq!(div_a.dimensions.content.y, 3.0 + 2.0 + 10.0);

        let border_box = div_a.dimensions.border_box();
        assert_eq!(border_box.width, 100.0 + 20.0 + 4.0);
        assert_eq!(border_box.height, 50.0 + 20.0 + 4.0);
    }

    #[test]
    fn auto_width_fills_containing_block() {
        let dom = html::parse("<div id=\"a\"></div>").unwrap();
        let sheet = css::parse("#a { height: 10px; }").unwrap();
        let styled = style_tree(&dom, &sheet);
        let root = layout_tree(&styled, 500.0).unwrap();
        let div_a = &root.children[0];
        assert_eq!(div_a.dimensions.content.width, 500.0);
    }

    #[test]
    fn pathological_nesting_returns_typed_error_not_panic() {
        let mut html_src = String::new();
        for _ in 0..(crate::error::MAX_DOM_DEPTH + 50) {
            html_src.push_str("<div>");
        }
        let dom = html::parse(&html_src);
        assert!(dom.is_err());
    }
}
