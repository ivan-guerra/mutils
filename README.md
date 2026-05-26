# Mouse Recorder

A lightweight mouse gesture recorder that captures and segments continuous mouse
movements into discrete gestures, saving them in a compressed binary format.

## Features

- **High-frequency sampling** - Configurable sampling rate (default 60Hz)
- **Automatic segmentation** - Detects meaningful gestures based on movement and
  inactivity
- **Efficient storage** - Binary serialization with postcard format
- **Automatic file rotation** - Prevents large file accumulation
- **Configurable thresholds** - Fine-tune movement detection and segment
  validity

## Installation

### From Source

```bash
cargo install --path recorder
```

### From Release

Download pre-built binaries from the [releases page](../../releases).

## Usage

Basic recording with defaults:

```bash
recorder recording_dir
```

With custom sampling rate and logging:

```bash
recorder recording_dir --rate-hz 120 --log
```

### Command Line Options

```
Recording:
  recording_dir                    Directory to store recordings
  --flush-interval-secs <SECS>     Write interval (default: 5)
  --rotation-interval-mins <MINS>  New file interval (default: 60)
  --rate-hz <HZ>                   Sampling rate (default: 60.0)
  --log                            Enable debug logging

Segmentation:
  --move-epsilon-px <PX>           Movement threshold (default: 2.0)
  --inactive-ms <MS>               Inactivity timeout (default: 300)
  --min-segment-duration-ms <MS>   Minimum duration (default: 40)
  --min-segment-displacement-px <PX> Minimum displacement (default: 10.0)
  --min-points <N>                 Minimum points (default: 3)
```

## Output Format

Recordings are saved as binary files with the naming pattern:

```
segments_YYYYMMDD_HHMMSS.bin
```

Each file contains concatenated segments in the format:

```
[4-byte length (LE)] [postcard-encoded segment]
[4-byte length (LE)] [postcard-encoded segment]
...
```

### Data Structure

Each segment contains:

- `t_ms`: Timestamp in milliseconds
- `x`: X coordinate in pixels
- `y`: Y coordinate in pixels

## How It Works

1. **Sampling** - Mouse position is captured at the specified rate
2. **Movement detection** - Samples are compared using epsilon threshold
3. **Segmentation** - Active movements become segments after inactivity timeout
4. **Validation** - Segments must meet minimum duration, displacement, and point
   count
5. **Storage** - Valid segments are buffered and periodically flushed to disk
