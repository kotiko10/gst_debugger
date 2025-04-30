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

/// Struct for real-time tracing data
#[derive(Debug, Clone)]
struct TracingData {
    element: String,
    bitrate: Option<u64>,
    framerate: Option<f64>,
}

/// CLI Arguments
#[derive(Parser, Debug)]
#[command(name = "gst_debugger")]
struct Args {
    /// GStreamer pipeline string
    #[arg(short, long)]
    pipeline: String,

    /// Tracing types (e.g., "bitrate;framerate")
    #[arg(short, long)]
    tracing: String,
}

/// GUI + graph data
struct GstDebugger {
    logs: Arc<Mutex<Vec<TracingData>>>,
    graph: DiGraph<String, ()>,
    node_map: HashMap<String, NodeIndex>,
    receiver: mpsc::Receiver<TracingData>,
}

impl GstDebugger {
    fn new(pipeline: String, receiver: mpsc::Receiver<TracingData>) -> Self {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();

        let elements: Vec<&str> = pipeline.split("!").map(|s| s.trim()).collect();
        let mut prev_node = None;

        for &element in &elements {
            let node = graph.add_node(element.to_string());
            node_map.insert(element.to_string(), node);

            if let Some(prev) = prev_node {
                graph.add_edge(prev, node, ());
            }
            prev_node = Some(node);
        }

        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
            graph,
            node_map,
            receiver,
        }
    }
}

impl eframe::App for GstDebugger {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Pull in new data from pipeline process
        while let Ok(data) = self.receiver.try_recv() {
            let mut logs = self.logs.lock().unwrap();
            logs.push(data);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("GStreamer Visual Debugger");

            let logs = self.logs.lock().unwrap();
            if logs.is_empty() {
                ui.label("No tracing data yet...");
                return;
            }

            let node_size = 120.0;
            let node_height = 70.0;
            let spacing = 150.0;
            let mut positions = HashMap::new();
            let mut x = 50.0;
            let y = 200.0;

            for node in self.graph.node_indices() {
                positions.insert(node, (x, y));
                x += spacing;
            }

            for node in self.graph.node_indices() {
                let (node_x, node_y) = positions[&node];
                let element_name = self.graph[node].clone();

                // Match log entries with prefix-based matching
                let tracing_data = logs.iter().rev().find(|e| {
                    e.element.starts_with(&element_name)
                });

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
                    egui::Rect::from_min_size(
                        egui::pos2(node_x, node_y),
                        egui::vec2(node_size, node_height),
                    ),
                    5.0,
                    egui::Color32::DARK_BLUE,
                );

                ui.painter().text(
                    egui::pos2(node_x + 10.0, node_y + 20.0),
                    egui::Align2::LEFT_CENTER,
                    display_text,
                    egui::FontId::proportional(13.0),
                    egui::Color32::WHITE,
                );
            }

            for edge in self.graph.edge_indices() {
                let (start, end) = self.graph.edge_endpoints(edge).unwrap();
                let (start_x, start_y) = positions[&start];
                let (end_x, end_y) = positions[&end];

                ui.painter().line_segment(
                    [
                        egui::pos2(start_x + node_size, start_y + node_height / 2.0),
                        egui::pos2(end_x, end_y + node_height / 2.0),
                    ],
                    egui::Stroke::new(2.0, egui::Color32::WHITE),
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

    task::spawn(run_pipeline_with_tracing(
        args.pipeline.clone(),
        args.tracing.clone(),
        tx,
    ));

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "GStreamer Debugger",
        options,
        Box::new(|_cc| Box::new(GstDebugger::new(args.pipeline, rx))),
    )
    .expect("Failed to start GUI");
}

/// Async: Run the GStreamer pipeline with tracers and parse the logs
async fn run_pipeline_with_tracing(pipeline: String, tracing: String, tx: mpsc::Sender<TracingData>) {
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
        }
    }
}

/// Extract bitrate/framerate data from GstTracer logs
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
