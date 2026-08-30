use std::{
    collections::VecDeque,
    io::Read,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

/// Default maximum retained stderr tail per runner process (64 KiB).
pub const DEFAULT_STDERR_TAIL_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub(crate) struct BoundedTail {
    bytes: VecDeque<u8>,
    capacity: usize,
}

impl BoundedTail {
    fn new(capacity: usize) -> Self {
        Self {
            bytes: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn extend(&mut self, incoming: &[u8]) {
        if self.capacity == 0 {
            return;
        }
        for byte in incoming {
            if self.bytes.len() == self.capacity {
                self.bytes.pop_front();
            }
            self.bytes.push_back(*byte);
        }
    }

    pub(crate) fn text(&self) -> String {
        let bytes: Vec<_> = self.bytes.iter().copied().collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

pub(crate) fn capture_stderr(
    mut stderr: impl Read + Send + 'static,
    capacity: usize,
) -> (Arc<Mutex<BoundedTail>>, JoinHandle<()>) {
    let tail = Arc::new(Mutex::new(BoundedTail::new(capacity)));
    let thread_tail = Arc::clone(&tail);
    let handle = thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if let Ok(mut guard) = thread_tail.lock() {
                        guard.extend(&buffer[..count]);
                    }
                }
            }
        }
    });
    (tail, handle)
}
