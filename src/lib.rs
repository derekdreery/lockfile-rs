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
//! let lockfile = Lockfile::create(PATH).unwrap();
//! assert_eq!(lockfile.path(), Path::new(PATH));
//! lockfile.release()?; // or just let the lockfile be dropped
//! // File has been unlinked/deleted.
//! assert_eq!(fs::metadata(PATH).unwrap_err().kind(),
//!            io::ErrorKind::NotFound);
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]

#[macro_use]
extern crate log;

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// A wrapper around io::Error to distinguish between the lock already existing and other errors.
///
/// Without this, it would not be possible to tell if the `io::ErrorKind::AlreadyExists` came from
/// the lockfile or from the parent directory (if it already exists and is a file).
///
/// The error is non-exhaustive, in case we want to give more granular errors in future.
///
/// # Examples
///
/// ```
/// use lockfile::Error;
/// # let err = Error::LockTaken;
/// // `err` is the value returned by Lockfile::open
/// match err {
///     Error::LockTaken => println!("lock exists, maybe we block in some way"),
///     // Use err.into_inner to handle non exhaustiveness
///     err => panic!("unrecoverable error: {}", err.into_inner()),
/// }
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Io(io::Error),
    LockTaken,
}

impl Error {
    /// Get at the underlying `io::Error`.
    pub fn into_inner(self) -> io::Error {
        match self {
            Error::Io(err) => err,
            Error::LockTaken => io::Error::new(io::ErrorKind::AlreadyExists, "lock already taken"),
        }
    }

    fn from_io(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::AlreadyExists => Error::LockTaken,
            _ => Error::Io(e),
        }
    }
}

/// A lockfile that cleans up after itself.
///
/// Inspired by `TempPath` in `tempfile` crate.
///
/// See module-level documentation for examples.
#[derive(Debug)]
pub struct Lockfile {
    handle: Option<File>,
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
    pub fn create(path: impl AsRef<Path>) -> Result<Lockfile, Error> {
        let path = path.as_ref();

        // create parent directory if not exists (match libalpm behaviour)
        let dir = path.parent().expect("lockfile path must have a parent");
        fs::create_dir_all(dir).map_err(Error::Io)?;
        debug!(
            r#"lockfile parent directories created/found at "{}""#,
            dir.display()
        );

        // create lockfile (or get a handle if file already exists)
        let mut lockfile_opts = OpenOptions::new();
        lockfile_opts.create_new(true).read(true).write(true);
        let lockfile = lockfile_opts.open(path).map_err(Error::from_io)?;
        debug!(r#"lockfile created at "{}""#, path.display());

        Ok(Lockfile {
            handle: Some(lockfile),
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
        // Closes the file.
        self.handle.take().expect("handle already dropped");
        fs::remove_file(&self.path)?;
        debug!(r#"Removed lockfile at "{}""#, self.path.display());
        Ok(())
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            drop(handle);

            match fs::remove_file(&self.path) {
                Ok(()) => debug!(r#"Removed lockfile at "{}""#, self.path.display()),
                Err(e) => warn!(
                    r#"could not remove lockfile at "{}": {}"#,
                    self.path.display(),
                    e
                ),
            }
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
        io::Read::read(&mut self.handle.as_ref().unwrap(), buf)
    }
}

impl<'a> io::Read for &'a Lockfile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        io::Read::read(&mut self.handle.as_ref().unwrap(), buf)
    }
}

impl io::Write for Lockfile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut self.handle.as_ref().unwrap(), buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut self.handle.as_ref().unwrap())
    }
}

impl<'a> io::Write for &'a Lockfile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        io::Write::write(&mut self.handle.as_ref().unwrap(), buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut self.handle.as_ref().unwrap())
    }
}

impl io::Seek for Lockfile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        io::Seek::seek(&mut self.handle.as_ref().unwrap(), pos)
    }
}

impl<'a> io::Seek for &'a Lockfile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        io::Seek::seek(&mut self.handle.as_ref().unwrap(), pos)
    }
}

#[cfg(test)]
mod tests {
    extern crate tempfile;

    use self::tempfile::NamedTempFile;
    use super::{Error, Lockfile};

    use std::fs;
    use std::io;
    use std::path::PathBuf;

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
        assert_eq!(
            fs::metadata(path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn lock_twice() {
        // check trying to lock twice is an error
        let path = tmp_path();
        let _lockfile = Lockfile::create(&path).unwrap();
        assert!(matches!(
            Lockfile::create(&path).unwrap_err(),
            Error::LockTaken
        ));
    }
}
