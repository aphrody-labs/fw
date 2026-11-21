//! Live reload: a debounced file watcher over settings, theme directories and templates.

use crate::error::{Error, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

pub struct Watch {
    _watcher: RecommendedWatcher,
    events: Receiver<notify::Result<Event>>,
    debounce: Duration,
}

/// The nearest existing path to watch for `path`: itself, or the parent directory of a file that does not exist yet.
fn watchable(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|candidate| candidate.exists()).map(Path::to_owned)
}

impl Watch {
    pub fn new(paths: &[PathBuf], debounce: Duration) -> Result<Self> {
        let (sender, events) = channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            // A closed receiver means the watch was dropped; nothing is left to notify.
            sender.send(event).ok();
        })?;
        for path in paths.iter().filter_map(|path| watchable(path)) {
            watcher.watch(&path, RecursiveMode::Recursive)?;
        }
        Ok(Self { _watcher: watcher, events, debounce })
    }

    /// Blocks until a change arrives (or `timeout` passes) and the files stay quiet for the debounce interval.
    pub fn wait(&self, timeout: Option<Duration>) -> Result<bool> {
        let stopped = || Error::Invalid("the file watcher stopped".to_owned());
        let first = match timeout {
            Some(limit) => match self.events.recv_timeout(limit) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => return Ok(false),
                Err(RecvTimeoutError::Disconnected) => return Err(stopped()),
            },
            None => self.events.recv().map_err(|_| stopped())?,
        };
        let mut changed = relevant(&first?);
        loop {
            match self.events.recv_timeout(self.debounce) {
                Ok(event) => changed |= relevant(&event?),
                Err(RecvTimeoutError::Timeout) => return Ok(changed),
                Err(RecvTimeoutError::Disconnected) => return Err(stopped()),
            }
        }
    }
}

fn relevant(event: &Event) -> bool {
    !matches!(event.kind, EventKind::Access(_))
}

/// Calls `on_change` after every settled change; an error of `on_change` goes to `on_error` and the loop continues, so a
/// half-written theme file does not end the watch.
pub fn run(paths: &[PathBuf], debounce: Duration, mut on_change: impl FnMut() -> Result<()>, mut on_error: impl FnMut(&Error)) -> Result<()> {
    let watch = Watch::new(paths, debounce)?;
    loop {
        if watch.wait(None)?
            && let Err(error) = on_change()
        {
            on_error(&error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_is_reported_once_it_settles() -> Result<()> {
        let dir = tempfile::tempdir().map_err(|source| Error::io("tempdir", source))?;
        let watch = Watch::new(&[dir.path().to_owned()], Duration::from_millis(100))?;
        std::fs::write(dir.path().join("theme.toml"), "name = \"x\"\n").map_err(|source| Error::io("theme.toml", source))?;
        assert!(watch.wait(Some(Duration::from_secs(5)))?);
        assert!(!watch.wait(Some(Duration::from_millis(300)))?);
        Ok(())
    }
}
