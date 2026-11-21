//! Point octree for radius and box queries over world entities.

use bevy::prelude::*;

pub const OCTREE_MAX_DEPTH: u8 = 6;
pub const OCTREE_NODE_CAPACITY: usize = 16;

pub const WORLD_INDEX_MIN: Vec3 = Vec3::new(-2000.0, -100.0, -2000.0);
pub const WORLD_INDEX_MAX: Vec3 = Vec3::new(2000.0, 1500.0, 2000.0);

/// Axis-aligned box, inclusive on both faces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OctreeAABB {
    pub min: Vec3,
    pub max: Vec3,
}

impl OctreeAABB {
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn from_center_half_extents(center: Vec3, half: Vec3) -> Self {
        Self::new(center - half, center + half)
    }

    pub fn contains_point(&self, point: Vec3) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }

    pub fn intersects_sphere(&self, center: Vec3, radius: f32) -> bool {
        let closest = center.clamp(self.min, self.max);
        closest.distance_squared(center) <= radius * radius
    }

    pub fn intersects_aabb(&self, other: &OctreeAABB) -> bool {
        self.min.cmple(other.max).all() && other.min.cmple(self.max).all()
    }

    /// Whether `other` lies entirely inside this box.
    pub fn contains_aabb(&self, other: &OctreeAABB) -> bool {
        self.min.cmple(other.min).all() && other.max.cmple(self.max).all()
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Half the box size along each axis.
    pub fn extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// The eight octants, indexed by bit 0 = +x, bit 1 = +y, bit 2 = +z.
    pub fn octants(&self) -> [OctreeAABB; 8] {
        let c = self.center();
        std::array::from_fn(|i| {
            let pick = |bit: usize, lo: f32, mid: f32, hi: f32| {
                if i & bit == 0 {
                    (lo, mid)
                } else {
                    (mid, hi)
                }
            };
            let (x0, x1) = pick(1, self.min.x, c.x, self.max.x);
            let (y0, y1) = pick(2, self.min.y, c.y, self.max.y);
            let (z0, z1) = pick(4, self.min.z, c.z, self.max.z);
            OctreeAABB::new(Vec3::new(x0, y0, z0), Vec3::new(x1, y1, z1))
        })
    }
}

#[derive(Debug, Clone)]
pub struct OctreeNode {
    pub bounds: OctreeAABB,
    pub entities: Vec<(Entity, Vec3)>,
    pub children: Option<Box<[OctreeNode; 8]>>,
}

