extern crate fs2;
#[macro_use]
extern crate log;

use std::fs::{self, File, OpenOptions};
use std::io;
use std::mem::{self, ManuallyDrop};
use std::path::{Path, PathBuf};

use fs2::{lock_contended_error, FileExt};

/// A lockfile that cleans up after itself.
///
/// Inspired by `TempPath` in `tempfile` crate.
pub struct Lockfile {
    handle: ManuallyDrop<File>,
    path: PathBuf,
}

impl Lockfile {
    /// Create a lockfile at the given path.
    ///
    /// This function will
    ///  1. create parent directories, if necessary,
    ///  2. create the lockfile, if necessary,
    ///  3. acquire an exclusive lock the lockfile.
    ///
    ///  - If the directories or lockfile already exist, it will skip creating them.
    ///  - If the lockfile is already locked it will block waiting for release.
    ///  - Any other error is returned.
    ///
    /// # Panics
    ///
    /// Will panic if the path doesn't have a parent directory.
    ///
    /// # Examples
    ///
    /// ```no-run
    /// # use lockfile::Lockfile;
    /// # use std::{mem, fs, io};
    /// # use std::path::Path;
    ///
    /// const PATH: &str = "/tmp/some_file/s8329894";
    /// # fn main() -> Result<(), io::Error> {
    /// let lockfile = Lockfile::create(PATH)?;
    /// drop(lockfile);
    /// // File has been unlinked/deleted.
    /// assert!(fs::metadata(PATH).is_err());
    /// # Ok(())
    /// # }
    /// ```
    pub fn create(path: impl AsRef<Path>) -> Result<Lockfile, io::Error> {
        let path = path.as_ref();

        // create parent directory if not exists (match libalpm behaviour)
        let dir = path.parent().expect("lockfile path must have a parent");
        fs::create_dir_all(dir)?;
        debug!(
            "lockfile parent directories created/found at {}",
            dir.display()
        );

        // create lockfile (or get a handle if file already exists)
        let mut lockfile_opts = OpenOptions::new();
        lockfile_opts.create(true).read(true).write(true);
        let lockfile = lockfile_opts.open(path)?;
        debug!("lockfile created/found at {}", path.display());

        // lock lockfile
        match lockfile.try_lock_exclusive() {
            Ok(_) => (),
            Err(ref e) if e.kind() == lock_contended_error().kind() => {
                warn!(
                    "Lockfile at {} already present and locked, blocking until released",
                    path.display()
                );
                lockfile.lock_exclusive()?;
            }
            Err(e) => Err(e)?,
        };
        debug!("lockfile locked at {}", path.display());

        Ok(Lockfile {
            handle: ManuallyDrop::new(lockfile),
            path: path.to_owned(),
        })
    }

    /// Get the path of the lockfile
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Close the file.
    ///
    /// Use this instead of the destructor when you want to see if any errors occured when
    /// removing the file.
    pub fn close(mut self) -> Result<(), io::Error> {
        // unpack self without running drop (todo please let me not use unsafe :D)
        let (handle, path) = unsafe {
            let handle = mem::replace(&mut self.handle, mem::zeroed());
            let path = mem::replace(&mut self.path, mem::zeroed());
            mem::forget(self);
            (handle, path)
        };
        let handle = ManuallyDrop::into_inner(handle);

        // unlock file
        if let Err(e) = handle.unlock() {
            error!("error releasing lockfile at {}: {}", path.display(), e);
        } else {
            debug!("lockfile unlocked at {}", path.display());
        }
        // close file
        drop(handle);

        // remove file
        fs::remove_file(&path)?;
        debug!("Removed lockfile at {}", path.display());
        Ok(())
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        // we cannot return errors, but we can report them to logs
        if let Err(e) = self.handle.unlock() {
            error!("error releasing lockfile at {}: {}", self.path.display(), e);
        } else {
            debug!("lockfile unlocked at {}", self.path.display());
        }
        // Safe because we don't use handle after dropping it.
        unsafe {
            // close file
            ManuallyDrop::drop(&mut self.handle);
            // remove file
            if let Err(e) = fs::remove_file(&self.path) {
                warn!(
                    "could not remove lockfile at {}: {}",
                    self.path.display(),
                    e
                );
            }
            // path destructor will be run as usual.
            debug!("Removed lockfile at {}", self.path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate tempfile;

    use self::tempfile::NamedTempFile;
    use super::Lockfile;

    use std::fs;
    use std::io;

    #[test]
    fn smoke() {
        // create and delete a temp file to get a tmp location.
        let path = NamedTempFile::new().unwrap().into_temp_path().to_owned();
        let lockfile = Lockfile::create(&path).unwrap();
        assert_eq!(lockfile.path(), path);
        drop(lockfile);
        assert_eq!(fs::metadata(path).unwrap_err().kind(), io::ErrorKind::NotFound);
    }
}
