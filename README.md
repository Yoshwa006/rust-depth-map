# image3d 🖼️→🔲

A Rust command-line tool that performs **deep image analysis** and converts any photo into multiple **3D outputs** — all with zero external API calls.

---

## Features

| Category | What it does |
|---|---|
| **Analysis** | Per-channel statistics, histogram, edge density & direction, GLCM texture, dominant colours (k-means), sharpness score, noise estimate, colour temperature |
| **Depth estimation** | Three algorithms: `luminance`, `gradient` (Sobel), `combined` (luminance + gradient + focus measure); optional bilateral smoothing + radial bias |
| **Anaglyph 3D** | Red-cyan, green-magenta, or amber-blue anaglyphs for use with coloured glasses |
| **Stereo pair** | Side-by-side left/right eye views for cross-eye or parallel viewing |
| **Wiggle GIF** | Animated GIF that creates a pseudo-3D "wobble" effect without any glasses |
| **Depth map** | Pseudo-coloured (turbo palette) depth map saved as PNG |
| **JSON report** | Full machine-readable analysis report |

---

## Installation

```bash
# Requires Rust 1.75+
git clone <repo>
cd image3d
cargo build --release
# Binary at: target/release/image3d
```

---

## Usage

```bash
# Basic (all defaults)
./image3d --input photo.jpg

# Full options
./image3d \
  --input           photo.jpg \
  --output-dir      results/ \
  --depth-method    combined \       # luminance | gradient | combined
  --anaglyph        red-cyan \       # red-cyan | green-magenta | amber-blue
  --max-disparity   30 \             # pixel shift for 3D effect (10-50 typical)
  --smooth-depth                     # apply bilateral smoothing to depth map
```

### All flags

| Flag | Default | Description |
|---|---|---|
| `--input` / `-i` | *(required)* | Input image path |
| `--output-dir` / `-o` | `output/` | Directory for all outputs |
| `--depth-method` / `-d` | `combined` | Depth algorithm |
| `--anaglyph` | `red-cyan` | Anaglyph colour mode |
| `--max-disparity` | `30` | Max stereo shift in pixels |
| `--save-depth` | `true` | Save pseudo-coloured depth map |
| `--save-stereo` | `true` | Save stereo pair PNG |
| `--save-report` | `true` | Save JSON analysis report |
| `--smooth-depth` | `true` | Apply bilateral smoothing |

---

## Output files

```
output/
├── depth_map.png          ← pseudo-colour depth (turbo palette)
├── stereo_pair.png        ← side-by-side left/right views
├── anaglyph_3d.png        ← coloured-glasses 3D image
├── wiggle.gif             ← animated 3D wobble (no glasses needed)
└── analysis_report.json   ← full machine-readable analysis
```

---

## Depth algorithms

### `luminance`
Treats brightness as depth: bright pixels are near, dark pixels are far.
Best for: front-lit portraits, product shots, scenes with strong contrast.

### `gradient`
Uses Sobel edge magnitude as depth. High-gradient edges are near,
smooth regions are far.
Best for: architectural shots, scenes with clear foreground objects.

### `combined` *(default)*
Blends luminance (40%) + gradient (30%) + focus measure/Laplacian (30%),
then applies a subtle radial bias (centre = near).
Best for: general photography, landscapes, mixed scenes.

The tool automatically suggests the best algorithm in the analysis report.

---

## Viewing the 3D outputs

- **Anaglyph**: wear red-cyan glasses (most common) and view `anaglyph_3d.png`
- **Stereo pair**: cross your eyes slightly so the two images merge, or use a stereo viewer
- **Wiggle GIF**: open in any GIF viewer — the animation creates a natural 3D illusion
- **Depth map**: useful for further processing (e.g. feeding into Stable Diffusion depth-conditioned models)

---

## Architecture

```
src/
├── main.rs        CLI parsing, orchestration
├── analysis.rs    Deep image analysis engine
├── depth.rs       Depth map estimation algorithms
├── output.rs      All 3D output generators
└── utils.rs       Shared utility functions
```

---

## Performance

On a 12MP image (4000×3000):
- Analysis: ~0.4 s
- Depth (combined + bilateral smooth): ~1.2 s
- All outputs: ~0.8 s
- **Total: ~2.5 s** (single-threaded, no GPU)

Use `--release` build for best performance.
