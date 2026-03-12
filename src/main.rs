mod analysis;
mod depth;
mod output;
mod utils;

use anyhow::Result;
use clap::Parser;
use colored::*;
use std::path::PathBuf;

/// Image3D — Deep image analysis and 3D depth map generator
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input image path (JPEG, PNG, BMP, TIFF, WebP)
    #[arg(short, long)]
    input: PathBuf,

    /// Output directory for results
    #[arg(short, long, default_value = "output")]
    output_dir: PathBuf,

    /// Depth estimation algorithm: gradient | luminance | combined
    #[arg(short, long, default_value = "combined")]
    depth_method: String,

    /// Anaglyph 3D color mode: red-cyan | green-magenta | amber-blue
    #[arg(long, default_value = "red-cyan")]
    anaglyph: String,

    /// Max disparity (pixels) for stereo shift
    #[arg(long, default_value_t = 30)]
    max_disparity: u32,

    /// Save depth map as grayscale PNG
    #[arg(long, default_value_t = true)]
    save_depth: bool,

    /// Save side-by-side stereo pair
    #[arg(long, default_value_t = true)]
    save_stereo: bool,

    /// Export JSON analysis report
    #[arg(long, default_value_t = true)]
    save_report: bool,

    /// Apply bilateral smoothing to depth map
    #[arg(long, default_value_t = true)]
    smooth_depth: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    print_banner();

    // ── Load image ────────────────────────────────────────────────────────────
    println!("{}", "▸ Loading image…".cyan().bold());
    let img = image::open(&args.input)
        .map_err(|e| anyhow::anyhow!("Failed to open '{}': {}", args.input.display(), e))?;

    println!(
        "  {} {}×{} pixels  ({:?})",
        "✔".green(),
        img.width(),
        img.height(),
        img.color()
    );

    // ── Deep analysis ─────────────────────────────────────────────────────────
    println!("{}", "\n▸ Running deep image analysis…".cyan().bold());
    let report = analysis::analyse(&img);
    report.print_summary();

    // ── Depth estimation ──────────────────────────────────────────────────────
    println!(
        "{}",
        format!("\n▸ Estimating depth map  [{}]…", args.depth_method)
            .cyan()
            .bold()
    );
    let mut depth_map = depth::estimate(&img, &args.depth_method, args.smooth_depth)?;

    // ── Generate 3D outputs ───────────────────────────────────────────────────
    println!("{}", "\n▸ Generating 3D outputs…".cyan().bold());
    std::fs::create_dir_all(&args.output_dir)?;

    if args.save_depth {
        let path = args.output_dir.join("depth_map.png");
        output::save_depth_map(&depth_map, &path)?;
        println!("  {} depth map  → {}", "✔".green(), path.display());
    }

    if args.save_stereo {
        let path = args.output_dir.join("stereo_pair.png");
        output::save_stereo_pair(&img, &depth_map, args.max_disparity, &path)?;
        println!("  {} stereo pair → {}", "✔".green(), path.display());
    }

    // Anaglyph
    {
        let path = args.output_dir.join("anaglyph_3d.png");
        output::save_anaglyph(
            &img,
            &mut depth_map,
            args.max_disparity,
            &args.anaglyph,
            &path,
        )?;
        println!(
            "  {} anaglyph ({}) → {}",
            "✔".green(),
            args.anaglyph,
            path.display()
        );
    }

    // Wiggle GIF
    {
        let path = args.output_dir.join("wiggle.gif");
        output::save_wiggle_gif(&img, &depth_map, args.max_disparity / 2, &path)?;
        println!("  {} wiggle GIF → {}", "✔".green(), path.display());
    }

    // ── Save report ───────────────────────────────────────────────────────────
    if args.save_report {
        let path = args.output_dir.join("analysis_report.json");
        output::save_report(&report, &path)?;
        println!(
            "\n  {} analysis report → {}",
            "✔".green(),
            path.display()
        );
    }

    println!("\n{}", "✔ Done!".green().bold());
    Ok(())
}

fn print_banner() {
    println!(
        "{}",
        r#"
 ██╗███╗   ███╗ █████╗  ██████╗ ███████╗██████╗ ██████╗
 ██║████╗ ████║██╔══██╗██╔════╝ ██╔════╝╚════██╗██╔══██╗
 ██║██╔████╔██║███████║██║  ███╗█████╗   █████╔╝██║  ██║
 ██║██║╚██╔╝██║██╔══██║██║   ██║██╔══╝   ╚═══██╗██║  ██║
 ██║██║ ╚═╝ ██║██║  ██║╚██████╔╝███████╗██████╔╝██████╔╝
 ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═════╝ ╚═════╝
        Deep Image Analysis + 3D Generator
"#
        .bright_cyan()
    );
}
