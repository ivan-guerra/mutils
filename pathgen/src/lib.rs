//! Human-like mouse movement generation using recorded gesture segments.
//!
//! This module provides functionality to move the mouse cursor between two points
//! using natural, human-like trajectories. It works by querying a database of
//! previously recorded gesture segments, finding the closest match to the desired
//! movement vector, and replaying it with proportional error correction to reach
//! the exact target position.

use std::thread;
use std::time::Duration;

use enigo::{Coordinate, Enigo, Mouse, Settings};
use thiserror::Error;

mod database;

pub use database::{DatabaseError, SegmentDatabase};

#[derive(Error, Debug)]
pub enum PathgenError {
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Failed to initialize mouse controller: {0}")]
    EnigoInit(String),

    #[error("Mouse input error: {0}")]
    Input(#[from] enigo::InputError),

    #[error("No segments available in database")]
    EmptyDatabase,

    #[error("Insufficient trajectory samples (found {0}, need at least 2)")]
    InsufficientSamples(usize),

    #[error("Mouse movement failed: {0}")]
    MouseMovement(String),
}

/// Moves the mouse cursor from `src` to `dst` using a human-like trajectory pattern
pub fn move_mouse_humanlike(
    segdb: &SegmentDatabase,
    src: (i32, i32),
    dst: (i32, i32),
) -> Result<(), PathgenError> {
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| PathgenError::EnigoInit(e.to_string()))?;

    // Calculate target total displacement vector
    let target_dx = f64::from(dst.0 - src.0);
    let target_dy = f64::from(dst.1 - src.1);

    // Query the database for the closest trajectory match
    let matched_seg = segdb
        .find_nearest(target_dx, target_dy)
        .ok_or(PathgenError::EmptyDatabase)?;

    let total_samples = matched_seg.points().len();
    if total_samples < 2 {
        return Err(PathgenError::InsufficientSamples(total_samples));
    }

    // Establish our baseline recording origin from the first sample
    let (origin_x, origin_y) = (matched_seg.points()[0].x, matched_seg.points()[0].y);

    // Calculate total recorded displacement of this segment
    let (final_abs_x, final_abs_y) = (
        matched_seg.points()[total_samples - 1].x,
        matched_seg.points()[total_samples - 1].y,
    );
    let recorded_dx = final_abs_x - origin_x;
    let recorded_dy = final_abs_y - origin_y;

    // Compute the target discrepancy (Error vector)
    let error_x = target_dx - recorded_dx;
    let error_y = target_dy - recorded_dy;

    // Execute the playback loop
    for (i, sample) in matched_seg.points().iter().enumerate() {
        // Since timestamps are relative to the previous frame, we sleep *before* moving.
        // The first sample is 0ms, so it executes instantly.
        if sample.t_ms > 0 {
            thread::sleep(Duration::from_millis(sample.t_ms));
        }

        // Calculate progress factor (0.0 to 1.0)
        let progress = i as f64 / (total_samples - 1) as f64;

        // Convert the absolute coordinate to a relative offset from its original starting point
        let human_offset_x = sample.x - origin_x;
        let human_offset_y = sample.y - origin_y;

        // Blend everything together: Target Start + Human Movement + Proportional Error Nudge
        let final_x = (f64::from(src.0) + human_offset_x + (progress * error_x)).round() as i32;
        let final_y = (f64::from(src.1) + human_offset_y + (progress * error_y)).round() as i32;

        // Move the mouse
        enigo.move_mouse(final_x, final_y, Coordinate::Abs)?;
    }

    Ok(())
}
