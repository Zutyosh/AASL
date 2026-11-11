//! A session's frame log boiled down to a few hundred points: what the
//! session looked like over time. Kept next to the report, never shared.

use serde::{Deserialize, Serialize};

/// Points kept per session; enough for a chart, small on disk.
pub const POINTS: usize = 180;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Average frame rate over the slice.
    pub fps: f32,
    /// Frame rate of the slowest frame in the slice.
    pub fps_min: f32,
    /// GPU and CPU load in percent (0 when the log lacks them).
    pub gpu: f32,
    pub cpu: f32,
    /// Frames frozen for at least [`crate::community::HITCH_MS`].
    pub hitches: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    /// Seconds each point covers.
    pub step_s: f32,
    pub points: Vec<Point>,
    pub has_load: bool,
}

impl Timeline {
    pub fn duration_s(&self) -> f32 {
        self.step_s * self.points.len() as f32
    }
}

/// Slice a MangoHud CSV by playing time (the sum of its frame times).
pub fn from_mangohud(text: &str) -> Option<Timeline> {
    let has_load = text
        .lines()
        .find(|l| l.starts_with("fps,"))
        .is_some_and(|h| {
            h.split(',')
                .any(|c| matches!(c.trim(), "gpu_load" | "cpu_load"))
        });
    let rows = crate::community::mangohud_rows(text, ["gpu_load", "cpu_load"])?;
    if rows.len() < 100 {
        return None;
    }
    let total_ms: f32 = rows.iter().map(|r| r.0).sum();
    let step_ms = total_ms / POINTS as f32;
    let mut points = Vec::with_capacity(POINTS);
    let (mut clock, mut next) = (0.0f32, step_ms);
    let mut acc = (0usize, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0u32);
    let flush = |acc: &mut (usize, f32, f32, f32, f32, u32), points: &mut Vec<Point>| {
        if acc.0 > 0 {
            let n = acc.0 as f32;
            points.push(Point {
                fps: 1000.0 * n / acc.1,
                fps_min: 1000.0 / acc.2,
                gpu: acc.3 / n,
                cpu: acc.4 / n,
                hitches: acc.5,
            });
        }
        *acc = (0, 0.0, 0.0, 0.0, 0.0, 0);
    };
    for (ft, [gpu, cpu]) in &rows {
        clock += ft;
        acc.0 += 1;
        acc.1 += ft;
        acc.2 = acc.2.max(*ft);
        acc.3 += gpu;
        acc.4 += cpu;
        if *ft >= crate::community::HITCH_MS {
            acc.5 += 1;
        }
        if clock >= next {
            flush(&mut acc, &mut points);
            while next <= clock {
                next += step_ms;
            }
        }
    }
    flush(&mut acc, &mut points);
    Some(Timeline {
        step_s: step_ms / 1000.0,
        has_load,
        points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_by_playing_time() {
        let mut s = String::from("fps,frametime,cpu_load,gpu_load\n");
        for i in 0..2000 {
            // Second half is slower.
            let ft = if i < 1000 { 10.0 } else { 20.0 };
            s.push_str(&format!("{},{ft},30,60\n", 1000.0 / ft));
        }
        let t = from_mangohud(&s).unwrap();
        assert!(t.points.len() >= POINTS - 1 && t.points.len() <= POINTS + 1);
        assert!((t.duration_s() - 30.0).abs() < 0.5, "{}", t.duration_s());
        let (a, b) = (t.points[5], t.points[t.points.len() - 5]);
        assert!(a.fps > 90.0 && b.fps < 60.0, "{a:?} {b:?}");
        assert!(t.has_load && (a.gpu - 60.0).abs() < 0.1);
        assert!(from_mangohud("fps,frametime\n1,2\n").is_none());
    }

    #[test]
    fn a_hole_in_the_log_is_not_a_frame() {
        // MangoHud logged 10 s, nothing for an hour and a half while the
        // game drew only to the headset, then 10 s: one bogus frame time.
        let mut s = String::from("fps,frametime,cpu_load,gpu_load\n");
        for i in 0..2000 {
            s.push_str("100,10,30,60\n");
            if i == 999 {
                s.push_str("0.0001,14473600,30,60\n");
            }
        }
        let t = from_mangohud(&s).unwrap();
        assert!((t.duration_s() - 20.0).abs() < 0.5, "{}", t.duration_s());
        assert!(t.points.iter().all(|p| p.fps > 90.0 && p.hitches == 0));
        let p = crate::community::parse_mangohud(&s).unwrap();
        assert_eq!(p.frames, 2000);
        assert!(p.avg_fps > 99.0 && p.low1_fps > 99.0, "{p:?}");
        assert!(p.hitches.is_none());
    }
}
