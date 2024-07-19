use std::{
    fs::{read_dir, remove_file, File},
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

use bevy::{math::DVec3, prelude::*};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::{
    game::{Authoritative, GameFiles},
    objects::prelude::{BodiesMapping, BodyID},
    physics::prelude::*,
    prelude::{exit_on_error_if_app, GameStage},
    utils::algebra::convert_orbital_to_global,
};

use super::{ShipID, ShipInfo, ShipsMapping};

pub const TRAJECTORIES_PATH: &str = "trajectories";

pub fn plugin(app: &mut App) {
    app.add_event::<TrajectoryEvent>()
        .add_event::<VelocityUpdate>()
        .add_systems(FixedUpdate, handle_thrusts.in_set(TrajectoryUpdate))
        .add_systems(
            OnEnter(GameStage::Action),
            dispatch_trajectories.run_if(in_state(Authoritative)),
        )
        .add_systems(Update, handle_trajectory_event.pipe(exit_on_error_if_app));
}

#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct TrajectoryUpdate;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ManeuverNode {
    pub name: String,
    pub time: f64,
    pub thrust: DVec3,
    pub origin: BodyID,
}

/// A succession of maneuver nodes sorted by order of time, with a single node per time
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Trajectory {
    nodes: Vec<ManeuverNode>,
}

pub enum TrajectoryError {
    MultipleNodesPerTime,
    NotSorted,
    IndexOutOfBounds,
}

impl Trajectory {
    pub fn get_nodes(&self) -> &Vec<ManeuverNode> {
        &self.nodes
    }

    pub fn push(&mut self, node: ManeuverNode) -> Result<(), TrajectoryError> {
        self.insert(self.nodes.len(), node)
    }

    pub fn insert(&mut self, index: usize, node: ManeuverNode) -> Result<(), TrajectoryError> {
        if index > self.nodes.len() {
            return Err(TrajectoryError::IndexOutOfBounds);
        }
        if let Some(previous) = index.checked_sub(1).and_then(|i| self.nodes.get(i)) {
            if previous.time > node.time {
                return Err(TrajectoryError::NotSorted);
            }
            if previous.time == node.time {
                return Err(TrajectoryError::MultipleNodesPerTime);
            }
        }
        if let Some(next) = self.nodes.get(index) {
            if next.time < node.time {
                return Err(TrajectoryError::NotSorted);
            }
            if next.time == node.time {
                return Err(TrajectoryError::MultipleNodesPerTime);
            }
        }
        self.nodes.insert(index, node);
        Ok(())
    }
}

/// A trajectory taken by an object, storing the index of the last processed maneuver node in the action stage the instance was created
#[derive(Component, Debug, Clone)]
pub struct CurrentTrajectory {
    trajectory: Trajectory,
    current_node: usize,
}

impl CurrentTrajectory {
    pub fn new(trajectory: Trajectory) -> Self {
        Self {
            trajectory,
            current_node: 0,
        }
    }

    pub fn current(&self) -> Option<&ManeuverNode> {
        self.trajectory.nodes.get(self.current_node)
    }

    pub fn advance(&mut self) {
        self.current_node += 1;
    }
}

#[derive(Event, Debug, Clone)]
pub enum TrajectoryEvent {
    Create {
        ship: ShipID,
        trajectory: Trajectory,
    },
    Delete(ShipID),
    AddNode {
        ship: ShipID,
        node: ManeuverNode,
    },
    PopNode(ShipID),
}

#[derive(Event, Debug)]
pub struct VelocityUpdate {
    pub ship_id: ShipID,
    pub thrust: DVec3,
}

pub fn read_trajectory(path: impl AsRef<Path>) -> color_eyre::Result<Trajectory> {
    let mut file = File::open(&path)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    toml::from_str(&buf).map_err(color_eyre::eyre::Error::from)
}

pub fn write_trajectory(path: impl AsRef<Path>, t: &Trajectory) -> color_eyre::Result<()> {
    let s = toml::to_string_pretty(t)?;
    File::create(path)?
        .write_all(s.as_bytes())
        .map_err(color_eyre::eyre::Error::from)
}

