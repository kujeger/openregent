//! The shadow proxies: buildings as 3D models, geometry only, fitted to
//! their sprites by hand (`docs/original/art-camera.md`). None ship: a user
//! folder ([`DIR`] under openregent's data directory) holds them, glTF
//! binary as Blender writes it (`tools/proxy`), read once at start.
//!
//! ```text
//!  units   art pixels about the sprite's anchor, on the ground
//!  axes    X, Y the building's ground axes, Z up; the art camera draws
//!          screen x = (X − Y)/√2, screen y up = (X + Y)/√2 sin 35° + Z cos 35°
//!  glTF    Y up, as Blender exports it: its X, Y, Z are ours X, Z, −Y
//!  folder  <dir>/<CODE>.glb, <CODE>.gltf (its buffers in .bin files
//!          beside it) or <CODE>.obj (Z up); CODE the unit's, e.g. ABV1
//! ```

use std::path::Path;

use bevy::prelude::*;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::SetKind;
use regent_core::sim::damage;

/// The user's model folder's name under the program's data directory.
pub const DIR: &str = "models";

/// One model: its faces as triangles, art pixels about the anchor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub triangles: Vec<[Vec3; 3]>,
}

/// Why a model will not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unread {
    /// An OBJ's line, from 1.
    Line(usize),
    /// What the glTF reader said.
    Gltf(String),
}

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unread::Line(line) => write!(f, "line {line} will not read"),
            Unread::Gltf(why) => write!(f, "{why}"),
        }
    }
}

impl Model {
    /// The OBJ subset the models are written in: `v x y z` and `f a b c
    /// ...`, indices from 1; a face fanned into triangles. Comments,
    /// `o` and anything else are skipped.
    ///
    /// # Errors
    ///
    /// The line of a vertex or face that will not read.
    pub fn parse(text: &str) -> Result<Model, Unread> {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();

        for (n, line) in text.lines().enumerate() {
            let bad = Unread::Line(n + 1);
            let mut words = line.split_whitespace();

            match words.next() {
                Some("v") => {
                    let xyz: Vec<f32> = words
                        .map(str::parse)
                        .collect::<Result<_, _>>()
                        .map_err(|_| bad.clone())?;
                    let [x, y, z] = xyz[..] else { return Err(bad) };

                    vertices.push(Vec3::new(x, y, z));
                }
                Some("f") => {
                    let corners: Vec<Vec3> = words
                        .map(|w| {
                            w.split('/')
                                .next()
                                .and_then(|i| i.parse::<usize>().ok())
                                .and_then(|i| i.checked_sub(1))
                                .and_then(|i| vertices.get(i).copied())
                        })
                        .collect::<Option<_>>()
                        .ok_or_else(|| bad.clone())?;

                    if corners.len() < 3 {
                        return Err(bad);
                    }

                    triangles.extend(
                        (1..corners.len() - 1).map(|i| [corners[0], corners[i], corners[i + 1]]),
                    );
                }
                _ => {}
            }
        }

        Ok(Model { triangles })
    }

    /// A glTF's triangles, every scene's nodes with their transforms, Y
    /// up turned to Z up. `bytes` a `.glb`, or a `.gltf` whose buffers
    /// are files in `beside`.
    ///
    /// # Errors
    ///
    /// What the reader said, or a buffer that is neither inside nor a
    /// file beside.
    pub fn gltf(bytes: &[u8], beside: Option<&Path>) -> Result<Model, Unread> {
        let bad = |e: &dyn std::fmt::Display| Unread::Gltf(e.to_string());
        let read = gltf::Gltf::from_slice(bytes).map_err(|e| bad(&e))?;
        let buffers = read
            .buffers()
            .map(|buffer| match buffer.source() {
                gltf::buffer::Source::Bin => {
                    read.blob.clone().ok_or_else(|| bad(&"no binary chunk"))
                }
                gltf::buffer::Source::Uri(uri) => beside
                    .ok_or_else(|| bad(&"a buffer outside the file"))
                    .and_then(|dir| std::fs::read(dir.join(uri)).map_err(|e| bad(&e))),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut triangles = Vec::new();

        for scene in read.scenes() {
            for node in scene.nodes() {
                gather(&node, Mat4::IDENTITY, &buffers, &mut triangles);
            }
        }

        Ok(Model { triangles })
    }
}

/// `node`'s triangles and its children's, under `parent`'s transform,
/// into `out`; Y up turned to Z up.
fn gather(node: &gltf::Node<'_>, parent: Mat4, buffers: &[Vec<u8>], out: &mut Vec<[Vec3; 3]>) {
    let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
    let upright = |p: Vec3| Vec3::new(p.x, -p.z, p.y);

    for primitive in node.mesh().iter().flat_map(gltf::Mesh::primitives) {
        let triangles = primitive.mode() == gltf::mesh::Mode::Triangles;
        let reader = primitive.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
        let points: Vec<Vec3> = reader
            .read_positions()
            .into_iter()
            .flatten()
            .map(|p| upright(transform.transform_point3(Vec3::from(p))))
            .collect();
        let corners: Vec<usize> = match reader.read_indices() {
            Some(indices) => indices.into_u32().map(|i| i as usize).collect(),
            None => (0..points.len()).collect(),
        };

        out.extend(
            corners
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|_| triangles)
                .filter_map(|c| Some([*points.get(c[0])?, *points.get(c[1])?, *points.get(c[2])?])),
        );
    }

    for child in node.children() {
        gather(&child, transform, buffers, out);
    }
}

/// The model in user file `path`, by its extension; `None` for another.
fn file(path: &Path) -> Option<Result<Model, Unread>> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let bytes = || std::fs::read(path).map_err(|e| Unread::Gltf(e.to_string()));

