use std::fmt;

use web_time::{Duration, Instant};

#[derive(Default)]
pub struct FrameTimer {
    cpu_ms: Vec<f32>,
    interval_ms: Vec<f32>,
    last_present: Option<Instant>,
}

impl FrameTimer {
    /// `cpu` is the time taken to prepare, encode and submit the frame.
    pub fn record(&mut self, cpu: Duration) {
        let now = Instant::now();
        if let Some(last) = self.last_present.replace(now) {
            self.interval_ms.push((now - last).as_secs_f32() * 1000.0);
        }
        self.cpu_ms.push(cpu.as_secs_f32() * 1000.0);
    }

    pub fn frames(&self) -> usize {
        self.cpu_ms.len()
    }

    pub fn summary(&self) -> Summary {
        Summary {
            frames: self.cpu_ms.len(),
            cpu: Percentiles::of(&self.cpu_ms),
            interval: Percentiles::of(&self.interval_ms),
        }
    }

    pub fn recent(&self, n: usize) -> Summary {
        let tail = |v: &[f32]| v[v.len().saturating_sub(n)..].to_vec();
        Summary {
            frames: n.min(self.cpu_ms.len()),
            cpu: Percentiles::of(&tail(&self.cpu_ms)),
            interval: Percentiles::of(&tail(&self.interval_ms)),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Percentiles {
    pub avg: f32,
    pub p50: f32,
    pub p95: f32,
    pub p99: f32,
    pub max: f32,
}

impl Percentiles {
    fn of(samples: &[f32]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(f32::total_cmp);
        let at = |q: f32| sorted[((sorted.len() - 1) as f32 * q).round() as usize];
        Self {
            avg: sorted.iter().sum::<f32>() / sorted.len() as f32,
            p50: at(0.50),
            p95: at(0.95),
            p99: at(0.99),
            max: *sorted.last().expect("non-empty"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Summary {
    pub frames: usize,
    pub cpu: Percentiles,
    pub interval: Percentiles,
}

impl Summary {
    pub fn fps(&self) -> f32 {
        if self.interval.avg > 0.0 {
            1000.0 / self.interval.avg
        } else {
            0.0
        }
    }
}

impl fmt::Display for Percentiles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "avg {:.2} / p50 {:.2} / p95 {:.2} / p99 {:.2} / max {:.2} ms",
            self.avg, self.p50, self.p95, self.p99, self.max
        )
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "frames:         {}", self.frames)?;
        writeln!(f, "fps:            {:.1}", self.fps())?;
        writeln!(f, "frame interval: {}", self.interval)?;
        write!(f, "cpu per frame:  {}", self.cpu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_of_known_samples() {
        let samples: Vec<f32> = (1..=100).map(|v| v as f32).collect();
        let p = Percentiles::of(&samples);
        assert_eq!(p.p50, 51.0);
        assert_eq!(p.p95, 95.0);
        assert_eq!(p.p99, 99.0);
        assert_eq!(p.max, 100.0);
        assert!((p.avg - 50.5).abs() < 1e-4);
    }
}
