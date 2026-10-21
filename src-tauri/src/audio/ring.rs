//! A bounded SPSC sample ring: the cpal callback must not allocate or block.
//! Overflow drops the whole block and counts its samples; the writer pads silence.
//! Until the writer takes that count, every later block is dropped into it too,
//! so the silence always belongs right after the samples drained with it.

use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    Arc,
};

struct Shared {
    buf: Box<[AtomicU32]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    dropped: AtomicUsize,
    closed: AtomicBool,
}

/// The capture side, used as the [`Sink`](super::Sink) a source delivers to.
pub struct Producer {
    shared: Arc<Shared>,
    tail: usize,
}

/// The writer side.
pub struct Consumer {
    shared: Arc<Shared>,
    head: usize,
}

/// What one [`Consumer::drain`] found besides samples.
pub struct Drain {
    /// Samples dropped right after this drain's samples, to pad with silence.
    pub dropped: usize,
    /// This drain is complete: nothing more will ever arrive.
    pub closed: bool,
}

/// Allocates and initializes a ring, rounding capacity up to a power of two.
pub fn ring(capacity: usize) -> (Producer, Consumer) {
    let capacity = capacity.max(1).next_power_of_two();
    let shared = Arc::new(Shared {
        buf: (0..capacity).map(|_| AtomicU32::new(0)).collect(),
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
        dropped: AtomicUsize::new(0),
        closed: AtomicBool::new(false),
    });
    (
        Producer {
            shared: Arc::clone(&shared),
            tail: 0,
        },
        Consumer { shared, head: 0 },
    )
}

impl Producer {
    /// The most samples one block can ever hold.
    pub fn capacity(&self) -> usize {
        self.shared.buf.len()
    }

    /// Copies a whole block without allocating, locking or blocking; counts it
    /// as dropped if it cannot fit. A block larger than capacity can never fit.
    pub fn push(&mut self, samples: &[f32]) {
        if !self.try_push(samples) {
            self.shared
                .dropped
                .fetch_add(samples.len(), Ordering::Release);
        }
    }

    /// Like `push`, but returns false without counting a drop when full or
    /// while a drop awaits the writer, so fakes and tests can retry. Whole
    /// blocks keep channel frames aligned.
    pub fn try_push(&mut self, samples: &[f32]) -> bool {
        if self.shared.dropped.load(Ordering::Acquire) != 0 {
            return false;
        }
        let head = self.shared.head.load(Ordering::Acquire);
        let capacity = self.shared.buf.len();
        if samples.len() > capacity - self.tail.wrapping_sub(head) {
            return false;
        }
        for (i, sample) in samples.iter().enumerate() {
            self.shared.buf[self.tail.wrapping_add(i) & (capacity - 1)]
                .store(sample.to_bits(), Ordering::Relaxed);
        }
        self.tail = self.tail.wrapping_add(samples.len());
        self.shared.tail.store(self.tail, Ordering::Release);
        true
    }
}

impl Drop for Producer {
    fn drop(&mut self) {
        self.shared.closed.store(true, Ordering::Release);
    }
}

impl Consumer {
    /// Appends all currently buffered samples to `out`.
    pub fn drain(&mut self, out: &mut Vec<f32>) -> Drain {
        // Acquire closed first so a final drain also observes every preceding
        // drop and tail publication; acquire tail before reading its samples.
        // A drop seen before tail is at that tail, since it holds back every
        // later block; one made after it waits for the next drain.
        let closed = self.shared.closed.load(Ordering::Acquire);
        let pending = self.shared.dropped.load(Ordering::Acquire) != 0;
        let tail = self.shared.tail.load(Ordering::Acquire);
        let mask = self.shared.buf.len() - 1;
        for i in 0..tail.wrapping_sub(self.head) {
            out.push(f32::from_bits(
                self.shared.buf[self.head.wrapping_add(i) & mask].load(Ordering::Relaxed),
            ));
        }
        self.head = tail;
        self.shared.head.store(tail, Ordering::Release);
        // Taking the count lets the producer accept blocks again; the swap
        // also takes any block dropped into it since the load above.
        let dropped = if pending {
            self.shared.dropped.swap(0, Ordering::AcqRel)
        } else {
            0
        };
        Drain { dropped, closed }
    }
}

#[cfg(test)]
mod tests {
    use super::ring;

    #[test]
    fn round_trip_across_wrap_around() {
        let (mut producer, mut consumer) = ring(8);
        let mut out = Vec::new();
        for start in [0, 6] {
            let samples: Vec<f32> = (start..start + 6).map(|n| n as f32).collect();
            assert!(producer.try_push(&samples));
            let drained = consumer.drain(&mut out);
            assert_eq!(drained.dropped, 0);
            assert!(!drained.closed);
        }
        assert_eq!(out, (0..12).map(|n| n as f32).collect::<Vec<_>>());
    }

