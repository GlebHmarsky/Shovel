//! Disposable presentation cache; it cannot create a live History or action context.
use crate::{
    git::{Entry, PAGE_SIZE, Snapshot},
    graph::{MAX_DISPLAY_LANES, MAX_LANES, StashLink},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_BYTES: usize = 4 * 1024 * 1024;
// Older previews encode a stash node as an inset from its base lane.
pub const PREVIEW_SCHEMA: u32 = 8;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preview {
    pub schema: u32,
    pub directory: PathBuf,
    pub identity: String,
    pub repository: Snapshot,
    pub entries: Vec<Entry>,
    pub offset: usize,
    pub selected: Option<String>,
    pub vertical: f32,
    pub horizontal: f32,
    pub saved_at: DateTime<Utc>,
}
impl Preview {
    pub fn valid(&self, path: &Path, identity: &str) -> bool {
        self.schema == PREVIEW_SCHEMA
            && self.identity == identity
            && self.repository.identity.fingerprint == identity
            && same_path(&self.directory, path)
            && {
                let matches = |selected: &Path| {
                    same_path(&self.repository.worktree, selected)
                        && same_path(&self.repository.identity.root, selected)
                };
                matches(path)
                    || crate::identity::read(path).is_ok_and(|live| {
                        live.matches_recorded_fingerprint(identity) && matches(&live.root)
                    })
            }
            && self.entries.len() <= PAGE_SIZE
            && self.offset <= i64::MAX as usize
            && self.offset.checked_add(self.entries.len()).is_some()
            && [self.vertical, self.horizontal]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.)
            && self.entries.iter().all(|entry| {
                let g = &entry.graph;
                (g.is_stash() || g.node < MAX_LANES)
                    && g.node < g.width
                    && g.width <= MAX_DISPLAY_LANES
                    && match g.stash_link {
                        Some(StashLink::Base { lane, .. }) => g.node < lane && lane < g.width,
                        _ => true,
                    }
                    && g.lines.len() <= 256
                    && g.lines.iter().all(|line| {
                        line.from < g.width.min(MAX_LANES)
                            && line.to < g.width.min(MAX_LANES)
                            && [line.from_y, line.to_y]
                                .iter()
                                .all(|y| y.is_finite() && (0. ..=1.).contains(y))
                    })
            })
    }
}
fn same_path(a: &Path, b: &Path) -> bool {
    crate::case::path_key(a) == crate::case::path_key(b)
}
#[derive(Clone)]
pub struct Store {
    pub directory: PathBuf,
}
impl Store {
    pub fn path(&self) -> PathBuf {
        self.directory.join("history-view.json")
    }
    pub fn read(&self, path: &Path, identity: &str) -> Option<Preview> {
        let file = File::open(self.path()).ok()?;
        if file.metadata().ok()?.len() > MAX_BYTES as u64 {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > MAX_BYTES {
            return None;
        }
        let preview: Preview = serde_json::from_slice(&bytes).ok()?;
        preview.valid(path, identity).then_some(preview)
    }
    /// Best effort and rebuildable. An invalid/absent/oversized cache removes only
    /// this profile's cache file. Temporary publication failures keep the prior file.
    pub fn save(&self, preview: Option<&Preview>) {
        let Some(preview) = preview.filter(|p| p.valid(&p.directory, &p.identity)) else {
            let _ = fs::remove_file(self.path());
            return;
        };
        let mut bytes = Bounded(Vec::new());
        if serde_json::to_writer(&mut bytes, preview).is_err() {
            let _ = fs::remove_file(self.path());
            return;
        }
        let _ = (|| -> std::io::Result<()> {
            fs::create_dir_all(&self.directory)?;
            let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
            file.write_all(&bytes.0)?;
            file.flush()?;
            // Close the handle before Windows atomic rename, as in the settings store.
            file.into_temp_path()
                .persist(self.path())
                .map_err(|e| e.error)?;
            Ok(())
        })();
    }
}
struct Bounded(Vec<u8>);
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_BYTES {
            return Err(std::io::Error::other("cache size limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
