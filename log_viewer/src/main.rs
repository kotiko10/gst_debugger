use eframe::egui;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use clap::Parser;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::task;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use regex::Regex;
//button for refreshin, adding latency and filiter data and complete thresholds, GUI and background constrast
//start testing using diffreent eleemtnsand measurments
/// Struct for real-time tracing data
#[derive(Debug, Clone)]
struct TracingData {
    element: String,
    bitrate: Option<u64>,
    framerate: Option<f64>,
}

/// Interlatency between two elements
#[derive(Debug, Clone)]
struct InterLatencyData {
    from: String,
    to: String,
    time: String,
}

/// CLI Arguments
#[derive(Parser, Debug)]
#[command(name = "gst_debugger")]
struct Args {
    /// GStreamer pipeline string
    #[arg(short, long)]
    pipeline: String,

    /// Tracing types (e.g., "bitrate;framerate;interlatency")
    #[arg(short, long)]
    tracing: String,
}

/// GUI state with drag positions
struct GstDebugger {
    logs: Arc<Mutex<Vec<TracingData>>>,
    interlatency: Arc<Mutex<Vec<InterLatencyData>>>,
    graph: DiGraph<String, ()>,
    node_map: HashMap<String, NodeIndex>,
    receiver: mpsc::Receiver<TracingData>,
    latency_receiver: mpsc::Receiver<InterLatencyData>,
    positions: HashMap<NodeIndex, egui::Pos2>,
}

impl GstDebugger {
    fn new(
        pipeline: String,
        receiver: mpsc::Receiver<TracingData>,
        latency_receiver: mpsc::Receiver<InterLatencyData>,
    ) -> Self {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();
        let mut positions = HashMap::new();

        let elements: Vec<&str> = pipeline.split("!").map(|s| s.trim()).collect();
        let mut prev_node = None;
        let mut x = 50.0;
        let y = 200.0;

        for &element in &elements {
            let node = graph.add_node(element.to_string());
            node_map.insert(element.to_string(), node);
            positions.insert(node, egui::pos2(x, y));
            x += 150.0;

            if let Some(prev) = prev_node {
                graph.add_edge(prev, node, ());
            }
            prev_node = Some(node);
        }

        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
            interlatency: Arc::new(Mutex::new(Vec::new())),
            graph,
            node_map,
            receiver,
            latency_receiver,
            positions,
        }
    }
}

impl eframe::App for GstDebugger {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(data) = self.receiver.try_recv() {
            self.logs.lock().unwrap().push(data);
        }

        while let Ok(lat) = self.latency_receiver.try_recv() {
            self.interlatency.lock().unwrap().push(lat);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("GStreamer Visual Debugger");

            let logs = self.logs.lock().unwrap();
            let inter = self.interlatency.lock().unwrap();

            let node_size = 120.0;
            let node_height = 70.0;

            for edge in self.graph.edge_indices() {
                let (start, end) = self.graph.edge_endpoints(edge).unwrap();
                let start_pos = self.positions[&start];
                let end_pos = self.positions[&end];

                ui.painter().line_segment(
                    [
                        egui::pos2(start_pos.x + node_size, start_pos.y + node_height / 2.0),
                        egui::pos2(end_pos.x, end_pos.y + node_height / 2.0),
                    ],
                    egui::Stroke::new(2.0, egui::Color32::WHITE),
                );

                let from_name = &self.graph[start];
                let to_name = &self.graph[end];

                if let Some(latency) = inter.iter().rev().find(|lat| {
                    lat.from.starts_with(from_name) && lat.to.starts_with(to_name)
                }) {
                    let label_pos = egui::pos2((start_pos.x + end_pos.x) / 2.0, start_pos.y - 10.0);
                    ui.painter().text(
                        label_pos,
                        egui::Align2::CENTER_CENTER,
                        format!("{} ns", latency.time),
                        egui::FontId::proportional(12.0),
                        egui::Color32::YELLOW,
                    );
                }
            }

            for node in self.graph.node_indices() {
                let pos = self.positions.entry(node).or_insert(egui::pos2(50.0, 200.0));
                let response = ui.allocate_rect(
                    egui::Rect::from_min_size(*pos, egui::vec2(node_size, node_height)),
                    egui::Sense::drag(),
                );

                if response.dragged() {
                    pos.x += response.drag_delta().x;
                    pos.y += response.drag_delta().y;
                }

                let element_name = self.graph[node].clone();
                let tracing_data = logs.iter().rev().find(|e| e.element.starts_with(&element_name));

                let display_text = match tracing_data {
                    Some(data) => format!(
                        "{}\nBitrate: {} bps\nFramerate: {:.1} fps",
                        element_name,
                        data.bitrate.unwrap_or(0),
                        data.framerate.unwrap_or(0.0)
                    ),
                    None => element_name.clone(),
                };

                ui.painter().rect_filled(
                    egui::Rect::from_min_size(*pos, egui::vec2(node_size, node_height)),
                    5.0,
                    egui::Color32::DARK_BLUE,
                );

                ui.painter().text(
                    egui::pos2(pos.x + 10.0, pos.y + 20.0),
                    egui::Align2::LEFT_CENTER,
                    display_text,
                    egui::FontId::proportional(13.0),
                    egui::Color32::WHITE,
                );
            }
        });

        ctx.request_repaint();
    }
}

