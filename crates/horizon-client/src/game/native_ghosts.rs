//! Ghost blocks a mod hands over (`native_ghosts`, `native_ghost_revision`,
//! `native_ghost_section`): block states drawn see-through in the world,
//! depth-tested, fully lit, at the mod's opacity: a schematic still to be
//! built. Each 16-block section is meshed once from the game's own block
//! models, faces against other ghosts and solid blocks culled, and kept until
//! the mod says the section changed; a frame only copies the kept quads into
//! the entities' translucent geometry, nearest sections first. Blocks with no
//! model (chests, signs: drawn by their block entity) show as a tinted box.
use super::Game;
use super::native_hooks::{NativeGhost, NativeGhosts};
use horizon_client::{
    block_entity_render::vertex_light, gizmos, hud::HudBatch, hud_layout::abgr, mesher::Vertex,
};
use horizon_world::registry::BlockStateId;
use std::collections::HashMap;

/// Sections meshed in a frame at most, nearest first: each is up to 4096 blocks.
const MESHES_PER_FRAME: usize = 2;
/// Quads kept over every section at most: past it, farther sections wait.
const MAX_QUADS: usize = 1 << 16;
/// Frames a section may go undrawn before its mesh is let go.
const KEEP_FRAMES: u64 = 60;
/// A modelless block's box colour, before the opacity: a pale blue.
const BOX_RGB: u32 = 0x88CCFF;
/// Boxes a mod may fill in a frame, at most.
const MAX_FILLS: usize = 1024;

/// One section's ghosts: vertices relative to its lowest corner, opaque.
struct Mesh {
    revision: u64,
    drawn_at: u64,
    vertices: Vec<Vertex>,
}

/// The kept meshes, and what they were made for.
#[derive(Default)]
pub(super) struct GhostMeshes {
    meshes: HashMap<[i32; 3], Mesh>,
    /// The mod's source, the dimension and the atlas: a change drops every mesh.
    made_for: Option<(u64, &'static str, usize, usize)>,
    frame: u64,
    quads: usize,
    /// Scratch: a section's ghost states by `y << 8 | z << 4 | x`.
    cells: Vec<Option<BlockStateId>>,
    listed: Vec<NativeGhost>,
}

impl Game<'_> {
    /// The ghosts a mod asked for this frame, into the entities' geometry
    /// (camera-relative, translucent). Called while a world is drawn.
    pub(super) fn mod_ghost_geometry(&mut self, camera: glam::DVec3, out: &mut Vec<Vertex>) {
        let mut ghosts = NativeGhosts::default();
        self.native_ghosts(&mut ghosts);
        let fills: Vec<_> = std::mem::take(&mut ghosts.fills)
            .into_iter()
            .take(MAX_FILLS)
            .collect();
        let start = out.len();
        self.mark_translucent_entity(start, "");
        self.mod_fill_geometry(camera, &fills, out);
        let mut cache = std::mem::take(&mut self.ghost_meshes);
        self.ghost_frame(&mut cache, &ghosts, camera, out);
        self.ghost_meshes = cache;
    }

