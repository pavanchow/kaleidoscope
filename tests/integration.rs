//! Public-surface integration tests for kaleidoscope.
//!
//! kaleidoscope is a binary crate: its modules are private to `main.rs` and it
//! exposes no library API, so a downstream consumer interacts with it through
//! the CLI. These tests drive the real compiled binary end to end (HTML + CSS
//! -> DOM -> style -> layout -> paint -> PPM/PNG), the same way a caller would.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_kaleidoscope"))
}

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("kaleidoscope_it_{}_{}", std::process::id(), name));
    p
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn layout_reports_the_full_document_tree() {
    let html = tmp("tree.html");
    fs::write(&html, "<html><body><div></div><div></div></body></html>").unwrap();

    let out = bin().arg("layout").arg(&html).args(["--width", "100"]).output().unwrap();
    assert!(out.status.success());
    let text = stdout(&out);

    // Every element in the source appears in the printed box tree, nested.
    assert!(text.contains("html x=0.0 y=0.0 width=100.0"));
    assert!(text.contains("  body"));
    assert!(text.contains("    div"));
    // Two child divs were parsed and laid out.
    assert_eq!(text.matches("div ").count(), 2);
    // A block with no styled height collapses to zero, spanning the full width.
    assert!(text.contains("width=100.0 height=0.0"));
}

#[test]
fn css_dimensions_flow_through_to_layout() {
    let html = tmp("styled.html");
    let css = tmp("styled.css");
    fs::write(&html, "<div></div>").unwrap();
    fs::write(&css, "div { height: 50px; width: 80px; }").unwrap();

    let out = bin()
        .arg("layout")
        .arg(&html)
        .arg("--css")
        .arg(&css)
        .args(["--width", "200"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("div x=0.0 y=0.0 width=80.0 height=50.0"), "got: {text}");
    // The root grows to contain its child's styled height.
    assert!(text.contains("html x=0.0 y=0.0 width=200.0 height=50.0"), "got: {text}");
}

#[test]
fn sibling_blocks_stack_without_off_by_one() {
    // Multi-unit case: N fixed-height blocks must stack, each offset by the
    // running height, and the root height must equal their sum exactly.
    let html = tmp("stack.html");
    let css = tmp("stack.css");
    fs::write(&html, "<div></div><div></div><div></div><div></div>").unwrap();
    fs::write(&css, "div { height: 20px; }").unwrap();

    let out = bin().arg("layout").arg(&html).arg("--css").arg(&css).args(["--width", "50"]).output().unwrap();
    assert!(out.status.success());
    let text = stdout(&out);

    assert!(text.contains("y=0.0 width=50.0 height=20.0"), "got: {text}");
    assert!(text.contains("y=20.0 width=50.0 height=20.0"), "got: {text}");
    assert!(text.contains("y=40.0 width=50.0 height=20.0"), "got: {text}");
    assert!(text.contains("y=60.0 width=50.0 height=20.0"), "got: {text}");
    // 4 blocks x 20px = 80px total on the root.
    assert!(text.contains("html x=0.0 y=0.0 width=50.0 height=80.0"), "got: {text}");
}

#[test]
fn snapshot_emits_a_valid_png_stream() {
    let out = bin()
        .args(["snapshot", "--html", "<div></div>", "--css", "div{height:10px}", "--width", "4", "--height", "4"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let b64 = stdout(&out);
    // base64 of the 8-byte PNG signature (\x89PNG\r\n\x1a\n) is "iVBORw0KGgo".
    assert!(b64.starts_with("iVBORw0KGgo"), "not a PNG stream: {}", &b64[..b64.len().min(24)]);
}

#[test]
fn render_writes_a_ppm_file() {
    let html = tmp("render.html");
    let ppm = tmp("out.ppm");
    fs::write(&html, "<div></div>").unwrap();

    let out = bin().arg("render").arg(&html).args(["--width", "3", "--height", "3", "-o"]).arg(&ppm).output().unwrap();
    assert!(out.status.success());
    let bytes = fs::read(&ppm).unwrap();
    // Binary PPM magic, followed by the declared dimensions and maxval.
    assert!(bytes.starts_with(b"P6\n3 3\n255\n"), "bad ppm header");
    let _ = fs::remove_file(&ppm);
}

#[test]
fn render_writes_a_png_file_when_extension_is_png() {
    let html = tmp("render2.html");
    let png = tmp("out.png");
    fs::write(&html, "<div></div>").unwrap();

    let out = bin().arg("render").arg(&html).args(["--width", "2", "--height", "2", "-o"]).arg(&png).output().unwrap();
    assert!(out.status.success());
    let bytes = fs::read(&png).unwrap();
    assert_eq!(&bytes[..8], &[137, 80, 78, 71, 13, 10, 26, 10], "bad PNG signature");
    let _ = fs::remove_file(&png);
}

#[test]
fn empty_document_still_renders() {
    // Minimum-size input: an empty page must not crash and still yields a PNG.
    let out = bin().args(["snapshot", "--html", "", "--width", "2", "--height", "2"]).output().unwrap();
    assert!(out.status.success());
    assert!(stdout(&out).starts_with("iVBORw0KGgo"));
}

#[test]
fn invalid_css_is_reported_as_a_typed_error() {
    let out = bin()
        .args(["snapshot", "--html", "<div></div>", "--css", "div }", "--width", "4", "--height", "4"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("invalid css"), "got: {err}");
}

#[test]
fn missing_input_file_fails_gracefully() {
    let missing = tmp("does_not_exist.html");
    let _ = fs::remove_file(&missing);
    let out = bin().arg("layout").arg(&missing).output().unwrap();
    assert!(!out.status.success());
    // The pipeline reports an IO failure instead of panicking.
    assert!(stderr(&out).contains("reading"), "got: {}", stderr(&out));
}

#[test]
fn snapshot_output_is_deterministic() {
    let run = || {
        bin()
            .args(["snapshot", "--html", "<div></div><div></div>", "--css", "div{height:7px}", "--width", "5", "--height", "20"])
            .output()
            .unwrap()
    };
    let a = stdout(&run());
    let b = stdout(&run());
    assert!(!a.is_empty());
    assert_eq!(a, b, "identical input must produce byte-identical output");
}
