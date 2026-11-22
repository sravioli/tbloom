// Binary FBX (7.x) loader: parses the node tree with bounds checks (the file is untrusted
// input), bakes the model hierarchy's transforms, triangulates polygons and groups triangles
// by material. Returns groups for scene/model.rs to turn into meshes.
use std::collections::HashMap;

const MAX_ARRAY: u64 = 256 * 1024 * 1024; // bytes; refuse arrays a corrupt header claims are huge
const MAX_DEPTH: usize = 256; // node nesting; real files use < 10

#[derive(Clone, Debug)]
pub enum Prop {
    /// Y, I, F, D
    Num(f64),
    /// C (bool): parsed to keep property positions, never read
    Other,
    /// L: ids can exceed 2^53, so they stay integers
    I64(i64),
    /// S, R
    Str(String),
    // shortcut: int64 arrays lose precision past 2^53; none of the arrays read here are int64
    Arr(Vec<f64>),
}

impl Prop {
    fn num(&self) -> Option<f64> {
        if let Prop::Num(x) = self {
            Some(*x)
        } else {
            None
        }
    }
    fn str(&self) -> Option<&str> {
        if let Prop::Str(s) = self {
            Some(s)
        } else {
            None
        }
    }
    fn arr(&self) -> Option<&[f64]> {
        if let Prop::Arr(a) = self {
            Some(a)
        } else {
            None
        }
    }
    fn id(&self) -> Option<i64> {
        if let Prop::I64(i) = self {
            Some(*i)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct Node {
    pub name: String,
    pub props: Vec<Prop>,
    pub kids: Vec<Node>,
}

fn fail<T>(msg: &str) -> Result<T, String> {
    Err(format!("fbx: {msg}"))
}

struct Reader<'a> {
    d: &'a [u8],
    big: bool,
}

impl Reader<'_> {
    fn need(&self, off: u64, n: u64) -> Result<(), String> {
        match off.checked_add(n) {
            Some(e) if e <= self.d.len() as u64 => Ok(()),
            _ => fail("truncated or corrupt file"),
        }
    }
    fn bytes(&self, off: u64, n: u64) -> Result<&[u8], String> {
        self.need(off, n)?;
        Ok(&self.d[off as usize..(off + n) as usize])
    }
    fn take(&self, off: &mut u64, n: u64) -> Result<&[u8], String> {
        let b = self.bytes(*off, n)?;
        *off += n;
        Ok(b)
    }
    fn u32(&self, off: u64) -> Result<u64, String> {
        Ok(u32::from_le_bytes(self.bytes(off, 4)?.try_into().unwrap()) as u64)
    }
    fn u64(&self, off: u64) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.bytes(off, 8)?.try_into().unwrap()))
    }

    fn array(&self, t: u8, off: &mut u64) -> Result<Vec<f64>, String> {
        let (n, enc, clen) = (self.u32(*off)?, self.u32(*off + 4)?, self.u32(*off + 8)?);
        *off += 12;
        let es = match t {
            b'f' | b'i' => 4,
            b'd' | b'l' => 8,
            _ => 1,
        };
        if n * es > MAX_ARRAY {
            return fail("array too large");
        }
        let src = self.take(off, clen)?;
        let raw = match enc {
            0 if clen == n * es => src.to_vec(),
            0 => return fail("bad array size"),
            1 => match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
                src,
                (n * es) as usize,
            ) {
                Ok(v) if v.len() as u64 == n * es => v,
                _ => return fail("bad compressed array"),
            },
            _ => return fail("unknown array encoding"),
        };
        let c = raw.chunks_exact(es as usize);
        Ok(match t {
            b'f' => c
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64)
                .collect(),
            b'd' => c
                .map(|b| f64::from_le_bytes(b.try_into().unwrap()))
                .collect(),
            b'l' => c
                .map(|b| i64::from_le_bytes(b.try_into().unwrap()) as f64)
                .collect(),
            b'i' => c
                .map(|b| i32::from_le_bytes(b.try_into().unwrap()) as f64)
                .collect(),
            _ => c.map(|b| b[0] as f64).collect(),
        })
    }

    /// One node at off; None for the null record that ends a child list. Returns the end offset.
    fn node(&self, off: u64, limit: u64, depth: usize) -> Result<(Option<Node>, u64), String> {
        if depth > MAX_DEPTH {
            return fail("nodes nested too deep");
        }
        let hdr = if self.big { 25 } else { 13 };
        self.need(off, hdr)?;
        let (endo, nprops, plen, nlen) = if self.big {
            (
                self.u64(off)?,
                self.u64(off + 8)?,
                self.u64(off + 16)?,
                self.d[off as usize + 24],
            )
        } else {
            (
                self.u32(off)?,
                self.u32(off + 4)?,
                self.u32(off + 8)?,
                self.d[off as usize + 12],
            )
        };
        let mut off = off + hdr;
        if endo == 0 {
            return Ok((None, off));
        }
        if endo > limit || endo < off {
            return fail("bad node bounds");
        }
        let name = String::from_utf8_lossy(self.bytes(off, nlen as u64)?).into_owned();
        off += nlen as u64;
        let pend = match off.checked_add(plen) {
            Some(p) if p <= endo => p,
            _ => return fail("bad property bounds"),
        };
        let mut props = Vec::new();
        for _ in 0..nprops {
            let t = self.bytes(off, 1)?[0];
            off += 1;
            let v = match t {
                b'Y' => Prop::Num(
                    i16::from_le_bytes(self.take(&mut off, 2)?.try_into().unwrap()) as f64,
                ),
                b'C' => self.take(&mut off, 1).map(|_| Prop::Other)?,
                b'I' => Prop::Num(
                    i32::from_le_bytes(self.take(&mut off, 4)?.try_into().unwrap()) as f64,
                ),
                b'F' => Prop::Num(
                    f32::from_le_bytes(self.take(&mut off, 4)?.try_into().unwrap()) as f64,
                ),
                b'D' => Prop::Num(f64::from_le_bytes(
                    self.take(&mut off, 8)?.try_into().unwrap(),
                )),
                b'L' => Prop::I64(i64::from_le_bytes(
                    self.take(&mut off, 8)?.try_into().unwrap(),
                )),
                b'S' | b'R' => {
                    let n = self.u32(off)?;
                    off += 4;
                    Prop::Str(String::from_utf8_lossy(self.take(&mut off, n)?).into_owned())
                }
                b'f' | b'd' | b'l' | b'i' | b'b' => Prop::Arr(self.array(t, &mut off)?),
                _ => return fail(&format!("unknown property type {}", t as char)),
            };
            props.push(v);
        }
        off = pend;
        let mut kids = Vec::new();
        while off < endo {
            let (kid, next) = self.node(off, endo, depth + 1)?;
            off = next;
            match kid {
                Some(k) => kids.push(k),
                None => break,
            }
        }
        Ok((Some(Node { name, props, kids }), endo))
    }
}

