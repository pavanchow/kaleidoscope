# Kaleidoscope

**Kaleidoscope is a browser engine in Rust, built from scratch, small enough to read end to end.**

Real browser engines run tens of millions of lines. Kaleidoscope is the same pipeline, HTML to DOM to CSS to style to layout to paint, at a size where you can trace exactly how a box on screen got its position and its color. Hand it an HTML file and a stylesheet and it produces a painted image, or a printed layout tree so you can inspect the box model directly.

## An HTML parser and CSS parser from scratch

The HTML parser is tolerant. It builds a DOM tree out of elements, attributes, text, and comments, handles nested tags, self-closing and void elements, and recovers from an unclosed tag instead of failing. The CSS parser turns a stylesheet into rules: type, id, class, and universal selectors, descendant combinators, and declarations for `display`, `width`, `height`, `margin`, `padding`, `border-width`, `border-color`, `background`/`background-color`, `color`, and `font-size`. Lengths parse in px, colors parse as `#rgb`, `#rrggbb`, and a handful of named colors.

## The cascade, correctly ordered

Rules are matched against every DOM node and sorted by specificity, then by source order, so an id beats a class beats a type, and a later rule of equal specificity wins over an earlier one. An inline `style="..."` attribute always wins over the stylesheet, matching real cascade behavior.

## Box model layout without a framework

Layout implements the standard block box model: each box takes its containing block's width, stacks its block children vertically, and derives its own height from its content, honoring margin, border, and padding with the usual equation:

```
width + padding + border + margin = containing block width
```

`auto` width fills the remaining space, `auto` margins share it. The result is a layout tree of boxes, each with exact `x`, `y`, `width`, `height` in pixels.

## Paint: display list to pixels

The layout tree walks into a display list of solid rectangles, backgrounds and borders, then the display list rasterizes into an RGBA pixel buffer and writes out as a PPM (P6) image, viewable with no image crate involved.

## Resource bounds from the start

A maximum input size, a maximum DOM and CSS nesting depth, and bounded layout recursion are enforced from the first byte. Malformed or pathological input returns a typed `EngineError`, never a panic.

## Usage

```
kaleidoscope render page.html --css style.css --width 800 --height 600 -o out.ppm
kaleidoscope layout page.html --css style.css --width 800
```

`render` writes a painted PPM image. `layout` prints the box tree, one line per box, with tag name and exact pixel coordinates, so the box model is directly inspectable.

## Live demo

`docs/index.html` runs the same pipeline in JavaScript, in the browser, live on every keystroke: edit the HTML or CSS and watch the parse, cascade, layout, and paint stages run again on a `<canvas>`, with an optional box-outline overlay so you can see the layout tree drawn over the render.

## Testing

```
cargo test
```

Tests pin the exact behavior of every stage: the DOM tree shape for nested and malformed HTML, rule and specificity ordering for CSS, computed values and inline-style overrides for the cascade, exact box coordinates for a known layout (including a case with padding and border), the display list and pixel buffer for a colored box, and typed-error recovery for pathological nesting.

By Pavan Nallamothu (pavanchow)
