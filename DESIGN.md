# Design

Kaleidoscope is organized as five stages, each in its own module, each taking the previous stage's output as its input. Nothing is shared global state, every stage is a pure function from one tree to the next.

```
html.rs   -> dom.rs tree
css.rs    -> Stylesheet
style.rs  -> StyledNode tree   (dom tree + css cascade)
layout.rs -> LayoutBox tree    (styled tree + containing widths)
paint.rs  -> DisplayList, Canvas
```

## The DOM (`dom.rs`)

A node is either an `Element` (a tag name, a `BTreeMap<String, String>` of attributes, and children) or a `Text` run, or a `Comment`. That is the entire vocabulary a layout engine needs. `BTreeMap` was picked over `HashMap` for attributes so the tree has a deterministic iteration order, which keeps tests and debug output stable across runs.

## The HTML tokenizer and parser (`html.rs`)

The parser is a single hand-rolled pass over the byte slice with a cursor, no separate tokenization step. It keeps an explicit stack of open elements rather than recursing, so nesting depth is a `Vec` length that can be checked against `MAX_DOM_DEPTH` on every iteration instead of growing the call stack.

Recovery is the interesting part. Three cases matter:

- **A close tag with no open ancestor of that name.** The parser walks the stack looking for a matching frame. If it does not find one, the close tag is dropped rather than corrupting the tree.
- **A close tag for an ancestor several levels up.** Every frame between the top of the stack and the match is folded closed in order, so `<div><span><p>text</div>` closes `<p>` and `<span>` implicitly when `</div>` arrives.
- **Unclosed tags at end of input.** Whatever is left on the stack at EOF is folded closed the same way, deepest first, instead of raising an error.

Void elements (`br`, `img`, `input`, and the rest of the HTML5 void list) and explicit self-closing tags (`<x/>`) never push a frame, they attach directly as a childless element. `<script>` and `<style>` bodies are consumed as raw text up to their matching close tag rather than parsed as markup, since `<` inside a script is not a tag.

One structural decision: a document that already declares its own `<html>` element is returned as that element directly, instead of being wrapped in a second, synthetic `<html>`. A document that is just a fragment (`<div>...</div>` with no `<html>`) gets wrapped in a synthetic root, so `layout`/`render` always have a single root box to start from.

## The CSS parser (`css.rs`)

A `Stylesheet` is an ordered `Vec<Rule>`; order is kept because the cascade needs it. A `Rule` has a list of `Selector`s (comma-separated alternatives) and a list of `Declaration`s. A `Selector` is a `Vec<SimpleSelector>`, read as a descendant chain: `div .card p` parses to three simple selectors, and only the last one has to match the node directly, the earlier ones have to match some ancestor, in order, further up the tree.

Values are parsed into a small closed enum, `Keyword`, `Length(f32, Px)`, or `Color`, rather than kept as strings, so later stages never re-parse. Unrecognized declarations (a bad property, a value the parser cannot interpret) are skipped rather than aborting the whole rule, matching how real stylesheets tolerate unknown or unsupported CSS.

## The cascade (`style.rs`)

`style_tree` walks the DOM alongside an ancestor stack and, for every element, computes the winning value for every property that appears in any matching rule. The winner is picked in two steps:

1. Collect every declaration from every rule whose selector matches, tagged with that selector's specificity `(id_count, class_count, type_count)` and the rule's position in the stylesheet.
2. Sort by `(specificity, source_order)` and let later entries overwrite earlier ones in a map. This is exactly "higher specificity wins, ties go to whoever came later in the stylesheet."

An inline `style="..."` attribute is parsed with the same CSS declaration parser (wrapped in a synthetic `x { ... }` rule) and applied last, unconditionally, which is how it wins over every stylesheet rule regardless of specificity.

Text and comment nodes carry no computed values of their own; `display: none` is read straight off the computed map by later stages to decide whether a node produces a box at all.

## Layout (`layout.rs`)

