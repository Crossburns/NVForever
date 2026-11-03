//! The console's location markers (docs/design/console.md): `PlaceLocationMarker x y z
//! [name] [colour]` (`plm`) puts a small box at a point with its name above it, both seen
//! through walls; `ClearLocationMarkers` (`clm`) takes away those of the current place
//! ("some tiny debug geometry … billboarded text that renders through walls", "clears
//! any location markers placed in current cell" [documented: GECK wiki
//! PlaceLocationMarker, ClearLocationMarkers]). The box's size and the colours 1-9 are
//! ours [design]; a marker is shown only in the place (interior, or worldspace outdoors)
//! it was placed in, and markers are not saved [design].

use bevy::prelude::*;

use crate::{space, PlayerCamera};

/// Half the box's side, game units [design: "tiny"].
pub const HALF: f32 = 8.0;

/// Colour 0 white, then 1-9 [design: the wiki gives no colours].
pub const COLOURS: [[f32; 3]; 10] = [
    [1.0, 1.0, 1.0],
    [1.0, 0.25, 0.25],
    [0.3, 1.0, 0.3],
    [0.35, 0.55, 1.0],
    [1.0, 1.0, 0.3],
    [0.3, 1.0, 1.0],
    [1.0, 0.35, 1.0],
    [1.0, 0.6, 0.2],
    [0.65, 0.4, 1.0],
    [0.6, 0.6, 0.6],
];

/// One marker: where (game space), its label and colour, and the place it belongs to
/// (`crate::Location`'s worldspace and interior).
#[derive(Debug, Clone, PartialEq)]
pub struct LocationMarker {
    pub pos: [f32; 3],
    pub name: String,
    pub color: u8,
    pub world: u32,
    pub interior: Option<u32>,
}

impl LocationMarker {
    fn here(&self, loc: Option<&crate::Location>) -> bool {
        loc.is_some_and(|l| l.interior == self.interior && (self.interior.is_some() || l.world == self.world))
    }
}

/// The markers placed, every place's.
#[derive(Resource, Default, Debug)]
pub struct LocationMarkers(pub Vec<LocationMarker>);

impl LocationMarkers {
    /// Take away the markers of the current place; how many went.
    pub fn clear_here(&mut self, loc: Option<&crate::Location>) -> usize {
        let before = self.0.len();
        self.0.retain(|m| !m.here(loc));
        before - self.0.len()
    }
}

/// Gizmos drawn in front of everything (the boxes, through walls).
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct MarkerGizmos;

/// A marker's label on the screen: the index of its marker.
#[derive(Component)]
struct MarkerLabel(usize);

pub struct LocationMarkersPlugin;

impl Plugin for LocationMarkersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocationMarkers>()
            .init_gizmo_group::<MarkerGizmos>()
            .add_systems(Startup, through_walls)
            .add_systems(PostUpdate, (labels, draw).chain().before(bevy::transform::TransformSystems::Propagate));
    }
}

fn through_walls(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<MarkerGizmos>();
    config.depth_bias = -1.0;
}

fn colour(m: &LocationMarker) -> Color {
    let [r, g, b] = COLOURS[usize::from(m.color.min(9))];
    Color::srgb(r, g, b)
}

/// One label per marker, made again when the markers change.
fn labels(markers: Res<LocationMarkers>, mut commands: Commands, old: Query<Entity, With<MarkerLabel>>) {
    if !markers.is_changed() {
        return;
    }
    for e in &old {
        commands.entity(e).despawn();
    }
    for (i, m) in markers.0.iter().enumerate() {
        commands.spawn((
            MarkerLabel(i),
            Node { position_type: PositionType::Absolute, ..default() },
            Text::new(m.name.clone()),
            TextFont { font_size: FontSize::Px(14.0), ..default() },
            TextColor(colour(m)),
            Visibility::Hidden,
            GlobalZIndex(4),
        ));
    }
}

/// The boxes, and each label over its box on the screen while its place is played.
fn draw(
    markers: Res<LocationMarkers>,
    loc: Option<Res<crate::Location>>,
    cameras: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    mut texts: Query<(&MarkerLabel, &mut Node, &mut Visibility)>,
    mut gizmos: Gizmos<MarkerGizmos>,
) {
    let camera = cameras.single().ok();
    for (l, mut node, mut vis) in &mut texts {
        let Some(m) = markers.0.get(l.0) else { continue };
        let above = [m.pos[0], m.pos[1], m.pos[2] + HALF * 2.5];
        let at = camera.filter(|_| m.here(loc.as_deref())).and_then(|(cam, t)| cam.world_to_viewport(t, space::to_bevy(above)).ok());
        let want = if at.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if let Some(at) = at {
            node.left = Val::Px(at.x);
            node.top = Val::Px(at.y);
        }
    }
    for m in markers.0.iter().filter(|m| m.here(loc.as_deref())) {
        let t = Transform::from_translation(space::to_bevy(m.pos)).with_scale(Vec3::splat(HALF * 2.0));
        gizmos.cube(t, colour(m));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(world: u32, interior: Option<u32>) -> crate::Location {
        crate::Location { world, interior }
    }

    #[test]
    fn clearing_takes_only_the_current_places_markers() {
        let m = |world, interior| LocationMarker { pos: [0.0; 3], name: "m".into(), color: 0, world, interior };
        let mut ms = LocationMarkers(vec![m(1, None), m(1, Some(7)), m(2, None), m(1, None)]);
        assert_eq!(ms.clear_here(Some(&at(1, None))), 2);
        // Indoors the interior counts, not the worldspace last walked in.
        assert_eq!(ms.clear_here(Some(&at(2, Some(7)))), 1);
        assert_eq!(ms.0, [m(2, None)]);
        assert_eq!(ms.clear_here(None), 0);
    }
}
