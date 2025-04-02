use std::process::{Command, exit};
use std::fs::{File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use clap::Parser;
use regex::Regex;
use std::fs;

/// Command-line arguments for pipeline and tracing options
#[derive(Parser)]
#[command(name = "GstShark Visualizer")]
#[command(version = "1.1")]
#[command(about = "Runs a GStreamer pipeline with multiple tracers and visualizes the results")]
struct Args {
    /// GStreamer pipeline
    #[arg(short, long)]
    pipeline: String,

    /// Tracing types (comma-separated: bitrate, latency, framerate)
    #[arg(short, long)]
    tracing: String,

    /// Output graph file
    #[arg(short, long, default_value = "output.png")]
    output: String,
}

/// Runs the GStreamer pipeline with tracing enabled
fn run_pipeline_with_tracing(pipeline: &str, tracing: &str, log_file: &str, dot_dir: &str) {
    let command = format!(
        "GST_DEBUG_DUMP_DOT_DIR=/tmp GST_TRACERS=\"{}\" GST_DEBUG=\"GST_TRACER:7\" gst-launch-1.0 {} 2> {}",
        tracing, pipeline, log_file
    );
    
    println!("Running: {}", command);

    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .status()
        .expect("Failed to execute GStreamer pipeline");

    if !status.success() {
        eprintln!("Error: GStreamer pipeline execution failed!");
        exit(1);
    }
}

/// Parses tracing log file and extracts values (bitrate, latency, framerate)
fn parse_tracing_log(log_file: &str) -> Vec<(String, String)> {
    let path = Path::new(log_file);
    if !path.exists() {
        eprintln!("Error: Log file not found!");
        return vec![];
    }

    let file = File::open(log_file).expect("Failed to open log file");
    let reader = BufReader::new(file);
    let mut data = Vec::new();

    // Regex patterns for different tracing metrics
   // let re = Regex::new(r"(\d+):(\d+):(\d+\.\d+) .* bitrate, .* bitrate=\(guint64\)(\d+);").unwrap();
    let bitrate_re = Regex::new(r"(\d+):(\d+):(\d+\.\d+) .* bitrate, .* bitrate=\(guint64\)(\d+);").unwrap();
    let latency_re = Regex::new(r"pad=\(string\)(\S+), .* latency=\(guint64\)(\d+)").unwrap();
    let framerate_re = Regex::new(r"(\d+):(\d+):(\d+\.\d+) .* framerate=\(guint64\)(\d+)").unwrap();

    for line in reader.lines() {
        let line = line.unwrap();

        if let Some(caps) = bitrate_re.captures(&line) {
            let element = caps[1].to_string();
            let value = format!("Bitrate: {} kbps", caps[2].parse::<u64>().unwrap() / 1000);
            data.push((element, value));
        }

        if let Some(caps) = latency_re.captures(&line) {
            let element = caps[1].to_string();
            let value = format!("Latency: {} ms", caps[2].parse::<u64>().unwrap() / 1000000);
            data.push((element, value));
        }

        if let Some(caps) = framerate_re.captures(&line) {
            let element = caps[1].to_string();
            let value = format!("Framerate: {} fps", caps[2].parse::<u64>().unwrap());
            data.push((element, value));
        }
    }
    data
}

/// Modifies the DOT file to include traced values
fn modify_dot_file(dot_file: &str, output_dot_file: &str, traced_data: Vec<(String, String)>) {
    let mut contents = fs::read_to_string(dot_file).expect("Failed to read DOT file");

    for (element, value) in traced_data {
        let pattern = format!("{} \\[label=\"{}", element, element);
        let replacement = format!("{} [label=\"{}\n{}\"", element, element, value);
        contents = contents.replace(&pattern, &replacement);
    }

    fs::write(output_dot_file, contents).expect("Failed to write modified DOT file");
}

/// Converts the modified DOT file to an image
fn generate_visualization(dot_file: &str, output_file: &str) {
    let status = Command::new("dot")
        .args(&["-Tpng", dot_file, "-o", output_file])
        .status()
        .expect("Failed to run Graphviz dot command");

    if status.success() {
        println!("Pipeline visualization saved as: {}", output_file);
    } else {
        eprintln!("Error generating visualization!");
    }
}
fn get_latest_dot_file(dot_dir: &str) -> Option<String> {
    let entries = fs::read_dir(dot_dir).ok()?;
    let mut dot_files: Vec<_> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "dot"))
        .collect();

    dot_files.sort_by_key(|entry| entry.metadata().unwrap().modified().unwrap());
    dot_files.last().map(|entry| entry.path().display().to_string())
}

fn main() {
    let args = Args::parse();

    let log_file = "gst_shark_log.txt";
    let dot_dir = "/tmp";
    let dot_file = match get_latest_dot_file("/tmp") {
        Some(file) => file,
        None => {
            eprintln!("Error: No DOT file found!");
            exit(1);
        }
    };
    let modified_dot_file = "/tmp/modified_pipeline.dot";

    run_pipeline_with_tracing(&args.pipeline, &args.tracing, log_file, dot_dir);

    let traced_data = parse_tracing_log(log_file);
    if traced_data.is_empty() {
        eprintln!("No tracing data found!");
        return;
    }

    modify_dot_file(&dot_file, &modified_dot_file, traced_data);
    generate_visualization(&modified_dot_file, &args.output);
}
