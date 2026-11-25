//! A small padlock drawn in code (no borrowed artwork), in the colour of the current urgency.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Fine,
    Warn,
    Urgent,
    Unknown,
}

impl Level {
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Level::Fine => [37, 99, 235],
            Level::Warn => [217, 119, 6],
            Level::Urgent => [220, 38, 38],
            Level::Unknown => [100, 116, 139],
        }
    }
}

fn inside(x: f32, y: f32) -> (bool, bool) {
    // coordinates in 0..1. Returns (background, white lock)
    let r = 0.2;
    let (cx, cy) = (x.clamp(r, 1.0 - r), y.clamp(r, 1.0 - r));
    let bg = (x - cx).powi(2) + (y - cy).powi(2) <= r * r;
    // lock body
    let body = x >= 0.24 && x <= 0.76 && y >= 0.46 && y <= 0.84;
    // shackle: ring between radii, top half, centred at (0.5, 0.46)
    let d = ((x - 0.5).powi(2) + (y - 0.44).powi(2)).sqrt();
    let shackle = d >= 0.13 && d <= 0.21 && y <= 0.47;
    // keyhole
    let hole = ((x - 0.5).powi(2) + (y - 0.6).powi(2)).sqrt() <= 0.06
        || (x >= 0.475 && x <= 0.525 && y >= 0.6 && y <= 0.74);
    (bg, (body && !hole) || shackle)
}

/// Straight (not premultiplied) RGBA pixels, top row first.
pub fn rgba(size: u32, level: Level) -> Vec<u8> {
    let [r, g, b] = level.rgb();
    let mut out = vec![0u8; (size * size * 4) as usize];
    let ss = 4;
    for py in 0..size {
        for px in 0..size {
            let (mut bgc, mut wc) = (0u32, 0u32);
            for sy in 0..ss {
                for sx in 0..ss {
                    let x = (px as f32 + (sx as f32 + 0.5) / ss as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / ss as f32) / size as f32;
                    let (bg, w) = inside(x, y);
                    if bg {
                        bgc += 1;
                        if w {
                            wc += 1;
                        }
                    }
                }
            }
            let n = (ss * ss) as f32;
            let a = bgc as f32 / n;
            let wf = if bgc > 0 { wc as f32 / bgc as f32 } else { 0.0 };
            let mix = |c: u8| (c as f32 * (1.0 - wf) + 255.0 * wf).round() as u8;
            let i = ((py * size + px) * 4) as usize;
            out[i] = mix(r);
            out[i + 1] = mix(g);
            out[i + 2] = mix(b);
            out[i + 3] = (a * 255.0).round() as u8;
        }
    }
    out
}
