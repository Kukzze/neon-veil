//! Distance-based mesh level of detail.

use bevy::prelude::*;

pub const DEFAULT_LOD_THRESHOLDS: [f32; 3] = [35.0, 90.0, 180.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Reflect)]
#[reflect(Debug, PartialEq, Hash, Default)]
pub enum LODLevel {
    /// Full detail (0-35 m).
    #[default]
    LOD0 = 0,
    /// High detail (35-90 m).
    LOD1 = 1,
    /// Medium detail (90-180 m).
    LOD2 = 2,
    /// Low detail / billboard (180 m+).
    LOD3 = 3,
}

impl LODLevel {
    pub const ALL: [Self; 4] = [Self::LOD0, Self::LOD1, Self::LOD2, Self::LOD3];

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Component, Debug, Clone, PartialEq, Reflect)]
#[reflect(Component, Debug)]
pub struct LODComponent {
    /// Index = level. LOD0 is always present.
    pub lod_meshes: [Option<Handle<Mesh>>; 4],
    pub current_level: LODLevel,
    /// Distances at which LOD1, LOD2 and LOD3 begin, before scaling.
    pub base_thresholds: [f32; 3],
}

impl LODComponent {
    pub fn new(
        lod0: Handle<Mesh>,
        lod1: Option<Handle<Mesh>>,
        lod2: Option<Handle<Mesh>>,
        lod3: Option<Handle<Mesh>>,
    ) -> Self {
        Self {
            lod_meshes: [Some(lod0), lod1, lod2, lod3],
            current_level: LODLevel::LOD0,
            base_thresholds: DEFAULT_LOD_THRESHOLDS,
        }
    }

    pub fn with_thresholds(mut self, thresholds: [f32; 3]) -> Self {
        self.base_thresholds = thresholds;
        self
    }

    /// Thresholds scale with `distance_factor`: a larger factor keeps
    /// detailed meshes out to greater distances.
    pub fn determine_level(&self, distance: f32, distance_factor: f32) -> LODLevel {
        let [t0, t1, t2] = self.base_thresholds.map(|t| t * distance_factor);
        if distance < t0 {
            LODLevel::LOD0
        } else if distance < t1 {
            LODLevel::LOD1
        } else if distance < t2 {
            LODLevel::LOD2
        } else {
            LODLevel::LOD3
        }
    }

