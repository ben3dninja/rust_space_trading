use bevy::prelude::*;

use crate::utils::Direction2;

/// Number of server updates (ticks) per real time second
pub const TPS: f32 = 1.;

/// Number of simulation updates (simticks) per real time second
pub const STPS: f64 = 64.;

/// Game time that is added at each client update (in days, multiplied by simstepsize)
pub const GAMETIME_PER_SIMTICK: f64 = 1e-3;

pub fn plugin(app: &mut App) {
    app.insert_resource(Time::<Fixed>::from_hz(STPS))
        .init_resource::<ToggleTime>()
        .init_resource::<GameTime>()
        .init_resource::<SimStepSize>()
        .init_resource::<TickTimer>()
        .add_event::<TimeEvent>()
        .add_event::<TickEvent>()
        .add_systems(
            FixedUpdate,
            (update_simtick, update_tick).in_set(TimeUpdate),
        )
        .add_systems(Update, handle_time_events);
}

#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct TimeUpdate;

#[derive(Resource, PartialEq, Default)]
pub struct ToggleTime(pub bool);

/// The elapsed time in game (stores simulation ticks)
#[derive(Resource, Default)]
pub struct GameTime {
    pub simtick: u64,
}

impl GameTime {
    pub fn time(&self) -> f64 {
        self.simtick as f64 * GAMETIME_PER_SIMTICK
    }
}

/// The number of simticks that are added at each update
#[derive(Resource)]
pub struct SimStepSize(pub u64);

impl Default for SimStepSize {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Resource)]
struct TickTimer(Timer);

impl Default for TickTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(1. / TPS, TimerMode::Repeating))
    }
}

#[derive(Event, Default)]
pub struct TickEvent;

#[derive(Event, Clone, Copy)]
pub enum TimeEvent {
    /// Change the number of simticks that are simulated per update.
    ///
    /// **THIS CAN CHANGE SIMULATION OUTCOME**
    ChangeStepSize(Direction2),
    /// Change the number of updates that are processed per real time second.
    ///
    /// This does not change simulation outcome, but leads to heavier CPU load.
    ChangeUpdateRate(Direction2),
    ToggleTime,
}

fn update_tick(mut timer: ResMut<TickTimer>, time: Res<Time>, mut writer: EventWriter<TickEvent>) {
    timer.0.tick(time.delta());
    if timer.0.just_finished() {
        writer.send_default();
    }
}

fn update_simtick(mut game_time: ResMut<GameTime>, step: Res<SimStepSize>) {
    game_time.simtick += step.0;
}

fn handle_time_events(
    mut reader: EventReader<TimeEvent>,
    mut toggle_time: ResMut<ToggleTime>,
    mut time: ResMut<Time<Virtual>>,
    mut step_size: ResMut<SimStepSize>,
) {
    use TimeEvent::*;
    for event in reader.read() {
        match event {
            ChangeUpdateRate(d) => {
                let speed = time.relative_speed_f64();
                time.set_relative_speed_f64(match d {
                    Direction2::Up => speed * 2.,
                    Direction2::Down => speed / 2.,
                })
            }
            ChangeStepSize(d) => match d {
                Direction2::Up => step_size.0 *= 2,
                Direction2::Down => step_size.0 /= 2,
            },
            ToggleTime => toggle_time.0 = !toggle_time.0,
        }
    }
}
