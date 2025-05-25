# GStreamer Visual Debugger

**GStreamer Visual Debugger** is a Rust-based GUI application that visually represents a GStreamer pipeline and provides real-time tracing metrics such as **bitrate**, **framerate**, and **inter-element latency** using GStreamer tracer data.

Built with [`eframe`](https://docs.rs/eframe/latest/eframe/) + [`egui`](https://docs.rs/egui/latest/egui/) for the frontend and leverages asynchronous tasks via [`tokio`](https://tokio.rs/) for concurrent tracing and parsing.

---

## ✨ Features

- 📊 **Real-Time Visualization**: Displays elements in the GStreamer pipeline graphically.
- 🔄 **Live Metrics Update**: Continuously updates bitrate, framerate, and interlatency values.
- 🎯 **Drag & Drop UI**: Move pipeline elements in the GUI for better visual clarity.
- 🔧 **Customizable Tracing**: Supports multiple GStreamer tracers via CLI.
- ⚡ **Asynchronous Tracing Engine**: Uses async I/O to efficiently parse tracer logs.
- 🎨 **Clean and Interactive UI**: Styled with egui for clarity and performance.

---

## 📦 Dependencies

Make sure you have the following installed:

- Rust (2021 edition)
- `cargo` (comes with Rust)
- [GStreamer](https://gstreamer.freedesktop.org/documentation/installing/index.html)
- GStreamer development tools (`gst-launch-1.0`, tracers, etc.)

### Cargo Dependencies

- `eframe`
- `egui`
- `clap`
- `tokio`
- `petgraph`
- `regex`

---

## 🚀 Usage

### 🛠️ Build

```sh
cargo build --release
```
### 🛠️ Run

```sh
cargo run -- --pipeline "videotestsrc ! autovideosink" --tracing "bitrate;framerate;interlatency"
```


📡 Data Tracing Internals

The tracing mechanism leverages GStreamer’s built-in tracers:
```

    Launches the pipeline with environment variables:
    GST_TRACERS, GST_DEBUG=GST_TRACER:7

    Parses logs in real time from stderr.

    Regex is used to extract:

        Bitrate: from lines like bitrate.*pad=..., bitrate=...

        Framerate: from lines like framerate.*pad=..., fps=...

        Interlatency: between two pads (elements)
```


### Architecture

```
+--------------+       +-----------------------+       +-------------+
| CLI Parser   |-----> | Tracer Log Processor  |-----> |  GUI Model  |
+--------------+       +-----------------------+       +-------------+
       |                       |                              |
       v                       v                              v
Args::parse()          Regex Extractors             egui Graph + Metrics
```