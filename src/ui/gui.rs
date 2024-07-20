use bevy::{
    a11y::AccessibilityPlugin,
    color::palettes::css::{BLACK, DARK_GRAY, GOLD, PURPLE, TEAL},
    core_pipeline::CorePipelinePlugin,
    gizmos::GizmoPlugin,
    input::{
        common_conditions::input_pressed,
        mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
        InputPlugin,
    },
    math::{DVec2, DVec3},
    prelude::*,
    render::{camera::ScalingMode, pipelined_rendering::PipelinedRenderingPlugin, RenderPlugin},
    sprite::{MaterialMesh2dBundle, Mesh2dHandle, SpritePlugin},
    text::TextPlugin,
    ui::UiPlugin,
    winit::{WakeUp, WinitPlugin},
};

use crate::{
    physics::{influence::HillRadius, orbit::SystemSize, predictions::Prediction},
    prelude::*,
    utils::algebra::project_onto_plane,
};

use super::{
    widget::space_map::{SpaceMap, ZOOM_STEP},
    RenderSet, UiUpdate,
};

const MAX_WIDTH: f32 = 1000.;
const MIN_RADIUS: f32 = 1e-4;
const SCROLL_SENSITIVITY: f32 = 10.;
pub struct GuiPlugin;

impl Plugin for GuiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            TransformPlugin,
            InputPlugin,
            WindowPlugin::default(),
            AccessibilityPlugin,
            AssetPlugin::default(),
            WinitPlugin::<WakeUp>::default(),
            RenderPlugin::default(),
            ImagePlugin::default(),
            PipelinedRenderingPlugin,
            CorePipelinePlugin,
            SpritePlugin,
            TextPlugin,
            UiPlugin,
            GizmoPlugin,
        ))
        .insert_resource(ClearColor(Color::Srgba(BLACK)))
        .add_systems(Startup, gui_setup)
        .add_systems(
            OnEnter(Loaded),
            (insert_display_components, update_transform)
                .chain()
                .in_set(GUIUpdate),
        )
        .add_systems(
            PostUpdate,
            (
                (update_transform, update_camera_pos)
                    .chain()
                    .in_set(UiUpdate),
                draw_gizmos.in_set(RenderSet),
            )
                .run_if(resource_exists::<SpaceMap>)
                .run_if(in_state(Loaded)),
        )
        .add_systems(
            Update,
            (
                zoom_with_scroll,
                pan_when_dragging.run_if(input_pressed(MouseButton::Left)),
            )
                .run_if(resource_exists::<SpaceMap>),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct GUIUpdate;

fn gui_setup(mut commands: Commands, mut materials: ResMut<Assets<ColorMaterial>>) {
    let mut cam = Camera2dBundle::default();
    cam.projection.scaling_mode = ScalingMode::FixedVertical(MAX_WIDTH);
    commands.spawn(cam);
    let colors = Colors {
        stars: materials.add(Color::Srgba(GOLD)),
        planets: materials.add(Color::Srgba(TEAL)),
        other: materials.add(Color::Srgba(DARK_GRAY)),
    };
    commands.insert_resource(colors);
}

#[derive(Resource)]
pub struct Colors {
    stars: Handle<ColorMaterial>,
    planets: Handle<ColorMaterial>,
    other: Handle<ColorMaterial>,
}

fn insert_display_components(
    mut commands: Commands,
    bodies: Query<(Entity, &BodyInfo)>,
    ships: Query<Entity, With<ShipInfo>>,
    mut meshes: ResMut<Assets<Mesh>>,
    colors: Res<Colors>,
    system_size: Res<SystemSize>,
) {
    let scale = MAX_WIDTH as f64 / system_size.0;
    bodies.iter().for_each(|(e, BodyInfo(data))| {
        let (material, z) = match data.body_type {
            BodyType::Star => (colors.stars.clone(), 0.),
            BodyType::Planet => (colors.planets.clone(), -1.),
            _ => (colors.other.clone(), -2.),
        };
        commands.entity(e).insert(MaterialMesh2dBundle {
            mesh: Mesh2dHandle(meshes.add(Circle {
                radius: MIN_RADIUS.max((data.radius * scale) as f32),
            })),
            material,
            transform: Transform::from_xyz(0., 0., z),
            ..default()
        });
    });
    for e in ships.iter() {
        commands
            .entity(e)
            .insert(TransformBundle::from_transform(Transform::from_xyz(
                0., 0., 1.,
            )));
    }
}

fn zoom_with_scroll(mut events: EventReader<MouseWheel>, mut space_map: ResMut<SpaceMap>) {
    for event in events.read() {
        space_map.zoom_level *= ZOOM_STEP.powf(match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y * SCROLL_SENSITIVITY,
        } as f64);
    }
}

