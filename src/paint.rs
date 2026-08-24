use crate::css::Color;
use crate::layout::{BoxKind, LayoutBox, Rect};
use std::io::{self, Write};

/// A flattened list of solid rectangles ready to rasterize, in paint
/// order (later items draw over earlier ones). Kaleidoscope keeps this
/// deliberately small: backgrounds and borders, no images or gradients.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    SolidColor(Rect, Color),
}

pub type DisplayList = Vec<DisplayItem>;

pub fn build_display_list(layout_box: &LayoutBox) -> DisplayList {
    let mut list = Vec::new();
    render_box(layout_box, &mut list);
    list
}

fn render_box(layout_box: &LayoutBox, list: &mut DisplayList) {
    if let BoxKind::Block(styled) = &layout_box.kind {
        let border = layout_box.dimensions.border;
        if border.left > 0.0 || border.right > 0.0 || border.top > 0.0 || border.bottom > 0.0 {
            let color = styled
                .value("border-color")
                .and_then(|v| v.as_color())
                .unwrap_or(Color::rgb(0, 0, 0));
            list.push(DisplayItem::SolidColor(layout_box.dimensions.border_box(), color));
        }

        let background = styled
            .value("background-color")
            .and_then(|v| v.as_color())
            .or_else(|| styled.value("background").and_then(|v| v.as_color()));
        if let Some(color) = background {
            list.push(DisplayItem::SolidColor(layout_box.dimensions.padding_box(), color));
        }
    }

    for child in &layout_box.children {
        render_box(child, list);
    }
}

/// An RGBA pixel buffer, row-major, top-left origin.
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 4]>,
}

impl Canvas {
    pub fn new(width: usize, height: usize, background: Color) -> Canvas {
        Canvas {
            width,
            height,
            pixels: vec![[background.r, background.g, background.b, background.a]; width * height],
        }
    }

    pub fn paint_item(&mut self, item: &DisplayItem) {
        match item {
            DisplayItem::SolidColor(rect, color) => self.fill_rect(*rect, *color),
        }
    }

    fn fill_rect(&mut self, rect: Rect, color: Color) {
        let x0 = rect.x.max(0.0) as i64;
        let y0 = rect.y.max(0.0) as i64;
        let x1 = (rect.x + rect.width).min(self.width as f32) as i64;
        let y1 = (rect.y + rect.height).min(self.height as f32) as i64;
        for y in y0.max(0)..y1.max(0) {
            for x in x0.max(0)..x1.max(0) {
                if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
                    continue;
                }
                let idx = y as usize * self.width + x as usize;
                self.pixels[idx] = blend(self.pixels[idx], color);
            }
        }
    }

    /// Encode as a binary PPM (P6), viewable with any standard image
    /// viewer with no image crate involved.
    pub fn write_ppm<W: Write>(&self, mut out: W) -> io::Result<()> {
        writeln!(out, "P6")?;
        writeln!(out, "{} {}", self.width, self.height)?;
        writeln!(out, "255")?;
        let mut rgb = Vec::with_capacity(self.width * self.height * 3);
        for px in &self.pixels {
            rgb.push(px[0]);
            rgb.push(px[1]);
            rgb.push(px[2]);
        }
        out.write_all(&rgb)
    }
}

fn blend(dst: [u8; 4], src: Color) -> [u8; 4] {
    if src.a == 255 {
        return [src.r, src.g, src.b, 255];
    }
    if src.a == 0 {
        return dst;
    }
    let a = src.a as f32 / 255.0;
    let r = (src.r as f32 * a + dst[0] as f32 * (1.0 - a)) as u8;
    let g = (src.g as f32 * a + dst[1] as f32 * (1.0 - a)) as u8;
    let b = (src.b as f32 * a + dst[2] as f32 * (1.0 - a)) as u8;
    [r, g, b, 255]
}

pub fn paint(layout_box: &LayoutBox, width: usize, height: usize) -> Canvas {
    let mut canvas = Canvas::new(width, height, Color::rgb(255, 255, 255));
    let list = build_display_list(layout_box);
    for item in &list {
        canvas.paint_item(item);
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css;
    use crate::html;
    use crate::layout::layout_tree;
    use crate::style::style_tree;

    #[test]
    fn display_list_has_rect_at_expected_position_and_color() {
        let dom = html::parse("<div id=\"a\"></div>").unwrap();
        let sheet = css::parse("#a { width: 40px; height: 20px; background-color: #ff0000; }").unwrap();
        let styled = style_tree(&dom, &sheet);
        let root = layout_tree(&styled, 200.0).unwrap();
        let list = build_display_list(&root);

        let found = list.iter().any(|item| match item {
            DisplayItem::SolidColor(rect, color) => {
                rect.x == 0.0
                    && rect.y == 0.0
                    && rect.width == 40.0
                    && rect.height == 20.0
                    && *color == Color::rgb(255, 0, 0)
            }
        });
        assert!(found, "expected a red 40x20 rect at (0,0) in {list:?}");
    }

    #[test]
    fn painting_sets_expected_pixels_in_buffer() {
        let dom = html::parse("<div id=\"a\"></div>").unwrap();
        let sheet = css::parse("#a { width: 2px; height: 2px; background-color: #00ff00; }").unwrap();
        let styled = style_tree(&dom, &sheet);
        let root = layout_tree(&styled, 10.0).unwrap();
        let canvas = paint(&root, 10, 10);

        // The 2x2 green box sits at the top-left origin.
        assert_eq!(canvas.pixels[0], [0, 255, 0, 255]);
        assert_eq!(canvas.pixels[1], [0, 255, 0, 255]);
        assert_eq!(canvas.pixels[10], [0, 255, 0, 255]); // row 1, col 0
        // Outside the box stays background white.
        assert_eq!(canvas.pixels[3], [255, 255, 255, 255]);
    }

    #[test]
    fn ppm_output_has_correct_header() {
        let canvas = Canvas::new(3, 2, Color::rgb(1, 2, 3));
        let mut buf = Vec::new();
        canvas.write_ppm(&mut buf).unwrap();
        let text = String::from_utf8_lossy(&buf[..15]);
        assert!(text.starts_with("P6\n3 2\n255\n"));
    }
}
