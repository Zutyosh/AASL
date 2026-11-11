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
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .by_ref()
        .find(|l| l.starts_with("fps,"))?
        .split(',')
        .map(str::trim)
        .collect();
    let col = |name: &str| header.iter().position(|c| *c == name);
    let ft_col = col("frametime")?;
    let (gpu_col, cpu_col) = (col("gpu_load"), col("cpu_load"));
    let rows: Vec<(f32, f32, f32)> = lines
        .filter_map(|l| {
            let v: Vec<&str> = l.split(',').collect();
            let num = |c: Option<usize>| {
                c.and_then(|c| v.get(c)?.trim().parse::<f32>().ok())
                    .filter(|x| x.is_finite())
                    .unwrap_or(0.0)
            };
            let ft = v.get(ft_col)?.trim().parse::<f32>().ok()?;
            (ft > 0.0 && ft.is_finite()).then(|| (ft, num(gpu_col), num(cpu_col)))
        })
        .collect();
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
    for (ft, gpu, cpu) in &rows {
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
            next += step_ms;
        }
    }
    flush(&mut acc, &mut points);
    Some(Timeline {
        step_s: step_ms / 1000.0,
        has_load: gpu_col.is_some() || cpu_col.is_some(),
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
}
