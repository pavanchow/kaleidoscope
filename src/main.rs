mod css;
mod dom;
mod error;
mod html;
mod layout;
mod paint;
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
            let file = fs::File::create(&output).map_err(|e| e.to_string())?;
            canvas.write_ppm(file).map_err(|e| e.to_string())?;
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
    }
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
