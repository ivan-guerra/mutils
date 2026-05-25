use crate::recorder::{Recorder, RecorderConfig};
use crate::segmenter::{Sample, Segmenter, SegmenterConfig};

use anyhow::Result;
use clap::Parser;

mod recorder;
mod segmenter;

/// Mouse gesture recorder
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(flatten)]
    recorder_config: RecorderConfig,

    #[command(flatten)]
    config: SegmenterConfig,
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

    let mut recorder = Recorder::new(args.recorder_config)?;
    let mut segmenter = Segmenter::new(args.config);

    // Example usage with test data
    let samples = vec![
        Sample {
            t_ms: 0,
            x: 100.0,
            y: 100.0,
        },
        Sample {
            t_ms: 16,
            x: 100.0,
            y: 100.0,
        },
        Sample {
            t_ms: 32,
            x: 104.0,
            y: 101.0,
        },
        Sample {
            t_ms: 48,
            x: 110.0,
            y: 104.0,
        },
        Sample {
            t_ms: 64,
            x: 118.0,
            y: 108.0,
        },
        Sample {
            t_ms: 80,
            x: 118.0,
            y: 118.0,
        },
        Sample {
            t_ms: 200,
            x: 118.0,
            y: 108.0,
        },
        Sample {
            t_ms: 400,
            x: 118.0,
            y: 108.0,
        },
    ];

    for s in samples {
        if let Some(segment) = segmenter.push(s)? {
            println!("Recording segment with {} points", segment.points().len());
            recorder.record(segment)?;
        }
    }

    if let Some(segment) = segmenter.finish()? {
        println!(
            "Recording final segment with {} points",
            segment.points().len()
        );
        recorder.record(segment)?;
    }

    recorder.finish()?;
    println!("Recording complete");

    Ok(())
}