`Dimensions` is a content `Rect` plus three `EdgeSizes` (padding, border, margin). `padding_box`, `border_box`, and `margin_box` each grow the content rect outward by one more edge, so a box's visible shape at any layer is one function call away.

Layout is block-only: every element becomes a block box regardless of its `display: inline` computed value (Kaleidoscope does not build a separate inline formatting context or line boxes). A non-empty text node becomes its own simple box sized by `font-size * 1.2` for its height and the containing width for its width; it carries no background or border, it exists purely to take up vertical space in the stack. This is a deliberate simplification: real text shaping and line wrapping is its own large subsystem, and approximating text as a block keeps the box model itself the star of the pipeline. `display: none` nodes produce no box at all, and their content is skipped entirely rather than laid out and hidden.

The width and margin resolution follows the standard block-formatting-context algorithm:

1. Read the specified `width`, `margin` (all four sides share one value, since Kaleidoscope does not implement per-side shorthand), `border-width`, and `padding`, defaulting anything unspecified to `0`, except `width`/`margin` which can be explicitly `auto`.
2. Compute `underflow = containing_width - (margins + borders*2 + padding*2 + width)`.
3. If `width` is not `auto`: an `auto` margin absorbs the underflow (split evenly if both sides are auto), otherwise the underflow is folded into the right margin (the over-constrained case).
4. If `width` is `auto`: `auto` margins become `0`, and `width` becomes the underflow directly, clamped to `0` if that would be negative.

Position falls out of two numbers: `x` is the containing block's content x plus this box's own margin, border, and padding; `y` is the containing block's content y **plus its accumulated content height so far**, plus this box's margin, border, and padding. That accumulator is the whole trick behind block stacking, every layout call for a parent walks its children in order, laying each one out against the parent's current dimensions, then adds that child's margin-box height to the parent's running content height before laying out the next child. By the time the last child is placed, the parent's content height *is* the sum of its children's margin-box heights, which becomes the parent's own auto height unless an explicit `height` overrides it.

Recursion depth is checked on every call into `layout_block` against `MAX_LAYOUT_DEPTH`, returning `EngineError::LayoutTooDeep` instead of overflowing the stack on pathological nesting.

The CLI's `layout` command and the test suite report each box's **content box** `x`/`y`/`width`/`height`, since that is the rectangle the CSS `width`/`height` properties actually describe; `border_box()`/`padding_box()`/`margin_box()` are one call away for anything that needs the outer shape.

## Paint (`paint.rs`)

`build_display_list` walks the layout tree once and emits `DisplayItem::SolidColor` items in a fixed order per box: the border box in `border-color` first (only if any border width is nonzero), then the padding box in `background-color`/`background` on top of it. Painting a border as one full rectangle underneath the background, rather than four separate edge rectangles, is the simplification that keeps this stage a single loop: the background always exactly covers the padding box, so what remains visible around it is the border's color, by construction.

`Canvas` is a flat `Vec<[u8; 4]>` RGBA buffer. Rectangles are filled by clamping to the canvas bounds and writing every covered pixel; colors with `alpha < 255` blend against whatever is already there with a standard `src-over` mix, `alpha == 255` just overwrites (the common case, since none of the parsed color formats currently produce alpha other than the `transparent` keyword). `write_ppm` strips the alpha channel and writes a binary PPM (P6): a three-line ASCII header (`P6`, `<width> <height>`, `255`) followed by raw RGB bytes, which any standard image viewer opens with no image-decoding crate on Kaleidoscope's side.

## The browser demo

`docs/index.html` is a line-by-line JavaScript port of the same five stages: the same tokenizer shape for HTML, the same selector and specificity model for CSS, the same specificity-then-source-order cascade, the same width/margin/underflow algorithm and content-height accumulator for layout, and the same border-then-background display list for paint, just drawing to a `<canvas>` 2D context instead of writing a PPM. It exists so the pipeline can be inspected without installing Rust: type HTML or CSS into the two text areas and the whole pipeline reruns on every keystroke, with an optional overlay that outlines every layout box and labels it with its tag name.