/// Parse the binary node tree (top-level nodes).
pub fn parse(data: &[u8]) -> Result<Vec<Node>, String> {
    if data.len() < 27 || !data.starts_with(b"Kaydara FBX Binary  \0") {
        return fail("not a binary FBX file");
    }
    let mut r = Reader {
        d: data,
        big: false,
    };
    r.big = r.u32(23)? >= 7500; // 7.5+ uses 64-bit node headers
    let (hdr, len) = (if r.big { 25 } else { 13 }, data.len() as u64);
    let (mut top, mut off) = (Vec::new(), 27);
    while off + hdr <= len {
        // top level ends with a null record, then a footer
        let (n, next) = r.node(off, len, 0)?;
        off = next;
        match n {
            Some(n) => top.push(n),
            None => break,
        }
    }
    Ok(top)
}

fn child<'a>(n: &'a Node, name: &str) -> Option<&'a Node> {
    n.kids.iter().find(|k| k.name == name)
}
/// First property of child `name`.
fn child_prop<'a>(n: &'a Node, name: &str) -> Option<&'a Prop> {
    child(n, name).and_then(|c| c.props.first())
}

pub type Props70 = HashMap<String, Vec<Prop>>;
fn props70(n: &Node) -> Props70 {
    let mut t = Props70::new();
    for k in child(n, "Properties70").map_or(&[][..], |p| &p.kids) {
        if let Some(name) = k.props.first().and_then(Prop::str) {
            t.insert(name.to_string(), k.props.clone());
        }
    }
    t
}
fn vec3(p: &Props70, key: &str, d: f64) -> (f64, f64, f64) {
    let n = |i: usize| p.get(key)?.get(i)?.num(); // P: name, type, label, flags, x, y, z
    match (n(4), n(5), n(6)) {
        (Some(x), Some(y), Some(z)) => (x, y, z),
        _ => (d, d, d),
    }
}

