use std::f32::consts::TAU;

use bevy::{
    color::{Alpha, Color},
    gizmos::gizmos::Gizmos,
    math::{Quat, Vec2, Vec3},
    render::camera::Camera,
    transform::components::GlobalTransform,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// helper function to create a centered rect using up certain percentage of the available rect `r`
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    // Cut the given rectangle into three vertical pieces
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    // Then cut the middle vertical piece into three width-wise pieces
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1] // Return the middle chunk
}

pub fn cycle_add(i: &mut usize, size: usize, value: isize) {
    *i = ((*i as isize + value) % size as isize) as usize
}

pub fn viewable_radius(camera: &Camera) -> Option<f32> {
    camera
        .logical_viewport_size()
        .and_then(|size| camera.viewport_to_world_2d(&GlobalTransform::default(), size))
        .map(|v| v.length())
}

fn ellipse_inner(
    half_size: Vec2,
    resolution: usize,
    initial_angle: f32,
    sign: f32,
) -> impl Iterator<Item = Vec2> {
    (0..resolution + 1).map(move |i| {
        let angle = (i as f32 * TAU * sign / resolution as f32) + initial_angle;
        let (y, x) = angle.sin_cos();
        Vec2::new(x, y) * half_size
    })
}

pub struct EllipseBuilder {
    pub position: Vec3,
    pub rotation: Quat,
    pub half_size: Vec2,
    pub color: Color,
    pub resolution: usize,
    pub initial_angle: f32,
    pub sign: f32,
}

impl EllipseBuilder {
    pub fn draw(&self, gizmos: &mut Gizmos) {
        let positions = ellipse_inner(
            self.half_size,
            self.resolution,
            self.initial_angle,
            self.sign,
        )
        .map(|vec2| self.rotation * vec2.extend(0.))
        .map(|vec3| vec3 + self.position);
        gizmos.linestrip_gradient(draw_decreasing_alpha(
            positions,
            self.resolution,
            self.color,
        ));
    }
}

/// Transforms an iterator of points into an iterator of points and colors, for use in [bevy_gizmos::gizmos::Gizmos::linestrip_gradient].
/// The resolution has to be equal to the number of points
pub fn draw_decreasing_alpha(
    points: impl Iterator<Item = Vec3>,
    resolution: usize,
    color: impl Into<Color>,
) -> impl Iterator<Item = (Vec3, impl Into<Color>)> {
    let color = color.into();
    points.enumerate().map(move |(i, pos)| {
        (
            pos,
            color.with_alpha(color.alpha() * (1. - i as f32 / resolution as f32)),
        )
    })
}
