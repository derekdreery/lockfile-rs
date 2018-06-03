//! This crate provides a lockfile struct that marks a location in the filesystem as locked.
//!
//! A lock is conceptually created when the file is created, and released when it is deleted.
//!
//! If the file is already present, the `create` function will fail.
//!
//! # Examples
//!
//! ```rust,no_run
//! use lockfile::Lockfile;
//! # use std::{mem, fs, io};
//! # use std::path::Path;
//!
//! const PATH: &str = "/tmp/some_file/s8329894";
//! # fn main() -> Result<(), io::Error> {
//! let lockfile = Lockfile::create(PATH)?;
//! assert_eq!(lockfile.path(), Path::new(PATH));
//! lockfile.release()?; // or just let the lockfile be dropped
//! // File has been unlinked/deleted.
//! assert_eq!(fs::metadata(PATH).unwrap_err().kind(),
//!            io::ErrorKind::NotFound);
//! # Ok(())
//! # }
//! ```
#[macro_use]
extern crate log;

use std::fs::{self, File, OpenOptions};
use std::io;
use std::mem::{self, ManuallyDrop};
use std::path::{Path, PathBuf};
use std::ops::Deref;

/// A lockfile that cleans up after itself.
///
/// Inspired by `TempPath` in `tempfile` crate.
///
/// See module-level documentation for examples.
#[derive(Debug)]
pub struct Lockfile {
    handle: ManuallyDrop<File>,
    path: PathBuf,
}

impl Lockfile {
    /// Create a lockfile at the given path.
    ///
    /// This function will
    ///  1. create parent directories, if necessary,
    ///  2. create the lockfile.
    ///
    ///  - If the directories already exist, it will skip creating them.
    ///  - Any other error is returned.
    ///
    /// # Panics
    ///
    /// Will panic if the path doesn't have a parent directory.
    pub fn create(path: impl AsRef<Path>) -> Result<Lockfile, io::Error> {
        let path = path.as_ref();

        // create parent directory if not exists (match libalpm behaviour)
        let dir = path.parent().expect("lockfile path must have a parent");
        fs::create_dir_all(dir)?;
        debug!(
            r#"lockfile parent directories created/found at "{}""#,
            dir.display()
        );

        // create lockfile (or get a handle if file already exists)
        let mut lockfile_opts = OpenOptions::new();
        lockfile_opts.create_new(true).read(true).write(true);
        let lockfile = lockfile_opts.open(path)?;
        debug!(r#"lockfile created at "{}""#, path.display());

        Ok(Lockfile {
            handle: ManuallyDrop::new(lockfile),
            path: path.to_owned(),
        })
    }

    /// Get the path of the lockfile.
    ///
    /// The impl of `AsRef<Path>` can also be used.
    #[inline]
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Close and remove the file, releasing the lock.
    ///
    /// Use this instead of the destructor when you want to see if any errors occured when
    /// removing the file.
    pub fn release(mut self) -> Result<(), io::Error> {
        // unpack self without running drop (todo please let me not use unsafe :D)
        let (handle, path) = unsafe {
            let handle = mem::replace(&mut self.handle, mem::zeroed());
            let path = mem::replace(&mut self.path, mem::zeroed());
            mem::forget(self);
            (handle, path)
        };
        let handle = ManuallyDrop::into_inner(handle);

        // close file
        drop(handle);

        // remove file
        fs::remove_file(&path)?;
        debug!(r#"Removed lockfile at "{}""#, path.display());
        Ok(())
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        // Safe because we don't use handle after dropping it.
        unsafe {
            // close file (this must happen before removal on windows)
            ManuallyDrop::drop(&mut self.handle);
            // remove file
            if let Err(e) = fs::remove_file(&self.path) {
                warn!(
                    r#"could not remove lockfile at "{}": {}"#,
                    self.path.display(),
                    e
                );
            }
            // path destructor will be run as usual.
            debug!(r#"Removed lockfile at "{}""#, self.path.display());
        }
    }
}

impl AsRef<Path> for Lockfile {
    #[inline]
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}

impl io::Read for Lockfile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut self.handle.deref(), buf)
    }
}

impl<'a> io::Read for &'a Lockfile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut self.handle.deref(), buf)
    }
}

impl io::Write for Lockfile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut self.handle.deref(), buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut self.handle.deref())
    }
}

impl<'a> io::Write for &'a Lockfile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut self.handle.deref(), buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut self.handle.deref())
    }
}

impl io::Seek for Lockfile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        io::Seek::seek(&mut self.handle.deref(), pos)
    }
}

impl<'a> io::Seek for &'a Lockfile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        io::Seek::seek(&mut self.handle.deref(), pos)
    }
}

#[cfg(test)]
mod tests {
    extern crate tempfile;

    use self::tempfile::NamedTempFile;
    use super::Lockfile;

    use std::path::PathBuf;
    use std::fs;
    use std::io;

    /// create and delete a temp file to get a tmp location.
    fn tmp_path() -> PathBuf {
        NamedTempFile::new().unwrap().into_temp_path().to_owned()
    }

    #[test]
    fn smoke() {
        let path = tmp_path();
        let lockfile = Lockfile::create(&path).unwrap();
        assert_eq!(lockfile.path(), path);
        lockfile.release().unwrap();
        assert_eq!(fs::metadata(path).unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn lock_twice() {
        // check trying to lock twice is an error
        let path = tmp_path();
        let _lockfile = Lockfile::create(&path).unwrap();
        assert_eq!(Lockfile::create(&path).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    }
}