/// 4x4 row-major, column-vector convention: p' = M p.
type M4 = [f64; 16];
fn mul(a: &M4, b: &M4) -> M4 {
    let mut r = [0.0; 16];
    for i in 0..4 {
        for j in 0..4 {
            r[i * 4 + j] = (0..4).map(|k| a[i * 4 + k] * b[k * 4 + j]).sum();
        }
    }
    r
}
fn tr((x, y, z): (f64, f64, f64)) -> M4 {
    [1., 0., 0., x, 0., 1., 0., y, 0., 0., 1., z, 0., 0., 0., 1.]
}
fn sc((x, y, z): (f64, f64, f64)) -> M4 {
    [x, 0., 0., 0., 0., y, 0., 0., 0., 0., z, 0., 0., 0., 0., 1.]
}
fn axis(ax: u8, deg: f64) -> M4 {
    let a = deg.to_radians();
    let (c, s) = (a.cos(), a.sin());
    match ax {
        b'X' => [1., 0., 0., 0., 0., c, -s, 0., 0., s, c, 0., 0., 0., 0., 1.],
        b'Y' => [c, 0., s, 0., 0., 1., 0., 0., -s, 0., c, 0., 0., 0., 0., 1.],
        _ => [c, -s, 0., 0., s, c, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.],
    }
}
/// Euler rotation in degrees; "XYZ" applies X first, so the matrix is Rz * Ry * Rx.
fn euler((x, y, z): (f64, f64, f64), order: Option<f64>) -> M4 {
    // 6 = spheric, treated as XYZ
    let o: &[u8; 3] = match order.map(|o| o as i64) {
        Some(1) => b"XZY",
        Some(2) => b"YZX",
        Some(3) => b"YXZ",
        Some(4) => b"ZXY",
        Some(5) => b"ZYX",
        _ => b"XYZ",
    };
    let ang = |a: u8| match a {
        b'X' => x,
        b'Y' => y,
        _ => z,
    };
    mul(
        &axis(o[2], ang(o[2])),
        &mul(&axis(o[1], ang(o[1])), &axis(o[0], ang(o[0]))),
    )
}
fn transpose3(m: &M4) -> M4 {
    // inverse of a pure rotation
    [
        m[0], m[4], m[8], 0., m[1], m[5], m[9], 0., m[2], m[6], m[10], 0., 0., 0., 0., 1.,
    ]
}

/// Model's local matrix per the FBX SDK:
/// T * Roff * Rp * Rpre * R * Rpost^-1 * Rp^-1 * Soff * Sp * S * Sp^-1
pub fn local_matrix(p: &Props70) -> M4 {
    let (rpx, rpy, rpz) = vec3(p, "RotationPivot", 0.0);
    let (spx, spy, spz) = vec3(p, "ScalingPivot", 0.0);
    let order = p
        .get("RotationOrder")
        .and_then(|o| o.get(4))
        .and_then(Prop::num);
    let mut m = tr(vec3(p, "Lcl Translation", 0.0));
    m = mul(&m, &tr(vec3(p, "RotationOffset", 0.0)));
    m = mul(&m, &tr((rpx, rpy, rpz)));
    m = mul(&m, &euler(vec3(p, "PreRotation", 0.0), None));
    m = mul(&m, &euler(vec3(p, "Lcl Rotation", 0.0), order));
    m = mul(&m, &transpose3(&euler(vec3(p, "PostRotation", 0.0), None)));
    m = mul(&m, &tr((-rpx, -rpy, -rpz)));
    m = mul(&m, &tr(vec3(p, "ScalingOffset", 0.0)));
    m = mul(&m, &tr((spx, spy, spz)));
    m = mul(&m, &sc(vec3(p, "Lcl Scaling", 1.0)));
    mul(&m, &tr((-spx, -spy, -spz)))
}

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

struct Model {
    p: Props70,
    mats: Vec<i64>,
}

