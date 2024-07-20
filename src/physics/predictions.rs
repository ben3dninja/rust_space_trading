use bevy::{math::DVec3, prelude::*, utils::HashMap};

use crate::{objects::prelude::*, physics::prelude::*};

use super::{
    leapfrog::{get_acceleration, get_dv, get_dx},
    time::GAMETIME_PER_SIMTICK,
};

/// Number of client updates between two predictions
const PREDICTIONS_STEP: usize = 20;

/// A component representing identifying a prediction of a ship at a selected time
#[derive(Component)]
pub struct Prediction {
    pub ship: Entity,
    pub index: usize,
}

#[derive(Debug)]
pub struct PredictionStart {
    pub pos: DVec3,
    pub speed: DVec3,
    pub acc: DVec3,
    pub time: f64,
}

impl PredictionStart {
    /// Compute the future positions of this point with respect to a given referential and considering some influencer's gravitationnal pull on it
    pub fn compute_predictions(
        &self,
        number: usize,
        influencers: impl Iterator<Item = Entity> + Clone,
        reference: Option<Entity>,
        bodies: &Query<(&EllipticalOrbit, &BodyInfo)>,
        mapping: &HashMap<BodyID, Entity>,
    ) -> Vec<DVec3> {
        let dt = GAMETIME_PER_SIMTICK * PREDICTIONS_STEP as f64;
        let mut pos = self.pos;
        let mut speed = self.speed;
        let mut predictions = Vec::new();
        let ref_index = reference.and_then(|r| influencers.clone().position(|e| e == r));
        let masses = influencers
            .clone()
            .map(|e| bodies.get(e).unwrap().1 .0.mass);
        let initial_positions = get_bodies_pos(influencers.clone(), bodies, mapping, self.time);
        let initial_ref_pos = ref_index.map_or(DVec3::ZERO, |i| initial_positions[i]);
        let mut acc = self.acc;
        let mut previous_acc;
        for i in 1..number + 1 {
            pos += get_dx(speed, acc, dt);
            let positions = get_bodies_pos(
                influencers.clone(),
                bodies,
                mapping,
                self.time + (i * PREDICTIONS_STEP) as f64 * GAMETIME_PER_SIMTICK,
            );
            let ref_pos = ref_index.map_or(DVec3::ZERO, |i| positions[i]);
            previous_acc = acc;
            acc = get_acceleration(pos, positions.into_iter().zip(masses.clone()));
            speed += get_dv(previous_acc, acc, dt);
            predictions.push(pos - ref_pos + initial_ref_pos);
        }
        predictions
    }
}

fn get_bodies_pos(
    selected_bodies: impl Iterator<Item = Entity>,
    bodies: &Query<(&EllipticalOrbit, &BodyInfo)>,
    mapping: &HashMap<BodyID, Entity>,
    time: f64,
) -> Vec<DVec3> {
    fn compute_pos_rec(
        e: Entity,
        map: &mut HashMap<Entity, DVec3>,
        time: f64,
        bodies: &Query<(&EllipticalOrbit, &BodyInfo)>,
        mapping: &HashMap<BodyID, Entity>,
    ) -> DVec3 {
        let (orbit, BodyInfo(data)) = bodies.get(e).unwrap();
        let mut o = orbit.clone();
        o.update_pos(time);
        if let Some(pos) = map.get(&e) {
            *pos
        } else {
            let pos = data.host_body.map_or(DVec3::ZERO, |parent| {
                compute_pos_rec(mapping[&parent], map, time, bodies, mapping) + o.local_pos
            });
            map.insert(e, pos);
            pos
        }
    }

    let mut map = HashMap::new();

    selected_bodies
        .map(|e| compute_pos_rec(e, &mut map, time, bodies, mapping))
        .collect()
}

#[cfg(test)]
mod tests {
    use bevy::{ecs::system::SystemState, prelude::*};

    use crate::{
        physics::leapfrog::get_acceleration, prelude::*, utils::algebra::circular_orbit_around_body,
    };

    use super::*;

    #[test]
    fn test_predictions() {
        let mut app = App::new();
        app.add_plugins(ClientPlugin::testing().in_mode(ClientMode::Singleplayer));
        app.update();
        let world = app.world_mut();
        let mapping = &world.resource::<BodiesMapping>().0;
        let earth = *mapping.get(&id_from("terre")).unwrap();
        let sun = *mapping.get(&id_from("soleil")).unwrap();
        let (&mass, &earth_pos, &earth_speed) = world
            .query::<(&Mass, &Position, &Velocity)>()
            .get(world, earth)
            .unwrap();
        let (pos, speed) = circular_orbit_around_body(1e5, mass.0, earth_pos.0, earth_speed.0);
        let influencers = vec![sun, earth];
        #[allow(clippy::type_complexity)]
        let mut system_state: SystemState<(
            Res<BodiesMapping>,
            Query<(&EllipticalOrbit, &BodyInfo)>,
            Query<(&Position, &Mass)>,
        )> = SystemState::new(world);
        let (mapping, bodies, query) = system_state.get(world);
        let predictions = PredictionStart {
            pos,
            speed,
            time: 0.,
            acc: get_acceleration(pos, query.iter_many(&influencers).map(|(p, m)| (p.0, m.0))),
        }
        .compute_predictions(3, influencers.into_iter(), Some(earth), &bodies, &mapping.0);
        for (i, p) in predictions.into_iter().enumerate() {
            // dbg!(p);
            // dbg!(pos + (i + 1) as f64 * (speed - earth_speed.0) * GAMETIME_PER_SIMTICK);
            assert!(
                (p - (pos + (i + 1) as f64 * (speed - earth_speed.0) * GAMETIME_PER_SIMTICK))
                    .length()
                    <= 5e4
            );
        }
    }
}
