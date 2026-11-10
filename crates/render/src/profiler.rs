//! GPU time per graph node, measured with timestamp queries.
//!
//! Each node's passes get a start and an end timestamp. At the end of a frame
//! the timestamps are copied to a read-back buffer; a later frame reads them
//! when the GPU is done (never waiting for it). Only works when the device
//! was created with `Features::TIMESTAMP_QUERY`; otherwise there is no
//! profiler and no numbers.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Nodes timed per frame (more are drawn, just not timed).
pub const MAX_TIMED: u32 = 128;
/// Read-back buffers in flight (frames the GPU may lag behind).
const READBACKS: usize = 3;
const BYTES: u64 = 8 * 2 * MAX_TIMED as u64;

struct Pending {
    buf: wgpu::Buffer,
    keys: Vec<u64>,
    ready: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
}

pub struct Profiler {
    pub(crate) set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    /// Nanoseconds per timestamp tick.
    period_ns: f32,
    /// Keys timed this frame, in slot order.
    frame_keys: Vec<u64>,
    spare: Vec<wgpu::Buffer>,
    pending: Vec<Pending>,
    /// Smoothed milliseconds per node key.
    times: HashMap<u64, f32>,
    total: f32,
}

impl Profiler {
    /// A profiler, if the device can do timestamp queries.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("node timings"),
            ty: wgpu::QueryType::Timestamp,
            count: 2 * MAX_TIMED,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("node timings resolve"),
            size: BYTES,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let spare = (0..READBACKS)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("node timings readback"),
                    size: BYTES,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        Some(Self {
            set,
            resolve,
            period_ns: queue.get_timestamp_period(),
            frame_keys: Vec::new(),
            spare,
            pending: Vec::new(),
            times: HashMap::new(),
            total: 0.0,
        })
    }

    /// A slot for timing `key` this frame (None = not timed this frame).
    pub fn slot(&mut self, key: u64) -> Option<u32> {
        if self.spare.is_empty() || self.frame_keys.len() as u32 >= MAX_TIMED {
            return None;
        }
        self.frame_keys.push(key);
        Some(self.frame_keys.len() as u32 - 1)
    }

    /// Read finished timings (non-blocking). Call at the start of a frame.
    pub fn collect(&mut self, device: &wgpu::Device) {
        if self.pending.is_empty() {
            return;
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let mut i = 0;
        while i < self.pending.len() {
            let p = &self.pending[i];
            if p.failed.load(Ordering::Acquire) {
                let p = self.pending.remove(i);
                self.spare.push(p.buf);
                continue;
            }
            if !p.ready.load(Ordering::Acquire) {
                i += 1;
                continue;
            }
            let p = self.pending.remove(i);
            let n = p.keys.len() as u64;
            let mut fresh = HashMap::with_capacity(p.keys.len());
            if let Ok(view) = p.buf.get_mapped_range(..16 * n) {
                for (k, key) in p.keys.iter().enumerate() {
                    let at = |j: usize| {
                        let b = &view[j * 8..j * 8 + 8];
                        u64::from_le_bytes(b.try_into().unwrap_or([0; 8]))
                    };
                    let (start, end) = (at(2 * k), at(2 * k + 1));
                    let ms = end.saturating_sub(start) as f32 * self.period_ns / 1.0e6;
                    // A bogus reading (counter reset) is ignored.
                    if ms.is_finite() && ms < 1000.0 {
                        *fresh.entry(*key).or_insert(0.0) += ms;
                    }
                }
            }
            p.buf.unmap();
            self.spare.push(p.buf);
            // Smooth so the numbers are readable, and forget nodes no longer drawn.
            let old = std::mem::take(&mut self.times);
            for (key, ms) in fresh {
                let v = match old.get(&key) {
                    Some(o) => o + 0.15 * (ms - o),
                    None => ms,
                };
                self.times.insert(key, v);
            }
            self.total = self.times.values().sum();
        }
    }

    /// Copy this frame's timestamps out for reading later.
    pub fn finish(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.frame_keys.is_empty() {
            return;
        }
        let Some(buf) = self.spare.pop() else {
            self.frame_keys.clear();
            return;
        };
        let keys = std::mem::take(&mut self.frame_keys);
        let n = keys.len() as u32;
        let bytes = 16 * n as u64;
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("node timings"),
        });
        enc.resolve_query_set(&self.set, 0..2 * n, &self.resolve, 0);
        enc.copy_buffer_to_buffer(&self.resolve, 0, &buf, 0, bytes);
        queue.submit([enc.finish()]);
        let ready = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let (r, f) = (ready.clone(), failed.clone());
        buf.map_async(wgpu::MapMode::Read, ..bytes, move |res| match res {
            Ok(()) => r.store(true, Ordering::Release),
            Err(_) => f.store(true, Ordering::Release),
        });
        self.pending.push(Pending {
            buf,
            keys,
            ready,
            failed,
        });
    }

    pub fn times(&self) -> &HashMap<u64, f32> {
        &self.times
    }

    pub fn total(&self) -> f32 {
        self.total
    }
}
