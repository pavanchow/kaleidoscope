<img src="docs/logo.svg" alt="Kaleidoscope logo" width="96">

# Kaleidoscope: a browser engine in Rust

Kaleidoscope is a browser engine in Rust, built from scratch and small enough to read end to end: an HTML parser, a CSS parser, the cascade, block layout, and paint, the same pipeline real engines run at tens of millions of lines. Hand it an HTML file and a stylesheet and it produces a painted PNG or PPM image, or a printed layout tree so you can inspect the CSS box model directly. Use it as a readable reference implementation of a browser rendering pipeline where you can trace exactly how a box on screen got its position and its color.

**[Live demo](https://pavanchow.github.io/kaleidoscope/)** · MIT licensed · pure Rust, teaching-grade engine

Built from scratch by [Pavan Nallamothu](https://pavanchow.github.io/) ([LinkedIn](https://www.linkedin.com/in/pavanchow/), [GitHub](https://github.com/pavanchow)).

## An HTML parser and CSS parser from scratch

The HTML parser is tolerant. It builds a DOM tree out of elements, attributes, text, and comments, handles nested tags, self-closing and void elements, and recovers from an unclosed tag instead of failing. The CSS parser turns a stylesheet into rules: type, id, class, and universal selectors, descendant combinators, and declarations for `display`, `width`, `height`, `margin`, `padding`, `border-width`, `border-color`, `background`/`background-color`, `color`, and `font-size`. Lengths parse in px, colors parse as `#rgb`, `#rrggbb`, and a handful of named colors.

## The cascade, correctly ordered

Rules are matched against every DOM node and sorted by specificity, then by source order, so an id beats a class beats a type, and a later rule of equal specificity wins over an earlier one. An inline `style="..."` attribute always wins over the stylesheet, matching real cascade behavior. Inherited properties, `color` and `font-size`, propagate down the tree, so a child and its text pick up a parent's values unless they set their own.

## CSS box model implementation

Layout implements the standard block box model: each box takes its containing block's width, stacks its block children vertically, and derives its own height from its content, honoring margin, border, and padding with the usual equation:

```
width + padding + border + margin = containing block width
```

`auto` width fills the remaining space, `auto` margins share it. The result is a layout tree of boxes, each with exact `x`, `y`, `width`, `height` in pixels.

## Paint: display list to pixels

The layout tree walks into a display list of solid rectangles, backgrounds and borders, then the display list rasterizes into an RGBA pixel buffer with integer alpha compositing. It writes out as a PNG (a dependency-free encoder that opens in any native image viewer) or a PPM (P6) image, with no image crate involved.

## Resource bounds from the start

A maximum input size, a maximum DOM and CSS nesting depth, and bounded layout recursion are enforced from the first byte. Malformed or pathological input returns a typed `EngineError`, never a panic.

## Kaleidoscope vs Chromium, Servo, and Robinson

Kaleidoscope is not trying to render the whole web. It is trying to be the one engine you can actually read. The comparison is about comprehensibility, not coverage.

| | Kaleidoscope | Chromium (Blink) | Servo | Robinson |
|---|---|---|---|---|
| Language | modern Rust | C++ | Rust | Rust |
| Size | about 2,000 lines | tens of millions | very large, parallelized | small, tutorial-sized |
| Read one render path | yes, end to end | not realistically | hard to trace | yes |
| Maintained | yes | yes | yes | no, archived tutorial |
| Live browser demo | yes | n/a | no | no |
| Goal | learn how an engine works | ship the web | research-grade parallel engine | teach the box model |

Chromium and Servo are production engines solving a vastly larger problem. Robinson is the classic teaching engine but is an unmaintained tutorial. Kaleidoscope aims to be the readable, maintained, end-to-end engine with a demo you can try in the browser.

## Usage

```
kaleidoscope render page.html --css style.css --width 800 --height 600 -o out.png
kaleidoscope render page.html --css style.css -o out.ppm
kaleidoscope layout page.html --css style.css --width 800
kaleidoscope snapshot --html "<div style='background:teal;height:40px'></div>" --css "div{display:block}" --width 200 --height 80
```

`render` writes a painted image, PNG when the output ends in `.png` and PPM otherwise. `layout` prints the box tree, one line per box, with tag name and exact pixel coordinates, so the box model is directly inspectable. `snapshot` renders inline HTML and CSS to a base64 PNG on stdout, for programmatic callers.

## MCP server for agents

`mcp/` is a Model Context Protocol server that exposes a `render_html` tool. An AI agent hands it HTML and CSS and gets back a PNG, so it can see how a snippet looks without a headless browser. Kaleidoscope boots in milliseconds and uses a couple of megabytes, where a headless Chromium needs seconds and hundreds of megabytes. See `mcp/README.md` for setup.

## Live demo

`docs/index.html` runs the same pipeline in JavaScript, in the browser, live on every keystroke: edit the HTML or CSS and watch the parse, cascade, layout, and paint stages run again on a `<canvas>`, with an optional box-outline overlay so you can see the layout tree drawn over the render.

## Testing

```
cargo test
```

Tests pin the exact behavior of every stage: the DOM tree shape for nested and malformed HTML, rule and specificity ordering for CSS, computed values and inline-style overrides for the cascade, exact box coordinates for a known layout (including a case with padding and border), the display list and pixel buffer for a colored box, and typed-error recovery for pathological nesting.

## License

MIT.

By Pavan Nallamothu (pavanchow)
