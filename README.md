# plottercontroll

A cross-platform (Windows/Linux) pen plotter controller for Marlin-based CNC machines.
Converts SVG, PNG, and text into G-code, previews toolpaths, and streams to the printer via USB serial.

## Features

- **SVG Import** — Vector extraction via `usvg` with full path support, or raster-render + threshold + contour tracing. Ramer-Douglas-Peucker simplification.
- **PNG Import** — Three modes: Edge Detection (Canny + Sobel fallback), Monochrome (Otsu threshold + contour trace), Centerlines (Zhang-Suen thinning skeleton). Optional hatch fill.
- **Text Rendering** — TTF/OTF outline extraction via `ttf_parser`, auto-fallback to built-in 95-character Hershey single-line font. Multiline, letter spacing, hatch fill.
- **Device Profiles** — Built-in profiles (Ender 5 Pro Pen Plotter Mod, Default) loaded from `src/profiles/*.txt` at compile time. Select via Device toolbar menu.
- **Constant Pen Speed** — Per-segment feedrate capping so every segment takes the same minimum time. Clamped between configurable min/max feedrate to avoid ink bleeding and uneven lines.
- **G-Code Engine** — Configurable Z up/down, XY/Z/travel feedrates, Y-axis inversion (bed-slinger), auto-home, bed dimensions.
- **USB Serial Streaming** — Send-wait-`ok` handshake protocol. Background thread with `AtomicBool` cancellation, `mpsc` status channel.
- **Unified Element Manager** — Multiple file sources (SVG/PNG) and text elements in a single ordered list. Per-element scale slider, font size adjust, visibility toggle, drag-to-position, reorder.
- **Project Save/Open** — JSON `.pcp` format via serde — self-contained, stores all paths, config, and positions.
- **Undo/Redo** — 50-level snapshot stack covering position, scale, visibility, and element changes.
- **Toolpath Preview** — Color-coded paths, zoom/pan, grid, bed outline, start-point markers, position offset display.
- **Embedded Text Editor** — Custom Rust editor with copy/paste, cursor navigation, multiline (avoids egui TextEdit issues on Wayland/ARM).

## Quick Start

### GUI Mode (default)
```bash
cargo run --release
```

### CLI Mode (headless)
```bash
# Export SVG to G-code file
cargo run --release --no-default-features -- --input drawing.svg --output output.gcode

# Convert PNG to G-code
cargo run --release --no-default-features -- --input photo.png --output output.gcode

# Render text and stream to printer
cargo run --release --no-default-features -- \
  --text "Hello World" --port /dev/ttyUSB0 \
  --pen-up-z 5.0 --pen-down-z 0.0

# List available serial ports
cargo run --release --no-default-features -- --list-ports
```

## CLI Arguments

| Argument | Description | Default |
|---|---|---|
| `-i, --input <FILE>` | Input SVG or PNG file | — |
| `-t, --text <TEXT>` | Text to plot | — |
| `--font <PATH>` | TTF/OTF font file path | Auto-detect |
| `--font-size <MM>` | Font size in mm | 10.0 |
| `--force-hershey` | Use Hershey single-line font | false |
| `-o, --output <FILE>` | Output .gcode file | — |
| `-p, --port <PORT>` | Serial port to stream to | — |
| `-b, --baud <BAUD>` | Baud rate | 115200 |
| `--pen-up-z <Z>` | Pen up Z height | 5.0 |
| `--pen-down-z <Z>` | Pen down Z height | 0.0 |
| `--feedrate-xy <F>` | XY travel speed (mm/min) | 3000 |
| `--feedrate-z <F>` | Z travel speed (mm/min) | 300 |
| `--list-ports` | List serial ports and exit | — |
| `--gui` | Force GUI mode | Auto if no args |

## GUI Walkthrough

### Top Bar
- **File**: Export G-code, Save Project (`.pcp`), Open Project, Clear, Quit
- **Import**: SVG, PNG (each opens a config dialog)
- **View**: Zoom to Fit, Zoom In/Out, Reset Zoom, Reset Pan
- **Device**: Profile picker (Custom + all built-in profiles)

### Left Sidebar (collapsible sections)
| Section | Controls |
|---|---|
| **Pen** | Up Z, Down Z, Thickness, Max/Min/Z/Travel feedrates, Bed dimensions, Auto-home, Invert Y, Constant pen speed (segment time, min/max F) |
| **Positioning** | Pan/Position drag mode toggle, Reset Position |
| **Serial Port** | Port combo box, Baud rate, Refresh Ports |
| **Actions** | Undo, Redo, Auto-Fit to Bed, Export G-code, Stream to Printer (with progress) |
| **Info** | Status, path/point counts |

### Right Panel (element manager)
- Unified list of file sources and text elements
- Per-element: select, scale slider (file) / font size ± (text), Show/Hide, Remove, ▲/▼ reorder
- "+ Add Text" button

### Toolpath Preview
- **Pan**: Left mouse drag (default mode)
- **Position**: Toggle mode — drag moves selected element on the bed
- **Zoom**: Scroll wheel at cursor
- Grid at 10 mm intervals, solid bed outline, start-point markers

## Device Profiles

Built-in profiles live in `src/profiles/` as `.txt` files. The build script embeds them at compile time. Add a new one by creating a new `.txt` file:

```text
name = My Printer
pen_up_z = 5.0
pen_down_z = 0.0
feedrate_xy = 3000.0
bed_width = 220.0
bed_height = 220.0
y_axis_invert = true
auto_home = true
travel_feedrate = 5000.0
```

Select a profile from Device → menu to apply its config values.

## Building from Source

### Prerequisites
- Rust 2021 edition (1.75+)
- Linux: `sudo apt install libudev-dev pkg-config`

```bash
# GUI version (default)
cargo build --release

# CLI-only (no GUI deps)
cargo build --release --no-default-features

# Tests
cargo test --no-default-features
```

## Feature Flags

| Flag | Default | Description |
|---|---|---|
| `gui` | Yes | egui/eframe/rfd for GUI. Disable with `--no-default-features` |

## Troubleshooting

| Problem | Solution |
|---|---|
| `libudev` build error | `sudo apt install libudev-dev pkg-config` |
| GUI OpenGL failure | `sudo apt install libgl1-mesa-dev` |
| No serial ports | `sudo usermod -a -G dialout $USER` then log out/in |
| Paths off bed | Auto-Fit to Bed, or drag to position |
| SVG/PNG import slow | Enable RDP simplification, reduce source complexity |

## License

GNU General Public License v3.0
