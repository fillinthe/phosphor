//! 3D models as points: a layer whose source is `Live { format: "model", url: path }` shows the model's vertices
//! as a point picture (one texel a vertex, three in a row a triangle; see the engine's points.wgsl), read from the
//! OBJ file once and uploaded as 16-bit float. Points: Dots / Lines / Mesh draw them with "Read: In order (a model)".

use std::collections::HashMap;

use phosphor_engine::{Composition, Engine, SourceSpec};

/// The live-source format that marks a model layer (the engine keeps such a layer's pictures float).
pub const FORMAT: &str = phosphor_engine::shaders::MODEL_POINTS;
/// Texels a row (a model of 300,000 vertices is 1024 x 293).
const WIDTH: u32 = 1024;

/// A layer source for the model file at `path`.
pub fn source(path: &str) -> SourceSpec {
    let file = std::path::Path::new(path).file_name().map_or_else(|| path.to_string(), |f| f.to_string_lossy().to_string());
    SourceSpec::Live { name: format!("Model · {file}"), format: FORMAT.into(), url: path.into() }
}

/// Every model layer's upload (by the path uploaded, so a changed file goes up again).
#[derive(Default)]
pub struct Models {
    uploaded: HashMap<u64, String>,
}

impl Models {
    /// Uploads the vertices of any model layer that hasn't got them yet. Returns what went wrong, per layer.
    pub fn update(&mut self, comp: &Composition, engine: &mut Engine) -> Vec<(u64, String)> {
        let wanted: Vec<(u64, &str)> = comp
            .layers
            .iter()
            .filter_map(|l| match &l.source {
                SourceSpec::Live { format, url, .. } if format == FORMAT => Some((l.id, url.as_str())),
                _ => None,
            })
            .collect();
        self.uploaded.retain(|id, p| wanted.iter().any(|(w, path)| w == id && path == p));
        let mut errors = Vec::new();
        for (id, path) in wanted {
            if self.uploaded.contains_key(&id) {
                continue;
            }
            self.uploaded.insert(id, path.to_string());
            match phosphor_engine::mapping::obj_mesh(path).as_ref() {
                Ok(mesh) => {
                    let (cols, rows, bytes) = texels(mesh);
                    let texture = engine.device().create_texture(&wgpu::TextureDescriptor {
                        label: Some("model points"),
                        size: wgpu::Extent3d { width: cols, height: rows, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba16Float,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
                        view_formats: &[],
                    });
                    engine.queue().write_texture(
                        wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                        &bytes,
                        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(cols * 8), rows_per_image: Some(rows) },
                        wgpu::Extent3d { width: cols, height: rows, depth_or_array_layers: 1 },
                    );
                    engine.set_layer_texture(id, texture);
                }
                Err(e) => errors.push((id, format!("Couldn't read the model: {e}"))),
            }
        }
        errors
    }
}

/// A model's triangles as point texels: (columns, rows, 16-bit float RGBA bytes), a vertex a texel, in order, the
/// place in rgb (the model fits -0.5..0.5) and 1 in alpha; the rest of the last row is empty (not drawn).
pub fn texels(mesh: &phosphor_engine::mapping::ObjMesh) -> (u32, u32, Vec<u8>) {
    let n = (mesh.tris.len() * 3).max(1) as u32;
    let cols = n.min(WIDTH).max(1);
    let rows = n.div_ceil(cols).max(1);
    let mut bytes = Vec::with_capacity((cols * rows * 8) as usize);
    for tri in &mesh.tris {
        for (p, _) in tri {
            for v in [p[0], p[1], p[2], 1.0] {
                bytes.extend_from_slice(&f16(v).to_le_bytes());
            }
        }
    }
    bytes.resize((cols * rows * 8) as usize, 0);
    (cols, rows, bytes)
}

/// A 32-bit float as a 16-bit one (round to nearest; too big = infinity).
pub fn f16(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32;
    let mant = b & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | (((mant != 0) as u16) << 9);
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = (mant | 0x80_0000) >> (1 - e);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    sign | (((e as u32) << 10) + ((mant + 0x1000) >> 13)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_becomes_point_texels() {
        assert_eq!(f16(1.0), 0x3c00);
        assert_eq!(f16(-0.5), 0xb800);
        assert_eq!(f16(0.0), 0);
        assert_eq!(f16(65504.0), 0x7bff);
        assert_eq!(f16(1e6), 0x7c00, "too big: infinity");
        let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
        let mesh = phosphor_engine::mapping::parse_obj(obj).unwrap();
        let (cols, rows, bytes) = texels(&mesh);
        assert_eq!((cols, rows, bytes.len()), (6, 1, 6 * 8), "a square = two triangles = six vertices");
        let x0 = u16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(x0, f16(-0.5), "the first corner, fitted to -0.5..0.5");
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), f16(1.0), "drawn");
        assert_eq!(source("/tmp/x.obj"), SourceSpec::Live { name: "Model · x.obj".into(), format: "model".into(), url: "/tmp/x.obj".into() });
    }
}
