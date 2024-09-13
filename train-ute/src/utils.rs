use rayon::{ThreadPool, ThreadPoolBuildError};
use rgb::RGB8;

#[allow(dead_code)]
pub fn create_pool(num_threads: usize) -> Result<ThreadPool, ThreadPoolBuildError> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
}

pub fn mix_rgb(a: RGB8, b: RGB8, t: f32) -> RGB8 {
    RGB8 {
        r: (a.r as f32 * (1. - t) + b.r as f32 * t) as u8,
        g: (a.g as f32 * (1. - t) + b.g as f32 * t) as u8,
        b: (a.b as f32 * (1. - t) + b.b as f32 * t) as u8,
    }
}

// Simple quadratic easing.
pub fn quadratic_ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        2. * t * t
    } else {
        (4. - 2. * t) * t - 1.
    }
}

// Inverse of quadratic easing (for easing time)
pub fn quadratic_inv_ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        (t * 0.5).sqrt()
    } else {
        1. - ((1. - t) * 0.5).sqrt()
    }
}

