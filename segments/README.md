# Segments

A library for segmenting continuous mouse position samples into discrete gesture
segments.

## Overview

Segments provides functionality to process streams of mouse position data and
identify meaningful gesture segments based on movement patterns and inactivity
criteria. It's designed for applications that need to analyze, record, or
recognize mouse gestures.

## Features

- **Stream-based segmentation** - Process mouse samples in real-time with a
  state machine
- **Configurable thresholds** - Fine-tune movement detection, inactivity
  timeouts, and validity criteria
- **Noise filtering** - Automatically filters out jittery movements and invalid
  segments
- **Spatial indexing** - Segments implement R-tree traits for efficient spatial
  queries
- **Zero-copy access** - Efficient access to segment data through slices
- **Comprehensive validation** - Built-in checks for segment duration,
  displacement, and point count

## Installation

To use as a library, add it to your `Cargo.toml`:

````toml
```toml
[dependencies]
segments = { git = "https://github.com/ivan-guerra/mutils", branch = "master" }
````

## Basic Example

```rust
use segments::{Segmenter, SegmenterConfig, Sample};

// Create a segmenter with default configuration
let config = SegmenterConfig {
    move_epsilon_px: 2.0,      // Minimum movement distance
    inactive_ms: 300,           // Inactivity timeout
    min_segment_duration_ms: 40,
    min_segment_displacement_px: 10.0,
    min_points: 3,
};

let mut segmenter = Segmenter::new(config);

// Process mouse samples
let sample1 = Sample { t_ms: 0, x: 100.0, y: 100.0 };
let sample2 = Sample { t_ms: 10, x: 150.0, y: 120.0 };
let sample3 = Sample { t_ms: 20, x: 200.0, y: 140.0 };

segmenter.push(sample1)?;
segmenter.push(sample2)?;

// Returns Some(segment) when a gesture is complete
if let Some(segment) = segmenter.push(sample3)? {
    println!("Captured gesture with {} points", segment.points().len());
    println!("Duration: {}ms", segment.duration()?);
    println!("Displacement: {}px", segment.displacement()?);
}

// Don't forget to finalize at the end
if let Some(final_segment) = segmenter.finish()? {
    // Handle the last segment
}
```

## How It Works

The segmenter operates as a state machine with two states:

1. **Idle** - Waiting for significant movement to start a new segment
2. **Recording** - Actively capturing points for the current segment

A segment is finalized when:

- The mouse becomes inactive for the configured timeout period
- `finish()` is called on the segmenter

Invalid segments (too short, too small displacement, etc.) are automatically
discarded.

## Related Crates

- **[recorder](../recorder)** - Binary application that uses this library to
  record mouse gestures
- **[mousesim](../mousesim)** - Library and binary for simulating mouse
  movements from recorded segments
