use crate::recorder::{Recorder, RecorderConfig};
use crate::segmenter::{Sample, Segmenter, SegmenterConfig};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use enigo::{Enigo, Mouse, Settings};

mod recorder;
mod segmenter;

/// Mouse gesture recorder
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Recording rate in Hertz (samples per second)
    #[arg(long, default_value_t = 60.0)]
    rate_hz: f64,

    #[command(flatten)]
    recorder_config: RecorderConfig,

    #[command(flatten)]
    segmenter_config: SegmenterConfig,
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!(
        "Recording to directory: {:?}",
        args.recorder_config.recording_dir
    );
    println!(
        "Flush interval: {}s",
        args.recorder_config.flush_interval_secs
    );
    println!(
        "Rotation interval: {}min",
        args.recorder_config.rotation_interval_mins
    );
    println!("Sample rate: {:.1}Hz", args.rate_hz);

    // Setup Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        println!("\nReceived Ctrl+C, shutting down gracefully...");
        running_clone.store(false, Ordering::SeqCst);
    })?;

    let mut recorder = Recorder::new(args.recorder_config)?;
    let mut segmenter = Segmenter::new(args.segmenter_config);
    let enigo = Enigo::new(&Settings::default())?;

    let sample_interval = Duration::from_secs_f64(1.0 / args.rate_hz);
    let start_time = Instant::now();

    println!("Recording started. Press Ctrl+C to stop.");

    while running.load(Ordering::SeqCst) {
        let loop_start = Instant::now();

        // Get current mouse position
        let (x, y) = enigo.location()?;
        let t_ms = u64::try_from(start_time.elapsed().as_millis())?;

        let sample = Sample {
            t_ms,
            x: f64::from(x),
            y: f64::from(y),
        };

        // Process sample through segmenter
        if let Some(segment) = segmenter.push(sample)? {
            let first = segment
                .points()
                .first()
                .context("segment must have at least one point")?;
            let last = segment
                .points()
                .last()
                .context("segment must have at least one point")?;

            println!(
                "Segment detected: {} points, duration: {}ms",
                segment.points().len(),
                last.t_ms - first.t_ms
            );
            recorder.record(segment)?;
        }

        // Sleep to maintain sample rate
        let elapsed = loop_start.elapsed();
        if elapsed < sample_interval {
            thread::sleep(sample_interval - elapsed);
        }
    }

    // Flush final segment and recorder
    if let Some(segment) = segmenter.finish()? {
        println!(
            "Recording final segment with {} points",
            segment.points().len()
        );
        recorder.record(segment)?;
    }

    recorder.finish()?;
    println!("Recording complete. Data saved.");

    Ok(())
}
