use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Sample {
    pub t_ms: u64,
    pub x: f64,
    pub y: f64,
}

impl Sample {
    fn distance_to(&self, other: &Self) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        (dx * dx + dy * dy).sqrt()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Segment {
    points: Vec<Sample>,
}

impl Segment {
    pub fn points(&self) -> &[Sample] {
        &self.points
    }

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

    fn is_valid(&self, config: &SegmenterConfig) -> Result<bool> {
        Ok(self.points.len() >= config.min_points
            && self.duration()? >= config.min_segment_duration_ms
            && self.displacement()? >= config.min_segment_displacement_px)
    }
}

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

enum State {
    Idle {
        last_sample: Option<Sample>,
    },
    Recording {
        segment: Vec<Sample>,
        last_sample: Sample,
        last_recorded_point: Sample,
        last_movement_time: u64,
    },
}

pub struct Segmenter {
    config: SegmenterConfig,
    state: State,
}

impl Segmenter {
    pub fn new(config: SegmenterConfig) -> Self {
        Self {
            config,
            state: State::Idle { last_sample: None },
        }
    }

    pub fn push(&mut self, sample: Sample) -> Result<Option<Segment>> {
        match std::mem::replace(&mut self.state, State::Idle { last_sample: None }) {
            State::Idle { last_sample: None } => {
                self.state = State::Idle {
                    last_sample: Some(sample),
                };
                Ok(None)
            }

            State::Idle {
                last_sample: Some(prev),
            } => {
                if prev.distance_to(&sample) > self.config.move_epsilon_px {
                    self.state = State::Recording {
                        segment: vec![prev, sample],
                        last_sample: sample,
                        last_recorded_point: sample,
                        last_movement_time: sample.t_ms,
                    };
                } else {
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
                if prev_sample.distance_to(&sample) > self.config.move_epsilon_px {
                    last_movement_time = sample.t_ms;
                }

                let last_recorded_point =
                    if last_recorded_point.distance_to(&sample) > self.config.move_epsilon_px {
                        segment.push(sample);
                        sample
                    } else {
                        last_recorded_point
                    };

                let inactive_duration = sample.t_ms.saturating_sub(last_movement_time);

                if inactive_duration >= self.config.inactive_ms {
                    self.state = State::Idle {
                        last_sample: Some(sample),
                    };

                    let finished = Segment { points: segment };

                    if finished.is_valid(&self.config)? {
                        return Ok(Some(finished));
                    }
                    Ok(None)
                } else {
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

    pub fn finish(self) -> Result<Option<Segment>> {
        match self.state {
            State::Recording { segment, .. } => {
                let finished = Segment { points: segment };
                if finished.is_valid(&self.config)? {
                    Ok(Some(finished))
                } else {
                    Ok(None)
                }
            }
            State::Idle { .. } => Ok(None),
        }
    }
}
