/// Depth map estimation from a single image.
///
/// Three algorithms are provided:
///   - `luminance`  : bright = near, dark = far (works well for front-lit scenes)
///   - `gradient`   : high gradient = near edge/object, low = far/background
///   - `combined`   : weighted blend of luminance + gradient with spatial bias
///
/// All algorithms return a 2D vec of f32 values in [0.0, 1.0] where
///   1.0 = closest to camera  /  0.0 = farthest.

use anyhow::Result;
use image::{DynamicImage, GenericImageView, GrayImage, Luma};

pub type DepthMap = Vec<Vec<f32>>;

// ─────────────────────────────────────────────────────────────────────────────

pub fn estimate(img: &DynamicImage, method: &str, smooth: bool) -> Result<DepthMap> {
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();

    let mut depth = match method {
        "luminance" => luminance_depth(&gray, w, h),
        "gradient"  => gradient_depth(&gray, w, h),
        _           => combined_depth(&gray, w, h),   // "combined" is the default
    };

    // Optional bilateral-style smoothing
    if smooth {
        depth = bilateral_smooth(&depth, w, h, 5, 15.0, 30.0);
    }

    // Apply a subtle depth-of-field bias: centre of image is slightly closer
    apply_radial_bias(&mut depth, w, h, 0.12);

    Ok(depth)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Algorithm 1 – Luminance
// ─────────────────────────────────────────────────────────────────────────────

fn luminance_depth(gray: &GrayImage, w: u32, h: u32) -> DepthMap {
    let mut dm = vec![vec![0.0f32; w as usize]; h as usize];
    for y in 0..h {
        for x in 0..w {
            let lum = gray.get_pixel(x, y).0[0] as f32 / 255.0;
            dm[y as usize][x as usize] = lum;
        }
    }
    normalise(&mut dm);
    dm
}

// ─────────────────────────────────────────────────────────────────────────────
//  Algorithm 2 – Gradient magnitude (Sobel)
// ─────────────────────────────────────────────────────────────────────────────

fn gradient_depth(gray: &GrayImage, w: u32, h: u32) -> DepthMap {
    let mut dm = vec![vec![0.0f32; w as usize]; h as usize];

    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let p = |dx: i32, dy: i32| -> f32 {
                gray.get_pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32).0[0] as f32
            };
            let gx = -p(-1,-1) - 2.0*p(-1,0) - p(-1,1) + p(1,-1) + 2.0*p(1,0) + p(1,1);
            let gy = -p(-1,-1) - 2.0*p(0,-1) - p(1,-1) + p(-1,1) + 2.0*p(0,1) + p(1,1);
            dm[y as usize][x as usize] = (gx*gx + gy*gy).sqrt();
        }
    }
    normalise(&mut dm);
    dm
}

// ─────────────────────────────────────────────────────────────────────────────
//  Algorithm 3 – Combined (luminance + gradient + defocus + haze)
// ─────────────────────────────────────────────────────────────────────────────

fn combined_depth(gray: &GrayImage, w: u32, h: u32) -> DepthMap {
    let lum  = luminance_depth(gray, w, h);
    let grad = gradient_depth(gray, w, h);

    // Local focus measure via Laplacian absolute value
    let focus = focus_depth(gray, w, h);

    let mut dm = vec![vec![0.0f32; w as usize]; h as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            // Weighted blend: luminance 40%, gradient 30%, focus 30%
            dm[y][x] = 0.40 * lum[y][x] + 0.30 * grad[y][x] + 0.30 * focus[y][x];
        }
    }
    normalise(&mut dm);
    dm
}

/// Focus measure: high Laplacian response = in-focus = nearer
fn focus_depth(gray: &GrayImage, w: u32, h: u32) -> DepthMap {
    let mut dm = vec![vec![0.0f32; w as usize]; h as usize];
    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let p = |dx: i32, dy: i32| -> f32 {
                gray.get_pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32).0[0] as f32
            };
            let lap = (4.0*p(0,0) - p(1,0) - p(-1,0) - p(0,1) - p(0,-1)).abs();
            dm[y as usize][x as usize] = lap;
        }
    }
    normalise(&mut dm);
    dm
}

// ─────────────────────────────────────────────────────────────────────────────
//  Bilateral smoothing
// ─────────────────────────────────────────────────────────────────────────────

/// A simple bilateral-like filter: spatial Gaussian × range Gaussian.
/// `spatial_sigma` controls neighbourhood size, `range_sigma` controls
/// how much depth discontinuities are preserved.
pub fn bilateral_smooth(
    dm: &DepthMap,
    w: u32,
    h: u32,
    radius: i32,
    spatial_sigma: f64,
    range_sigma: f64,
) -> DepthMap {
    let ss2 = 2.0 * spatial_sigma * spatial_sigma;
    let rs2 = 2.0 * range_sigma * range_sigma;

    let mut out = vec![vec![0.0f32; w as usize]; h as usize];

    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let center = dm[y as usize][x as usize] as f64;
            let (mut sum, mut weight) = (0.0f64, 0.0f64);

            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let ny = (y + dy).clamp(0, h as i32 - 1) as usize;
                    let nx = (x + dx).clamp(0, w as i32 - 1) as usize;
                    let val = dm[ny][nx] as f64;

                    let spatial_w = (-(dx*dx + dy*dy) as f64 / ss2).exp();
                    let range_w   = (-((center - val).powi(2)) / rs2).exp();
                    let w = spatial_w * range_w;

                    sum    += w * val;
                    weight += w;
                }
            }
            out[y as usize][x as usize] = (sum / weight) as f32;
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Normalise depth map to [0, 1]
fn normalise(dm: &mut DepthMap) {
    let max = dm.iter().flat_map(|r| r.iter()).cloned().fold(f32::NEG_INFINITY, f32::max);
    let min = dm.iter().flat_map(|r| r.iter()).cloned().fold(f32::INFINITY,     f32::min);
    let range = (max - min).max(1e-6);
    for row in dm.iter_mut() {
        for v in row.iter_mut() {
            *v = (*v - min) / range;
        }
    }
}

/// Subtle radial depth bias: centre = slightly nearer, edges = slightly farther.
pub fn apply_radial_bias(dm: &mut DepthMap, w: u32, h: u32, strength: f32) {
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let max_r = (cx*cx + cy*cy).sqrt();

    for y in 0..h as usize {
        for x in 0..w as usize {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let r = (dx*dx + dy*dy).sqrt() / max_r; // 0 = centre, 1 = corner
            // Closer to centre → add depth, farther → subtract
            dm[y][x] = (dm[y][x] + strength * (1.0 - r)).clamp(0.0, 1.0);
        }
    }
    // Re-normalise after bias
    normalise(dm);
}

/// Convert depth map to a grayscale image (u8) for export
pub fn to_gray_image(dm: &DepthMap) -> GrayImage {
    let h = dm.len() as u32;
    let w = if h > 0 { dm[0].len() as u32 } else { 0 };
    let mut img = GrayImage::new(w, h);
    for y in 0..h as usize {
        for x in 0..w as usize {
            img.put_pixel(x as u32, y as u32, Luma([(dm[y][x] * 255.0) as u8]));
        }
    }
    img
}
