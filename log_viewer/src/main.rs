use eframe::egui;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use clap::Parser;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::task;
use tokio::fs::OpenOptions;
use chrono::Local;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use std::process::Stdio;
use regex::Regex;

#[derive(Debug, Clone)]
struct TracingData {
    element: String,
    bitrate: Option<u64>,
    framerate: Option<f64>,
}

#[derive(Debug, Clone)]
struct InterLatencyData {
    from: String,
    to: String,
    time: String,
}

#[derive(Parser, Debug)]
#[command(name = "gst_debugger")]
struct Args {
    #[arg(short, long)]
    pipeline: String,

    #[arg(short, long)]
    tracing: String,
}

struct GstDebugger {
    logs: Arc<Mutex<Vec<TracingData>>>,
    interlatency: Arc<Mutex<Vec<InterLatencyData>>>,
    graph: DiGraph<String, ()>,
    node_map: HashMap<String, NodeIndex>,
    receiver: mpsc::Receiver<TracingData>,
    latency_receiver: mpsc::Receiver<InterLatencyData>,
    positions: HashMap<NodeIndex, egui::Pos2>,
    bitrate_threshold: u64,
    framerate_threshold: f64,
    latency_threshold_ns: u64,
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
            bitrate_threshold: 0,
            framerate_threshold: 0.0,
            latency_threshold_ns: 0,
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

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(egui::Color32::from_gray(30)))
            .show(ctx, |ui| {
                ui.heading("GStreamer Visual Debugger");

                if ui.button("🔄 Refresh").clicked() {
                    self.logs.lock().unwrap().clear();
                    self.interlatency.lock().unwrap().clear();
                }

                ui.horizontal(|ui| {
                    ui.label("Min Bitrate:");
                    ui.add(egui::Slider::new(&mut self.bitrate_threshold, 0..=10_000_000));
                    ui.label("Min Framerate:");
                    ui.add(egui::Slider::new(&mut self.framerate_threshold, 0.0..=120.0));
                    ui.label("Max Latency (ns):");
                    ui.add(egui::Slider::new(&mut self.latency_threshold_ns, 0..=1_000_000));
                });

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
                        lat.from.starts_with(to_name)
                    }) {
                        let latency_val = latency.time.parse::<u64>().unwrap_or(0);
                        let color = if latency_val > self.latency_threshold_ns {
                            egui::Color32::RED
                        } else {
                            egui::Color32::YELLOW
                        };
                        let label_pos = egui::pos2((start_pos.x + end_pos.x) / 2.0, start_pos.y - 10.0);
                        ui.painter().text(
                            label_pos,
                            egui::Align2::CENTER_CENTER,
                            format!("{} ns", latency.time),
                            egui::FontId::proportional(12.0),
                            color,
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
                        Some(data) if data.bitrate.unwrap_or(0) >= self.bitrate_threshold
                            && data.framerate.unwrap_or(0.0) >= self.framerate_threshold =>
                        {
                            format!(
                                "{}\nBitrate: {} bps\nFramerate: {} fps",
                                element_name,
                                data.bitrate.unwrap_or(0),
                                data.framerate.unwrap_or(0.0)
                            )
                        }
                        _ => element_name.clone(),
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

    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let filename = format!("tracer_output_{}.log", timestamp);

  let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&filename)
        .await
        .expect("Failed to open tracer log file");

    while let Ok(Some(line)) = lines.next_line().await {
        // Write line to file with newline
        let _ = file.write_all(format!("{}\n", line).as_bytes()).await;

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

    let mut from = caps[1].split('.').next()?.to_string().split('_').next()?.to_string();
    from.truncate(from.len() - 1);
    let to = caps[2].split('.').next()?.to_string().split('_').next()?.to_string();

    Some(InterLatencyData {
        from,
        to,
        time: caps[3].to_string(),
    })
}