#[tokio::main]
async fn main() {
    let args: Args = Args::parse();
    let (tx, rx) = mpsc::channel(100);
    let (lat_tx, lat_rx) = mpsc::channel(100);

    task::spawn(run_pipeline_with_tracing(
        args.pipeline.clone(),
        args.tracing.clone(),
        tx,
        lat_tx,
    ));

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "GStreamer Debugger",
        options,
        Box::new(|_cc| Box::new(GstDebugger::new(args.pipeline, rx, lat_rx))),
    )
    .expect("Failed to start GUI");
}

async fn run_pipeline_with_tracing(
    pipeline: String,
    tracing: String,
    tx: mpsc::Sender<TracingData>,
    lat_tx: mpsc::Sender<InterLatencyData>,
) {
    let cmd = format!(
        "GST_TRACERS=\"{}\" GST_DEBUG=\"GST_TRACER:7\" gst-launch-1.0 {}",
        tracing, pipeline
    );

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to launch GStreamer");

    let stderr = child.stderr.take().expect("No stderr");
    let reader = BufReader::new(stderr);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(entry) = parse_gst_tracer_output(&line) {
            let _ = tx.send(entry).await;
        } else if let Some(latency) = parse_interlatency(&line) {
            let _ = lat_tx.send(latency).await;
        }
    }
}

fn parse_gst_tracer_output(line: &str) -> Option<TracingData> {
    let bitrate_re = Regex::new(r"bitrate.*pad=\(string\)(\S+), bitrate=\(guint64\)(\d+);").ok()?;
    let framerate_re = Regex::new(r"framerate.*pad=\(string\)(\S+), fps=\(uint\)(\d+);").ok()?;

    if let Some(caps) = bitrate_re.captures(line) {
        return Some(TracingData {
            element: caps[1].to_string(),
            bitrate: Some(caps[2].parse().ok()?),
            framerate: None,
        });
    }

    if let Some(caps) = framerate_re.captures(line) {
        return Some(TracingData {
            element: caps[1].to_string(),
            bitrate: None,
            framerate: Some(caps[2].parse().ok()?),
        });
    }

    None
}

fn parse_interlatency(line: &str) -> Option<InterLatencyData> {
    let regex = Regex::new(r"interlatency.*from_pad=\(string\)(\S+), to_pad=\(string\)(\S+), time=\(string\)(\S+);").ok()?;
    let caps = regex.captures(line)?;
    Some(InterLatencyData {
        from: caps[1].to_string(),
        to: caps[2].to_string(),
        time: caps[3].to_string(),
    })
}
