/// Output generators:
///   - Depth map (grayscale PNG)
///   - Stereo pair (side-by-side PNG)
///   - Anaglyph (red-cyan / green-magenta / amber-blue PNG)
///   - Wiggle GIF (pseudo-3D animation)
///   - JSON analysis report

use std::path::Path;

use anyhow::Result;
use image::{
    DynamicImage, GenericImageView, ImageBuffer, Rgb, RgbaImage,
    codecs::gif::{GifEncoder, Repeat},
    Frame, RgbaImage as GifFrame, Delay,
};

use crate::analysis::AnalysisReport;
use crate::depth::{DepthMap, to_gray_image};

// ─────────────────────────────────────────────────────────────────────────────
//  1. Depth map
// ─────────────────────────────────────────────────────────────────────────────

pub fn save_depth_map(dm: &DepthMap, path: &Path) -> Result<()> {
    let gray = to_gray_image(dm);
    // Apply pseudo-colour (turbo-like): cool-blue far, warm-red near
    let (w, h) = (gray.width(), gray.height());
    let mut rgb: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = gray.get_pixel(x, y).0[0] as f32 / 255.0;
            rgb.put_pixel(x, y, Rgb(turbo_color(v)));
        }
    }
    rgb.save(path)?;
    Ok(())
}

/// Turbo-like colour map: 0 → blue, 0.5 → green, 1 → red
fn turbo_color(t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    if t < 0.25 {
        let u = t * 4.0;
        [0, (u * 128.0) as u8, 255]
    } else if t < 0.5 {
        let u = (t - 0.25) * 4.0;
        [0, 128 + (u * 127.0) as u8, (255.0 * (1.0 - u)) as u8]
    } else if t < 0.75 {
        let u = (t - 0.5) * 4.0;
        [(u * 255.0) as u8, 255, 0]
    } else {
        let u = (t - 0.75) * 4.0;
        [255, (255.0 * (1.0 - u)) as u8, 0]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
//  2. Stereo pair (side-by-side)
// ─────────────────────────────────────────────────────────────────────────────

pub fn save_stereo_pair(
    img: &DynamicImage,
    dm: &DepthMap,
    max_disparity: u32,
    path: &Path,
) -> Result<()> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let left  = shift_image(&rgba, dm, max_disparity, -1.0); // left eye: shift right
    let right = shift_image(&rgba, dm, max_disparity,  1.0); // right eye: shift left

    let mut out: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(w * 2 + 4, h);
    // Copy left
    for y in 0..h {
        for x in 0..w {
            let p = left.get_pixel(x, y);
            out.put_pixel(x, y, Rgb([p[0], p[1], p[2]]));
        }
    }
    // Divider
    for y in 0..h {
        out.put_pixel(w,   y, Rgb([50, 50, 50]));
        out.put_pixel(w+1, y, Rgb([50, 50, 50]));
        out.put_pixel(w+2, y, Rgb([50, 50, 50]));
        out.put_pixel(w+3, y, Rgb([50, 50, 50]));
    }
    // Copy right
    for y in 0..h {
        for x in 0..w {
            let p = right.get_pixel(x, y);
            out.put_pixel(w + 4 + x, y, Rgb([p[0], p[1], p[2]]));
        }
    }
    out.save(path)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
//  3. Anaglyph
// ─────────────────────────────────────────────────────────────────────────────

pub fn save_anaglyph(
    img: &DynamicImage,
    dm: &mut DepthMap,
    max_disparity: u32,
    mode: &str,
    path: &Path,
) -> Result<()> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let left  = shift_image(&rgba, dm, max_disparity, -1.0);
    let right = shift_image(&rgba, dm, max_disparity,  1.0);

    let mut out: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let l = left.get_pixel(x, y);
            let r = right.get_pixel(x, y);
            let pixel = match mode {
                "green-magenta" => Rgb([
                    ((r[0] as u16 + r[2] as u16) / 2) as u8,  // magenta left ← right eye
                    l[1],                                       // green  right ← left eye
                    ((r[0] as u16 + r[2] as u16) / 2) as u8,
                ]),
                "amber-blue" => Rgb([
                    ((l[0] as u16 + l[1] as u16) / 2) as u8,  // amber ← left eye
                    ((l[0] as u16 + l[1] as u16) / 2) as u8,
                    r[2],                                       // blue ← right eye
                ]),
                _ => Rgb([l[0], r[1], r[2]]),  // red-cyan (default)
            };
            out.put_pixel(x, y, pixel);
        }
    }
    out.save(path)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
//  4. Wiggle GIF
// ─────────────────────────────────────────────────────────────────────────────

pub fn save_wiggle_gif(
    img: &DynamicImage,
    dm: &DepthMap,
    max_disparity: u32,
    path: &Path,
) -> Result<()> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    // Generate 5 frames at disparity offsets: -1, -0.5, 0, 0.5, 1
    let offsets = [-1.0f32, -0.5, 0.0, 0.5, 1.0, 0.5, 0.0, -0.5];
    let delay = Delay::from_numer_denom_ms(120, 1);

    let file = std::fs::File::create(path)?;
    let mut encoder = GifEncoder::new(file);
    encoder.set_repeat(Repeat::Infinite)?;

    for &offset in &offsets {
        let shifted = shift_image(&rgba, dm, max_disparity, offset);
        let frame = Frame::from_parts(shifted, 0, 0, delay);
        encoder.encode_frame(frame)?;
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
//  5. JSON report
// ─────────────────────────────────────────────────────────────────────────────

pub fn save_report(report: &AnalysisReport, path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(path, json)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
//  Helper: depth-based image shift
// ─────────────────────────────────────────────────────────────────────────────

/// Shift every pixel horizontally by `sign * depth * max_disparity` pixels.
/// Positive sign → shift right (right eye view).
/// Negative sign → shift left (left eye view).
fn shift_image(rgba: &RgbaImage, dm: &DepthMap, max_disparity: u32, sign: f32) -> RgbaImage {
    let (w, h) = rgba.dimensions();
    let mut out = RgbaImage::new(w, h);

    // Fill with black
    for pixel in out.pixels_mut() {
        *pixel = image::Rgba([0, 0, 0, 255]);
    }

    for y in 0..h {
        for x in 0..w {
            let depth = dm[y as usize][x as usize];
            let shift = (sign * depth * max_disparity as f32).round() as i32;
            let nx = x as i32 + shift;
            if nx >= 0 && nx < w as i32 {
                out.put_pixel(nx as u32, y, *rgba.get_pixel(x, y));
            }
        }
    }
    out
}
