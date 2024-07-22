use std::f64::consts::PI;

use bevy::{
    math::{DVec2, DVec3},
    prelude::*,
};

use crate::{
    game::Loaded,
    objects::prelude::*,
    physics::prelude::*,
    utils::algebra::{mod_180, rotate},
};

use super::time::GameTime;

pub fn plugin(app: &mut App) {
    app.add_systems(
        OnEnter(Loaded),
        (update_local, update_global, insert_system_size)
            .chain()
            .in_set(OrbitsUpdate),
    )
    .add_systems(
        FixedUpdate,
        (update_local, update_global).chain().in_set(OrbitsUpdate),
    );
}

#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct OrbitsUpdate;

#[derive(Component, Default, Clone, Debug)]
pub struct EllipticalOrbit {
    pub eccentricity: f64,
    pub semimajor_axis: f64,
    pub inclination: f64,
    pub long_asc_node: f64,
    pub arg_periapsis: f64,
    pub initial_mean_anomaly: f64,
    pub revolution_period: f64,

    pub mean_anomaly: f64,
    pub eccentric_anomaly: f64,
    /// 2D position in the orbital plane around the host body
    pub orbital_position: DVec2,
    pub orbital_velocity: DVec2,
    /// 3D position with respect to the host body (in kilometers)
    pub local_pos: DVec3,
    /// 3D velocity (in kilometers per day)
    pub local_velocity: DVec3,
}

const E_TOLERANCE: f64 = 1e-6;
// see https://ssd.jpl.nasa.gov/planets/approx_pos.html
#[allow(non_snake_case)]
impl EllipticalOrbit {
    fn update_M(&mut self, time: f64) {
        if self.revolution_period == 0. {
            return;
        }
        self.mean_anomaly =
            mod_180(self.initial_mean_anomaly + 360. * time / self.revolution_period);
    }
    fn update_E(&mut self, time: f64) {
        self.update_M(time);
        let M = self.mean_anomaly;
        let e = self.eccentricity;
        let ed = e.to_degrees();
        let mut E = M + ed * M.to_radians().sin();
        // TODO : change formulas to use radians instead
        let mut dM;
        let mut dE;
        for _ in 0..10 {
            dM = M - (E - ed * E.to_radians().sin());
            dE = dM / (1. - e * E.to_radians().cos());
            E += dE;
            if dE.abs() <= E_TOLERANCE {
                break;
            }
        }
        self.eccentric_anomaly = E;
    }
    fn update_orb_pos(&mut self, time: f64) {
        self.update_E(time);
        let a = self.semimajor_axis;
        let E = self.eccentric_anomaly.to_radians();
        let e = self.eccentricity;
        let x = a * (E.cos() - e);
        let y = a * (1. - e * e).sqrt() * E.sin();
        self.orbital_position = DVec2::new(x, y);
        if self.revolution_period == 0. {
            return;
        }
        let Mdot = 2. * PI / self.revolution_period;
        let Edot = Mdot / (1. - e * E.cos());
        let Pdot = -a * (E.sin()) * Edot;
        let Qdot = a * (E.cos()) * Edot * (1. - e * e).sqrt();
        self.orbital_velocity = DVec2::new(Pdot, Qdot);
    }

    pub fn update_pos(&mut self, time: f64) {
        self.update_orb_pos(time);
        let o = self.arg_periapsis.to_radians();
        let O = self.long_asc_node.to_radians();
        let I = self.inclination.to_radians();
        self.local_pos = rotate(self.orbital_position, o, O, I);
        self.local_velocity = rotate(self.orbital_velocity, o, O, I);
    }
}

impl From<&BodyData> for EllipticalOrbit {
    fn from(data: &BodyData) -> Self {
        Self {
            eccentricity: data.eccentricity,
            semimajor_axis: data.semimajor_axis,
            inclination: data.inclination,
            long_asc_node: data.long_asc_node,
            arg_periapsis: data.arg_periapsis,
            initial_mean_anomaly: data.initial_mean_anomaly,
            revolution_period: data.revolution_period,
            mean_anomaly: data.initial_mean_anomaly,
            ..Default::default()
        }
    }
}

pub fn update_local(mut orbits: Query<&mut EllipticalOrbit>, time: Res<GameTime>) {
    orbits
        .par_iter_mut()
        .for_each(|mut o| o.update_pos(time.time()));
}

pub fn update_global(
    mut query: Query<(&mut Position, &mut Velocity, &EllipticalOrbit, &BodyInfo)>,
    primary: Query<&BodyInfo, With<PrimaryBody>>,
    mapping: Res<BodiesMapping>,
) {
    let mut queue = vec![(primary.single().0.id, (DVec3::ZERO, DVec3::ZERO))];
    let mut i = 0;
    while i < queue.len() {
        let (id, (parent_pos, parent_velocity)) = queue[i];
        if let Some(entity) = mapping.0.get(&id) {
            if let Ok((mut world_pos, mut world_velocity, orbit, info)) = query.get_mut(*entity) {
                let pos = parent_pos + orbit.local_pos;
                let velocity = parent_velocity + orbit.local_velocity;
                world_pos.0 = pos;
                world_velocity.0 = velocity;
                queue.extend(info.0.orbiting_bodies.iter().map(|c| (*c, (pos, velocity))));
            }
        }
        i += 1;
    }
}

#[derive(Resource)]
pub struct SystemSize(pub f64);

pub fn insert_system_size(mut commands: Commands, body_positions: Query<&Position>) {
    let system_size = body_positions
        .iter()
        .map(|pos| pos.0.length())
        .max_by(|a, b| a.total_cmp(b))
        .unwrap();
    commands.insert_resource(SystemSize(system_size));
}

#[cfg(test)]
mod tests {
    use bevy::app::App;

    use crate::prelude::*;

    #[test]
    fn test_update_local() {
        let mut app = App::new();
        app.add_plugins(ClientPlugin::testing().in_mode(ClientMode::Explorer));
        app.update();
        let world = app.world_mut();
        let mut query = world.query::<(&EllipticalOrbit, &BodyInfo)>();
        let (orbit, _) = query
            .iter(world)
            .find(|(_, BodyInfo(data))| data.id == id_from("terre"))
            .unwrap();
        let (earth_dist, earth_speed) = (orbit.local_pos.length(), orbit.local_velocity.length());
        assert!(147095000. <= earth_dist);
        assert!(earth_dist <= 152100000.);
        assert!((earth_speed / 24. - 107200.).abs() <= 20000.);
    }

    #[test]
    fn test_update_global() {
        let mut app = App::new();
        let plugins = ClientPlugin::testing()
            .in_mode(ClientMode::Explorer)
            .with_bodies(BodiesConfig::SmallestBodyType(BodyType::Moon));
        app.add_plugins(plugins);
        app.update();
        let world = app.world_mut();
        let mut query = world.query::<(&Position, &BodyInfo)>();
        let (&Position(moon_pos), _) = query
            .iter(world)
            .find(|(_, BodyInfo(data))| data.id == id_from("lune"))
            .unwrap();
        let moon_length = moon_pos.length();
        let min = 147095000. - 405500.;
        let max = 152100000. + 405500.;
        assert!(min <= moon_length);
        assert!(moon_length <= max)
    }
}
