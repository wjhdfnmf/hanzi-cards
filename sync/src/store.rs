//! The server's shelf: the latest box, numbered, and the few before it.
//!
//! A box is only ever replaced by someone who has seen the one it replaces (they say which
//! number they are replacing), so two devices syncing at once can't overwrite each other:
//! the second is told to fetch and merge again. Each box is written to a new file, forced to
//! disk and then renamed into place, so a crash or power cut leaves the old box or the new
//! one, never half of one.

use crate::Error;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Earlier boxes kept beside the current one.
const KEEP: usize = 5;

pub struct BoxStore {
    dir: PathBuf,
    lock: Mutex<()>,
}

fn generation_of(path: &Path) -> Option<u64> {
    path.file_name()?.to_str()?.strip_prefix("box-")?.strip_suffix(".bin")?.parse().ok()
}

impl BoxStore {
    pub fn open(dir: &Path) -> Result<BoxStore, Error> {
        std::fs::create_dir_all(dir)?;
        Ok(BoxStore { dir: dir.to_path_buf(), lock: Mutex::new(()) })
    }

    fn boxes(&self) -> Result<Vec<(u64, PathBuf)>, Error> {
        let mut found: Vec<(u64, PathBuf)> =
            std::fs::read_dir(&self.dir)?.filter_map(|e| e.ok()).map(|e| e.path()).filter_map(|p| generation_of(&p).map(|g| (g, p))).collect();
        found.sort();
        Ok(found)
    }

    /// The number of the current box (0: there is none yet) and its size.
    pub fn current(&self) -> Result<(u64, u64), Error> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        Ok(match self.boxes()?.last() {
            Some((g, p)) => (*g, std::fs::metadata(p)?.len()),
            None => (0, 0),
        })
    }

    pub fn get(&self) -> Result<(u64, Vec<u8>), Error> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        Ok(match self.boxes()?.last() {
            Some((g, p)) => (*g, std::fs::read(p)?),
            None => (0, Vec::new()),
        })
    }

    /// Put a new box in place of the one numbered `expected`. `Conflict` (with the current
    /// number) if that isn't the current one any more.
    pub fn put(&self, expected: u64, boxed: &[u8]) -> Result<u64, Error> {
        if boxed.len() > crate::MAX_BOX {
            return Err(Error::TooLarge);
        }
        if !crate::seal::looks_sealed(boxed) {
            return Err(Error::Corrupt("not a sealed box".into()));
        }
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let boxes = self.boxes()?;
        let current = boxes.last().map(|(g, _)| *g).unwrap_or(0);
        if current != expected {
            return Err(Error::Conflict(current));
        }
        let next = current + 1;
        let tmp = self.dir.join(format!("incoming-{next}.tmp"));
        let write = || -> std::io::Result<()> {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(boxed)?;
            f.sync_all()?;
            std::fs::rename(&tmp, self.dir.join(format!("box-{next:010}.bin")))?;
            // (the rename itself on disk too, where a folder can be synced)
            #[cfg(unix)]
            std::fs::File::open(&self.dir)?.sync_all()?;
            Ok(())
        };
        if let Err(e) = write() {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }
        for (_, old) in boxes.iter().rev().skip(KEEP) {
            let _ = std::fs::remove_file(old);
        }
        Ok(next)
    }
}

#[cfg(test)]
pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hanzi-sync-test-{name}-{}-{}", std::process::id(), crate::hex(&crate::random::<4>())));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_are_numbered_and_only_replaced_by_someone_who_saw_them() {
        let dir = temp_dir("store");
        let key = crate::random::<32>();
        let boxed = |text: &str| crate::seal::seal(&key, text.as_bytes()).unwrap();
        let store = BoxStore::open(&dir).unwrap();
        assert_eq!(store.get().unwrap(), (0, Vec::new()));
        let first = boxed("one");
        assert_eq!(store.put(0, &first).unwrap(), 1);
        assert_eq!(store.get().unwrap(), (1, first.clone()));
        // someone who hasn't seen box 1 can't replace it
        assert!(matches!(store.put(0, &boxed("late")), Err(Error::Conflict(1))));
        assert_eq!(store.get().unwrap().1, first);
        // nor can something that isn't a box
        assert!(matches!(store.put(1, b"rubbish"), Err(Error::Corrupt(_))));
        for n in 1..10 {
            assert_eq!(store.put(n, &boxed("more")).unwrap(), n + 1);
        }
        // opened again (the server restarted): the same box, and only a few old ones kept
        let again = BoxStore::open(&dir).unwrap();
        assert_eq!(again.current().unwrap().0, 10);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), KEEP + 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
