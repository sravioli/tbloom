// FBX loader on ufbx (binary and ASCII; ufbx validates the untrusted file): bakes each node's
// geometry-to-world transform, triangulates faces and groups triangles by material. Returns
// groups for scene/model.rs to turn into meshes.
use std::collections::HashMap;

#[derive(Clone)]
pub struct Material {
    pub color: [f64; 3],
    pub texture: Option<String>,
}

/// x, y, z, u, v
pub type Corner = [f64; 5];

pub struct Group {
    pub material: Material,
    pub tris: Vec<[Corner; 3]>,
}

fn material(m: &ufbx::Material) -> Material {
    let d = &m.fbx.diffuse_color;
    let c = d.value_vec4;
    // the texture on the diffuse color, else any texture the material has
    let tex = d
        .texture
        .as_deref()
        .or(m.textures.first().map(|t| &*t.texture));
    Material {
        color: if d.has_value {
            [c.x, c.y, c.z]
        } else {
            [1.0; 3]
        },
        texture: tex.map(|t| {
            let rel = &*t.relative_filename;
            (if rel.is_empty() { &*t.filename } else { rel }).to_string()
        }),
    }
}

/// Load an FBX file: triangles grouped by material, positions in the file's world space.
pub fn load(path: &str) -> Result<Vec<Group>, String> {
    let scene = ufbx::load_file(path, ufbx::LoadOpts::default())
        .map_err(|e| format!("fbx: {} ({path})", e.description))?;
    let (mut groups, mut by_mat) = (Vec::<Group>::new(), HashMap::new());
    let mut idx = Vec::new();
    for node in &scene.nodes {
        let Some(mesh) = node.mesh.as_deref() else {
            continue;
        };
        // a node's own materials win (instanced meshes), else the mesh's
        let mats = if node.materials.is_empty() {
            &mesh.materials
        } else {
            &node.materials
        };
        let w = &node.geometry_to_world;
        idx.resize(mesh.max_face_triangles * 3, 0);
        for (fi, &face) in mesh.faces.iter().enumerate() {
            let mat = mesh
                .face_material
                .get(fi)
                .and_then(|&s| mats.get(s as usize))
                .or(mats.first());
            let gi = *by_mat
                .entry(mat.map(|m| m.element.element_id))
                .or_insert_with(|| {
                    groups.push(Group {
                        material: mat.map_or(
                            Material {
                                color: [0.8; 3],
                                texture: None,
                            },
                            |m| material(m),
                        ),
                        tris: Vec::new(),
                    });
                    groups.len() - 1
                });
            let corner = |i: u32| {
                let i = i as usize;
                let p = ufbx::transform_position(w, mesh.vertex_position[i]);
                let uv = if mesh.vertex_uv.exists {
                    mesh.vertex_uv[i]
                } else {
                    ufbx::Vec2 { x: 0.0, y: 0.0 }
                };
                [p.x, p.y, p.z, uv.x, uv.y]
            };
            let n = mesh.triangulate_face(&mut idx, face) as usize;
            for t in idx[..n * 3].chunks_exact(3) {
                groups[gi]
                    .tris
                    .push([corner(t[0]), corner(t[1]), corner(t[2])]);
            }
        }
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    // a minimal binary file built here (7.5 layout, 64-bit node headers)
    enum P {
        S(&'static str),
        L(u64),
        D(Vec<f64>),
        I(Vec<i32>),
    }
    fn array(o: &mut Vec<u8>, t: u8, n: usize, raw: Vec<u8>) {
        o.push(t);
        for x in [n as u32, 0, raw.len() as u32] {
            o.extend(x.to_le_bytes()); // count, encoding (raw), byte length
        }
        o.extend(raw);
    }
    fn prop(v: &P) -> Vec<u8> {
        let mut o = Vec::new();
        match v {
            P::S(s) => {
                o.push(b'S');
                o.extend((s.len() as u32).to_le_bytes());
                o.extend(s.as_bytes());
            }
            P::L(n) => {
                o.push(b'L');
                o.extend(n.to_le_bytes());
            }
            P::D(a) => array(
                &mut o,
                b'd',
                a.len(),
                a.iter().flat_map(|x| x.to_le_bytes()).collect(),
            ),
            P::I(a) => array(
                &mut o,
                b'i',
                a.len(),
                a.iter().flat_map(|x| x.to_le_bytes()).collect(),
            ),
        }
        o
    }
    struct N(&'static str, Vec<P>, Vec<N>);
    fn node(off: usize, n: &N) -> Vec<u8> {
        let ps: Vec<u8> = n.1.iter().flat_map(prop).collect();
        let head = 25 + n.0.len() + ps.len();
        let (mut body, mut o) = (Vec::new(), off + head);
        for k in &n.2 {
            let b = node(o, k);
            o += b.len();
            body.extend(b);
        }
        if !n.2.is_empty() {
            body.extend([0u8; 25]);
            o += 25;
        }
        let mut out = Vec::new();
        for x in [o as u64, n.1.len() as u64, ps.len() as u64] {
            out.extend(x.to_le_bytes());
        }
        out.push(n.0.len() as u8);
        out.extend(n.0.as_bytes());
        out.extend(ps);
        out.extend(body);
        out
    }

    fn quad_file() -> Vec<u8> {
        let mut header = b"Kaydara FBX Binary  \0\x1a\0".to_vec();
        header.extend(7500u32.to_le_bytes());
        let objects = N(
            "Objects",
            vec![],
            vec![
                N(
                    "Geometry",
                    vec![P::L(10), P::S("quad\0\x01Geometry"), P::S("Mesh")],
                    vec![
                        N(
                            "Vertices",
                            vec![P::D(vec![0., 0., 0., 1., 0., 0., 1., 1., 0., 0., 1., 0.])],
                            vec![],
                        ),
                        N("PolygonVertexIndex", vec![P::I(vec![0, 1, 2, -4])], vec![]), // negative index closes the quad
                    ],
                ),
                N(
                    "Model",
                    vec![P::L(20), P::S("q\0\x01Model"), P::S("Mesh")],
                    vec![N(
                        "Properties70",
                        vec![],
                        vec![N(
                            "P",
                            vec![
                                P::S("Lcl Translation"),
                                P::S(""),
                                P::S(""),
                                P::S("A"),
                                P::D(vec![0.]),
                            ],
                            vec![],
                        )],
                    )],
                ),
            ],
        );
        let conns = N(
            "Connections",
            vec![],
            vec![N("C", vec![P::S("OO"), P::L(10), P::L(20)], vec![])],
        );
        let o1 = node(header.len(), &objects);
        let o2 = node(header.len() + o1.len(), &conns);
        [header, o1, o2, vec![0; 25 + 160]].concat()
    }

    #[test]
    fn loads_and_rejects() {
        let data = quad_file();
        let dir = std::env::temp_dir().join(format!("tbloom-fbx-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("q.fbx");
        let p = path.to_str().unwrap();
        std::fs::write(&path, &data).unwrap();
        let groups = load(p).unwrap();
        assert!(
            groups.len() == 1 && groups[0].tris.len() == 2,
            "quad triangulates into 2 triangles"
        );
        let area: f64 = groups[0]
            .tris
            .iter()
            .map(|[a, b, c]| {
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
            })
            .sum();
        assert!(
            (area - 1.0).abs() < 1e-9,
            "triangles cover the unit quad: {area}"
        );
        // truncated or corrupted input fails with an error, not a crash
        for cut in [30, 60, data.len() - 300] {
            std::fs::write(&path, &data[..cut]).unwrap();
            match load(p) {
                Ok(g) => assert!(
                    g.iter().all(|g| g.tris.is_empty()),
                    "truncated file must not load geometry"
                ),
                Err(e) => assert!(e.starts_with("fbx:"), "{e}"),
            }
        }
        let bad = [&data[..40], &[255u8; 64], &data[104..]].concat();
        std::fs::write(&path, bad).unwrap();
        assert!(
            load(p).is_err_and(|e| e.starts_with("fbx:")),
            "garbage node header is rejected"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
