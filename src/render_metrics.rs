use smithay::utils::{Physical, Rectangle};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct RenderMetrics {
    backend_name: &'static str,
    frame_count: AtomicU64,
    total_render_time_ns: AtomicU64,
    total_pixels: AtomicU64,
    damaged_pixels: AtomicU64,
    damage_rect_count: AtomicU64,
    last_log_time: std::sync::Mutex<Option<Instant>>,
}

impl RenderMetrics {
    pub fn new(backend_name: &'static str) -> Self {
        Self {
            backend_name,
            frame_count: AtomicU64::new(0),
            total_render_time_ns: AtomicU64::new(0),
            total_pixels: AtomicU64::new(0),
            damaged_pixels: AtomicU64::new(0),
            damage_rect_count: AtomicU64::new(0),
            last_log_time: std::sync::Mutex::new(None),
        }
    }

    pub fn start_frame(&self) -> FrameTimer {
        FrameTimer {
            start: Instant::now(),
            metrics: self,
        }
    }

    pub fn record_damage(&self, output_size: (i32, i32), damage: &[Rectangle<i32, Physical>]) {
        let total = (output_size.0 * output_size.1) as u64;
        let damaged: u64 = damage
            .iter()
            .map(|rect| (rect.size.w * rect.size.h) as u64)
            .sum();

        self.total_pixels.fetch_add(total, Ordering::Relaxed);
        self.damaged_pixels.fetch_add(damaged, Ordering::Relaxed);
        self.damage_rect_count
            .fetch_add(damage.len() as u64, Ordering::Relaxed);
    }

    fn record_frame_time(&self, duration: Duration) {
        self.frame_count.fetch_add(1, Ordering::Relaxed);
        self.total_render_time_ns
            .fetch_add(duration.as_nanos() as u64, Ordering::Relaxed);
    }

    pub fn maybe_log_stats(&self, force: bool) {
        let mut last_log = self.last_log_time.lock().unwrap();
        let should_log = if force {
            true
        } else if let Some(last) = *last_log {
            last.elapsed() >= Duration::from_secs(5)
        } else {
            true
        };

        if !should_log {
            return;
        }

        let frame_count = self.frame_count.load(Ordering::Relaxed);
        if frame_count == 0 {
            return;
        }

        let total_render_ns = self.total_render_time_ns.load(Ordering::Relaxed);
        let total_pixels = self.total_pixels.load(Ordering::Relaxed);
        let damaged_pixels = self.damaged_pixels.load(Ordering::Relaxed);
        let damage_rect_count = self.damage_rect_count.load(Ordering::Relaxed);

        let avg_render_ms = (total_render_ns as f64 / frame_count as f64) / 1_000_000.0;
        let damage_ratio = if total_pixels > 0 {
            (damaged_pixels as f64 / total_pixels as f64) * 100.0
        } else {
            0.0
        };
        let avg_rects = damage_rect_count as f64 / frame_count as f64;

        tracing::info!(
            "RENDER METRICS [{}]: {} frames, avg {:.2}ms/frame, damage {:.1}% ({}/{} px), avg {:.1} rects/frame",
            self.backend_name,
            frame_count,
            avg_render_ms,
            damage_ratio,
            damaged_pixels,
            total_pixels,
            avg_rects
        );

        self.reset();
        *last_log = Some(Instant::now());
    }

    pub fn reset(&self) {
        self.frame_count.store(0, Ordering::Relaxed);
        self.total_render_time_ns.store(0, Ordering::Relaxed);
        self.total_pixels.store(0, Ordering::Relaxed);
        self.damaged_pixels.store(0, Ordering::Relaxed);
        self.damage_rect_count.store(0, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> MetricsSnapshot {
        let frame_count = self.frame_count.load(Ordering::Relaxed);
        let total_render_ns = self.total_render_time_ns.load(Ordering::Relaxed);
        let total_pixels = self.total_pixels.load(Ordering::Relaxed);
        let damaged_pixels = self.damaged_pixels.load(Ordering::Relaxed);
        let damage_rect_count = self.damage_rect_count.load(Ordering::Relaxed);

        MetricsSnapshot {
            frame_count,
            avg_render_time_ms: if frame_count > 0 {
                (total_render_ns as f64 / frame_count as f64) / 1_000_000.0
            } else {
                0.0
            },
            damage_ratio: if total_pixels > 0 {
                (damaged_pixels as f64 / total_pixels as f64) * 100.0
            } else {
                0.0
            },
            total_pixels,
            damaged_pixels,
            avg_damage_rects: if frame_count > 0 {
                damage_rect_count as f64 / frame_count as f64
            } else {
                0.0
            },
        }
    }
}

pub struct FrameTimer<'a> {
    start: Instant,
    metrics: &'a RenderMetrics,
}

impl<'a> Drop for FrameTimer<'a> {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        self.metrics.record_frame_time(duration);
    }
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub frame_count: u64,
    pub avg_render_time_ms: f64,
    pub damage_ratio: f64,
    pub total_pixels: u64,
    pub damaged_pixels: u64,
    pub avg_damage_rects: f64,
}

impl MetricsSnapshot {
    pub fn print_summary(&self, label: &str) {
        println!("\n=== {} ===", label);
        println!("Frames rendered: {}", self.frame_count);
        println!("Avg render time: {:.3}ms", self.avg_render_time_ms);
        println!("Damage ratio: {:.1}%", self.damage_ratio);
        println!(
            "Pixels: {}/{} damaged",
            self.damaged_pixels, self.total_pixels
        );
        println!("Avg damage rects: {:.1}", self.avg_damage_rects);
        println!("================\n");
    }
}
