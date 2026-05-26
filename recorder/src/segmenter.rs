//! Segmentation of continuous mouse position samples into discrete gesture segments.
//!
//! This module provides functionality to process a stream of mouse position samples
//! and identify meaningful gesture segments based on movement and inactivity criteria.
//! A segment is created when the mouse moves and finalized when it becomes inactive
//! or when explicitly flushed.

use anyhow::{Context, Result};
use log::{debug, trace};
use serde::{Deserialize, Serialize};

/// A single mouse position sample with timestamp.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Sample {
    /// Timestamp in milliseconds from start of recording.
    pub t_ms: u64,
    /// X coordinate in screen pixels.
    pub x: f64,
    /// Y coordinate in screen pixels.
    pub y: f64,
}

impl Sample {
    /// Calculates Euclidean distance to another sample.
    fn distance_to(&self, other: &Self) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// A sequence of samples representing a discrete gesture or mouse movement.
#[derive(Debug, Serialize, Deserialize)]
pub struct Segment {
    points: Vec<Sample>,
}

impl Segment {
    /// Returns a slice of all samples in this segment.
    pub fn points(&self) -> &[Sample] {
        &self.points
    }

    /// Calculates the duration of the segment in milliseconds.
    ///
    /// Returns the time difference between the first and last sample.
    fn duration(&self) -> Result<u64> {
        let first = self
            .points
            .first()
            .context("segment must have at least one point for duration")?;
        let last = self
            .points
            .last()
            .context("segment must have at least one point for duration")?;
        Ok(last.t_ms.saturating_sub(first.t_ms))
    }

    /// Calculates the displacement of the segment in pixels.
    ///
    /// Returns the straight-line distance from the first to the last sample.
    fn displacement(&self) -> Result<f64> {
        let first = self
            .points
            .first()
            .context("segment must have at least one point for displacement")?;
        let last = self
            .points
            .last()
            .context("segment must have at least one point for displacement")?;
        Ok(first.distance_to(last))
    }

    /// Checks if the segment meets validity criteria.
    ///
    /// A segment is valid if it has enough points, sufficient duration,
    /// and sufficient displacement as defined by the configuration.
    fn is_valid(&self, config: &SegmenterConfig) -> Result<bool> {
        Ok(self.points.len() >= config.min_points
            && self.duration()? >= config.min_segment_duration_ms
            && self.displacement()? >= config.min_segment_displacement_px)
    }
}

/// Configuration parameters for gesture segmentation.
#[derive(Debug, Clone, Copy, clap::Args)]
pub struct SegmenterConfig {
    /// Movement epsilon in pixels - minimum distance to consider as movement
    #[arg(long, default_value_t = 2.0)]
    pub move_epsilon_px: f64,

    /// Inactivity timeout in milliseconds - time without movement to end segment
    #[arg(long, default_value_t = 300)]
    pub inactive_ms: u64,

    /// Minimum segment duration in milliseconds
    #[arg(long, default_value_t = 40)]
    pub min_segment_duration_ms: u64,

    /// Minimum segment displacement in pixels - straight-line distance from start to end
    #[arg(long, default_value_t = 10.0)]
    pub min_segment_displacement_px: f64,

    /// Minimum number of points in a valid segment
    #[arg(long, default_value_t = 3)]
    pub min_points: usize,
}

#[doc(hidden)]
enum State {
    /// Not currently recording a segment.
    Idle { last_sample: Option<Sample> },
    /// Actively recording a segment.
    Recording {
        segment: Vec<Sample>,
        last_sample: Sample,
        last_recorded_point: Sample,
        last_movement_time: u64,
    },
}

/// State machine for segmenting a stream of mouse samples into discrete gestures.
///
/// The segmenter tracks mouse movement and identifies meaningful gesture segments
/// by monitoring movement distance and inactivity periods. It filters out noise
/// and only records segments that meet minimum criteria for duration, displacement,
/// and point count.
pub struct Segmenter {
    config: SegmenterConfig,
    state: State,
}

impl Segmenter {
    /// Creates a new segmenter with the given configuration.
    pub fn new(config: SegmenterConfig) -> Self {
        Self {
            config,
            state: State::Idle { last_sample: None },
        }
    }