pub fn follow_trajectory(
    mut velocity_events: EventWriter<VelocityUpdate>,
    mapping: Res<BodiesMapping>,
    coords: Query<(&Position, &Velocity)>,
    mut trajectories: Query<(Entity, &mut CurrentTrajectory, &ShipInfo)>,
    time: Res<GameTime>,
) {
    let events = Arc::new(Mutex::new(Vec::new()));
    trajectories.par_iter_mut().for_each(|(e, mut t, info)| {
        if let Some(n) = t.current() {
            if n.time >= time.time() {
                if let Some(origin) = mapping.0.get(&n.origin) {
                    let (&Position(o_pos), &Velocity(o_speed)) = coords.get(*origin).unwrap();
                    let (&Position(pos), &Velocity(speed)) = coords.get(e).unwrap();
                    let thrust = convert_orbital_to_global(n.thrust, o_pos, o_speed, pos, speed);
                    events.lock().unwrap().push(VelocityUpdate {
                        ship_id: info.id,
                        thrust,
                    });
                }
                t.advance();
            }
        }
    });
    velocity_events.send_batch(Arc::try_unwrap(events).unwrap().into_inner().unwrap());
}

pub fn handle_thrusts(
    mut velocity_events: EventReader<VelocityUpdate>,
    mut speeds: Query<&mut Velocity>,
    mapping: Res<ShipsMapping>,
) {
    for event in velocity_events.read() {
        if let Some(entity) = mapping.0.get(&event.ship_id) {
            speeds.get_mut(*entity).unwrap().0 += event.thrust;
        }
    }
}

pub fn dispatch_trajectories(
    mut commands: Commands,
    dir: Res<GameFiles>,
    mapping: Res<ShipsMapping>,
) {
    if let Ok(dir) = read_dir(&dir.trajectories) {
        for entry in dir.flatten() {
            let path = entry.path();
            if let Ok(traj) = read_trajectory(&path) {
                if let Some(e) = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .and_then(|s| ShipID::from(s).ok())
                    .and_then(|id| mapping.0.get(&id))
                {
                    commands.entity(*e).insert(CurrentTrajectory::new(traj));
                }
            }
        }
    }
}

pub fn handle_trajectory_event(
    mut reader: EventReader<TrajectoryEvent>,
    dir: Res<GameFiles>,
) -> color_eyre::Result<()> {
    use TrajectoryEvent::*;
    for event in reader.read() {
        let path = dir.trajectories.join(
            match event {
                Create { ship, .. } => ship,
                Delete(s) => s,
                AddNode { ship, .. } => ship,
                PopNode(s) => s,
            }
            .to_string(),
        );
        match event {
            Create { trajectory, .. } => {
                write_trajectory(path, trajectory)?;
            }
            Delete(_) => remove_file(path)?,
            AddNode { node, .. } => {
                let mut t = read_trajectory(&path)?;
                t.nodes.push(node.clone());
                write_trajectory(path, &t)?;
            }
            PopNode(_) => {
                let mut t = read_trajectory(&path)?;
                t.nodes.pop();
                write_trajectory(path, &t)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{read_dir, File},
        io::Read,
    };

    use bevy::{app::App, math::DVec3, state::state::NextState};

    use crate::{objects::ships::ShipEvent, prelude::*};

    use super::*;

    fn new_app() -> App {
        let mut app = App::new();
        app.add_plugins(ClientPlugin::testing().in_mode(ClientMode::Singleplayer));
        app.update();
        app
    }

    fn new_trajectory() -> Trajectory {
        Trajectory {
            nodes: vec![ManeuverNode {
                name: "1".to_owned(),
                time: 0.,
                thrust: DVec3::new(1e4, 0., 0.),
                origin: id_from("terre"),
            }],
        }
    }

    #[test]
    fn test_handle_trajectory_event() -> color_eyre::Result<()> {
        let mut app = new_app();
        let trajectory = new_trajectory();
        app.world_mut().send_event(TrajectoryEvent::Create {
            ship: ShipID::from("s")?,
            trajectory: trajectory.clone(),
        });
        app.update();
        let path = app.world_mut().resource::<GameFiles>().trajectories.clone();
        let mut files = read_dir(path).unwrap();
        let file_path = files.next().unwrap()?.path();
        let mut buf = String::new();
        File::open(&file_path)?.read_to_string(&mut buf)?;
        let traj = read_trajectory(file_path)?;
        assert_eq!(traj, trajectory);
        Ok(())
    }

    #[test]
    fn test_dispatch_trajectory() {
        let mut app = new_app();
        let id = id_from("s");
        app.world_mut().send_event(ShipEvent::Create(ShipInfo {
            id,
            spawn_pos: DVec3::new(1e6, 0., 0.),
            spawn_speed: DVec3::new(0., 1e6, 0.),
        }));
        let trajectory = new_trajectory();
        app.world_mut().send_event(TrajectoryEvent::Create {
            ship: id,
            trajectory: trajectory.clone(),
        });
        app.update();
        app.world_mut()
            .resource_mut::<NextState<GameStage>>()
            .set(GameStage::Action);
        app.update();
        let world = app.world_mut();
        let mut query = world.query::<&CurrentTrajectory>();
        assert_eq!(query.single(world).trajectory, trajectory);
    }
}
