//! Pause/resume support via JSON checkpoint files.
//!
//! Long-running crawls can be interrupted and resumed without losing progress.
//! The [`CheckpointManager`] periodically (or on demand) writes a
//! [`CheckpointData`] snapshot to disk. The snapshot contains the URLs of all
//! pending requests and the set of seen fingerprints, which is enough to
//! reconstruct the scheduler's state on restart.
//!
//! Checkpoints are written atomically (write to a temp file, then rename) to
//! prevent corruption if the process is killed mid-write. When the crawl
//! completes normally, the checkpoint file is cleaned up automatically.
//!
//! The engine creates a `CheckpointManager` when you pass a `crawldir` path to
//! [`CrawlerEngine::new`](crate::spider::CrawlerEngine::new). If no `crawldir`
//! is provided, checkpointing is disabled entirely.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::error::{Result, SpiderError};

/// The default filename used for checkpoint data on disk.
const CHECKPOINT_FILE: &str = "checkpoint.json";

/// Serializable snapshot of crawler state for pause/resume support.
///
/// This struct is serialized to JSON and written to the crawl directory. On
/// resume, the engine reads it back, re-enqueues the pending URLs, and
/// repopulates the "seen" fingerprint set so that already-fetched pages are
/// not fetched again.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CheckpointData {
    /// URLs of requests still pending in the scheduler's queue at the time the
    /// checkpoint was taken. These are re-enqueued on resume.
    pub request_urls: Vec<String>,
    /// SHA-1 fingerprints of requests that have already been seen. Restoring
    /// these into the scheduler prevents re-fetching pages that were completed
    /// before the pause.
    pub seen_fingerprints: Vec<Vec<u8>>,
}

/// Handles saving and loading crawler checkpoints to disk.
///
/// You do not create this directly; the [`CrawlerEngine`](crate::spider::CrawlerEngine)
/// manages it internally. It writes to `<crawldir>/checkpoint.json` and
/// supports configurable save intervals so you can trade off between
/// checkpoint frequency and disk I/O overhead.
pub struct CheckpointManager {
    crawldir: PathBuf,
    checkpoint_path: PathBuf,
    /// The minimum interval in seconds between automatic checkpoint saves.
    pub interval_secs: f64,
}

impl CheckpointManager {
    /// Creates a new checkpoint manager writing to the given directory.
    ///
    /// `interval_secs` controls how often the engine auto-saves during the
    /// crawl loop. A value of 0.0 disables periodic saves (checkpoints are
    /// still taken on pause). Returns an error if `interval_secs` is negative.
    pub fn new(crawldir: impl Into<PathBuf>, interval_secs: f64) -> Result<Self> {
        if interval_secs < 0.0 {
            return Err(SpiderError::Checkpoint("interval must be >= 0".into()));
        }
        let crawldir = crawldir.into();
        let checkpoint_path = crawldir.join(CHECKPOINT_FILE);
        Ok(Self {
            crawldir,
            checkpoint_path,
            interval_secs,
        })
    }

    /// Returns `true` if a checkpoint file exists on disk. The engine checks
    /// this at startup to decide whether to resume from a previous run or start
    /// fresh.
    pub fn has_checkpoint(&self) -> bool {
        self.checkpoint_path.exists()
    }

    /// Atomically writes checkpoint data to disk using a temporary file and
    /// rename. The two-step write ensures that a crash during serialization
    /// cannot corrupt an existing checkpoint. The crawl directory is created
    /// automatically if it does not exist.
    pub fn save(&self, data: &CheckpointData) -> Result<()> {
        std::fs::create_dir_all(&self.crawldir)
            .map_err(|e| SpiderError::Checkpoint(format!("failed to create crawldir: {e}")))?;

        let temp_path = self.crawldir.join(".checkpoint.tmp");
        let json = serde_json::to_vec(data)
            .map_err(|e| SpiderError::Checkpoint(format!("serialization failed: {e}")))?;

        std::fs::write(&temp_path, &json)
            .map_err(|e| SpiderError::Checkpoint(format!("failed to write checkpoint: {e}")))?;

        std::fs::rename(&temp_path, &self.checkpoint_path).map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            SpiderError::Checkpoint(format!("failed to rename checkpoint: {e}"))
        })?;

        debug!("checkpoint saved");
        Ok(())
    }

    /// Loads checkpoint data from disk, returning `None` if no checkpoint
    /// exists. If the file exists but cannot be deserialized (e.g., it was
    /// corrupted), a warning is logged and `None` is returned rather than
    /// propagating an error, so the crawl can start fresh.
    pub fn load(&self) -> Result<Option<CheckpointData>> {
        if !self.has_checkpoint() {
            return Ok(None);
        }

        let data = std::fs::read(&self.checkpoint_path)
            .map_err(|e| SpiderError::Checkpoint(format!("failed to read checkpoint: {e}")))?;

        match serde_json::from_slice(&data) {
            Ok(cp) => {
                info!("checkpoint loaded");
                Ok(Some(cp))
            }
            Err(e) => {
                warn!(error = %e, "failed to deserialize checkpoint");
                Ok(None)
            }
        }
    }

    /// Deletes the checkpoint file from disk if it exists. The engine calls
    /// this after a crawl completes normally (not paused) to avoid accidentally
    /// resuming a finished crawl on the next run.
    pub fn cleanup(&self) -> Result<()> {
        if self.checkpoint_path.exists() {
            std::fs::remove_file(&self.checkpoint_path).map_err(|e| {
                SpiderError::Checkpoint(format!("failed to cleanup checkpoint: {e}"))
            })?;
            debug!("checkpoint cleaned up");
        }
        Ok(())
    }
}