/// Load a binary FBX file: triangles grouped by material, positions in the file's world space.
pub fn load(path: &str) -> Result<Vec<Group>, String> {
    let data = std::fs::read(path).map_err(|_| format!("cannot open {path}"))?;
    let top = parse(&data)?;
    let root = |name: &str| top.iter().find(|n| n.name == name);
    let (Some(objects), Some(connections)) = (root("Objects"), root("Connections")) else {
        return fail("no Objects/Connections");
    };

    let (mut models, mut order) = (HashMap::new(), Vec::new());
    let (mut geoms, mut mats, mut texs) = (HashMap::new(), HashMap::new(), HashMap::new());
    for o in &objects.kids {
        let Some(oid) = o.props.first().and_then(Prop::id) else {
            continue;
        };
        match o.name.as_str() {
            "Model" => {
                models.insert(
                    oid,
                    Model {
                        p: props70(o),
                        mats: Vec::new(),
                    },
                );
                order.push(oid);
            }
            "Geometry" => {
                geoms.insert(oid, o);
            }
            "Material" => {
                let p = props70(o);
                let key = if p.contains_key("DiffuseColor") {
                    "DiffuseColor"
                } else {
                    "Diffuse"
                };
                let (r, g, b) = vec3(&p, key, 1.0);
                mats.insert(
                    oid,
                    Material {
                        color: [r, g, b],
                        texture: None,
                    },
                );
            }
            "Texture" => {
                let f = child(o, "RelativeFilename").or_else(|| child(o, "FileName"));
                if let Some(name) = f.and_then(|f| f.props.first()).and_then(Prop::str) {
                    texs.insert(oid, name.to_string());
                }
            }
            _ => {}
        }
    }
    // Connections: OO child -> parent object; OP child -> parent property.
    let (mut geom_of, mut parent_of) = (HashMap::new(), HashMap::new());
    for c in &connections.kids {
        let (Some(kind), Some(ch), Some(pa)) = (
            c.props.first().and_then(Prop::str),
            c.props.get(1).and_then(Prop::id),
            c.props.get(2).and_then(Prop::id),
        ) else {
            continue;
        };
        if kind == "OO" && models.contains_key(&pa) {
            if geoms.contains_key(&ch) {
                geom_of.insert(pa, ch);
            } else if models.contains_key(&ch) {
                parent_of.insert(ch, pa);
            } else if mats.contains_key(&ch) {
                models.get_mut(&pa).unwrap().mats.push(ch); // file order = material index
            }
        } else if kind == "OP" && texs.contains_key(&ch) {
            let prop = c.props.get(3).and_then(Prop::str);
            if let Some(m) = mats
                .get_mut(&pa)
                .filter(|m| prop == Some("DiffuseColor") || m.texture.is_none())
            {
                m.texture = Some(texs[&ch].clone());
            }
        }
    }
    // world matrix: the model's local one under its ancestors'
    let world = |mut mid: i64| -> Result<M4, String> {
        let mut m = local_matrix(&models[&mid].p);
        for _ in 0..=256 {
            let Some(&pa) = parent_of.get(&mid) else {
                return Ok(m);
            };
            m = mul(&local_matrix(&models[&pa].p), &m);
            mid = pa;
        }
        fail("model hierarchy loops")
    };

    let (mut groups, mut by_mat): (Vec<Group>, HashMap<Option<i64>, usize>) =
        (Vec::new(), HashMap::new());
    for mid in order {
        let Some(gid) = geom_of.get(&mid) else {
            continue;
        };
        let (mdl, p) = (&models[&mid], &models[&mid].p);
        let geo = mul(
            &tr(vec3(p, "GeometricTranslation", 0.0)),
            &mul(
                &euler(vec3(p, "GeometricRotation", 0.0), None),
                &sc(vec3(p, "GeometricScaling", 1.0)),
            ),
        );
        polygons(geoms[gid], &mul(&world(mid)?, &geo), |slot, poly| {
            let mat = usize::try_from(slot)
                .ok()
                .and_then(|i| mdl.mats.get(i))
                .or(mdl.mats.first())
                .copied();
            let gi = *by_mat.entry(mat).or_insert_with(|| {
                let material = mat.map_or(
                    Material {
                        color: [0.8; 3],
                        texture: None,
                    },
                    |m| mats[&m].clone(),
                );
                groups.push(Group {
                    material,
                    tris: Vec::new(),
                });
                groups.len() - 1
            });
            for t in 1..poly.len().saturating_sub(1) {
                groups[gi].tris.push([poly[0], poly[t], poly[t + 1]]); // fan triangulation
            }
        })?;
    }
    Ok(groups)
}

