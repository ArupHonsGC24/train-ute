use rgb::RGB8;

// Colour conversion utilities.

pub fn rgb_to_hsv(rgb: RGB8) -> (f64, f64, f64) {
    let r = rgb.r as f64;
    let g = rgb.g as f64;
    let b = rgb.b as f64;
    let min = r.min(g).min(b);
    let max = r.max(g).max(b);
    
    let v = max;
    let delta = max - min;
    let s = if max != 0. { delta / max } else { return (0., 0., 0.); };
    let h = if r == max {
        (g - b) / delta
    } else if g == max {
        2. + (b - r) / delta
    } else {
        4. + (r - g) / delta
    };
    let h = h * 60.;
    let h = if h < 0. { h + 360. } else { h };
    (h, s, v)
}

pub fn hsv_to_rgb(hsv: (f64, f64, f64)) -> RGB8 {
    let h = hsv.0;
    let s = hsv.1;
    let v = hsv.2;
    if s == 0. {
        return RGB8::new(v as u8, v as u8, v as u8);
    }
    let h = h / 60.;
    let i = h.floor() as i32;
    let f = h - i as f64;
    let p = v * (1. - s);
    let q = v * (1. - s * f);
    let t = v * (1. - s * (1. - f));
    match i {
        0 => RGB8::new(v as u8, t as u8, p as u8),
        1 => RGB8::new(q as u8, v as u8, p as u8),
        2 => RGB8::new(p as u8, v as u8, t as u8),
        3 => RGB8::new(p as u8, q as u8, v as u8),
        4 => RGB8::new(t as u8, p as u8, v as u8),
        _ => RGB8::new(v as u8, p as u8, q as u8),
    }
}