// Scene from a binary FBX file: one engine mesh per material (so each has one texture),
// re-oriented to the engine's z-up model space and fitted to the same size as the built-in
// bouquet, so zoom limits, shadow extent and framing still hold.
use super::Scene;
use crate::engine::mesh::{Builder, Texture};
use crate::engine::raster::Item;
use crate::engine::{fbx, image};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

// Fit: the bulk of the model (90% of its vertices) gets the built-in bouquet's size; stray
// parts (floating petals) may reach further, but never past the shadow map's extent.
const BODY: f64 = 1.3;
const MAX_RADIUS: f64 = 2.5;
const TEX_SIZE: usize = 128;

/// Find a texture the FBX names: as given relative to the file, else by base name next to it
/// or in a sibling/child "textures" folder, trying common extensions (paper.jpg -> paper.jpeg).
fn resolve(dir: &str, name: &str) -> Option<String> {
    let name = name.replace('\\', "/");
    let base = name.rsplit('/').next().unwrap_or("");
    let (stem, ext) = match base.rfind('.') {
        Some(i) => (&base[..i], &base[i..]),
        None => (base, ""),
    };
    let mut tries = vec![format!("{dir}/{name}")];
    for d in [
        dir.to_string(),
        format!("{dir}/../textures"),
        format!("{dir}/textures"),
    ] {
        for e in [ext, ".png", ".jpg", ".jpeg", ".PNG", ".JPG", ".JPEG"] {
            tries.push(format!("{d}/{stem}{e}"));
        }
    }
    tries.into_iter().find(|p| Path::new(p).is_file())
}

/// FBX here is y-up; the engine's model space is z-up (stems along -z): (x, y, z) -> (x, -z, y).
fn z_up(p: &fbx::Corner) -> [f64; 3] {
    [p[0], -p[2], p[1]]
}

/// Center (per-axis median: robust to the stray parts that skew a bounding box), scale, and the
/// scaled distance of the farthest point.
fn fit(pts: &[[f64; 3]]) -> ([f64; 3], f64, f64) {
    let median = |a: usize| {
        let mut s: Vec<f64> = pts.iter().map(|p| p[a]).collect();
        s.sort_by(f64::total_cmp);
        s[s.len() / 2]
    };
    let c = [median(0), median(1), median(2)];
    let mut d: Vec<f64> = pts
        .iter()
        .map(|p| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt())
        .collect();
    d.sort_by(f64::total_cmp);
    let p90 = d[(d.len() as f64 * 0.9).floor() as usize - 1].max(1e-9);
    let dmax = d[d.len() - 1].max(1e-9);
    let k = (BODY / p90).min(MAX_RADIUS / dmax);
    (c, k, dmax * k)
}

/// The texture a material names, loaded once per file; None (with a warning) when not found.
fn texture(
    dir: &str,
    name: &str,
    cache: &mut HashMap<String, Rc<Texture>>,
) -> Result<Option<Rc<Texture>>, String> {
    let Some(file) = resolve(dir, name) else {
        eprintln!("warning: texture not found: {name} (using the material color)");
        return Ok(None);
    };
    if !cache.contains_key(&file) {
        cache.insert(file.clone(), Rc::new(image::load(&file, TEX_SIZE)?));
    }
    Ok(Some(cache[&file].clone()))
}

pub fn load(path: &str) -> Result<Scene, String> {
    let groups = fbx::load(path)?;
    let dir = path.rfind('/').map_or(".", |i| &path[..i]);
    let pts: Vec<[f64; 3]> = groups
        .iter()
        .flat_map(|g| g.tris.iter().flatten())
        .map(z_up)
        .collect();
    if pts.is_empty() {
        return Err(format!("fbx: no geometry in {path}"));
    }
    let (c, k, radius) = fit(&pts);

    let mut items = Vec::new();
    let mut textures = HashMap::new();
    for g in &groups {
        let mat = &g.material;
        let tex = match &mat.texture {
            Some(name) => texture(dir, name, &mut textures)?,
            None => None,
        };
        let [cr, cg, cb] = mat.color;
        let mut b = Builder::new();
        // corners with the same position and uv (to 5 decimals) become one vertex, so normals come out smooth
        let mut seen: HashMap<[i64; 5], usize> = HashMap::new();
        let mut vert = |b: &mut Builder, p: &fbx::Corner| {
            let key = p.map(|x| (x * 1e5).round() as i64);
            *seen.entry(key).or_insert_with(|| {
                let [x, y, z] = z_up(p);
                let p7 = [
                    (x - c[0]) * k,
                    (y - c[1]) * k,
                    (z - c[2]) * k,
                    cr,
                    cg,
                    cb,
                    0.1,
                ];
                b.v(p7, [p[3], p[4]])
            })
        };
        for t in &g.tris {
            let (i0, i1, i2) = (
                vert(&mut b, &t[0]),
                vert(&mut b, &t[1]),
                vert(&mut b, &t[2]),
            );
            b.t(i0, i1, i2);
        }
        let mut m = b.finish();
        m.tex = tex;
        items.push(Item {
            mesh: m,
            rotate: true,
            caster: true,
        });
    }
    items.push(super::backdrop::build());
    Ok(Scene { items, radius })
}