impl OctreeNode {
    pub fn new(bounds: OctreeAABB) -> Self {
        Self {
            bounds,
            entities: Vec::new(),
            children: None,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_none()
    }

    /// Octant of `pos`, which must lie inside `bounds`. Points on a split
    /// plane go to the upper octant.
    fn octant_index(&self, pos: Vec3) -> usize {
        let c = self.bounds.center();
        usize::from(pos.x >= c.x) | usize::from(pos.y >= c.y) << 1 | usize::from(pos.z >= c.z) << 2
    }

    /// Inserts at a node `depth` levels below the root (pass 0 at the root).
    /// Returns `false` without storing anything if `pos` is outside `bounds`
    /// or not finite.
    pub fn insert(&mut self, entity: Entity, pos: Vec3, depth: u8) -> bool {
        if !pos.is_finite() || !self.bounds.contains_point(pos) {
            return false;
        }
        if self.is_leaf() {
            if self.entities.len() < OCTREE_NODE_CAPACITY || depth >= OCTREE_MAX_DEPTH {
                self.entities.push((entity, pos));
                return true;
            }
            self.subdivide(depth);
        }
        let index = self.octant_index(pos);
        let children = self.children.as_mut().expect("subdivided above");
        children[index].insert(entity, pos, depth + 1)
    }

    /// Splits this leaf (at `depth`) into eight octants and pushes its
    /// entities down. Does nothing on an internal node.
    pub fn subdivide(&mut self, depth: u8) {
        if !self.is_leaf() {
            return;
        }
        let mut children = Box::new(self.bounds.octants().map(OctreeNode::new));
        for (entity, pos) in std::mem::take(&mut self.entities) {
            let index = self.octant_index(pos);
            children[index].insert(entity, pos, depth + 1);
        }
        self.children = Some(children);
    }

    pub fn query_radius(&self, center: Vec3, radius: f32, results: &mut Vec<Entity>) {
        if !self.bounds.intersects_sphere(center, radius) {
            return;
        }
        let r_sq = radius * radius;
        results.extend(
            self.entities
                .iter()
                .filter(|(_, p)| p.distance_squared(center) <= r_sq)
                .map(|(e, _)| *e),
        );
        if let Some(children) = &self.children {
            for child in children.iter() {
                child.query_radius(center, radius, results);
            }
        }
    }

    pub fn query_aabb(&self, aabb: &OctreeAABB, results: &mut Vec<Entity>) {
        if !self.bounds.intersects_aabb(aabb) {
            return;
        }
        if aabb.contains_aabb(&self.bounds) {
            self.collect_all(results);
            return;
        }
        results.extend(
            self.entities
                .iter()
                .filter(|(_, p)| aabb.contains_point(*p))
                .map(|(e, _)| *e),
        );
        if let Some(children) = &self.children {
            for child in children.iter() {
                child.query_aabb(aabb, results);
            }
        }
    }

    fn collect_all(&self, results: &mut Vec<Entity>) {
        results.extend(self.entities.iter().map(|(e, _)| *e));
        if let Some(children) = &self.children {
            for child in children.iter() {
                child.collect_all(results);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entities.len()
            + self
                .children
                .as_ref()
                .map_or(0, |c| c.iter().map(OctreeNode::len).sum())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Depth of the deepest node below this one (0 for a leaf).
    pub fn max_depth(&self) -> u8 {
        self.children.as_ref().map_or(0, |c| {
            1 + c.iter().map(OctreeNode::max_depth).max().unwrap_or(0)
        })
    }

    /// Removes every entity and collapses back to a single leaf.
    pub fn clear(&mut self) {
        self.entities.clear();
        self.children = None;
    }
}

/// Entities with this marker are indexed each fixed tick by position.
#[derive(Component, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Component, Default, Debug)]
pub struct SpatialIndexed;

/// Octree over the whole playable world.
#[derive(Resource, Debug, Clone)]
pub struct WorldSpatialIndex {
    pub root: OctreeNode,
}

impl Default for WorldSpatialIndex {
    fn default() -> Self {
        Self {
            root: OctreeNode::new(OctreeAABB::new(WORLD_INDEX_MIN, WORLD_INDEX_MAX)),
        }
    }
}

impl WorldSpatialIndex {
    pub fn bounds(&self) -> OctreeAABB {
        self.root.bounds
    }

    /// Returns `false` for positions outside the world bounds.
    pub fn insert(&mut self, entity: Entity, pos: Vec3) -> bool {
        self.root.insert(entity, pos, 0)
    }

    pub fn query_radius(&self, center: Vec3, radius: f32) -> Vec<Entity> {
        let mut results = Vec::new();
        self.root.query_radius(center, radius, &mut results);
        results
    }

    pub fn query_aabb(&self, aabb: &OctreeAABB) -> Vec<Entity> {
        let mut results = Vec::new();
        self.root.query_aabb(aabb, &mut results);
        results
    }

    /// Replaces the contents; returns how many positions were inside bounds.
    pub fn rebuild(
        &mut self,
        entities_with_pos: impl IntoIterator<Item = (Entity, Vec3)>,
    ) -> usize {
        self.root.clear();
        entities_with_pos
            .into_iter()
            .filter(|&(entity, pos)| self.root.insert(entity, pos, 0))
            .count()
    }

    pub fn len(&self) -> usize {
        self.root.len()
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }
}

/// Rebuilds the index from every `SpatialIndexed` entity.
pub fn rebuild_world_spatial_index(
    mut index: ResMut<WorldSpatialIndex>,
    indexed: Query<(Entity, &GlobalTransform), With<SpatialIndexed>>,
) {
    index.rebuild(indexed.iter().map(|(e, t)| (e, t.translation())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Deterministic xorshift so tests need no RNG crate.
    struct Rng(u64);

    impl Rng {
        fn next_f32(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }

        fn point_in(&mut self, b: &OctreeAABB) -> Vec3 {
            let t = Vec3::new(self.next_f32(), self.next_f32(), self.next_f32());
            b.min + (b.max - b.min) * t
        }
    }

    fn e(i: u32) -> Entity {
        Entity::from_raw(i)
    }

    fn cube(half: f32) -> OctreeAABB {
        OctreeAABB::new(Vec3::splat(-half), Vec3::splat(half))
    }

    #[test]
    fn aabb_geometry() {
        let b = OctreeAABB::new(Vec3::ZERO, Vec3::new(4.0, 2.0, 6.0));
        assert_eq!(b.center(), Vec3::new(2.0, 1.0, 3.0));
        assert_eq!(b.extents(), Vec3::new(2.0, 1.0, 3.0));
        assert!(b.contains_point(Vec3::ZERO) && b.contains_point(b.max));
        assert!(!b.contains_point(Vec3::new(4.01, 1.0, 1.0)));
        assert!(
            b.intersects_sphere(Vec3::new(5.0, 1.0, 3.0), 1.0),
            "touching face"
        );
        assert!(
            !b.intersects_sphere(Vec3::new(5.0, 3.0, 3.0), 1.0),
            "corner gap"
        );
        assert!(b.intersects_sphere(b.center(), 0.1), "center inside");
        assert!(b.intersects_aabb(&OctreeAABB::new(Vec3::new(3.0, 1.0, 3.0), Vec3::splat(9.0))));
        assert!(
            !b.intersects_aabb(&OctreeAABB::new(Vec3::splat(3.0), Vec3::splat(9.0))),
            "clear of the top face"
        );
        assert!(!b.intersects_aabb(&OctreeAABB::new(Vec3::splat(7.0), Vec3::splat(9.0))));
        let octants = b.octants();
        assert_eq!(octants[0], OctreeAABB::new(Vec3::ZERO, b.center()));
        assert_eq!(octants[7], OctreeAABB::new(b.center(), b.max));
        let volume: f32 = octants
            .iter()
            .map(|o| (o.max - o.min).element_product())
            .sum();
        assert_eq!(volume, (b.max - b.min).element_product());
    }

    #[test]
    fn rejects_points_outside_bounds() {
        let mut node = OctreeNode::new(cube(10.0));
        assert!(!node.insert(e(0), Vec3::new(10.5, 0.0, 0.0), 0));
        assert!(!node.insert(e(1), Vec3::new(0.0, -11.0, 0.0), 0));
        assert!(!node.insert(e(2), Vec3::NAN, 0));
        assert!(node.insert(e(3), Vec3::splat(10.0), 0), "faces are inside");
        assert_eq!(node.len(), 1);
        let mut world = WorldSpatialIndex::default();
        assert!(!world.insert(e(4), Vec3::new(0.0, 2000.0, 0.0)));
        assert!(world.insert(e(5), Vec3::new(1999.0, 1499.0, -1999.0)));
    }

    #[test]
    fn seventeenth_point_subdivides_into_octants() {
        let mut node = OctreeNode::new(cube(8.0));
        let mut rng = Rng(42);
        for i in 0..16 {
            node.insert(e(i), rng.point_in(&cube(8.0)), 0);
        }
        assert!(node.is_leaf());
        assert_eq!(node.entities.len(), 16);
        node.insert(e(16), Vec3::splat(1.0), 0);
        let children = node.children.as_ref().expect("subdivided");
        assert_eq!(children.len(), 8);
        assert!(node.entities.is_empty(), "all entities pushed down");
        assert_eq!(children.iter().map(OctreeNode::len).sum::<usize>(), 17);
        for child in children.iter() {
            for (_, p) in &child.entities {
                assert!(child.bounds.contains_point(*p));
            }
        }
    }

    #[test]
    fn coincident_points_stop_at_max_depth() {
        let mut node = OctreeNode::new(cube(64.0));
        for i in 0..100 {
            assert!(node.insert(e(i), Vec3::splat(3.0), 0));
        }
        assert_eq!(node.len(), 100);
        assert_eq!(node.max_depth(), OCTREE_MAX_DEPTH);
    }

    #[test]
    fn radius_query_matches_brute_force() {
        let bounds = cube(100.0);
        let mut node = OctreeNode::new(bounds);
        let mut rng = Rng(7);
        let points: Vec<Vec3> = (0..100).map(|_| rng.point_in(&bounds)).collect();
        for (i, p) in points.iter().enumerate() {
            assert!(node.insert(e(i as u32), *p, 0));
        }
        for (center, radius) in [
            (Vec3::ZERO, 40.0),
            (Vec3::new(90.0, -90.0, 0.0), 60.0),
            (Vec3::splat(50.0), 5.0),
            (Vec3::ZERO, 500.0),
        ] {
            let mut found = Vec::new();
            node.query_radius(center, radius, &mut found);
            found.sort();
            let mut expected: Vec<Entity> = points
                .iter()
                .enumerate()
                .filter(|(_, p)| p.distance(center) <= radius)
                .map(|(i, _)| e(i as u32))
                .collect();
            expected.sort();
            assert_eq!(found, expected, "center {center} radius {radius}");
        }
    }

    #[test]
    fn aabb_query_matches_brute_force() {
        let bounds = cube(100.0);
        let mut node = OctreeNode::new(bounds);
        let mut rng = Rng(99);
        let points: Vec<Vec3> = (0..400).map(|_| rng.point_in(&bounds)).collect();
        for (i, p) in points.iter().enumerate() {
            node.insert(e(i as u32), *p, 0);
        }
        for query in [
            OctreeAABB::new(Vec3::splat(-30.0), Vec3::new(10.0, 50.0, 0.0)),
            cube(100.0),
            OctreeAABB::new(Vec3::splat(150.0), Vec3::splat(200.0)),
        ] {
            let mut found = Vec::new();
            node.query_aabb(&query, &mut found);
            found.sort();
            let mut expected: Vec<Entity> = points
                .iter()
                .enumerate()
                .filter(|(_, p)| query.contains_point(**p))
                .map(|(i, _)| e(i as u32))
                .collect();
            expected.sort();
            assert_eq!(found, expected, "{query:?}");
        }
    }

    #[test]
    fn clear_and_rebuild() {
        let mut index = WorldSpatialIndex::default();
        let mut rng = Rng(3);
        let bounds = index.bounds();
        let inserted = index.rebuild((0..500).map(|i| (e(i), rng.point_in(&bounds))));
        assert_eq!((inserted, index.len()), (500, 500));
        assert!(!index.root.is_leaf());
        let inserted = index.rebuild([(e(1), Vec3::ZERO), (e(2), Vec3::splat(9_999.0))]);
        assert_eq!(
            (inserted, index.len()),
            (1, 1),
            "rebuild replaces and drops out-of-bounds"
        );
        index.root.clear();
        assert!(index.is_empty() && index.root.is_leaf());
    }

    #[test]
    fn radius_queries_over_5000_points_take_under_0_2_ms() {
        let mut index = WorldSpatialIndex::default();
        let mut rng = Rng(0xC0FFEE);
        let bounds = index.bounds();
        let points: Vec<Vec3> = (0..5_000).map(|_| rng.point_in(&bounds)).collect();
        index.rebuild(points.iter().enumerate().map(|(i, p)| (e(i as u32), *p)));
        assert_eq!(index.len(), 5_000);

        const QUERIES: u32 = 2_000;
        let centers: Vec<Vec3> = (0..QUERIES).map(|_| rng.point_in(&bounds)).collect();
        let mut hits = 0;
        let start = Instant::now();
        for center in &centers {
            hits += index.query_radius(*center, 150.0).len();
        }
        let per_query = start.elapsed() / QUERIES;
        println!("octree: 5,000 points, {per_query:?} per 150 m radius query ({hits} hits total)");
        assert!(hits > 0);
        assert!(per_query.as_secs_f64() < 0.2e-3, "{per_query:?}");
    }

    #[test]
    fn system_indexes_marked_entities() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .init_resource::<WorldSpatialIndex>()
            .add_systems(Update, rebuild_world_spatial_index);
        let near = app
            .world_mut()
            .spawn((SpatialIndexed, Transform::from_xyz(5.0, 0.0, 0.0)))
            .id();
        app.world_mut()
            .spawn((SpatialIndexed, Transform::from_xyz(500.0, 0.0, 0.0)));
        app.world_mut().spawn(Transform::from_xyz(1.0, 0.0, 0.0));
        app.update();
        app.update();
        let index = app.world().resource::<WorldSpatialIndex>();
        assert_eq!(index.len(), 2, "unmarked entities are ignored");
        assert_eq!(index.query_radius(Vec3::ZERO, 10.0), vec![near]);
    }
}
