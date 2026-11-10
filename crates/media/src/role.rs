//! What part a picture can play in a set: a **backdrop** (a busy picture
//! that fills the frame: a landscape, a texture, a crowd) or a **subject**
//! to place on top (a logo, a product shot, a person on a plain wall, a
//! see-through PNG). Templates use this to put each of your pictures where it
//! works, and to cut subjects out automatically.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureRole {
    Backdrop,
    Subject,
}

/// Look at a picture file (decoded small, so it's quick).
pub fn picture_role(path: &Path) -> Result<PictureRole, String> {
    let img = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let small = image::imageops::thumbnail(&img.to_rgba8(), 64, 64);
    Ok(role_of(&small))
}

/// The decision, on a small RGBA picture.
pub fn role_of(img: &image::RgbaImage) -> PictureRole {
    let (w, h) = img.dimensions();
    if w < 4 || h < 4 {
        return PictureRole::Backdrop;
    }
    let px = |x: u32, y: u32| {
        let p = img.get_pixel(x, y).0;
        (
            [
                p[0] as f32 / 255.0,
                p[1] as f32 / 255.0,
                p[2] as f32 / 255.0,
            ],
            p[3],
        )
    };
    // Already see-through in places: a cut-out PNG.
    let see_through = img.pixels().filter(|p| p.0[3] < 128).count();
    if see_through as f32 > (w * h) as f32 * 0.02 {
        return PictureRole::Subject;
    }
    // The border ring (2 pixels deep).
    let mut border = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if x < 2 || y < 2 || x >= w - 2 || y >= h - 2 {
                border.push(px(x, y).0);
            }
        }
    }
    let n = border.len() as f32;
    let mut mean = [0.0f32; 3];
    for c in &border {
        for i in 0..3 {
            mean[i] += c[i] / n;
        }
    }
    let dist = |a: [f32; 3], b: [f32; 3]| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    let agree = border.iter().filter(|c| dist(**c, mean) < 0.15).count() as f32 / n;
    if agree < 0.75 {
        return PictureRole::Backdrop;
    }
    // A subject sits *inside* the plain colour: at most one side (the bottom
    // of a person standing, say) may be broken. A band of light running
    // right across a dark sky breaks two sides, so it's a backdrop.
    let side_ok = |pts: &mut dyn Iterator<Item = (u32, u32)>| {
        let (mut ok, mut all) = (0u32, 0u32);
        for (x, y) in pts {
            all += 1;
            if dist(px(x, y).0, mean) < 0.15 {
                ok += 1;
            }
        }
        ok as f32 >= all as f32 * 0.85
    };
    let broken = [
        side_ok(&mut (0..w).map(|x| (x, 0))),
        side_ok(&mut (0..w).map(|x| (x, h - 1))),
        side_ok(&mut (0..h).map(|y| (0, y))),
        side_ok(&mut (0..h).map(|y| (w - 1, y))),
    ]
    .iter()
    .filter(|ok| !**ok)
    .count();
    if broken > 1 {
        return PictureRole::Backdrop;
    }
    // A plain border: is there something different in the middle?
    let (x0, x1, y0, y1) = (w / 4, w * 3 / 4, h / 4, h * 3 / 4);
    let mut differ = 0u32;
    let mut total = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            total += 1;
            if dist(px(x, y).0, mean) > 0.2 {
                differ += 1;
            }
        }
    }
    if differ as f32 > total as f32 * 0.1 {
        PictureRole::Subject
    } else {
        // Plain all over (a gradient, an empty wall): it's a backdrop.
        PictureRole::Backdrop
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn noise(w: u32, h: u32) -> RgbaImage {
        let mut s = 12345u32;
        RgbaImage::from_fn(w, h, |_, _| {
            let mut c = [0u8; 4];
            for v in c.iter_mut().take(3) {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *v = (s >> 24) as u8;
            }
            c[3] = 255;
            Rgba(c)
        })
    }

    #[test]
    fn busy_photos_are_backdrops() {
        assert_eq!(role_of(&noise(64, 48)), PictureRole::Backdrop);
    }

    #[test]
    fn a_logo_on_plain_white_is_a_subject() {
        let img = RgbaImage::from_fn(64, 64, |x, y| {
            let inside = (20..44).contains(&x) && (20..44).contains(&y);
            if inside {
                Rgba([200, 20, 40, 255])
            } else {
                Rgba([250, 250, 250, 255])
            }
        });
        assert_eq!(role_of(&img), PictureRole::Subject);
    }

    #[test]
    fn see_through_pngs_are_subjects() {
        let img = RgbaImage::from_fn(64, 64, |x, _| {
            Rgba([255, 255, 255, if x < 32 { 0 } else { 255 }])
        });
        assert_eq!(role_of(&img), PictureRole::Subject);
    }

    #[test]
    fn a_band_across_a_dark_sky_is_a_backdrop() {
        // Like an aurora photo: dark all round, but the light crosses the
        // whole picture from side to side.
        let img = RgbaImage::from_fn(64, 64, |_, y| {
            if (24..40).contains(&y) {
                Rgba([40, 200, 150, 255])
            } else {
                Rgba([5, 8, 20, 255])
            }
        });
        assert_eq!(role_of(&img), PictureRole::Backdrop);
    }

    #[test]
    fn a_person_standing_on_the_bottom_edge_is_a_subject() {
        let img = RgbaImage::from_fn(64, 64, |x, y| {
            if (26..38).contains(&x) && y >= 16 {
                Rgba([180, 120, 90, 255])
            } else {
                Rgba([230, 230, 225, 255])
            }
        });
        assert_eq!(role_of(&img), PictureRole::Subject);
    }

    #[test]
    fn a_plain_wall_is_a_backdrop() {
        let img = RgbaImage::from_fn(64, 64, |_, _| Rgba([40, 60, 90, 255]));
        assert_eq!(role_of(&img), PictureRole::Backdrop);
    }

    #[test]
    fn missing_files_say_so() {
        assert!(picture_role(Path::new("no/such/file.png")).is_err());
    }
}
