mod css;
mod dom;
mod error;
mod html;
mod layout;
mod paint;
mod png;
mod style;

use clap::{Parser, Subcommand};
use layout::LayoutBox;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "kaleidoscope", about = "A browser rendering engine you can read end to end")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse, style, lay out, and paint a page to a PPM image.
    Render {
        html: PathBuf,
        #[arg(long)]
        css: Option<PathBuf>,
        #[arg(long, default_value_t = 800)]
        width: u32,
        #[arg(long, default_value_t = 600)]
        height: u32,
        #[arg(short, long, default_value = "out.ppm")]
        output: PathBuf,
    },
    /// Parse, style, and lay out a page, printing the box tree.
    Layout {
        html: PathBuf,
        #[arg(long)]
        css: Option<PathBuf>,
        #[arg(long, default_value_t = 800)]
        width: u32,
    },
    /// Render inline HTML and CSS strings to a base64-encoded PNG on stdout.
    /// Built for programmatic callers (for example the MCP server) that want
    /// an image back without touching the filesystem.
    Snapshot {
        #[arg(long)]
        html: String,
        #[arg(long, default_value = "")]
        css: String,
        #[arg(long, default_value_t = 800)]
        width: u32,
        #[arg(long, default_value_t = 600)]
        height: u32,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("kaleidoscope: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Render { html, css, width, height, output } => {
            let (html_src, css_src) = read_sources(&html, css.as_deref())?;
            let dom = self::html::parse(&html_src).map_err(|e| e.to_string())?;
            let sheet = self::css::parse(&css_src).map_err(|e| e.to_string())?;
            let styled = style::style_tree(&dom, &sheet);
            let root = layout::layout_tree(&styled, width as f32).map_err(|e| e.to_string())?;
            let canvas = paint::paint(&root, width as usize, height as usize);
            let is_png = output
                .extension()
                .map(|e| e.eq_ignore_ascii_case("png"))
                .unwrap_or(false);
            if is_png {
                let bytes = png::encode(&canvas);
                fs::write(&output, bytes).map_err(|e| e.to_string())?;
            } else {
                let file = fs::File::create(&output).map_err(|e| e.to_string())?;
                canvas.write_ppm(file).map_err(|e| e.to_string())?;
            }
            println!("wrote {} ({}x{})", output.display(), width, height);
            Ok(())
        }
        Command::Layout { html, css, width } => {
            let (html_src, css_src) = read_sources(&html, css.as_deref())?;
            let dom = self::html::parse(&html_src).map_err(|e| e.to_string())?;
            let sheet = self::css::parse(&css_src).map_err(|e| e.to_string())?;
            let styled = style::style_tree(&dom, &sheet);
            let root = layout::layout_tree(&styled, width as f32).map_err(|e| e.to_string())?;
            print_layout_tree(&root, 0);
            Ok(())
        }
        Command::Snapshot { html, css, width, height } => {
            let dom = self::html::parse(&html).map_err(|e| e.to_string())?;
            let sheet = self::css::parse(&css).map_err(|e| e.to_string())?;
            let styled = style::style_tree(&dom, &sheet);
            let root = layout::layout_tree(&styled, width as f32).map_err(|e| e.to_string())?;
            let canvas = paint::paint(&root, width as usize, height as usize);
            let bytes = png::encode(&canvas);
            println!("{}", base64_encode(&bytes));
            Ok(())
        }
    }
}

/// Standard base64 (RFC 4648), written out so the crate keeps its tiny
/// dependency set. Used to hand a PNG back to a programmatic caller as text.
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { ALPHABET[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn read_sources(html_path: &PathBuf, css_path: Option<&std::path::Path>) -> Result<(String, String), String> {
    let html_src = fs::read_to_string(html_path).map_err(|e| format!("reading {}: {e}", html_path.display()))?;
    let css_src = match css_path {
        Some(p) => fs::read_to_string(p).map_err(|e| format!("reading {}: {e}", p.display()))?,
        None => String::new(),
    };
    Ok((html_src, css_src))
}

fn print_layout_tree(node: &LayoutBox, depth: usize) {
    let d = &node.dimensions.content;
    println!(
        "{}{} x={:.1} y={:.1} width={:.1} height={:.1}",
        "  ".repeat(depth),
        node.tag(),
        d.x,
        d.y,
        d.width,
        d.height
    );
    for child in &node.children {
        print_layout_tree(child, depth + 1);
    }
}
