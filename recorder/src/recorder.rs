//! Persistent storage of gesture segments to disk.
//!
//! This module handles writing gesture segments to binary files with automatic
//! flushing and rotation. Segments are serialized using postcard format and written
//! with length prefixes for easy deserialization.

use crate::segmenter::Segment;

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use log::{debug, info, trace};

/// Configuration parameters for the recorder.
#[derive(Debug, Clone, clap::Args)]
pub struct RecorderConfig {
    /// Directory to store recordings
    pub recording_dir: PathBuf,

    /// Flush interval in seconds - how often to write segments to disk
    #[arg(long, default_value_t = 5)]
    pub flush_interval_secs: u64,

    /// Rotation interval in minutes - how often to create a new recording file
    #[arg(long, default_value_t = 60)]
    pub rotation_interval_mins: u64,
}

impl RecorderConfig {
    /// Returns the flush interval as a Duration.
    pub fn flush_interval(&self) -> Duration {
        Duration::from_secs(self.flush_interval_secs)
    }

    /// Returns the rotation interval as a Duration.
    pub fn rotation_interval(&self) -> Duration {
        Duration::from_secs(self.rotation_interval_mins * 60)
    }
}

/// Manages recording of gesture segments to disk with periodic flushing and file rotation.
///
/// The recorder buffers segments in memory and writes them to disk at regular intervals
/// to balance I/O overhead with data safety. Files are rotated periodically to keep
/// individual files manageable.
pub struct Recorder {
    config: RecorderConfig,
    writer: BufWriter<File>,
    current_file: PathBuf,
    last_flush: std::time::SystemTime,
    last_rotation: std::time::SystemTime,
    pending_segments: Vec<Segment>,
}

impl Recorder {
    /// Creates a new recorder with the given configuration.
    ///
    /// Creates the recording directory if it doesn't exist and opens the first
    /// recording file.
    pub fn new(config: RecorderConfig) -> Result<Self> {
        fs::create_dir_all(&config.recording_dir)
            .context("failed to create recording directory")?;

        let (file_path, writer) = Self::create_new_file(&config.recording_dir)?;
        info!("Created initial recording file: {}", file_path.display());
        let now = std::time::SystemTime::now();

        Ok(Self {
            config,
            writer,
            current_file: file_path,
            last_flush: now,
            last_rotation: now,
            pending_segments: Vec::new(),
        })
    }

    /// Creates a new recording file with a timestamp-based name.
    fn create_new_file(dir: &Path) -> Result<(PathBuf, BufWriter<File>)> {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("segments_{}.bin", timestamp);
        let path = dir.join(filename);

        debug!("Creating new recording file: {}", path.display());
        let file = File::create(&path)
            .with_context(|| format!("failed to create file: {}", path.display()))?;

        Ok((path, BufWriter::new(file)))
    }

    /// Records a segment for writing.
    ///
    /// The segment is buffered and will be written to disk during the next flush.
    /// Automatically triggers flush and rotation checks.
    pub fn record(&mut self, segment: Segment) -> Result<()> {
        trace!(
            "Buffering segment ({} points) for recording",
            segment.points().len()
        );
        self.pending_segments.push(segment);
        debug!("Total pending segments: {}", self.pending_segments.len());
        self.check_flush()?;
        self.check_rotation()?;
        Ok(())
    }

    /// Checks if it's time to flush pending segments and does so if needed.
    fn check_flush(&mut self) -> Result<()> {
        let now = std::time::SystemTime::now();
        let elapsed = now
            .duration_since(self.last_flush)
            .unwrap_or(Duration::ZERO);

        if elapsed >= self.config.flush_interval() && !self.pending_segments.is_empty() {
            debug!(
                "Flush interval reached ({}s), flushing {} segments",
                elapsed.as_secs(),
                self.pending_segments.len()
            );
            self.flush()?;
            self.last_flush = now;
        } else {
            trace!(
                "No flush needed: elapsed={}s, pending={}",
                elapsed.as_secs(),
                self.pending_segments.len()
            );
        }

        Ok(())
    }

    /// Checks if it's time to rotate to a new file and does so if needed.
    fn check_rotation(&mut self) -> Result<()> {
        let now = std::time::SystemTime::now();
        let elapsed = now
            .duration_since(self.last_rotation)
            .unwrap_or(Duration::ZERO);

        if elapsed >= self.config.rotation_interval() {
            info!(
                "Rotation interval reached ({}min), rotating to new file",
                elapsed.as_secs() / 60
            );
            self.rotate()?;
            self.last_rotation = now;
        } else {
            trace!("No rotation needed: elapsed={}min", elapsed.as_secs() / 60);
        }

        Ok(())
    }

    /// Writes all pending segments to disk.
    ///
    /// Each segment is serialized with postcard and prefixed with its length
    /// as a little-endian u32 for deserialization.
    fn flush(&mut self) -> Result<()> {
        let segment_count = self.pending_segments.len();
        let mut total_bytes = 0usize;

        for segment in self.pending_segments.drain(..) {
            let encoded = postcard::to_allocvec(&segment).context("failed to serialize segment")?;

            // Write length prefix so we can deserialize multiple segments
            let len = encoded.len() as u32;
            self.writer
                .write_all(&len.to_le_bytes())
                .context("failed to write segment length")?;

            self.writer
                .write_all(&encoded)
                .context("failed to write segment data")?;

            total_bytes += 4 + encoded.len(); // length prefix + data
        }

        self.writer.flush().context("failed to flush writer")?;
        info!(
            "Flushed {} segments ({} bytes) to {}",
            segment_count,
            total_bytes,
            self.current_file.display()
        );
        Ok(())
    }

    /// Rotates to a new recording file.
    ///
    /// Flushes pending data before creating a new file with a fresh timestamp.
    fn rotate(&mut self) -> Result<()> {
        // Flush any pending data before rotation
        if !self.pending_segments.is_empty() {
            debug!(
                "Flushing {} pending segments before rotation",
                self.pending_segments.len()
            );
            self.flush()?;
        }

        let old_file = self.current_file.display().to_string();
        let (new_path, new_writer) = Self::create_new_file(&self.config.recording_dir)?;
        self.writer = new_writer;
        self.current_file = new_path;

        info!(
            "Rotated from {} to {}",
            old_file,
            self.current_file.display()
        );
        Ok(())
    }

    /// Finalizes recording by flushing any remaining segments.
    ///
    /// Should be called before dropping the recorder to ensure no data is lost.
    pub fn finish(mut self) -> Result<()> {
        if !self.pending_segments.is_empty() {
            info!(
                "Finishing recorder, flushing {} remaining segments",
                self.pending_segments.len()
            );
            self.flush()?;
        } else {
            info!("Finishing recorder (no pending segments)");
        }
        Ok(())
    }
}
