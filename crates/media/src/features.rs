//! Video signals: how much is moving, where, how bright, what colour.
//!
//! Every decoded picture gets a tiny summary ([`Features`]: a 32 × 18 grid of
//! average brightness plus overall brightness and colour), worked out on the
//! decode / capture thread. [`VideoSignals`] turns the summaries of the
//! pictures a node shows into signals, normalised like the audio ones
//! (1.0 = typical for this source, so a busy video and a calm one both
//! drive things the same amount), smoothed by elapsed time.
//!
//! Pure and deterministic: the same pictures and time steps give the same
//! signals.

/// Cells across and down in the brightness grid.
pub const GRID_W: usize = 32;
pub const GRID_H: usize = 18;

/// The summary of one picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Features {
    /// Average brightness (0..1) of each cell, rows top to bottom.
    pub grid: Vec<f32>,
    /// Average brightness of the whole picture, 0..1.
    pub luma: f32,
    /// Average colour as a vector on the colour wheel (red = +x, green at
    /// 120°, blue at 240°); its length is how colourful the picture is.
    pub chroma: [f32; 2],
}

/// Summarise a straight RGBA8 picture. Samples up to 8 × 8 points per cell,
/// so it costs about the same for any picture size.
pub fn features(width: u32, height: u32, rgba: &[u8]) -> Features {
    let (w, h) = (width as usize, height as usize);
    let mut grid = vec![0.0; GRID_W * GRID_H];
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return Features {
            grid,
            luma: 0.0,
            chroma: [0.0, 0.0],
        };
    }
    const S: usize = 8;
    let (mut total, mut cx, mut cy) = (0.0f64, 0.0f64, 0.0f64);
    for gy in 0..GRID_H {
        for gx in 0..GRID_W {
            let mut sum = 0.0f32;
            for sy in 0..S {
                let y = ((gy * S + sy) * 2 + 1) * h / (GRID_H * S * 2);
                for sx in 0..S {
                    let x = ((gx * S + sx) * 2 + 1) * w / (GRID_W * S * 2);
                    let i = (y.min(h - 1) * w + x.min(w - 1)) * 4;
                    let (r, g, b) = (
                        rgba[i] as f32 / 255.0,
                        rgba[i + 1] as f32 / 255.0,
                        rgba[i + 2] as f32 / 255.0,
                    );
                    sum += 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    cx += (r - 0.5 * (g + b)) as f64;
                    cy += (0.866_025_4 * (g - b)) as f64;
                }
            }
            let cell = sum / (S * S) as f32;
            grid[gy * GRID_W + gx] = cell;
            total += cell as f64;
        }
    }
    let n = (GRID_W * GRID_H) as f64;
    let samples = n * (S * S) as f64;
    Features {
        grid,
        luma: (total / n) as f32,
        chroma: [(cx / samples) as f32, (cy / samples) as f32],
    }
}

/// Hue (0..1, 0 = red, 1/3 = green, 2/3 = blue) of a colour-wheel vector.
pub fn hue_of(chroma: [f32; 2]) -> f32 {
    (chroma[1].atan2(chroma[0]) / std::f32::consts::TAU).rem_euclid(1.0)
}

/// The five video signals, in bus order.
pub const VIDEO_SIGNAL_NAMES: [&str; 5] =
    ["motion", "motion_x", "motion_y", "brightness", "video_hue"];

/// Turns the pictures one source shows into signals (see the module docs).
#[derive(Clone, Debug, Default)]
pub struct VideoSignals {
    prev: Option<(u64, Vec<f32>)>,
    /// Seconds since a new picture arrived.
    since_new: f32,
    raw_motion: f32,
    motion_mean: f32,
    bright_mean: f32,
    started: bool,
    seen_picture: bool,
    /// Outputs.
    pub motion: f32,
    pub motion_x: f32,
    pub motion_y: f32,
    pub brightness: f32,
    pub hue: f32,
    chroma: [f32; 2],
}

/// Time constants (seconds).
const MEAN_TAU: f32 = 8.0;
const MOTION_ATTACK: f32 = 0.03;
const MOTION_RELEASE: f32 = 0.3;
const PLACE_TAU: f32 = 0.15;
const PLACE_RETURN_TAU: f32 = 2.0;
const BRIGHT_ATTACK: f32 = 0.03;
const BRIGHT_RELEASE: f32 = 0.12;
const HUE_TAU: f32 = 0.4;
/// No new picture for this long = the picture is frozen: no motion.
const FROZEN_AFTER: f32 = 0.25;
/// Typical motion is never taken as less than this (mean change per cell),
/// so sensor noise in a still shot doesn't count as "typical".
const MOTION_FLOOR: f32 = 0.004;
const BRIGHT_FLOOR: f32 = 0.02;
const MAX_LEVEL: f32 = 8.0;