    #[test]
    fn overflow_drops_whole_blocks_and_counts_only_push() {
        let (mut producer, mut consumer) = ring(8);
        let mut out = Vec::new();
        assert!(producer.try_push(&[1.0; 6]));
        assert!(!producer.try_push(&[2.0; 3]));
        assert_eq!(consumer.drain(&mut out).dropped, 0);
        assert_eq!(out, vec![1.0; 6]);

        assert!(producer.try_push(&[3.0; 6]));
        producer.push(&[4.0; 3]);
        out.clear();
        assert_eq!(consumer.drain(&mut out).dropped, 3);
        assert_eq!(out, vec![3.0; 6]);
        assert_eq!(consumer.drain(&mut out).dropped, 0);

        producer.push(&[5.0; 9]);
        out.clear();
        assert_eq!(consumer.drain(&mut out).dropped, 9);
        assert!(out.is_empty());
        assert_eq!(consumer.drain(&mut out).dropped, 0);
    }

    #[test]
    fn a_block_after_a_drop_waits_until_the_drop_is_drained() {
        let (mut producer, mut consumer) = ring(8);
        let mut out = Vec::new();
        assert!(producer.try_push(&[1.0; 6]));
        producer.push(&[2.0; 4]);
        // It would fit, but it must not reach the writer ahead of the silence.
        assert!(!producer.try_push(&[3.0; 2]));
        producer.push(&[3.0; 2]);
        assert_eq!(consumer.drain(&mut out).dropped, 6);
        assert_eq!(out, vec![1.0; 6]);

        assert!(producer.try_push(&[4.0; 2]));
        out.clear();
        assert_eq!(consumer.drain(&mut out).dropped, 0);
        assert_eq!(out, vec![4.0; 2]);
    }

    #[test]
    fn closing_returns_the_final_data_and_drops() {
        let (mut producer, mut consumer) = ring(2);
        producer.push(&[1.0, 2.0]);
        producer.push(&[3.0]);
        drop(producer);
        let mut out = Vec::new();
        let drained = consumer.drain(&mut out);
        assert!(drained.closed);
        assert_eq!(drained.dropped, 1);
        assert_eq!(out, vec![1.0, 2.0]);
    }

    #[test]
    fn two_threads_preserve_a_numbered_sequence() {
        const N: usize = 100_000;
        let (mut producer, mut consumer) = ring(64);
        let writer = std::thread::spawn(move || {
            let mut start = 0;
            let mut size = 1;
            while start < N {
                let end = (start + size).min(N);
                let samples: Vec<f32> = (start..end).map(|n| n as f32).collect();
                while !producer.try_push(&samples) {
                    std::thread::yield_now();
                }
                start = end;
                size = size % 40 + 1;
            }
        });
        let mut out = Vec::new();
        loop {
            let drained = consumer.drain(&mut out);
            assert_eq!(drained.dropped, 0);
            if drained.closed {
                break;
            }
            std::thread::yield_now();
        }
        writer.join().unwrap();
        assert_eq!(out, (0..N).map(|n| n as f32).collect::<Vec<_>>());
    }

    #[test]
    fn two_threads_keep_drops_in_place_on_the_timeline() {
        const N: usize = 200_000;
        let (mut producer, mut consumer) = ring(64);
        let writer = std::thread::spawn(move || {
            // A block larger than the ring must overflow, independent of how
            // quickly the consumer gets scheduled.
            producer.push(&(0..65).map(|n| n as f32).collect::<Vec<_>>());
            let mut start = 65;
            let mut size = 1;
            while start < N {
                let end = (start + size).min(N);
                let samples: Vec<f32> = (start..end).map(|n| n as f32).collect();
                producer.push(&samples);
                start = end;
                size = size % 40 + 1;
            }
        });
        let mut out = Vec::new();
        let mut dropped = 0;
        loop {
            let drained = consumer.drain(&mut out);
            out.extend(std::iter::repeat(f32::NAN).take(drained.dropped));
            dropped += drained.dropped;
            if drained.closed {
                break;
            }
        }
        writer.join().unwrap();
        assert_eq!(out.len(), N);
        assert!(dropped > 0, "the test should overflow the ring");
        for (i, &sample) in out.iter().enumerate() {
            assert!(
                sample.is_nan() || sample == i as f32,
                "sample {i} is {sample}"
            );
        }
    }
}
