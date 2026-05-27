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
    pub fn duration(&self) -> Result<u64> {
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
    pub fn displacement(&self) -> Result<f64> {
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
    pub fn is_valid(&self, config: &SegmenterConfig) -> Result<bool> {
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
                        distance.trunc(),
                        sample.t_ms
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
                        distance.trunc(),
                        self.config.move_epsilon_px
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
                        sample_distance.trunc(),
                        sample.t_ms
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
                            displacement.trunc(),
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
                            displacement.trunc(),
                            self.config.min_segment_displacement_px.trunc(),
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
                        duration,
                        displacement.trunc()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> SegmenterConfig {
        SegmenterConfig {
            move_epsilon_px: 2.0,
            inactive_ms: 300,
            min_segment_duration_ms: 40,
            min_segment_displacement_px: 10.0,
            min_points: 3,
        }
    }

    fn sample(t_ms: u64, x: f64, y: f64) -> Sample {
        Sample { t_ms, x, y }
    }

    #[test]
    fn test_sample_distance() {
        let s1 = sample(0, 0.0, 0.0);
        let s2 = sample(0, 3.0, 4.0);
        assert_eq!(s1.distance_to(&s2), 5.0);
    }

    #[test]
    fn test_segment_duration() {
        let segment = Segment {
            points: vec![sample(100, 0.0, 0.0), sample(250, 10.0, 10.0)],
        };
        assert_eq!(segment.duration().unwrap(), 150);
    }

    #[test]
    fn test_segment_displacement() {
        let segment = Segment {
            points: vec![sample(0, 0.0, 0.0), sample(100, 3.0, 4.0)],
        };
        assert_eq!(segment.displacement().unwrap(), 5.0);
    }

    #[test]
    fn test_segment_validity() {
        let config = default_config();

        // Valid segment
        let valid = Segment {
            points: vec![
                sample(0, 0.0, 0.0),
                sample(20, 5.0, 0.0),
                sample(50, 15.0, 0.0),
            ],
        };
        assert!(valid.is_valid(&config).unwrap());

        // Too few points
        let few_points = Segment {
            points: vec![sample(0, 0.0, 0.0), sample(100, 20.0, 0.0)],
        };
        assert!(!few_points.is_valid(&config).unwrap());

        // Too short duration
        let short_duration = Segment {
            points: vec![
                sample(0, 0.0, 0.0),
                sample(10, 5.0, 0.0),
                sample(20, 15.0, 0.0),
            ],
        };
        assert!(!short_duration.is_valid(&config).unwrap());

        // Too small displacement
        let small_displacement = Segment {
            points: vec![
                sample(0, 0.0, 0.0),
                sample(25, 1.0, 0.0),
                sample(50, 2.0, 0.0),
            ],
        };
        assert!(!small_displacement.is_valid(&config).unwrap());
    }

    #[test]
    fn test_initial_sample() {
        let mut segmenter = Segmenter::new(default_config());
        let result = segmenter.push(sample(0, 10.0, 10.0)).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_no_movement_stays_idle() {
        let mut segmenter = Segmenter::new(default_config());
        segmenter.push(sample(0, 10.0, 10.0)).unwrap();

        // Small movements below epsilon
        let result = segmenter.push(sample(100, 10.5, 10.5)).unwrap();
        assert!(result.is_none());

        let result = segmenter.push(sample(200, 11.0, 10.0)).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_movement_starts_recording() {
        let mut segmenter = Segmenter::new(default_config());
        segmenter.push(sample(0, 10.0, 10.0)).unwrap();

        // Movement above epsilon
        let result = segmenter.push(sample(100, 15.0, 10.0)).unwrap();
        assert!(result.is_none()); // Still recording, not finalized
    }

    #[test]
    fn test_inactivity_finalizes_segment() {
        let mut segmenter = Segmenter::new(default_config());

        // Start with initial position
        segmenter.push(sample(0, 0.0, 0.0)).unwrap();

        // Movement to start recording
        segmenter.push(sample(10, 5.0, 0.0)).unwrap();
        segmenter.push(sample(20, 10.0, 0.0)).unwrap();
        segmenter.push(sample(30, 15.0, 0.0)).unwrap();
        segmenter.push(sample(50, 20.0, 0.0)).unwrap();

        // Wait for inactivity timeout (300ms default)
        let result = segmenter.push(sample(400, 20.0, 0.0)).unwrap();
        assert!(result.is_some());

        let segment = result.unwrap();
        assert!(segment.points().len() == 5);
    }

    #[test]
    fn test_invalid_segment_discarded() {
        let mut segmenter = Segmenter::new(default_config());

        // Start with initial position
        segmenter.push(sample(0, 0.0, 0.0)).unwrap();

        // Very short movement
        segmenter.push(sample(5, 3.0, 0.0)).unwrap();

        // Immediate inactivity
        let result = segmenter.push(sample(350, 3.0, 0.0)).unwrap();
        assert!(result.is_none()); // Segment too short, discarded
    }

    #[test]
    fn test_continuous_recording() {
        let mut segmenter = Segmenter::new(default_config());

        segmenter.push(sample(0, 0.0, 0.0)).unwrap();
        segmenter.push(sample(10, 5.0, 0.0)).unwrap();
        segmenter.push(sample(20, 10.0, 0.0)).unwrap();
        segmenter.push(sample(30, 15.0, 0.0)).unwrap();

        // Continue moving, no inactivity
        let result = segmenter.push(sample(40, 20.0, 0.0)).unwrap();
        assert!(result.is_none()); // Still recording
    }

    #[test]
    fn test_point_filtering() {
        let mut segmenter = Segmenter::new(default_config());

        segmenter.push(sample(0, 0.0, 0.0)).unwrap();
        segmenter.push(sample(10, 5.0, 0.0)).unwrap();

        // This point is too close to previous, should be filtered
        segmenter.push(sample(20, 5.5, 0.0)).unwrap();

        // This point is far enough
        segmenter.push(sample(30, 10.0, 0.0)).unwrap();
        segmenter.push(sample(50, 20.0, 0.0)).unwrap();

        // Trigger finalization
        let result = segmenter.push(sample(400, 20.0, 0.0)).unwrap();
        assert!(result.is_some());

        let segment = result.unwrap();
        // Should have filtered out the close point
        assert_eq!(segment.points().len(), 4); // Initial + 3 recorded (one filtered)
    }

    #[test]
    fn test_finish_with_active_segment() {
        let mut segmenter = Segmenter::new(default_config());

        segmenter.push(sample(0, 0.0, 0.0)).unwrap();
        segmenter.push(sample(10, 5.0, 0.0)).unwrap();
        segmenter.push(sample(20, 10.0, 0.0)).unwrap();
        segmenter.push(sample(50, 20.0, 0.0)).unwrap();

        // Finish without waiting for inactivity
        let result = segmenter.finish().unwrap();
        assert!(result.is_some());

        let segment = result.unwrap();
        assert!(segment.points().len() == 4);
    }

    #[test]
    fn test_finish_while_idle() {
        let mut segmenter = Segmenter::new(default_config());

        segmenter.push(sample(0, 0.0, 0.0)).unwrap();
        segmenter.push(sample(100, 0.5, 0.0)).unwrap(); // No movement

        let result = segmenter.finish().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_finish_with_invalid_segment() {
        let mut segmenter = Segmenter::new(default_config());

        segmenter.push(sample(0, 0.0, 0.0)).unwrap();
        segmenter.push(sample(5, 3.0, 0.0)).unwrap(); // Too short

        let result = segmenter.finish().unwrap();
        assert!(result.is_none()); // Invalid segment discarded
    }

    #[test]
    fn test_multiple_segments() {
        let mut segmenter = Segmenter::new(default_config());

        // First segment
        segmenter.push(sample(0, 0.0, 0.0)).unwrap();
        segmenter.push(sample(10, 5.0, 0.0)).unwrap();
        segmenter.push(sample(20, 10.0, 0.0)).unwrap();
        segmenter.push(sample(50, 20.0, 0.0)).unwrap();

        let result1 = segmenter.push(sample(400, 20.0, 0.0)).unwrap();
        assert!(result1.is_some());
        assert_eq!(result1.unwrap().points().len(), 4);

        // Second segment
        segmenter.push(sample(500, 30.0, 0.0)).unwrap();
        segmenter.push(sample(510, 35.0, 0.0)).unwrap();
        segmenter.push(sample(520, 40.0, 0.0)).unwrap();
        segmenter.push(sample(550, 50.0, 0.0)).unwrap();

        let result2 = segmenter.push(sample(900, 50.0, 0.0)).unwrap();
        assert!(result2.is_some());
        assert_eq!(result2.unwrap().points().len(), 5); // Includes the idle sample at t=400
    }

    #[test]
    fn test_zero_displacement_segment() {
        let mut segmenter = Segmenter::new(default_config());

        // Start and end at same position
        segmenter.push(sample(0, 10.0, 10.0)).unwrap();
        segmenter.push(sample(10, 15.0, 10.0)).unwrap();
        segmenter.push(sample(20, 20.0, 10.0)).unwrap();
        segmenter.push(sample(50, 10.0, 10.0)).unwrap(); // Back to start

        let result = segmenter.push(sample(400, 10.0, 10.0)).unwrap();
        assert!(result.is_none()); // Zero displacement, invalid
    }

    #[test]
    fn test_custom_config() {
        let config = SegmenterConfig {
            move_epsilon_px: 5.0,
            inactive_ms: 100,
            min_segment_duration_ms: 20,
            min_segment_displacement_px: 5.0,
            min_points: 2,
        };

        let mut segmenter = Segmenter::new(config);

        segmenter.push(sample(0, 0.0, 0.0)).unwrap();
        segmenter.push(sample(10, 10.0, 0.0)).unwrap();
        segmenter.push(sample(35, 20.0, 0.0)).unwrap();

        // Inactivity timeout - enough time has passed
        let result = segmenter.push(sample(150, 20.0, 0.0)).unwrap();
        assert!(result.is_some());
    }
}