    fn ghost_frame(
        &self,
        cache: &mut GhostMeshes,
        ghosts: &NativeGhosts,
        camera: glam::DVec3,
        out: &mut Vec<Vertex>,
    ) {
        let Some((min, max)) = ghosts
            .bounds
            .filter(|_| ghosts.alpha > 0 && ghosts.range > 0)
        else {
            cache.meshes.clear();
            cache.quads = 0;
            cache.made_for = None;
            return;
        };
        let atlas = &self.gui.atlas;
        let made_for = (
            ghosts.source,
            self.dim.id(),
            atlas.pixels.as_ptr() as usize,
            atlas.sprites.len(),
        );
        if cache.made_for != Some(made_for) {
            cache.meshes.clear();
            cache.quads = 0;
            cache.made_for = Some(made_for);
        }
        cache.frame += 1;
        let range = ghosts.range.min(128) as i32;
        let cam = [camera.x, camera.y, camera.z];
        let c = cam.map(|v| (v.floor() as i32) >> 4);
        let reach = (range >> 4) + 1;
        let lo: [i32; 3] = std::array::from_fn(|i| (min[i] >> 4).max(c[i] - reach));
        let hi: [i32; 3] = std::array::from_fn(|i| ((max[i] - 1) >> 4).min(c[i] + reach));
        if (0..3).any(|i| lo[i] > hi[i]) {
            return;
        }
        let rings = (0..3)
            .map(|i| (c[i] - lo[i]).abs().max((hi[i] - c[i]).abs()))
            .max()
            .unwrap_or(0);
        let range_sq = f64::from(range) * f64::from(range);
        let alpha = u32::from(ghosts.alpha);
        let mut meshed = 0;
        // Ring by ring outward, so the nearest sections are meshed first.
        for ring in 0..=rings {
            for sy in lo[1].max(c[1] - ring)..=hi[1].min(c[1] + ring) {
                for sz in lo[2].max(c[2] - ring)..=hi[2].min(c[2] + ring) {
                    for sx in lo[0].max(c[0] - ring)..=hi[0].min(c[0] + ring) {
                        let edge = (sx - c[0])
                            .abs()
                            .max((sy - c[1]).abs())
                            .max((sz - c[2]).abs());
                        if edge != ring || section_distance_sq([sx, sy, sz], cam) > range_sq {
                            continue;
                        }
                        let key = [sx, sy, sz];
                        let mut revision = 0;
                        self.native_ghost_revision(key, &mut revision);
                        let stale = cache
                            .meshes
                            .get(&key)
                            .is_none_or(|m| m.revision != revision);
                        if stale && meshed < MESHES_PER_FRAME && cache.quads < MAX_QUADS {
                            let old = cache
                                .meshes
                                .remove(&key)
                                .map_or(0, |m| m.vertices.len() / 4);
                            cache.quads -= old;
                            let vertices = self.mesh_ghost_section(cache, key);
                            cache.quads += vertices.len() / 4;
                            cache.meshes.insert(
                                key,
                                Mesh {
                                    revision,
                                    drawn_at: 0,
                                    vertices,
                                },
                            );
                            meshed += 1;
                        }
                        let Some(mesh) = cache.meshes.get_mut(&key) else {
                            continue;
                        };
                        mesh.drawn_at = cache.frame;
                        let origin =
                            (glam::DVec3::new(f64::from(sx), f64::from(sy), f64::from(sz)) * 16.0
                                - camera)
                                .as_vec3();
                        // Each vertex's own alpha (a box's half) times the mod's.
                        out.extend(mesh.vertices.iter().map(|v| Vertex {
                            pos: (glam::Vec3::from_array(v.pos) + origin).to_array(),
                            color: (v.color & 0x00FF_FFFF) | ((v.color >> 24) * alpha / 255) << 24,
                            ..*v
                        }));
                    }
                }
            }
        }
        // Sections out of view for a while are let go.
        let frame = cache.frame;
        let mut freed = 0;
        cache.meshes.retain(|_, m| {
            let keep = frame - m.drawn_at <= KEEP_FRAMES;
            if !keep {
                freed += m.vertices.len() / 4;
            }
            keep
        });
        cache.quads -= freed;
    }

