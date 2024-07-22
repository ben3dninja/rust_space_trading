use std::f64::consts::TAU;

use bevy::math::{DVec2, DVec3};
use rand::Rng;

use crate::physics::G;

pub fn mod_180(x: f64) -> f64 {
    let x = x % 360.;
    if x > 180. {
        x - 360.
    } else {
        x
    }
}

#[allow(non_snake_case)]
/// Rotates u with respect to angles in radians
pub fn rotate(u: DVec2, o: f64, O: f64, I: f64) -> DVec3 {
    let (x, y) = u.into();
    let x_glob = (o.cos() * O.cos() - o.sin() * O.sin() * I.cos()) * x
        + (-o.sin() * O.cos() - o.cos() * O.sin() * I.cos()) * y;
    let y_glob = (o.cos() * O.sin() + o.sin() * O.cos() * I.cos()) * x
        + (-o.sin() * O.sin() + o.cos() * O.cos() * I.cos()) * y;
    let z_glob = (o.sin() * I.sin()) * x + (o.cos() * I.sin()) * y;
    DVec3::new(x_glob, y_glob, z_glob)
}

/// Computes the coordinates of the projection of v on the (v1, v2) plane, provided they form an orthonormal basis
pub fn project_onto_plane(v: DVec3, basis: (DVec3, DVec3)) -> DVec2 {
    DVec2::new(v.dot(basis.0), v.dot(basis.1))
}

/// Transforms radial/orthoradial/perp coordinates into absolute coordinates
pub fn convert_orbital_to_global(
    thrust: DVec3,
    origin: DVec3,
    origin_speed: DVec3,
    pos: DVec3,
    speed: DVec3,
) -> DVec3 {
    let v1 = (pos - origin).normalize();
    let v2 = (speed - origin_speed).try_normalize().unwrap_or(DVec3::Z);
    let v3 = v1.cross(v2);

    thrust.x * v1 + thrust.y * v2 + thrust.z * v3
}

/// Position and velocity for circular orbit at given altitude around body
/// For now : in the eccliptic plane, rotating trigonometrically  (TODO: random inclination and direction too)
pub fn circular_orbit_around_body(
    altitude: f64,
    body_mass: f64,
    body_pos: DVec3,
    body_speed: DVec3,
) -> (DVec3, DVec3) {
    let angle = rand::thread_rng().gen_range(0. ..TAU);
    let unit_pos = DVec2::from_angle(angle);
    let unit_speed = unit_pos.perp();
    let ((x, y), (vx, vy)) = (unit_pos.into(), unit_speed.into());
    (
        altitude * DVec3::new(x, y, 0.) + body_pos,
        (G * body_mass / altitude).sqrt() * DVec3::new(vx, vy, 0.) + body_speed,
    )
}

#[allow(non_snake_case)]
pub fn center_to_periapsis_direction(o: f64, O: f64, I: f64) -> DVec3 {
    DVec3::new(
        O.cos() * o.cos() - O.sin() * I.cos() * o.sin(),
        O.cos() * o.cos() + O.cos() * I.cos() * o.sin(),
        I.sin() * o.sin(),
    )
}

pub fn half_sizes(a: f64, e: f64) -> DVec2 {
    DVec2::new(1., (1. - e * e).sqrt()) * a
}
