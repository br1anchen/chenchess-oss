use std::{
    io,
    path::{Path, PathBuf},
};

use tokio::io::AsyncRead;

pub type BoxedCommandInput = Box<dyn AsyncRead + Unpin + Send>;

pub struct CommandFifoGuard {
    path: Option<PathBuf>,
}

impl CommandFifoGuard {
    pub fn open(path: Option<PathBuf>) -> io::Result<(BoxedCommandInput, Self)> {
        let Some(path) = path else {
            return Ok((Box::new(tokio::io::stdin()), Self { path: None }));
        };
        open_fifo(&path)?;
        let guard = Self {
            path: Some(path.clone()),
        };
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        Ok((Box::new(tokio::fs::File::from_std(file)), guard))
    }
}

impl Drop for CommandFifoGuard {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(unix)]
fn open_fifo(path: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "FIFO path contains a NUL byte")
    })?;
    // SAFETY: `path` is a NUL-terminated CString from a filesystem path.
    // `mkfifo` only reads that pointer and the mode bits.
    if unsafe { libc::mkfifo(path.as_ptr(), 0o600) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn open_fifo(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Review Session command FIFOs require Unix",
    ))
}
