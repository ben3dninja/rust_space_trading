use std::collections::BTreeMap;

use bevy::{math::DVec3, prelude::*};
use bevy_ratatui::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::{
    layout::{Constraint, Layout},
    widgets::{Block, List, ListState, StatefulWidget},
};

use crate::{
    input::prelude::Keymap,
    objects::ships::trajectory::ManeuverNode,
    physics::{
        orbit::SystemSize,
        predictions::{Prediction, PredictionStart},
    },
    ui::{
        gui::{SelectObjectEvent, SelectionRadius, MAX_HEIGHT},
        widget::space_map::SpaceMap,
        EventHandling, InputReading,
    },
    utils::{list::ClampedList, Direction2},
};

use super::AppScreen;
use crate::objects::prelude::*;
use crate::physics::prelude::*;

pub const PREDICTIONS_NUMBER: usize = 120;

pub fn plugin(app: &mut App) {
    app.add_computed_state::<InEditor>()
        .add_event::<EditorEvent>()
        .add_systems(
            Update,
            (
                read_input.in_set(InputReading),
                (handle_select_prediction, handle_editor_events)
                    .chain()
                    .in_set(EventHandling),
            )
                .run_if(in_state(InEditor))
                .run_if(resource_exists::<EditorContext>),
        )
        .add_systems(
            OnEnter(InEditor),
            (create_screen, create_predictions, update_predictions).chain(),
        )
        .add_systems(OnExit(InEditor), clear_screen);
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct InEditor;

impl ComputedStates for InEditor {
    type SourceStates = AppScreen;

    fn compute(sources: Self::SourceStates) -> Option<Self> {
        match sources {
            AppScreen::Editor(_) => Some(Self),
            _ => None,
        }
    }
}

#[derive(Resource)]
pub struct EditorContext {
    pub ship: Entity,
    pub pos: DVec3,
    pub speed: DVec3,
    pub tick: u64,
    list_state: ListState,
    /// Each maneuver node is stored here along with the associated tick, and corresponds to a prediction.
    /// Since there is a prediction for each tick, the index of the prediction is simply the number of ticks
    /// that separate the start from the maneuver node
    nodes: BTreeMap<u64, ManeuverNode>,
    predictions: Vec<Entity>,
}

impl EditorContext {
    pub fn new(
        ship: Entity,
        &Position(pos): &Position,
        &Velocity(speed): &Velocity,
        tick: u64,
    ) -> Self {
        Self {
            ship,
            pos,
            speed,
            tick,
            list_state: ListState::default(),
            nodes: BTreeMap::new(),
            predictions: Vec::new(),
        }
    }

    pub fn selected_node(&self) -> Option<&ManeuverNode> {
        self.selected_entry().map(|(_, n)| n)
    }

    pub fn selected_tick(&self) -> Option<u64> {
        self.selected_entry().map(|(t, _)| *t)
    }

    /// Attempts to select the node at the provided tick, returning the index if successful
    pub fn select_tick(&mut self, tick: u64) -> Option<usize> {
        self.nodes.keys().position(|t| *t == tick).map(|i| {
            self.list_state.select(Some(i));
            i
        })
    }

    pub fn selected_entry(&self) -> Option<(&u64, &ManeuverNode)> {
        self.list_state
            .selected()
            .and_then(|i| self.nodes.iter().nth(i))
    }

    fn index_of_prediction_at_tick(&self, tick: u64) -> usize {
        (tick - self.tick) as usize
    }

    fn prediction_at_tick(&self, tick: u64) -> Option<Entity> {
        self.predictions
            .get(self.index_of_prediction_at_tick(tick))
            .cloned()
    }

    pub fn selected_prediction_entity(&self) -> Option<Entity> {
        self.selected_tick()
            .and_then(|t| self.prediction_at_tick(t))
    }

    pub fn get_node(&self, simtick: u64) -> Option<&ManeuverNode> {
        self.nodes.get(&simtick)
    }

    pub fn select_or_insert(&mut self, simtick: u64, default: ManeuverNode) {
        self.nodes.entry(simtick).or_insert(default);
        self.select_tick(simtick);
    }
}
impl ClampedList for EditorContext {
    fn list_state(&mut self) -> &mut ListState {
        &mut self.list_state
    }

    fn len(&self) -> usize {
        self.nodes.len()
    }
}

#[derive(Component)]
pub struct ClearOnEditorExit;

pub struct EditorScreen;

#[allow(clippy::too_many_arguments)]
fn create_screen(
    mut commands: Commands,
    screen: Res<State<AppScreen>>,
    coords: Query<(&Position, &Velocity, &Influenced)>,
    ships_mapping: Res<ShipsMapping>,
    bodies_mapping: Res<BodiesMapping>,
    bodies: Query<&BodyInfo>,
    system_size: Res<SystemSize>,
    time: Res<GameTime>,
) {
    if let AppScreen::Editor(id) = screen.get() {
        if let Some(e) = ships_mapping.0.get(id) {
            let (
                pos,
                speed,
                Influenced {
                    main_influencer, ..
                },
            ) = coords.get(*e).unwrap();
            commands.insert_resource(EditorContext::new(*e, pos, speed, time.simtick));
            let mut map = SpaceMap::new(system_size.0, *main_influencer, *main_influencer);
            map.autoscale(&bodies_mapping.0, &bodies);
            commands.insert_resource(map);
        }
    }
}

fn create_predictions(mut commands: Commands, mut ctx: ResMut<EditorContext>) {
    let (ship, tick) = (ctx.ship, ctx.tick);
    (0..PREDICTIONS_NUMBER).for_each(|i| {
        ctx.predictions.push(
            commands
                .spawn((
                    Prediction {
                        ship,
                        index: i,
                        simtick: tick + i as u64,
                    },
                    Position::default(),
                    TransformBundle::from_transform(Transform::from_xyz(0., 0., -3.)),
                    SelectionRadius {
                        min_radius: MAX_HEIGHT / 100.,
                        actual_radius: 0.,
                    },
                    ClearOnEditorExit,
                ))
                .id(),
        )
    });
}

fn update_predictions(
    ctx: Res<EditorContext>,
    query: Query<(&Acceleration, &Influenced)>,
    bodies: Query<(&EllipticalOrbit, &BodyInfo)>,
    bodies_mapping: Res<BodiesMapping>,
    mut positions: Query<&mut Position, With<Prediction>>,
) {
    let (
        &Acceleration { current: acc, .. },
        Influenced {
            main_influencer,
            influencers,
        },
    ) = query.get(ctx.ship).unwrap();
    let start = PredictionStart {
        pos: ctx.pos,
        speed: ctx.speed,
        tick: ctx.tick,
        acc,
    };
    let predictions = start.compute_predictions(
        PREDICTIONS_NUMBER,
        influencers.iter().cloned(),
        *main_influencer,
        &bodies,
        &bodies_mapping.0,
    );
    let mut i = 0;
    let mut iter = positions.iter_many_mut(&ctx.predictions);
    while let Some(mut pos) = iter.fetch_next() {
        pos.0 = predictions[i];
        i += 1;
    }
}

fn clear_screen(mut commands: Commands, query: Query<Entity, With<ClearOnEditorExit>>) {
    commands.remove_resource::<EditorContext>();
    commands.remove_resource::<SpaceMap>();
    query.iter().for_each(|e| commands.entity(e).despawn());
}

fn read_input(
    mut key_event: EventReader<KeyEvent>,
    keymap: Res<Keymap>,
    mut internal_event: EventWriter<EditorEvent>,
) {
    use Direction2::*;
    use EditorEvent::*;
    let keymap = &keymap.editor;
    for event in key_event.read() {
        if event.kind == KeyEventKind::Release {
            return;
        }
        internal_event.send(match event {
            e if keymap.select_next.matches(e) => SelectAdjacent(Down),
            e if keymap.select_previous.matches(e) => SelectAdjacent(Up),
            e if keymap.back.matches(e) => Back,
            // e if keymap.new_node.matches(e) => NewNode(None),
            _ => return,
        });
    }
}

#[derive(Event, Clone, Copy)]
pub enum EditorEvent {
    SelectAdjacent(Direction2),
    SelectOrInsert(u64),
    Back,
}

fn handle_editor_events(
    mut screen: ResMut<NextState<AppScreen>>,
    mut context: ResMut<EditorContext>,
    mut events: EventReader<EditorEvent>,
    bodies: Query<&BodyInfo>,
    primary: Query<&BodyInfo, With<PrimaryBody>>,
    space_map: Res<SpaceMap>,
) {
    for event in events.read() {
        match *event {
            EditorEvent::SelectAdjacent(d) => context.select_adjacent(d),
            EditorEvent::SelectOrInsert(tick) => {
                let origin = space_map
                    .focus_body
                    .map_or(primary.single().0.id, |e| bodies.get(e).unwrap().0.id);
                context.select_or_insert(
                    tick,
                    ManeuverNode {
                        name: "Node".into(),
                        thrust: DVec3::ZERO,
                        origin,
                    },
                );
            }
            EditorEvent::Back => screen.set(AppScreen::Fleet),
        }
    }
}

fn handle_select_prediction(
    mut select_events: EventReader<SelectObjectEvent>,
    mut editor_events: EventWriter<EditorEvent>,
    predictions: Query<&Prediction>,
) {
    for event in select_events.read() {
        if let Ok(p) = predictions.get(event.entity) {
            editor_events.send(EditorEvent::SelectOrInsert(p.simtick));
        }
    }
}

impl StatefulWidget for EditorScreen {
    type State = EditorContext;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let chunks =
            Layout::horizontal([Constraint::Percentage(30), Constraint::Fill(1)]).split(area);
        List::new(state.nodes.values().map(|n| &n.name[..]))
            .highlight_symbol(">")
            .block(Block::bordered().title_top("Maneuver nodes"))
            .render(chunks[0], buf, &mut state.list_state);
    }
}
