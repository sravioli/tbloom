// Dark velvet backdrop behind the subject: barely catches the light, faint folds.
use crate::engine::mesh::Builder;
use crate::engine::raster::Item;

pub fn build() -> Item {
    let mut d = Builder::new();
    d.grid(8, 8, false, |u, w| {
        let (x, y) = (-9.0 + 18.0 * u, -9.0 + 18.0 * w);
        let k = 1.0 + 0.25 * (x * 1.1 + y * 0.6).sin() * (y * 0.9 - x * 0.3).sin();
        [x, y, -3.6, 0.07 * k, 0.05 * k, 0.04 * k, 0.0]
    });
    Item {
        mesh: d.finish(),
        rotate: false,
        caster: false,
    }
}
