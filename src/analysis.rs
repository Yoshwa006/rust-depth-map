/// Deep image analysis: statistics, histograms, edges, texture, dominant colours.

use image::{DynamicImage, GenericImageView, Luma, Pixel};
use serde::{Deserialize, Serialize};
use colored::*;

// ─────────────────────────────────────────────────────────────────────────────
//  Public data structures
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChannelStats {
    pub mean: f64,
    pub std_dev: f64,
    pub min: u8,
    pub max: u8,
    pub median: u8,
    pub histogram: Vec<u32>,         // 256 bins
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EdgeInfo {
    pub edge_density: f64,           // 0-1: fraction of edge pixels
    pub mean_gradient: f64,
    pub max_gradient: f64,
    pub direction_histogram: Vec<u32>, // 8 directional bins (N NE E SE S SW W NW)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TextureInfo {
    pub contrast: f64,               // GLCM-based contrast
    pub energy: f64,                 // GLCM energy (homogeneity measure)
    pub entropy: f64,                // local entropy estimate
    pub roughness: f64,              // Laplacian variance
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ColorInfo {
    pub dominant_colors: Vec<[u8; 3]>, // up to 8 RGB centroids (k-means lite)
    pub saturation_mean: f64,
    pub hue_histogram: Vec<u32>,      // 36 bins × 10°
    pub is_grayscale: bool,
    pub color_temperature_k: f64,     // approximate colour temperature in Kelvin
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnalysisReport {
    pub width: u32,
    pub height: u32,
    pub megapixels: f64,
    pub red: ChannelStats,
    pub green: ChannelStats,
    pub blue: ChannelStats,
    pub luminance: ChannelStats,
    pub edges: EdgeInfo,
    pub texture: TextureInfo,
    pub color: ColorInfo,
    pub sharpness_score: f64,        // 0-100
    pub noise_estimate: f64,         // std-dev of high-frequency noise
    pub dynamic_range: f64,          // perceived stops
    pub suggested_depth_method: String,
}

// ─────────────────────────────────────────────────────────────────────────────
//  Entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn analyse(img: &DynamicImage) -> AnalysisReport {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let pixels: Vec<[u8; 4]> = rgba.pixels().map(|p| p.0).collect();

    // Per-channel extraction
    let r_vals: Vec<u8> = pixels.iter().map(|p| p[0]).collect();
    let g_vals: Vec<u8> = pixels.iter().map(|p| p[1]).collect();
    let b_vals: Vec<u8> = pixels.iter().map(|p| p[2]).collect();
    let lum_vals: Vec<u8> = pixels
        .iter()
        .map(|p| {
            let l = 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64;
            l.round() as u8
        })
        .collect();

    let red = channel_stats(&r_vals);
    let green = channel_stats(&g_vals);
    let blue = channel_stats(&b_vals);
    let luminance = channel_stats(&lum_vals);

    let gray = image::imageops::grayscale(&rgba);
    let edges = edge_analysis(&gray, w, h);
    let texture = texture_analysis(&gray, w, h);
    let color = color_analysis(&pixels, w, h);
    let sharpness_score = laplacian_sharpness(&gray, w, h);
    let noise_estimate = estimate_noise(&gray, w, h);
    let dynamic_range = ((luminance.max as f64 + 1.0) / (luminance.min as f64 + 1.0)).log2();

    let suggested_depth_method = suggest_depth(&edges, &texture, &color);

    AnalysisReport {
        width: w,
        height: h,
        megapixels: (w as f64 * h as f64) / 1_000_000.0,
        red,
        green,
        blue,
        luminance,
        edges,
        texture,
        color,
        sharpness_score,
        noise_estimate,
        dynamic_range,
        suggested_depth_method,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Channel statistics
// ─────────────────────────────────────────────────────────────────────────────

fn channel_stats(vals: &[u8]) -> ChannelStats {
    let mut hist = vec![0u32; 256];
    for &v in vals {
        hist[v as usize] += 1;
    }
    let n = vals.len() as f64;
    let mean = vals.iter().map(|&v| v as f64).sum::<f64>() / n;
    let std_dev = (vals.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / n).sqrt();
    let min = *vals.iter().min().unwrap_or(&0);
    let max = *vals.iter().max().unwrap_or(&255);

    // Median via cumulative histogram
    let half = (n / 2.0) as u32;
    let mut cum = 0u32;
    let mut median = 0u8;
    for (i, &h) in hist.iter().enumerate() {
        cum += h;
        if cum >= half {
            median = i as u8;
            break;
        }
    }

    ChannelStats { mean, std_dev, min, max, median, histogram: hist }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Edge analysis (Sobel)
// ─────────────────────────────────────────────────────────────────────────────

fn edge_analysis(gray: &image::GrayImage, w: u32, h: u32) -> EdgeInfo {
    let threshold = 30.0f64;
    let mut edge_count = 0u64;
    let mut grad_sum = 0.0f64;
    let mut max_grad = 0.0f64;
    let mut dir_hist = vec![0u32; 8];
    let total = (w.saturating_sub(2) * h.saturating_sub(2)) as f64;

    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let px = |dx: i32, dy: i32| -> f64 {
                gray.get_pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32).0[0] as f64
            };

            let gx = -px(-1,-1) - 2.0*px(-1,0) - px(-1,1) + px(1,-1) + 2.0*px(1,0) + px(1,1);
            let gy = -px(-1,-1) - 2.0*px(0,-1) - px(1,-1) + px(-1,1) + 2.0*px(0,1) + px(1,1);
            let mag = (gx*gx + gy*gy).sqrt();

            if mag > threshold {
                edge_count += 1;
                let angle = gy.atan2(gx).to_degrees().rem_euclid(360.0);
                let bin = ((angle / 45.0).round() as usize) % 8;
                dir_hist[bin] += 1;
            }
            grad_sum += mag;
            if mag > max_grad { max_grad = mag; }
        }
    }

    EdgeInfo {
        edge_density: edge_count as f64 / total,
        mean_gradient: grad_sum / total,
        max_gradient: max_grad,
        direction_histogram: dir_hist,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Texture analysis (GLCM-inspired + Laplacian)
// ─────────────────────────────────────────────────────────────────────────────

fn texture_analysis(gray: &image::GrayImage, w: u32, h: u32) -> TextureInfo {
    // Build 64-level co-occurrence matrix (horizontal, offset 1)
    let levels = 64usize;
    let scale = levels as f64 / 256.0;
    let mut glcm = vec![vec![0u64; levels]; levels];

    for y in 0..h {
        for x in 0..(w - 1) {
            let i = (gray.get_pixel(x,   y).0[0] as f64 * scale) as usize;
            let j = (gray.get_pixel(x+1, y).0[0] as f64 * scale) as usize;
            glcm[i.min(levels-1)][j.min(levels-1)] += 1;
        }
    }

    let total: u64 = glcm.iter().flat_map(|r| r.iter()).sum();
    let total_f = total as f64;

    let (mut contrast, mut energy, mut entropy) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..levels {
        for j in 0..levels {
            let p = glcm[i][j] as f64 / total_f;
            contrast += p * (i as f64 - j as f64).powi(2);
            energy += p * p;
            if p > 1e-10 { entropy -= p * p.ln(); }
        }
    }

    let roughness = laplacian_variance(gray, w, h);

    TextureInfo { contrast, energy, entropy, roughness }
}

fn laplacian_variance(gray: &image::GrayImage, w: u32, h: u32) -> f64 {
    let mut vals = Vec::with_capacity((w * h) as usize);
    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let px = |dx: i32, dy: i32| gray.get_pixel((x as i32+dx) as u32, (y as i32+dy) as u32).0[0] as f64;
            let lap = 4.0*px(0,0) - px(1,0) - px(-1,0) - px(0,1) - px(0,-1);
            vals.push(lap);
        }
    }
    let mean = vals.iter().sum::<f64>() / vals.len() as f64;
    vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64
}

// ─────────────────────────────────────────────────────────────────────────────
//  Colour analysis
// ─────────────────────────────────────────────────────────────────────────────

fn color_analysis(pixels: &[[u8; 4]], _w: u32, _h: u32) -> ColorInfo {
    // Sample every 16th pixel for speed
    let sample: Vec<[u8; 3]> = pixels.iter().step_by(16).map(|p| [p[0], p[1], p[2]]).collect();

    let dominant_colors = kmeans_colors(&sample, 8, 20);

    let (mut sat_sum, mut gray_count) = (0.0f64, 0usize);
    let mut hue_hist = vec![0u32; 36];

    for p in &sample {
        let (r, g, b) = (p[0] as f64 / 255.0, p[1] as f64 / 255.0, p[2] as f64 / 255.0);
        let cmax = r.max(g).max(b);
        let cmin = r.min(g).min(b);
        let delta = cmax - cmin;

        // Saturation (HSV)
        let sat = if cmax > 0.0 { delta / cmax } else { 0.0 };
        sat_sum += sat;

        // Hue
        if delta > 0.01 {
            let hue = if cmax == r {
                60.0 * (((g - b) / delta).rem_euclid(6.0))
            } else if cmax == g {
                60.0 * ((b - r) / delta + 2.0)
            } else {
                60.0 * ((r - g) / delta + 4.0)
            };
            let bin = ((hue / 10.0) as usize).min(35);
            hue_hist[bin] += 1;
        } else {
            gray_count += 1;
        }
    }

    let saturation_mean = sat_sum / sample.len() as f64;
    let is_grayscale = saturation_mean < 0.05;

    // Approximate colour temperature from dominant R/B ratio
    let avg_r: f64 = sample.iter().map(|p| p[0] as f64).sum::<f64>() / sample.len() as f64;
    let avg_b: f64 = sample.iter().map(|p| p[2] as f64).sum::<f64>() / sample.len() as f64;
    let rb_ratio = avg_r / (avg_b + 1.0);
    // Rough empirical mapping: rb≈1 → 5500K, rb>1 → warmer, rb<1 → cooler
    let color_temperature_k = 5500.0 / rb_ratio;

    ColorInfo {
        dominant_colors,
        saturation_mean,
        hue_histogram: hue_hist,
        is_grayscale,
        color_temperature_k,
    }
}

/// Tiny k-means in RGB space (no external crates)
fn kmeans_colors(pixels: &[[u8; 3]], k: usize, iters: usize) -> Vec<[u8; 3]> {
    if pixels.is_empty() { return vec![]; }

    // Init centroids by spacing evenly through the sample
    let step = (pixels.len() / k).max(1);
    let mut centroids: Vec<[f64; 3]> = (0..k)
        .map(|i| {
            let p = pixels[(i * step).min(pixels.len()-1)];
            [p[0] as f64, p[1] as f64, p[2] as f64]
        })
        .collect();

    for _ in 0..iters {
        let mut sums = vec![[0.0f64; 3]; k];
        let mut counts = vec![0usize; k];

        for p in pixels {
            let pf = [p[0] as f64, p[1] as f64, p[2] as f64];
            let (ci, _) = centroids
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let d = (pf[0]-c[0]).powi(2) + (pf[1]-c[1]).powi(2) + (pf[2]-c[2]).powi(2);
                    (i, d)
                })
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .unwrap();

            sums[ci][0] += pf[0]; sums[ci][1] += pf[1]; sums[ci][2] += pf[2];
            counts[ci] += 1;
        }

        for i in 0..k {
            if counts[i] > 0 {
                let c = counts[i] as f64;
                centroids[i] = [sums[i][0]/c, sums[i][1]/c, sums[i][2]/c];
            }
        }
    }

    centroids.iter().map(|c| [c[0] as u8, c[1] as u8, c[2] as u8]).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
//  Sharpness & noise
// ─────────────────────────────────────────────────────────────────────────────

pub fn laplacian_sharpness(gray: &image::GrayImage, w: u32, h: u32) -> f64 {
    let var = laplacian_variance(gray, w, h);
    // Map to 0-100: 0 = blurry, 100 = very sharp
    (var / 10.0).min(100.0)
}

fn estimate_noise(gray: &image::GrayImage, w: u32, h: u32) -> f64 {
    // High-frequency noise via 3×3 median-subtraction approach
    let mut diffs = Vec::with_capacity((w * h) as usize);
    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let center = gray.get_pixel(x, y).0[0] as f64;
            let neighbors: f64 = [(-1i32,-1i32),(-1,0),(-1,1),(0,-1),(0,1),(1,-1),(1,0),(1,1)]
                .iter()
                .map(|(dx, dy)| gray.get_pixel((x as i32+dx) as u32, (y as i32+dy) as u32).0[0] as f64)
                .sum::<f64>() / 8.0;
            diffs.push((center - neighbors).abs());
        }
    }
    diffs.iter().sum::<f64>() / diffs.len() as f64
}

