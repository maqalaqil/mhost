use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

/// Read all lines from `path`.
pub fn read_all(path: &Path) -> io::Result<Vec<String>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    reader.lines().collect()
}

/// Read the last `n` lines from `path`.
pub fn tail(path: &Path, n: usize) -> io::Result<Vec<String>> {
    let all = read_all(path)?;
    let skip = all.len().saturating_sub(n);
    Ok(all.into_iter().skip(skip).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_lines(path: &Path, lines: &[&str]) {
        let mut f = std::fs::File::create(path).expect("create");
        for line in lines {
            writeln!(f, "{}", line).expect("write");
        }
    }

    #[test]
    fn tail_last_n() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("test.log");
        write_lines(&p, &["line1", "line2", "line3", "line4", "line5"]);

        let result = tail(&p, 3).expect("tail");
        assert_eq!(result, vec!["line3", "line4", "line5"]);
    }

    #[test]
    fn tail_fewer_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("test.log");
        write_lines(&p, &["only", "two"]);

        let result = tail(&p, 10).expect("tail");
        assert_eq!(result, vec!["only", "two"]);
    }
}
