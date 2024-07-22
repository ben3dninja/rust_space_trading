use bevy::{
    a11y::AccessibilityPlugin,
    color::palettes::css::{BLACK, DARK_GRAY, GOLD, TEAL},
    core_pipeline::CorePipelinePlugin,
    gizmos::GizmoPlugin,
    input::{
        common_conditions::input_pressed,
        mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel},
        ButtonState, InputPlugin,
    },
    math::{DVec2, DVec3},
    prelude::*,
    render::{camera::ScalingMode, pipelined_rendering::PipelinedRenderingPlugin, RenderPlugin},
    sprite::{MaterialMesh2dBundle, Mesh2dHandle, SpritePlugin},
    text::TextPlugin,
    ui::UiPlugin,
    window::PrimaryWindow,
    winit::{WakeUp, WinitPlugin},
};
use bevy_ratatui::event::KeyEvent;
use crossterm::event::KeyCode;

use crate::{
    physics::{influence::HillRadius, orbit::SystemSize, predictions::Prediction},
    prelude::*,
    utils::{
        algebra::{center_to_periapsis_direction, half_sizes, project_onto_plane},
        ui::{viewable_radius, EllipseBuilder},
    },
};

use super::{
    screen::editor::PREDICTIONS_NUMBER,
    widget::space_map::{SpaceMap, ZOOM_STEP},
    RenderSet, UiUpdate,
};

