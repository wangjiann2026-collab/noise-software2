//! Noise-level colour map.
//!
//! Maps A-weighted dBA values to RGBA colours following the WHO / EEA
//! recommended colour scale for environmental noise maps.
//!
//! # Default scale
//! | Range (dBA) | Colour |
//! |-------------|--------|
//! | < 45        | Dark green |
//! | 45 – 50     | Green |
//! | 50 – 55     | Yellow-green |
//! | 55 – 60     | Yellow |
//! | 60 – 65     | Orange |
//! | 65 – 70     | Red-orange |
//! | 70 – 75     | Red |
//! | ≥ 75        | Dark red |

/// RGBA colour (0–255 per channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoiseColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl NoiseColor {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self { Self { r, g, b, a } }
    /// Convert to `[f32; 4]` in linear 0.0–1.0 range (no gamma correction).
    pub fn to_f32_array(self) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            self.a as f32 / 255.0,
        ]
    }
    /// Blend two colours linearly by `t ∈ [0, 1]`.
    pub fn lerp(a: Self, b: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: (a.r as f32 + (b.r as f32 - a.r as f32) * t) as u8,
            g: (a.g as f32 + (b.g as f32 - a.g as f32) * t) as u8,
            b: (a.b as f32 + (b.b as f32 - a.b as f32) * t) as u8,
            a: (a.a as f32 + (b.a as f32 - a.a as f32) * t) as u8,
        }
    }
}

/// A colour stop: (dBA threshold, colour above this threshold).
#[derive(Debug, Clone, Copy)]
pub struct ColorStop {
    pub level_db: f32,
    pub color: NoiseColor,
}

/// Configurable dBA-to-colour map.
///
/// Colours are interpolated linearly between stops.
/// Values below the first stop use the first stop's colour;
/// values above the last stop use the last stop's colour.
#[derive(Debug, Clone)]
pub struct ColorMap {
    stops: Vec<ColorStop>,
    /// Alpha for all rendered pixels (0–255).
    pub alpha: u8,
    /// Colour for no-data / −∞ cells.
    pub no_data_color: NoiseColor,
}

impl Default for ColorMap {
    fn default() -> Self {
        Self::who_standard()
    }
}

impl ColorMap {
    /// WHO / EEA recommended noise map colour scale.
    pub fn who_standard() -> Self {
        let stops = vec![
            ColorStop { level_db: 35.0, color: NoiseColor::new(0,   114,  54, 255) }, // dark green
            ColorStop { level_db: 45.0, color: NoiseColor::new(0,   184,  86, 255) }, // green
            ColorStop { level_db: 50.0, color: NoiseColor::new(168, 224,  56, 255) }, // yellow-green
            ColorStop { level_db: 55.0, color: NoiseColor::new(255, 240,   0, 255) }, // yellow
            ColorStop { level_db: 60.0, color: NoiseColor::new(255, 175,   0, 255) }, // orange
            ColorStop { level_db: 65.0, color: NoiseColor::new(255, 100,   0, 255) }, // red-orange
            ColorStop { level_db: 70.0, color: NoiseColor::new(210,   0,   0, 255) }, // red
            ColorStop { level_db: 75.0, color: NoiseColor::new(120,   0,   0, 255) }, // dark red
        ];
        Self { stops, alpha: 200, no_data_color: NoiseColor::new(200, 200, 200, 0) }
    }

    /// Custom colour scale from caller-supplied stops.
    /// Stops are sorted internally by `level_db`.
    pub fn custom(mut stops: Vec<ColorStop>, alpha: u8) -> Self {
        stops.sort_by(|a, b| a.level_db.partial_cmp(&b.level_db).unwrap());
        Self { stops, alpha, no_data_color: NoiseColor::new(200, 200, 200, 0) }
    }

