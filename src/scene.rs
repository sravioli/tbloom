pub mod backdrop;
pub mod bouquet;
mod paper;

use crate::engine::raster::Item;

pub struct Scene {
    pub items: Vec<Item>,
    /// farthest point from the pivot (main keeps the eye outside it)
    pub radius: f64,
}

/// Deterministic LCG (in f64 arithmetic, as the shape was tuned with).
pub struct Rng(pub f64);
impl Rng {
    pub fn r(&mut self) -> f64 {
        self.0 = (self.0 * 1103515245.0 + 12345.0) % 2147483648.0;
        self.0 / 2147483648.0
    }
    pub fn rnd(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.r()
    }
}
