use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

/// Log file writer with size-based rotation.
pub struct LogWriter {
    path: PathBuf,
    writer: BufWriter<File>,
    current_size: u64,
    max_size: u64,
    max_files: u32,
}

/// Build the rotated path for rotation index `n`.
/// e.g. "app.log" -> "app.log.1", "app.log.2", etc.
pub fn rotated_path(base: &Path, n: u32) -> PathBuf {
    let mut s = base.as_os_str().to_os_string();
    s.push(format!(".{}", n));
    PathBuf::from(s)
}

impl LogWriter {
    /// Open (or create) the log file at `path` in append mode.
    pub fn new(path: impl Into<PathBuf>, max_size_bytes: u64, max_files: u32) -> io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let current_size = file.metadata()?.len();
        Ok(Self {
            path,
            writer: BufWriter::new(file),
            current_size,
            max_size: max_size_bytes,
            max_files,
        })
    }

    /// Write a line followed by a newline, rotating if the file exceeds `max_size`.
    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        let bytes = line.len() as u64 + 1; // +1 for newline
        writeln!(self.writer, "{}", line)?;
        self.writer.flush()?;
        self.current_size += bytes;
        if self.current_size >= self.max_size {
            self.rotate()?;
        }
        Ok(())
    }

    /// Rotate log files: shift .1->.2 ... up to max_files, rename current to .1, open fresh.
    pub fn rotate(&mut self) -> io::Result<()> {
        // Flush and drop current writer before renaming.
        self.writer.flush()?;

        // Delete the oldest rotated file if it would exceed max_files.
        let oldest = rotated_path(&self.path, self.max_files);
        if oldest.exists() {
            fs::remove_file(&oldest)?;
        }

        // Shift existing rotated files: .N-1 -> .N (highest first to avoid collisions).
        for n in (1..self.max_files).rev() {
            let src = rotated_path(&self.path, n);
            let dst = rotated_path(&self.path, n + 1);
            if src.exists() {
                fs::rename(&src, &dst)?;
            }
        }

        // Rename current log to .1.
        if self.path.exists() {
            fs::rename(&self.path, rotated_path(&self.path, 1))?;
        }

        // Open a fresh log file.
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)?;
        self.writer = BufWriter::new(file);
        self.current_size = 0;
        Ok(())
    }

    /// Path of the active log file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Current size of the active log file in bytes.
    pub fn current_size(&self) -> u64 {
        self.current_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn write_and_rotate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log_path = dir.path().join("test.log");

        // max_size small enough that writing a few lines triggers rotation.
        let mut writer = LogWriter::new(&log_path, 20, 3).expect("new writer");
        writer.write_line("hello world").expect("write 1"); // 12 bytes
        writer.write_line("second line").expect("write 2"); // triggers rotation (>=20)

        // After rotation a fresh file should exist plus a .1 backup.
        assert!(log_path.exists(), "active log should exist");
        let rotated = rotated_path(&log_path, 1);
        assert!(rotated.exists(), "rotated .1 should exist");
    }

    #[test]
    fn rotated_path_helper() {
        let base = Path::new("app.log");
        assert_eq!(rotated_path(base, 1), PathBuf::from("app.log.1"));
        assert_eq!(rotated_path(base, 3), PathBuf::from("app.log.3"));
    }
}