    /// Sample the colour map at `db` dBA.
    pub fn sample(&self, db: f32) -> NoiseColor {
        if db.is_nan() || db.is_infinite() {
            return self.no_data_color;
        }
        if self.stops.is_empty() {
            return NoiseColor::new(128, 128, 128, self.alpha);
        }
        // Below first stop.
        if db <= self.stops[0].level_db {
            let mut c = self.stops[0].color;
            c.a = self.alpha;
            return c;
        }
        // Above last stop.
        if db >= self.stops[self.stops.len() - 1].level_db {
            let mut c = self.stops[self.stops.len() - 1].color;
            c.a = self.alpha;
            return c;
        }
        // Interpolate between adjacent stops.
        for i in 0..self.stops.len() - 1 {
            let lo = &self.stops[i];
            let hi = &self.stops[i + 1];
            if db >= lo.level_db && db <= hi.level_db {
                let t = (db - lo.level_db) / (hi.level_db - lo.level_db);
                let mut c = NoiseColor::lerp(lo.color, hi.color, t);
                c.a = self.alpha;
                return c;
            }
        }
        // Fallback (shouldn't reach here).
        let mut c = self.stops.last().unwrap().color;
        c.a = self.alpha;
        c
    }

    /// Sample as `[f32; 4]` (r, g, b, a) in 0–1 range.
    pub fn sample_f32(&self, db: f32) -> [f32; 4] {
        self.sample(db).to_f32_array()
    }

    /// Generate `n` evenly spaced legend entries across the full range.
    pub fn legend_ticks(&self, n: usize) -> Vec<(f32, NoiseColor)> {
        if n == 0 || self.stops.is_empty() { return Vec::new(); }
        let min = self.stops[0].level_db;
        let max = self.stops[self.stops.len() - 1].level_db;
        (0..n).map(|i| {
            let db = min + (max - min) * i as f32 / (n - 1).max(1) as f32;
            (db, self.sample(db))
        }).collect()
    }

    /// Minimum dBA level of this colour scale.
    pub fn min_db(&self) -> f32 {
        self.stops.first().map(|s| s.level_db).unwrap_or(35.0)
    }

    /// Maximum dBA level of this colour scale.
    pub fn max_db(&self) -> f32 {
        self.stops.last().map(|s| s.level_db).unwrap_or(75.0)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_min_returns_first_stop_color() {
        let cm = ColorMap::who_standard();
        let c = cm.sample(20.0);
        let first = cm.stops[0].color;
        assert_eq!((c.r, c.g, c.b), (first.r, first.g, first.b));
    }

    #[test]
    fn above_max_returns_last_stop_color() {
        let cm = ColorMap::who_standard();
        let c = cm.sample(90.0);
        let last = cm.stops.last().unwrap().color;
        assert_eq!((c.r, c.g, c.b), (last.r, last.g, last.b));
    }

    #[test]
    fn midpoint_interpolation() {
        let stops = vec![
            ColorStop { level_db: 0.0, color: NoiseColor::new(0, 0, 0, 255) },
            ColorStop { level_db: 100.0, color: NoiseColor::new(100, 100, 100, 255) },
        ];
        let cm = ColorMap::custom(stops, 255);
        let c = cm.sample(50.0);
        assert!((c.r as i32 - 50).abs() <= 1);
        assert!((c.g as i32 - 50).abs() <= 1);
        assert!((c.b as i32 - 50).abs() <= 1);
    }

    #[test]
    fn nan_returns_no_data() {
        let cm = ColorMap::who_standard();
        let c = cm.sample(f32::NAN);
        assert_eq!(c.a, 0);
    }

    #[test]
    fn neg_inf_returns_no_data() {
        let cm = ColorMap::who_standard();
        let c = cm.sample(f32::NEG_INFINITY);
        assert_eq!(c.a, 0);
    }

    #[test]
    fn legend_ticks_count_correct() {
        let cm = ColorMap::who_standard();
        let ticks = cm.legend_ticks(5);
        assert_eq!(ticks.len(), 5);
    }

    #[test]
    fn legend_ticks_first_is_min() {
        let cm = ColorMap::who_standard();
        let ticks = cm.legend_ticks(5);
        assert!((ticks[0].0 - cm.min_db()).abs() < 0.01);
    }

    #[test]
    fn sample_f32_sum_in_range() {
        let cm = ColorMap::who_standard();
        let rgba = cm.sample_f32(65.0);
        for &v in &rgba { assert!((0.0..=1.0).contains(&v)); }
    }

    #[test]
    fn lerp_black_white_50pct() {
        let black = NoiseColor::new(0, 0, 0, 255);
        let white = NoiseColor::new(255, 255, 255, 255);
        let mid   = NoiseColor::lerp(black, white, 0.5);
        assert!((mid.r as i32 - 127).abs() <= 1);
    }
}