    /// Mesh to show at `level`: that level's mesh, or the nearest more
    /// detailed one when it was not provided.
    pub fn mesh_for(&self, level: LODLevel) -> Option<&Handle<Mesh>> {
        self.lod_meshes[..=level.index()]
            .iter()
            .rev()
            .find_map(Option::as_ref)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
#[reflect(Debug, PartialEq, Hash, Default)]
pub enum LODQuality {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
}

impl LODQuality {
    pub const fn distance_factor(self) -> f32 {
        match self {
            Self::Low => 0.65,
            Self::Medium => 0.85,
            Self::High => 1.0,
            Self::Ultra => 1.5,
        }
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Reflect)]
#[reflect(Resource, Debug, PartialEq)]
pub struct LODSettings {
    pub distance_factor: f32,
}

impl Default for LODSettings {
    fn default() -> Self {
        Self::from_quality(LODQuality::High)
    }
}

impl LODSettings {
    pub fn from_quality(quality: LODQuality) -> Self {
        Self {
            distance_factor: quality.distance_factor(),
        }
    }
}

/// Picks each LOD entity's level from its distance to the active 3D camera
/// with the highest render order, and swaps `Mesh3d` only when it changes.
pub fn update_lod_system(
    settings: Res<LODSettings>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut lods: Query<(&mut LODComponent, &GlobalTransform, &mut Mesh3d)>,
) {
    let Some((_, camera)) = cameras
        .iter()
        .filter(|(camera, _)| camera.is_active)
        .max_by_key(|(camera, _)| camera.order)
    else {
        return;
    };
    let eye = camera.translation();
    for (mut lod, transform, mut mesh) in &mut lods {
        let level = lod.determine_level(
            eye.distance(transform.translation()),
            settings.distance_factor,
        );
        let Some(wanted) = lod.mesh_for(level) else {
            continue;
        };
        if mesh.0 != *wanted {
            mesh.0 = wanted.clone();
        }
        if lod.current_level != level {
            lod.current_level = level;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handles() -> [Handle<Mesh>; 4] {
        std::array::from_fn(|i| Handle::weak_from_u128(0x10D0 + i as u128))
    }

    fn full() -> LODComponent {
        let [a, b, c, d] = handles();
        LODComponent::new(a, Some(b), Some(c), Some(d))
    }

    #[test]
    fn default_thresholds() {
        let lod = full();
        for (distance, level) in [
            (0.0, LODLevel::LOD0),
            (20.0, LODLevel::LOD0),
            (34.99, LODLevel::LOD0),
            (35.0, LODLevel::LOD1),
            (50.0, LODLevel::LOD1),
            (120.0, LODLevel::LOD2),
            (180.0, LODLevel::LOD3),
            (250.0, LODLevel::LOD3),
        ] {
            assert_eq!(lod.determine_level(distance, 1.0), level, "{distance} m");
        }
    }

    #[test]
    fn quality_factor_scales_thresholds() {
        let lod = full();
        let low = LODQuality::Low.distance_factor();
        // Low: 22.75 / 58.5 / 117 m.
        assert_eq!(lod.determine_level(20.0, low), LODLevel::LOD0);
        assert_eq!(lod.determine_level(30.0, low), LODLevel::LOD1);
        assert_eq!(lod.determine_level(60.0, low), LODLevel::LOD2);
        assert_eq!(lod.determine_level(120.0, low), LODLevel::LOD3);
        // Ultra: 52.5 / 135 / 270 m.
        let ultra = LODQuality::Ultra.distance_factor();
        assert_eq!(lod.determine_level(50.0, ultra), LODLevel::LOD0);
        assert_eq!(lod.determine_level(250.0, ultra), LODLevel::LOD2);
        assert_eq!(LODSettings::default().distance_factor, 1.0);
        assert_eq!(
            LODSettings::from_quality(LODQuality::Medium).distance_factor,
            0.85
        );
    }

    #[test]
    fn missing_levels_fall_back_to_more_detail() {
        let [a, _, c, _] = handles();
        let lod = LODComponent::new(a.clone(), None, Some(c.clone()), None);
        assert_eq!(lod.mesh_for(LODLevel::LOD0), Some(&a));
        assert_eq!(lod.mesh_for(LODLevel::LOD1), Some(&a));
        assert_eq!(lod.mesh_for(LODLevel::LOD2), Some(&c));
        assert_eq!(lod.mesh_for(LODLevel::LOD3), Some(&c));
    }

    #[test]
    fn system_swaps_meshes_by_camera_distance() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .init_resource::<LODSettings>()
            .add_systems(Update, update_lod_system);
        let h = handles();
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), Transform::default()))
            .id();
        let props: Vec<Entity> = [20.0, 50.0, 120.0, 250.0]
            .into_iter()
            .map(|z| {
                app.world_mut()
                    .spawn((
                        full(),
                        Mesh3d(h[0].clone()),
                        Transform::from_xyz(0.0, 0.0, z),
                    ))
                    .id()
            })
            .collect();
        app.update();
        app.update();
        let level_of =
            |app: &App, e: Entity| app.world().get::<LODComponent>(e).unwrap().current_level;
        for (i, e) in props.iter().enumerate() {
            assert_eq!(level_of(&app, *e), LODLevel::ALL[i]);
            assert_eq!(app.world().get::<Mesh3d>(*e).unwrap().0, h[i]);
        }
        // Walking the camera out to the far prop flips both ends.
        app.world_mut()
            .entity_mut(camera)
            .insert(Transform::from_xyz(0.0, 0.0, 240.0));
        app.update();
        app.update();
        assert_eq!(level_of(&app, props[3]), LODLevel::LOD0);
        assert_eq!(level_of(&app, props[0]), LODLevel::LOD3);
        assert_eq!(app.world().get::<Mesh3d>(props[0]).unwrap().0, h[3]);
        // On Low, LOD0 ends at 22.75 m, so the prop 30 m away shows LOD1.
        *app.world_mut().resource_mut::<LODSettings>() = LODSettings::from_quality(LODQuality::Low);
        app.world_mut()
            .entity_mut(camera)
            .insert(Transform::from_xyz(0.0, 0.0, 80.0));
        app.update();
        app.update();
        assert_eq!(level_of(&app, props[1]), LODLevel::LOD1, "30 m on Low");
    }
}
