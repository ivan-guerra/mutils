//! Spatial database for efficient segment storage and retrieval.
//!
//! This module provides a database for storing and querying gesture segments using R-tree spatial
//! indexing. Segments are indexed by their displacement vectors (dx, dy), enabling efficient
//! nearest neighbor searches and similarity queries based on movement patterns. The database can
//! load segments from binary files and supports k-nearest-neighbor queries.

use segments::Segment;

use std::fs;
use std::path::Path;

use log::{debug, warn};
use rstar::RTree;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Postcard error: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("Segment error: {0}")]
    Segment(#[from] segments::SegmentError),
    #[error("No segments found in directory")]
    NoSegments,
}

pub struct SegmentDatabase {
    // R-tree indexed by displacement vector (dx, dy)
    spatial_index: RTree<Segment>,
}

impl SegmentDatabase {
    /// Load all segments from .bin files in the specified directory
    pub fn load_from_directory(path: &Path) -> Result<Self, DatabaseError> {
        let mut segments = Vec::new();

        // Read all entries in the directory
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            // Only process .bin files
            if path.extension().and_then(|s| s.to_str()) == Some("bin") {
                match Self::load_segments_from_file(&path) {
                    Ok(mut file_segments) => segments.append(&mut file_segments),
                    Err(e) => {
                        warn!("Failed to load {:?}: {}", path, e);
                        // Continue loading other files
                    }
                }
            }
        }

        if segments.is_empty() {
            return Err(DatabaseError::NoSegments);
        }

        debug!("Loaded {} segments from {:?}", segments.len(), path);

        // Build R-tree index from segments
        let spatial_index = RTree::bulk_load(segments);

        Ok(Self { spatial_index })
    }

    /// Load segments from a single .bin file (may contain multiple segments)
    fn load_segments_from_file(path: &Path) -> Result<Vec<Segment>, DatabaseError> {
        let data = fs::read(path)?;
        let mut segments = Vec::new();
        let mut offset = 0;

        // Read segments with length prefixes (as written by recorder)
        while offset < data.len() {
            if offset + 4 > data.len() {
                break; // Not enough bytes for length prefix
            }

            // Read length prefix (u32 little-endian)
            let len = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            offset += 4;

            if offset + len > data.len() {
                break; // Not enough bytes for segment data
            }

            // Deserialize segment
            let segment: Segment = postcard::from_bytes(&data[offset..offset + len])?;
            segments.push(segment);
            offset += len;
        }

        Ok(segments)
    }

    /// Get the total number of segments in the database
    pub fn size(&self) -> usize {
        self.spatial_index.size()
    }

    /// Check if the database is empty
    pub fn is_empty(&self) -> bool {
        self.spatial_index.size() == 0
    }

    /// Find the k nearest segments to a given displacement vector
    ///
    /// Returns iterator over the k nearest segments with their squared distances
    pub fn find_k_nearest(
        &self,
        dx: f64,
        dy: f64,
        k: usize,
    ) -> impl Iterator<Item = (&Segment, f64)> {
        self.spatial_index
            .nearest_neighbor_iter_with_distance_2([dx, dy])
            .take(k)
    }

    // Find the single nearest segment to a given displacement vector
    //
    // Returns the nearest segment
    pub fn find_nearest(&self, dx: f64, dy: f64) -> Option<&Segment> {
        self.spatial_index
            .nearest_neighbor_with_distance_2([dx, dy])
            .map(|(segment, _)| segment)
    }

    /// Access the R-tree directly for custom queries
    pub fn rtree(&self) -> &RTree<Segment> {
        &self.spatial_index
    }
}