    /// Processes a new sample and returns a completed segment if one is finalized.
    ///
    /// The segmenter transitions between idle and recording states based on mouse
    /// movement. A segment is finalized and returned when inactivity is detected.
    /// Invalid segments (too short, too small displacement, etc.) are discarded.
    pub fn push(&mut self, sample: Sample) -> Result<Option<Segment>> {
        match std::mem::replace(&mut self.state, State::Idle { last_sample: None }) {
            State::Idle { last_sample: None } => {
                trace!(
                    "Segmenter initialized with first sample at t={}ms",
                    sample.t_ms
                );
                self.state = State::Idle {
                    last_sample: Some(sample),
                };
                Ok(None)
            }

            State::Idle {
                last_sample: Some(prev),
            } => {
                let distance = prev.distance_to(&sample);
                if distance > self.config.move_epsilon_px {
                    debug!(
                        "Movement detected ({}px), starting new segment at t={}ms",
                        distance as u32, sample.t_ms
                    );
                    self.state = State::Recording {
                        segment: vec![prev, sample],
                        last_sample: sample,
                        last_recorded_point: sample,
                        last_movement_time: sample.t_ms,
                    };
                } else {
                    trace!(
                        "No movement ({}px < {}px), remaining idle",
                        distance as u32, self.config.move_epsilon_px as u32
                    );
                    self.state = State::Idle {
                        last_sample: Some(sample),
                    };
                }
                Ok(None)
            }

            State::Recording {
                mut segment,
                last_sample: prev_sample,
                last_recorded_point,
                mut last_movement_time,
            } => {
                let sample_distance = prev_sample.distance_to(&sample);
                if sample_distance > self.config.move_epsilon_px {
                    trace!(
                        "Movement continues ({}px) at t={}ms",
                        sample_distance as u32, sample.t_ms
                    );
                    last_movement_time = sample.t_ms;
                }

                let last_recorded_point =
                    if last_recorded_point.distance_to(&sample) > self.config.move_epsilon_px {
                        trace!(
                            "Recording point {} at t={}ms",
                            segment.len() + 1,
                            sample.t_ms
                        );
                        segment.push(sample);
                        sample
                    } else {
                        trace!("Skipping point (too close to last recorded point)");
                        last_recorded_point
                    };

                let inactive_duration = sample.t_ms.saturating_sub(last_movement_time);

                if inactive_duration >= self.config.inactive_ms {
                    debug!(
                        "Inactivity timeout ({}ms), finalizing segment with {} points",
                        inactive_duration,
                        segment.len()
                    );
                    self.state = State::Idle {
                        last_sample: Some(sample),
                    };

                    let finished = Segment { points: segment };

                    if finished.is_valid(&self.config)? {
                        let duration = finished.duration()?;
                        let displacement = finished.displacement()?;
                        debug!(
                            "Segment valid: duration={}ms, displacement={}px, points={}",
                            duration,
                            displacement as u32,
                            finished.points.len()
                        );
                        return Ok(Some(finished));
                    } else {
                        let duration = finished.duration().unwrap_or(0);
                        let displacement = finished.displacement().unwrap_or(0.0);
                        debug!(
                            "Segment invalid (discarded): duration={}ms (min {}ms), displacement={}px (min {}px), points={} (min {})",
                            duration,
                            self.config.min_segment_duration_ms,
                            displacement as u32,
                            self.config.min_segment_displacement_px as u32,
                            finished.points.len(),
                            self.config.min_points
                        );
                    }
                    Ok(None)
                } else {
                    trace!(
                        "Recording continues: {} points, inactive for {}ms (< {}ms)",
                        segment.len(),
                        inactive_duration,
                        self.config.inactive_ms
                    );
                    self.state = State::Recording {
                        segment,
                        last_sample: sample,
                        last_recorded_point,
                        last_movement_time,
                    };
                    Ok(None)
                }
            }
        }
    }

    /// Finalizes any in-progress segment and returns it if valid.
    ///
    /// This should be called when the sample stream ends to ensure the last
    /// segment is not lost. Returns `None` if idle or if the segment is invalid.
    pub fn finish(self) -> Result<Option<Segment>> {
        match self.state {
            State::Recording { segment, .. } => {
                debug!(
                    "Finishing in-progress segment with {} points",
                    segment.len()
                );
                let finished = Segment { points: segment };
                if finished.is_valid(&self.config)? {
                    let duration = finished.duration()?;
                    let displacement = finished.displacement()?;
                    debug!(
                        "Final segment valid: duration={}ms, displacement={}px",
                        duration, displacement as u32
                    );
                    Ok(Some(finished))
                } else {
                    debug!("Final segment invalid (discarded)");
                    Ok(None)
                }
            }
            State::Idle { .. } => {
                debug!("Finishing with no active segment");
                Ok(None)
            }
        }
    }
}