/// Calls emit(material slot, corners) for each polygon of geometry node g, positions
/// transformed by w.
fn polygons(g: &Node, w: &M4, mut emit: impl FnMut(i64, &[Corner])) -> Result<(), String> {
    let (Some(verts), Some(pvi)) = (
        child_prop(g, "Vertices").and_then(Prop::arr),
        child_prop(g, "PolygonVertexIndex").and_then(Prop::arr),
    ) else {
        return Ok(());
    };
    let layer = |l: &str, name: &str| child(g, l).and_then(|l| child_prop(l, name));
    let uv = layer("LayerElementUV", "UV").and_then(Prop::arr);
    let uvidx = layer("LayerElementUV", "UVIndex").and_then(Prop::arr);
    let uvmap = layer("LayerElementUV", "MappingInformationType").and_then(Prop::str);
    let uvref = layer("LayerElementUV", "ReferenceInformationType").and_then(Prop::str);
    let mlist = layer("LayerElementMaterial", "Materials").and_then(Prop::arr);
    let mmap = layer("LayerElementMaterial", "MappingInformationType").and_then(Prop::str);
    let ncp = verts.len() / 3;
    // world position of control point i (validated index)
    let pos = |i: i64| -> Result<(f64, f64, f64), String> {
        if i < 0 || i as usize >= ncp {
            return fail("vertex index out of range");
        }
        let i = i as usize;
        let (x, y, z) = (verts[i * 3], verts[i * 3 + 1], verts[i * 3 + 2]);
        Ok((
            w[0] * x + w[1] * y + w[2] * z + w[3],
            w[4] * x + w[5] * y + w[6] * z + w[7],
            w[8] * x + w[9] * y + w[10] * z + w[11],
        ))
    };
    let texcoord = |corner: usize, cp: i64| -> Option<(f64, f64)> {
        let uv = uv?;
        let mut k = if uvmap == Some("ByPolygonVertex") {
            corner as i64
        } else {
            cp
        };
        if uvref == Some("IndexToDirect") {
            k = *uvidx?.get(k as usize)? as i64;
        }
        let k = usize::try_from(k).ok()?;
        Some((*uv.get(k * 2)?, *uv.get(k * 2 + 1)?))
    };
    let (mut poly, mut polyno): (Vec<Corner>, usize) = (Vec::new(), 0);
    for (corner, &raw) in pvi.iter().enumerate() {
        let cp = if raw < 0.0 { -raw - 1.0 } else { raw } as i64; // negative index closes a polygon
        let (x, y, z) = pos(cp)?;
        let (u, v) = texcoord(corner, cp).unwrap_or((0.0, 0.0));
        poly.push([x, y, z, u, v]);
        if raw < 0.0 {
            let k = if mmap == Some("AllSame") { 0 } else { polyno };
            emit(
                mlist.and_then(|ml| ml.get(k)).map_or(0, |&m| m as i64),
                &poly,
            );
            poly.clear();
            polyno += 1;
        }
    }
    Ok(())
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
            "quad fan-triangulates into 2 triangles"
        );
        let t2 = groups[0].tris[1];
        assert!(
            t2[0][0] == 0.0 && t2[1][0] == 1.0 && t2[2][1] == 1.0,
            "second triangle is 0, 2, 3"
        );
        // truncated or corrupted input fails with an error, not a crash
        for cut in [30, 60, data.len() - 300] {
            std::fs::write(&path, &data[..cut]).unwrap();
            match load(p) {
                Ok(g) => assert!(
                    g.is_empty() || g[0].tris.is_empty(),
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

    #[test]
    fn deep_nesting_is_an_error() {
        // a chain of nested nodes past MAX_DEPTH: an error, not a stack overflow
        fn chain(d: usize) -> N {
            N(
                "n",
                vec![],
                if d == 0 { vec![] } else { vec![chain(d - 1)] },
            )
        }
        let mut data = b"Kaydara FBX Binary  \0\x1a\0".to_vec();
        data.extend(7500u32.to_le_bytes());
        let n = node(data.len(), &chain(MAX_DEPTH + 5));
        data.extend(n);
        assert!(parse(&data).is_err_and(|e| e.contains("deep")));
    }

    #[test]
    fn pivot_stays_fixed() {
        // a rotation about a pivot leaves the pivot where it is
        let pv = |x: f64, y: f64, z: f64| {
            let s = || Prop::Str(String::new());
            vec![s(), s(), s(), s(), Prop::Num(x), Prop::Num(y), Prop::Num(z)]
        };
        let p = Props70::from([
            ("RotationPivot".into(), pv(3., 4., 5.)),
            ("Lcl Rotation".into(), pv(30., 45., 60.)),
        ]);
        let m = local_matrix(&p);
        let px = m[0] * 3. + m[1] * 4. + m[2] * 5. + m[3];
        let py = m[4] * 3. + m[5] * 4. + m[6] * 5. + m[7];
        let pz = m[8] * 3. + m[9] * 4. + m[10] * 5. + m[11];
        assert!(
            (px - 3.).abs() + (py - 4.).abs() + (pz - 5.).abs() < 1e-9,
            "pivot stays fixed"
        );
    }
}