pub const MAX_WIDTH: f32 = 1000.;
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
        .add_event::<SelectObjectEvent>()
        .add_systems(Startup, (camera_setup, color_setup))
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
                print_radius,
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
        )
        .add_systems(
            PreUpdate,
            send_select_object_event
                .run_if(on_event::<MouseButtonInput>().and_then(resource_exists::<SpaceMap>)),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct GUIUpdate;

#[derive(Event)]
pub struct SelectObjectEvent {
    pub entity: Entity,
}

#[derive(Component, Copy, Clone, Debug)]
pub struct SelectionRadius {
    min_radius: f32,
    actual_radius: f32,
}

#[derive(Resource)]
pub struct Colors {
    stars: Handle<ColorMaterial>,
    planets: Handle<ColorMaterial>,
    other: Handle<ColorMaterial>,
}

pub fn camera_setup(mut commands: Commands) {
    let mut cam = Camera2dBundle::default();
    cam.projection.scaling_mode = ScalingMode::FixedVertical(MAX_WIDTH);
    commands.spawn(cam);
}

pub fn color_setup(mut commands: Commands, mut materials: ResMut<Assets<ColorMaterial>>) {
    let colors = Colors {
        stars: materials.add(Color::Srgba(GOLD)),
        planets: materials.add(Color::Srgba(TEAL)),
        other: materials.add(Color::Srgba(DARK_GRAY)),
    };
    commands.insert_resource(colors);
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
        commands.entity(e).insert((
            MaterialMesh2dBundle {
                mesh: Mesh2dHandle(meshes.add(Circle {
                    radius: MIN_RADIUS.max((data.radius * scale) as f32),
                })),
                material,
                transform: Transform::from_xyz(0., 0., z),
                ..default()
            },
            SelectionRadius {
                min_radius: 10.,
                actual_radius: (data.radius * scale) as f32,
            },
        ));
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

fn send_select_object_event(
    mut clicks: EventReader<MouseButtonInput>,
    window: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform)>,
    mut writer: EventWriter<SelectObjectEvent>,
    objects: Query<(Entity, &Transform, &SelectionRadius)>,
    map: Res<SpaceMap>,
) {
    let (cam, cam_transform) = cam.single();
    for event in clicks.read() {
        if matches!(
            (event.state, event.button),
            (ButtonState::Pressed, MouseButton::Left)
        ) {
            if let Some(cursor_pos) = window.single().cursor_position() {
                if let Some(translation) = cam.viewport_to_world_2d(cam_transform, cursor_pos) {
                    objects
                        .iter()
                        .find(|(_, pos, rad)| {
                            (pos.translation.xy() - translation).length()
                                < rad
                                    .actual_radius
                                    .max(rad.min_radius / map.zoom_level as f32)
                        })
                        .map(|(entity, _, _)| writer.send(SelectObjectEvent { entity }));
                }
            }
        }
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

#[allow(non_snake_case)]
fn draw_gizmos(
    space_map: Res<SpaceMap>,
    mut gizmos: Gizmos,
    bodies: Query<(
        &Transform,
        &Velocity,
        &BodyInfo,
        &HillRadius,
        &EllipticalOrbit,
    )>,
    ships: Query<(&Transform, &Velocity, &Influenced), With<ShipInfo>>,
    mapping: Res<BodiesMapping>,
    predictions: Query<(&Transform, &Prediction)>,
    cam: Query<(&Camera, &GlobalTransform)>,
) {
    let scale = MAX_WIDTH as f64 / space_map.system_size;
    if let &SpaceMap {
        zoom_level,
        selected: Some(s),
        ..
    } = space_map.as_ref()
    {
        let (cam, cam_pos) = cam.single();
        if let Ok((pos, _, info, _, _)) = bodies.get(s) {
            gizmos.circle_2d(
                pos.translation.xy(),
                (10. / zoom_level).max(info.0.radius * scale + 15. / zoom_level) as f32,
                Color::srgba(1., 1., 1., 0.1),
            );
            let parent_translation = pos.translation;
            for &i in info
                .0
                .orbiting_bodies
                .iter()
                .filter_map(|id| mapping.0.get(id))
            {
                let &EllipticalOrbit {
                    semimajor_axis: a,
                    inclination: I,
                    long_asc_node: O,
                    arg_periapsis: o,
                    eccentricity: e,
                    eccentric_anomaly: E,
                    revolution_period,
                    ..
                } = bodies.get(i).unwrap().4;
                let (o, O, I, E) = (
                    o.to_radians(),
                    O.to_radians(),
                    I.to_radians(),
                    E.to_radians(),
                );
                let (peri, apo) = ((1. - e) * a, (1. + e) * a);
                if let Some(radius) = viewable_radius(cam) {
                    let distance_to_parent = (cam_pos.translation() - parent_translation).length();
                    if distance_to_parent + radius < (peri * scale) as f32
                        || distance_to_parent - radius > (apo * scale) as f32
                    {
                        continue;
                    }
                }
                let position = (scale * (peri - a) * center_to_periapsis_direction(o, O, I))
                    .as_vec3()
                    + parent_translation;
                let resolution = ((zoom_level * 100.) as usize).min(1000);
                EllipseBuilder {
                    position,
                    rotation: Quat::from_rotation_z(O as f32)
                        * Quat::from_rotation_x(I as f32)
                        * Quat::from_rotation_z(o as f32),
                    half_size: (half_sizes(a, e) * scale).as_vec2(),
                    color: Color::WHITE.with_alpha(0.1),
                    resolution,
                    initial_angle: E as f32,
                    sign: -revolution_period.signum() as f32,
                }
                .draw(&mut gizmos);
            }
        }
        for (pos, _, _, radius, _) in bodies.iter() {
            gizmos.circle_2d(
                pos.translation.xy(),
                (radius.0 * scale) as f32,
                Color::srgba(1., 0.1, 0.1, 0.1),
            );
        }

        for (t, speed, influence) in ships.iter() {
            let ref_speed = influence
                .main_influencer
                .map_or(DVec3::ZERO, |e| bodies.get(e).unwrap().1 .0);
            let speed = ((speed.0 - ref_speed).normalize_or(DVec3::X) * 30. / zoom_level)
                .xy()
                .as_vec2();
            let t = t.translation.xy() - speed / 3.;
            let perp = speed.perp() / 3.;
            gizmos.linestrip_2d(
                [t + speed, t + perp, t - perp, t + speed],
                Color::Srgba(GOLD),
            );
        }
        for (t, p) in predictions.iter() {
            gizmos.circle_2d(
                t.translation.xy(),
                (1. - p.index as f32 / PREDICTIONS_NUMBER as f32) * 2. / zoom_level as f32,
                Color::srgba(1., 1., 1., 0.1),
            );
        }
    }
}

fn print_radius(mut keys: EventReader<KeyEvent>, cam: Query<&Camera>) {
    for event in keys.read() {
        if event.code == KeyCode::Char('p') {
            eprintln!("{}", viewable_radius(cam.single()).unwrap());
        }
    }
}