// ─────────────────────────────────────────────────────────────────────────────
//  Suggest depth method based on analysis
// ─────────────────────────────────────────────────────────────────────────────

fn suggest_depth(edges: &EdgeInfo, texture: &TextureInfo, color: &ColorInfo) -> String {
    if color.is_grayscale || color.saturation_mean < 0.15 {
        // Low saturation — luminance-based depth is reliable
        "luminance".to_string()
    } else if edges.edge_density > 0.15 && texture.roughness > 500.0 {
        // High edge density + texture — gradient gives good structural depth
        "gradient".to_string()
    } else {
        "combined".to_string()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  Pretty console summary
// ─────────────────────────────────────────────────────────────────────────────

impl AnalysisReport {
    pub fn print_summary(&self) {
        println!(
            "  {}  {}×{}  ({:.2} MP)  sharpness {:.1}/100",
            "✔".green(), self.width, self.height, self.megapixels, self.sharpness_score
        );
        println!(
            "  luminance  mean={:.1}  σ={:.1}  range=[{},{}]  DR={:.1} stops",
            self.luminance.mean, self.luminance.std_dev,
            self.luminance.min, self.luminance.max, self.dynamic_range
        );
        println!(
            "  edges  density={:.3}  mean_grad={:.1}  max_grad={:.1}",
            self.edges.edge_density, self.edges.mean_gradient, self.edges.max_gradient
        );
        println!(
            "  texture  contrast={:.2}  energy={:.4}  entropy={:.2}  roughness={:.1}",
            self.texture.contrast, self.texture.energy, self.texture.entropy, self.texture.roughness
        );
        println!(
            "  colour  sat={:.3}  temp≈{:.0}K  grayscale={}",
            self.color.saturation_mean, self.color.color_temperature_k, self.color.is_grayscale
        );
        println!(
            "  noise_est={:.2}  → suggested depth method: {}",
            self.noise_estimate,
            self.suggested_depth_method.yellow()
        );

        // Print dominant colours as coloured blocks
        print!("  dominant colours: ");
        for c in &self.color.dominant_colors {
            let block = "██".truecolor(c[0], c[1], c[2]);
            print!("{} ", block);
        }
        println!();
    }
}
