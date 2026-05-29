# Mouse Recorder

A mouse gesture recorder that captures and segments continuous mouse movements
into discrete gestures, saving them in a compressed binary format.

## Features

- **High-frequency sampling** - Configurable sampling rate (default 60Hz)
- **Automatic segmentation** - Detects meaningful gestures based on movement and
  inactivity
- **Efficient storage** - Binary serialization with postcard format
- **Automatic file rotation** - Prevents large file accumulation
- **Configurable thresholds** - Fine-tune movement detection and segment
  validity

## Installation

You can download pre-built binaries for Windows and Linux from the [releases
page][1].

## Usage

Basic recording with defaults:

```bash
recorder recording_dir
```

Recording with a custom sampling rate and logging:

```bash
recorder recording_dir --rate-hz 125 --log-level info
```

Run `recorder --help` for all available options and configurations.

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

The segments are made of `Sample` records, each containing:

- `t_ms`: Timestamp in milliseconds relativ to the beginning of the recording
- `x`: Mouse X coordinate in pixels
- `y`: Mouse Y coordinate in pixels

[1]: https://github.com/ivan-guerra/mutils/releases