    /// One section's ghosts as quads relative to its corner, at full
    /// opacity: the frame multiplies in the mod's.
    fn mesh_ghost_section(&self, cache: &mut GhostMeshes, section: [i32; 3]) -> Vec<Vertex> {
        cache.listed.clear();
        self.native_ghost_section(section, &mut cache.listed);
        cache.cells.clear();
        cache.cells.resize(4096, None);
        for g in &cache.listed {
            if usize::from(g.at) < 4096 && g.state != self.world.air {
                cache.cells[usize::from(g.at)] = Some(g.state);
            }
        }
        let base = section.map(|v| v << 4);
        let facts = self.res.facts();
        // A neighbour hides a face when it's solid: a ghost in this section,
        // or a block of the world anywhere else.
        let solid_at = |x: i32, y: i32, z: i32| -> bool {
            let (lx, ly, lz) = (x - base[0], y - base[1], z - base[2]);
            if (0..16).contains(&lx)
                && (0..16).contains(&ly)
                && (0..16).contains(&lz)
                && let Some(s) = cache.cells[(ly << 8 | lz << 4 | lx) as usize]
            {
                return facts.get(s).solid_render;
            }
            self.world.loaded(x, z) && facts.get(self.world.block(x, y, z)).solid_render
        };
        let light = vertex_light(0xFF);
        let mut out = Vec::new();
        let mut white = HudBatch::new([1, 1]);
        for at in 0..4096usize {
            let Some(state) = cache.cells[at] else {
                continue;
            };
            let (lx, ly, lz) = ((at & 15) as i32, (at >> 8) as i32, ((at >> 4) & 15) as i32);
            let (x, y, z) = (base[0] + lx, base[1] + ly, base[2] + lz);
            let seed =
                horizon_core::random::position_seed(horizon_core::position::BlockPos::new(x, y, z));
            let mut modelled = false;
            if let Ok(models) = self.pack.render.model_catalog.collect_seeded(state, seed) {
                let m =
                    glam::Mat4::from_translation(glam::Vec3::new(lx as f32, ly as f32, lz as f32));
                let grass = self.pack.item_renderer.grass_color(0.5, 1.0) as u32;
                let tables = &self.pack.context.tables;
                for (_, model) in models {
                    for face in &model.faces {
                        modelled = true;
                        if let Some(cull) = face.quad.cull {
                            let [dx, dy, dz] = direction_offset(cull);
                            if solid_at(x + dx, y + dy, z + dz) {
                                continue;
                            }
                        }
                        self.ghost_face(
                            face,
                            m,
                            light,
                            |i| tables.default_tint(state, i, grass),
                            &mut out,
                        );
                    }
                }
            }
            if !modelled
                && facts.get(state).render_shape != horizon_client::blocks::RenderShape::Invisible
            {
                // No model to draw: a tinted box, a little inside the block.
                white.vertices.clear();
                white.fill(
                    &self.gui.atlas,
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                    abgr(0x8000_0000 | BOX_RGB),
                );
                let Some(w) = white.vertices.first().copied() else {
                    continue;
                };
                let (a, b) = ([0.05; 3], [0.95; 3]);
                let lo = [
                    f64::from(lx) + a[0],
                    f64::from(ly) + a[1],
                    f64::from(lz) + a[2],
                ];
                let hi = [
                    f64::from(lx) + b[0],
                    f64::from(ly) + b[1],
                    f64::from(lz) + b[2],
                ];
                for quad in gizmos::cuboid_fill_quads(lo, hi) {
                    for corner in quad {
                        out.push(Vertex {
                            normal: [0; 4],
                            pos: corner.map(|v| v as f32),
                            uv: w.uv,
                            layer: 0,
                            color: w.color,
                            light,
                            glint_uv: [0.0, 0.0],
                        });
                    }
                }
            }
        }
        out
    }

    /// One baked face, unlit but for its face shade, back faces dropped.
    fn ghost_face(
        &self,
        face: &horizon_client::models::catalog::BakedFace,
        m: glam::Mat4,
        light: u32,
        tint: impl Fn(i32) -> Option<u32>,
        out: &mut Vec<Vertex>,
    ) {
        let Some(s) = self
            .pack
            .layer_names
            .get(&face.layer)
            .and_then(|n| self.gui.atlas.sprites.get(n))
        else {
            return;
        };
        use horizon_assets::model::Direction as D;
        let shade: u32 = match face.quad.facing {
            D::Up => 255,
            D::Down => 127,
            D::North | D::South => 204,
            D::West | D::East => 153,
        };
        let mut color = 0xFF00_0000 | shade << 16 | shade << 8 | shade;
        if let Some(t) = tint(face.quad.tint_index) {
            color =
                horizon_client::hud_layout::multiply(color, horizon_client::mobs::render::abgr(t));
        }
        for k in 0..4 {
            let uv = face.quad.uv[k];
            out.push(Vertex {
                normal: horizon_client::mesher::CULL_BACK,
                pos: m
                    .transform_point3(glam::Vec3::from_array(face.quad.positions[k]))
                    .to_array(),
                uv: [s.u0 + uv[0] * (s.u1 - s.u0), s.v0 + uv[1] * (s.v1 - s.v0)],
                layer: 0,
                color,
                light,
                glint_uv: [0.0, 0.0],
            });
        }
    }

