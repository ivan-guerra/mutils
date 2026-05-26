//! Mouse gesture recorder application.
//!
//! Records continuous mouse movements and segments them into discrete gestures,
//! saving the results to disk in a compressed binary format. The application
//! samples mouse position at a configurable rate and uses movement detection
//! and inactivity timeouts to identify meaningful gesture segments.

use crate::recorder::{Recorder, RecorderConfig};
use crate::segmenter::{Sample, Segmenter, SegmenterConfig};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use enigo::{Enigo, Mouse, Settings};
use log::info;

pub mod recorder;
pub mod segmenter;

#[doc(hidden)]
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Recording rate in Hertz (samples per second)
    #[arg(long, default_value_t = 60.0)]
    rate_hz: f64,

    /// Set logging level (off, error, warn, info, debug, trace)
    #[arg(long, value_name = "LEVEL", default_value = "off")]
    log_level: simplelog::LevelFilter,

    #[command(flatten)]
    recorder_config: RecorderConfig,

    #[command(flatten)]
    segmenter_config: SegmenterConfig,
}

#[doc(hidden)]
fn main() -> Result<()> {
    let args = Args::parse();

    if args.log_level != simplelog::LevelFilter::Off {
        simplelog::TermLogger::init(
            args.log_level,
            simplelog::ConfigBuilder::new()
                .set_thread_level(simplelog::LevelFilter::Off)
                .add_filter_allow_str("recorder")
                .build(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        )
        .context("Failed to initialize logger")?;
    }

    info!(
        "Recording to directory: {}",
        args.recorder_config.recording_dir.display()
    );
    info!(
        "Flush interval: {}s",
        args.recorder_config.flush_interval_secs
    );
    info!(
        "Rotation interval: {}min",
        args.recorder_config.rotation_interval_mins
    );
    info!("Sample rate: {:.1}Hz", args.rate_hz);
    // Setup Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        info!("Received Ctrl+C, shutting down gracefully...");
        running_clone.store(false, Ordering::SeqCst);
    })?;

    let mut recorder = Recorder::new(args.recorder_config)?;
    let mut segmenter = Segmenter::new(args.segmenter_config);
    let enigo = Enigo::new(&Settings::default())?;

    let sample_interval = Duration::from_secs_f64(1.0 / args.rate_hz);
    let start_time = Instant::now();

    info!("Recording started. Press Ctrl+C to stop.");

    while running.load(Ordering::SeqCst) {
        let loop_start = Instant::now();

        // Get current mouse position
        let (x, y) = enigo.location()?;
        let t_ms = start_time.elapsed().as_millis() as u64;

        let sample = Sample {
            t_ms,
            x: x as f64,
            y: y as f64,
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

            info!(
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
        info!(
            "Recording final segment with {} points",
            segment.points().len()
        );
        recorder.record(segment)?;
    }

    recorder.finish()?;
    info!("Recording complete. Data saved.");

    Ok(())
}
