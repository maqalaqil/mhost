use std::collections::VecDeque;

/// Fixed-capacity ring buffer that evicts the oldest entry when full.
pub struct RingBuffer {
    lines: VecDeque<String>,
    capacity: usize,
}

impl RingBuffer {
    /// Create a new ring buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a line into the buffer, evicting the oldest if full.
    pub fn push(&mut self, line: String) {
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Return all buffered lines in insertion order.
    pub fn lines(&self) -> Vec<&str> {
        self.lines.iter().map(String::as_str).collect()
    }

    /// Return the last `n` lines (or fewer if not enough are buffered).
    pub fn last_n(&self, n: usize) -> Vec<&str> {
        let skip = self.lines.len().saturating_sub(n);
        self.lines.iter().skip(skip).map(String::as_str).collect()
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Number of lines currently buffered.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_read() {
        let mut buf = RingBuffer::new(3);
        buf.push("a".to_string());
        buf.push("b".to_string());
        buf.push("c".to_string());
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.lines(), vec!["a", "b", "c"]);
    }

    #[test]
    fn overflow() {
        let mut buf = RingBuffer::new(3);
        buf.push("a".to_string());
        buf.push("b".to_string());
        buf.push("c".to_string());
        buf.push("d".to_string()); // evicts "a"
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.lines(), vec!["b", "c", "d"]);
    }

    #[test]
    fn last_n_more_than_available() {
        let mut buf = RingBuffer::new(5);
        buf.push("x".to_string());
        buf.push("y".to_string());
        let result = buf.last_n(10);
        assert_eq!(result, vec!["x", "y"]);
    }

    #[test]
    fn clear() {
        let mut buf = RingBuffer::new(4);
        buf.push("a".to_string());
        buf.push("b".to_string());
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }
}