    match extension.as_str() {
        "glb" | "gltf" => Some(bytes().and_then(|b| Model::gltf(&b, path.parent()))),
        "obj" => Some(bytes().and_then(|b| Model::parse(&String::from_utf8_lossy(&b)))),
        _ => None,
    }
}

/// Every model, by descriptor, in the order read: a model's place is its
/// relief's layer (`sun::relief`).
#[derive(Resource, Debug, Default)]
pub struct Proxies(Vec<(FourCc, Model)>);

impl FromIterator<(FourCc, Model)> for Proxies {
    /// Models in the order given; a later one of a code replaces the
    /// earlier in its place.
    fn from_iter<I: IntoIterator<Item = (FourCc, Model)>>(models: I) -> Self {
        let mut proxies = Proxies::default();

        for (code, model) in models {
            proxies.put(code, model);
        }

        proxies
    }
}

impl Proxies {
    /// The user's folder `dir`'s models: a file named by a unit's code is
    /// that unit's model. A file that will not read is left out, warned
    /// of; no folder is no models.
    ///
    /// Read once, blocking, before the plugins, not by Bevy's asset loader:
    /// - the relief array has a layer per model, and Bevy's bind groups
    ///   miss a resized texture, so the count must be known at start;
    /// - placement and the bake want CPU triangles, not GPU meshes;
    /// - Bevy's glTF loader brings materials, images and scenes, unused;
    /// - the folder lies outside the asset root;
    /// - user files are few and small.
    ///
    /// Async would allow swapping models in play; most of that work is a
    /// relief array whose layer count changes.
    #[must_use]
    pub fn load(dir: Option<&Path>) -> Proxies {
        let mut proxies = Proxies::default();
        let mut paths: Vec<_> = dir
            .and_then(|d| std::fs::read_dir(d).ok())
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();

        // A stable order: a model's place is its relief's layer.
        paths.sort();

        for path in paths {
            let code = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| <[u8; 4]>::try_from(s.as_bytes()).ok())
                .map(|b| FourCc::new(&b));

            if let (Some(code), Some(read)) = (code, file(&path)) {
                match read {
                    Ok(model) => {
                        info!("shadow proxy {code}: {}", path.display());
                        proxies.put(code, model);
                    }
                    Err(e) => warn!("shadow proxy {}: {e}", path.display()),
                }
            }
        }