fn alpha(dt: f32, tau: f32) -> f32 {
    1.0 - (-dt / tau.max(1e-4)).exp()
}

fn follow(s: &mut f32, x: f32, dt: f32, attack: f32, release: f32) {
    let tau = if x > *s { attack } else { release };
    *s += alpha(dt, tau) * (x - *s);
}

impl VideoSignals {
    pub fn new() -> Self {
        Self {
            motion_x: 0.5,
            motion_y: 0.5,
            ..Default::default()
        }
    }

    /// Advance by `dt` seconds. `pic` is what the source shows now
    /// (picture id + summary), `None` if nothing.
    pub fn update(&mut self, pic: Option<(u64, &Features)>, dt: f32) -> [f32; 5] {
        let dt = dt.clamp(0.0, 0.25);
        if !self.started {
            self.started = true;
            self.motion_x = 0.5;
            self.motion_y = 0.5;
        }
        let mut place: Option<(f32, f32)> = None;
        match pic {
            Some((id, f)) if self.prev.as_ref().is_none_or(|(p, _)| *p != id) => {
                if let Some((_, prev)) = &self.prev
                    && prev.len() == f.grid.len()
                {
                    let (mut sum, mut wsum, mut wx, mut wy) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
                    for (i, (a, b)) in f.grid.iter().zip(prev).enumerate() {
                        let d = (a - b).abs();
                        sum += d;
                        let w = d * d;
                        wsum += w;
                        wx += w * ((i % GRID_W) as f32 + 0.5) / GRID_W as f32;
                        // 0 = bottom, 1 = top.
                        wy += w * (1.0 - ((i / GRID_W) as f32 + 0.5) / GRID_H as f32);
                    }
                    self.raw_motion = sum / f.grid.len() as f32;
                    if wsum > 1e-6 && self.raw_motion > MOTION_FLOOR * 0.5 {
                        place = Some((wx / wsum, wy / wsum));
                    }
                }
                self.prev = Some((id, f.grid.clone()));
                self.since_new = 0.0;
            }
            Some(_) => self.since_new += dt,
            None => {
                self.prev = None;
                self.since_new += dt;
            }
        }
        if self.since_new > FROZEN_AFTER {
            self.raw_motion = 0.0;
        }

        // Motion, 1 = typical for this source.
        self.motion_mean += alpha(dt, MEAN_TAU) * (self.raw_motion - self.motion_mean);
        let level = (self.raw_motion / self.motion_mean.max(MOTION_FLOOR)).min(MAX_LEVEL);
        follow(&mut self.motion, level, dt, MOTION_ATTACK, MOTION_RELEASE);

        // Where it moves; drifts back to the middle when nothing does.
        match place {
            Some((x, y)) => {
                let a = alpha(dt, PLACE_TAU);
                self.motion_x += a * (x - self.motion_x);
                self.motion_y += a * (y - self.motion_y);
            }
            None if self.raw_motion < MOTION_FLOOR * 0.5 => {
                let a = alpha(dt, PLACE_RETURN_TAU);
                self.motion_x += a * (0.5 - self.motion_x);
                self.motion_y += a * (0.5 - self.motion_y);
            }
            None => {}
        }

        // Brightness, 1 = typical; colour.
        let (luma, chroma) = pic
            .map(|(_, f)| (f.luma, f.chroma))
            .unwrap_or((0.0, [0.0; 2]));
        if !self.seen_picture && pic.is_some() {
            // Start "typical" at the first picture instead of at black.
            self.seen_picture = true;
            self.bright_mean = luma;
            self.brightness = 1.0;
            self.chroma = chroma;
        }
        self.bright_mean += alpha(dt, MEAN_TAU) * (luma - self.bright_mean);
        let level = (luma / self.bright_mean.max(BRIGHT_FLOOR)).min(MAX_LEVEL);
        follow(
            &mut self.brightness,
            level,
            dt,
            BRIGHT_ATTACK,
            BRIGHT_RELEASE,
        );
        let a = alpha(dt, HUE_TAU);
        self.chroma[0] += a * (chroma[0] - self.chroma[0]);
        self.chroma[1] += a * (chroma[1] - self.chroma[1]);
        if self.chroma[0].hypot(self.chroma[1]) > 1e-3 {
            self.hue = hue_of(self.chroma);
        }
        self.values()
    }

    pub fn values(&self) -> [f32; 5] {
        [
            self.motion,
            self.motion_x,
            self.motion_y,
            self.brightness,
            self.hue,
        ]
    }
}
