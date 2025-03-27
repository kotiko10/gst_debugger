use std::process::Command;
use std::fs::{File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use clap::Parser;
use plotters::prelude::*;
use regex::Regex;

/// Struct to store extracted log data
#[derive(Debug)]
struct BitrateEntry {
    timestamp: f64,
    bitrate: u64,
}

/// Command-line arguments for pipeline and tracing options
#[derive(Parser)]
#[command(name = "GstShark Visualizer")]
#[command(version = "1.0")]
#[command(about = "Runs a GStreamer pipeline with tracing and visualizes the results")]
struct Args {
    /// GStreamer pipeline
    #[arg(short, long)]
    pipeline: String,

    /// Tracing type (latency, bitrate, framerate, etc.)
    #[arg(short, long)]
    tracing: String,

    /// Output graph file
    #[arg(short, long, default_value = "output.png")]
    output: String,
}

/// Runs the GStreamer pipeline with GstShark tracing enabled
fn run_pipeline_with_tracing(pipeline: &str, tracing: &str, log_file: &str) {
    let command = format!(
        "GST_TRACERS=\"{}\" GST_DEBUG=\"GST_TRACER:7\" gst-launch-1.0 {} 2> {}",
        tracing, pipeline, log_file
    );
    println!("Running: {}", command);

    let _output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .expect("Failed to execute GStreamer pipeline");
}

/// Parses tracing log file and extracts timestamp + bitrate values
fn parse_tracing_log(log_file: &str) -> Vec<BitrateEntry> {
    let path = Path::new(log_file);
    if !path.exists() {
        eprintln!("Error: Log file not found!");
        return vec![];
    }

    let file = File::open(log_file).expect("Failed to open log file");
    let reader = BufReader::new(file);
    let mut data = Vec::new();

    // Regex pattern to extract timestamp and bitrate
    let re = Regex::new(r"(\d+):(\d+):(\d+\.\d+) .* bitrate, .* bitrate=\(guint64\)(\d+);").unwrap();

    for line in reader.lines() {
        let line = line.unwrap();
        if let Some(caps) = re.captures(&line) {
            println!("{}",line);
            let hours: f64 = caps[1].parse().unwrap();
            let minutes: f64 = caps[2].parse().unwrap();
            let seconds: f64 = caps[3].parse().unwrap();
            let bitrate: u64 = caps[4].parse().unwrap();

            // Convert time to seconds
            let timestamp = hours * 3600.0 + minutes * 60.0 + seconds;

            data.push(BitrateEntry { timestamp, bitrate });
        }
    }
    data
}

/// Generates a visual representation of the tracing data
fn plot_graph(data: Vec<BitrateEntry>, output_file: &str, title: &str) {
    let root = BitMapBackend::new(output_file, (800, 600)).into_drawing_area();
    root.fill(&WHITE).unwrap();

    let max_value = data.iter().map(|entry| entry.bitrate).fold(0, u64::max) as f64;

    let mut chart = ChartBuilder::on(&root)
        .caption(title, ("sans-serif", 30))
        .margin(5)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0.0..data.last().unwrap().timestamp, 0.0..max_value)
        .unwrap();

    chart.configure_mesh().draw().unwrap();

    chart
        .draw_series(LineSeries::new(
            data.iter().map(|entry| (entry.timestamp, entry.bitrate as f64)),
            &RED,
        ))
        .unwrap();

    println!("Graph saved as {}", output_file);
}

fn main() {
    let args = Args::parse();

    let log_file = "gst_shark_log.txt";
    run_pipeline_with_tracing(&args.pipeline, &args.tracing, log_file);

    let data = parse_tracing_log(log_file);
    if !data.is_empty() {
        plot_graph(data, &args.output, &format!("GStreamer {} Tracking", args.tracing));
    } else {
        eprintln!("No tracing data found!");
    }
}