fn pan_when_dragging(mut motions: EventReader<MouseMotion>, mut map: ResMut<SpaceMap>) {
    for event in motions.read() {
        let scale = map.system_size / (500. * map.zoom_level);
        // The horizontal inputs seem to be inversed
        map.offset_amount += scale * event.delta.as_dvec2() * DVec2::new(-1., 1.);
    }
}

fn update_camera_pos(
    space_map: Res<SpaceMap>,
    mut cam: Query<(&mut Transform, &mut OrthographicProjection)>,
    positions: Query<&Position>,
) {
    let scale = MAX_WIDTH as f64 / space_map.system_size;
    let (mut cam_pos, mut proj) = cam.single_mut();
    let focus_pos = space_map
        .focus_body
        .map_or(DVec3::default(), |f| positions.get(f).unwrap().0);
    cam_pos.translation = ((focus_pos
        + DVec3::new(space_map.offset_amount.x, space_map.offset_amount.y, 0.))
        * scale)
        .as_vec3();
    proj.scale = (1. / space_map.zoom_level) as f32;
}

fn update_transform(system_size: Res<SystemSize>, mut query: Query<(&mut Transform, &Position)>) {
    let scale = MAX_WIDTH as f64 / system_size.0;
    for (mut transform, Position(pos)) in query.iter_mut() {
        let (x, y) = (project_onto_plane(*pos, (DVec3::X, DVec3::Y)) * scale)
            .as_vec2()
            .into();
        transform.translation.x = x;
        transform.translation.y = y;
    }
}

fn draw_gizmos(
    space_map: Res<SpaceMap>,
    mut gizmos: Gizmos,
    bodies: Query<(&Transform, &BodyInfo, &HillRadius)>,
    ships: Query<&Transform, With<ShipInfo>>,
    predictions: Query<&Transform, With<Prediction>>,
) {
    let scale = MAX_WIDTH as f64 / space_map.system_size;
    if let SpaceMap {
        zoom_level,
        selected: Some(s),
        ..
    } = space_map.as_ref()
    {
        let (pos, info, ..) = bodies.get(*s).unwrap();
        gizmos.circle_2d(
            pos.translation.xy(),
            (10. / zoom_level).max(info.0.radius * scale + 15. / zoom_level) as f32,
            Color::srgba(1., 1., 1., 0.1),
        );
        for (pos, _, radius) in bodies.iter() {
            gizmos.circle_2d(
                pos.translation.xy(),
                (radius.0 * scale) as f32,
                Color::srgba(1., 0.1, 0.1, 0.1),
            );
        }
        for e in ships.iter() {
            gizmos.rect_2d(
                e.translation.xy(),
                0.,
                (10. / zoom_level) as f32 * Vec2::ONE,
                Color::Srgba(PURPLE),
            )
        }
        for e in predictions.iter() {
            gizmos.rect_2d(
                e.translation.xy(),
                0.,
                (10. / zoom_level) as f32 * Vec2::ONE,
                Color::srgba(1., 1., 1., 0.1),
            )
        }
    }
}
