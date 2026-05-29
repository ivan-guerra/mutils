# Mousesim

A library and command-line tool for simulating human-like mouse movements using
recorded segment data.

## Overview

Mousesim provides functionality to replay natural mouse movements by composing
sequences of real gesture segments. It uses spatial indexing (R-trees) to
efficiently find and chain similar movement patterns from a database of recorded
segments, creating smooth, human-like cursor trajectories.

## Features

- **Spatial segment database** - Efficient R-tree indexing for fast similarity
  queries
- **Human-like movement synthesis** - Chains real gesture segments to create
  natural trajectories
- **Flexible querying** - K-nearest neighbor and single nearest segment lookups
- **Binary segment loading** - Reads recordings from the recorder application
- **Interactive CLI** - Command-line interface for testing movements
- **Library API** - Integrate mouse simulation into your own applications

## Installation

You can download pre-built binaries for Windows and Linux from the [releases
page][1].

To use as a library, add it to your `Cargo.toml`:

```toml
[dependencies]
mousesim = { git = "https://github.com/ivan-guerra/mutils", branch = "master" }
```

## Usage

Basic usage with a recordings directory:

```bash
mousesim recording_dir
```

The interactive prompt accepts target coordinates:

```
> 500 300
Moving from (100, 100) to (500, 300)
> 1000 800
Moving from (500, 300) to (1000, 800)
> quit
```

Run `mousesim --help` for all available options.

## Library Usage

### Loading a Segment Database

```rust
use mousesim::SegmentDatabase;
use std::path::Path;

// Load all segments from a directory of recordings
let db = SegmentDatabase::load_from_directory(
    Path::new("recordings")
)?;

println!("Loaded {} segments", db.size());
```

### Finding Similar Segments

```rust
// Find the single nearest segment to a displacement vector
let segment = db.find_nearest(50.0, 30.0);

// Find the 5 nearest segments with their distances
let neighbors = db.find_k_nearest(50.0, 30.0, 5);
for (segment, dist_sq) in neighbors {
    println!("Segment: {:?}, Distance²: {}", segment, dist_sq);
}
```

### Simulating Mouse Movement

```rust
use mousesim::move_mouse_humanlike;

// Move from current position to target
let start = (100, 100);
let target = (500, 300);

move_mouse_humanlike(&db, start, target)?;
```

## Related Crates

- **[segments](../segments)** - Core library for segment representation and
  processing
- **[recorder](../recorder)** - Application for recording mouse gestures into
  segment databases

[1]: https://github.com/ivan-guerra/mutils/releases