        proxies
    }

    /// `model` as `code`'s: in its place if it has one, else last.
    fn put(&mut self, code: FourCc, model: Model) {
        match self.0.iter_mut().find(|(c, _)| *c == code) {
            Some((_, kept)) => *kept = model,
            None => self.0.push((code, model)),
        }
    }

    /// How many models.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Proxies {
    /// The model `code` casts with in set `set`: only where the building
    /// draws its finished picture, which the model was fitted to, fires or
    /// not. Under construction, wrecked or ruined it keeps its painted
    /// shadow.
    #[must_use]
    pub fn of(&self, code: FourCc, set: u16) -> Option<&Model> {
        let finished = matches!(
            SetKind::from_id(set),
            SetKind::Inactive | SetKind::Active | SetKind::Hotspot
        ) || (damage::DIE..damage::DIE + damage::UNMARKED).contains(&set);

        finished
            .then(|| self.0.iter().find(|(c, _)| *c == code))
            .flatten()
            .map(|(_, model)| model)
    }

    /// The relief layer of `code`'s model: its place.
    #[must_use]
    pub fn layer(&self, code: FourCc) -> Option<u32> {
        let at = self.0.iter().position(|(c, _)| *c == code)?;

        u32::try_from(at).ok()
    }

    /// Every model, by descriptor.
    pub fn all(&self) -> impl Iterator<Item = (FourCc, &Model)> {
        self.0.iter().map(|(code, model)| (*code, model))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Our own generic guardhouse, glTF binary, Y up.
    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/guardhouse.glb");

    /// A square face is two triangles, its first corner shared.
    #[test]
    fn a_face_is_fanned_into_triangles() {
        let model =
            Model::parse("o one\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n").expect("reads");

        assert_eq!(model.triangles.len(), 2);
        assert_eq!(
            model.triangles[1],
            [Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0), Vec3::Y]
        );
    }

    /// A face naming a vertex not yet given is the line's fault.
    #[test]
    fn a_face_past_the_vertices_will_not_read() {
        assert_eq!(Model::parse("v 0 0 0\nf 1 2 3\n"), Err(Unread::Line(2)));
    }

    /// A user's file replaces an earlier one of the same code in its
    /// place, a new code is added last, and a file that is no model is
    /// left out.
    #[test]
    fn the_users_folder_replaces_and_adds() {
        let dir = std::env::temp_dir().join(format!("regent-models-{}", std::process::id()));

        // Read in name order: ABE1.glb, then ABE1.obj in its place.
        std::fs::create_dir_all(&dir).expect("made");
        std::fs::write(dir.join("ABE1.glb"), FIXTURE).expect("written");
        std::fs::write(dir.join("ABE1.obj"), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n")
            .expect("written");
        std::fs::write(dir.join("XXX1.obj"), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n")
            .expect("written");
        std::fs::write(dir.join("notes.txt"), "not a model").expect("written");

        let proxies = Proxies::load(Some(&dir));
        let inactive = SetKind::Inactive.id().try_into().expect("small");

        assert_eq!(proxies.len(), 2);
        assert_eq!(proxies.layer(FourCc::new(b"ABE1")), Some(0));
        assert_eq!(
            proxies
                .of(FourCc::new(b"ABE1"), inactive)
                .map(|m| m.triangles.len()),
            Some(1)
        );
        assert_eq!(proxies.layer(FourCc::new(b"XXX1")), Some(1));

        std::fs::remove_dir_all(&dir).expect("removed");
    }

    /// A model through the same reader, no game data needed. The fixture
    /// is our own generic guardhouse, scripted in Blender; only its rough
    /// size was measured from the sprite, as `tools/proxy` scales any model.
    #[test]
    fn the_guardhouse_fixture_reads_without_game_data() {
        let model = Model::gltf(FIXTURE, None).expect("reads");
        let corners = || model.triangles.iter().flatten();
        let low = corners().fold(Vec3::MAX, |low, p| low.min(*p));
        let high = corners().fold(Vec3::MIN, |high, p| high.max(*p));

        assert_eq!(model.triangles.len(), 270);

        // Stands on the ground at Z 0, and reaches up Z: Y up was turned.
        assert!(low.z.abs() < 0.01, "{low}");
        assert!((high.z - 100.0).abs() < 0.01, "{high}");

        // About 76 art pixels wide each way, the porch past that at one side.
        let size = high - low;

        assert!((70.0..=90.0).contains(&size.x), "{size}");
        assert!((70.0..=90.0).contains(&size.y), "{size}");
    }

    /// Only the finished picture casts.
    #[test]
    fn only_the_finished_picture_casts() {
        let mut proxies = Proxies::default();
        let guild = FourCc::new(b"ABV1");

        proxies.put(guild, Model::gltf(FIXTURE, None).expect("reads"));

        assert!(
            proxies
                .of(guild, SetKind::Inactive.id().try_into().expect("small"))
                .is_some()
        );
        assert!(
            proxies
                .of(guild, SetKind::Build.id().try_into().expect("small"))
                .is_none()
        );

        // Damage: the finished picture's stages, not the wreck's.
        assert!(proxies.of(guild, damage::DIE).is_some());
        assert!(proxies.of(guild, damage::DIE + 1).is_some());
        assert!(proxies.of(guild, damage::DIE + 2).is_none());
        assert!(proxies.of(guild, damage::DEAD).is_none());
    }
}