    /// Boxes a mod filled (`NativeGhosts::fills`): see-through faces, depth-tested.
    fn mod_fill_geometry(
        &self,
        camera: glam::DVec3,
        fills: &[super::native_hooks::NativeFill],
        out: &mut Vec<Vertex>,
    ) {
        if fills.is_empty() {
            return;
        }
        let light = vertex_light(0xFF);
        let mut white = HudBatch::new([1, 1]);
        for f in fills {
            if f.argb >> 24 == 0 || !f.min.iter().chain(&f.max).all(|v| v.is_finite()) {
                continue;
            }
            white.vertices.clear();
            white.fill(&self.gui.atlas, 0.0, 0.0, 1.0, 1.0, abgr(f.argb));
            let Some(w) = white.vertices.first().copied() else {
                continue;
            };
            for quad in gizmos::cuboid_fill_quads(f.min, f.max) {
                for corner in quad {
                    out.push(Vertex {
                        normal: [0; 4],
                        pos: (glam::DVec3::from_array(corner) - camera)
                            .as_vec3()
                            .to_array(),
                        uv: w.uv,
                        layer: 0,
                        color: w.color,
                        light,
                        glint_uv: [0.0, 0.0],
                    });
                }
            }
        }
    }
}

/// The step to the neighbour a face looks at.
fn direction_offset(d: horizon_assets::model::Direction) -> [i32; 3] {
    use horizon_assets::model::Direction as D;
    match d {
        D::Up => [0, 1, 0],
        D::Down => [0, -1, 0],
        D::North => [0, 0, -1],
        D::South => [0, 0, 1],
        D::West => [-1, 0, 0],
        D::East => [1, 0, 0],
    }
}

/// The squared distance from `at` to the nearest point of section `s`.
fn section_distance_sq(s: [i32; 3], at: [f64; 3]) -> f64 {
    (0..3)
        .map(|i| {
            let lo = f64::from(s[i] << 4);
            let d = (lo - at[i]).max(at[i] - lo - 16.0).max(0.0);
            d * d
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_are_measured_from_their_nearest_point() {
        assert_eq!(section_distance_sq([0, 0, 0], [8.0, 8.0, 8.0]), 0.0);
        assert_eq!(section_distance_sq([1, 0, 0], [8.0, 8.0, 8.0]), 64.0);
        assert_eq!(section_distance_sq([-1, 0, 0], [0.0, 8.0, 8.0]), 0.0);
        assert_eq!(
            direction_offset(horizon_assets::model::Direction::North),
            [0, 0, -1]
        );
    }
}

/// The block-state and file helpers' bodies: the hook points stay free of `cfg`s and macros.
impl super::Game<'_> {
    pub(super) fn ghost_state_is_air(&self, state: BlockStateId) -> bool {
        matches!(
            &*self.res.facts().get(state).name,
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        )
    }
    pub(super) fn ghost_state_text(&self, state: BlockStateId) -> String {
        let Some(s) = self.pack.assets.registry.state(state) else {
            return String::new();
        };
        if s.properties.is_empty() {
            return s.name.clone();
        }
        let props: Vec<String> = s
            .properties
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        format!("{}[{}]", s.name, props.join(","))
    }
    pub(super) fn ghost_pick_file(&mut self, kind: &str, accept: &str) {
        #[cfg(target_arch = "wasm32")]
        horizon_os::web::pick_file(kind, accept);
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (kind, accept);
    }
    pub(super) fn ghost_take_picked_file(&mut self, kind: &str) -> Option<(String, Vec<u8>)> {
        #[cfg(target_arch = "wasm32")]
        {
            horizon_os::web::take_picked_named(kind)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = kind;
            None
        }
    }
}
