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
        time::GAMETIME_PER_SIMTICK,
    },
    ui::{widget::space_map::SpaceMap, EventHandling, InputReading},
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
                handle_editor_events.in_set(EventHandling),
            )
                .run_if(in_state(InEditor))
                .run_if(resource_exists::<EditorContext>),
        )
        .add_systems(
            OnEnter(InEditor),
            (create_screen, create_predictions).chain(),
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
    list_state: ListState,
    nodes: Vec<ManeuverNode>,
}

impl EditorContext {
    pub fn new(ship: Entity, &Position(pos): &Position, &Velocity(speed): &Velocity) -> Self {
        Self {
            ship,
            pos,
            speed,
            list_state: ListState::default(),
            nodes: Vec::new(),
        }
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

fn create_screen(
    mut commands: Commands,
    screen: Res<State<AppScreen>>,
    coords: Query<(&Position, &Velocity, &Influenced)>,
    ships_mapping: Res<ShipsMapping>,
    bodies_mapping: Res<BodiesMapping>,
    bodies: Query<&BodyInfo>,
    system_size: Res<SystemSize>,
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
            commands.insert_resource(EditorContext::new(*e, pos, speed));
            let mut map = SpaceMap::new(system_size.0, *main_influencer, *main_influencer);
            map.autoscale(&bodies_mapping.0, &bodies);
            commands.insert_resource(map);
        }
    }
}

fn create_predictions(
    mut commands: Commands,
    ctx: Res<EditorContext>,
    query: Query<(&Acceleration, &Influenced)>,
    bodies: Query<(&EllipticalOrbit, &BodyInfo)>,
    bodies_mapping: Res<BodiesMapping>,
    time: Res<GameTime>,
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
        time: time.time(),
        acc,
    };
    let predictions = start.compute_predictions(
        PREDICTIONS_NUMBER,
        influencers.iter().cloned(),
        *main_influencer,
        &bodies,
        &bodies_mapping.0,
    );
    predictions.into_iter().enumerate().for_each(|(i, p)| {
        commands.spawn((
            Prediction {
                ship: ctx.ship,
                index: i,
            },
            Position(p),
            TransformBundle::from_transform(Transform::from_xyz(0., 0., -3.)),
            ClearOnEditorExit,
        ));
    });
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
            e if keymap.select_next.matches(e) => Select(Down),
            e if keymap.select_previous.matches(e) => Select(Up),
            e if keymap.back.matches(e) => Back,
            e if keymap.new_node.matches(e) => NewNode,
            _ => return,
        });
    }
}

#[derive(Event)]
pub enum EditorEvent {
    Select(Direction2),
    NewNode,
    Back,
}

pub fn handle_editor_events(
    mut screen: ResMut<NextState<AppScreen>>,
    mut context: ResMut<EditorContext>,
    mut events: EventReader<EditorEvent>,
    influencer: Query<&Influenced>,
    bodies: Query<&BodyInfo>,
    primary: Query<&BodyInfo, With<PrimaryBody>>,
) {
    for event in events.read() {
        match event {
            EditorEvent::Select(d) => context.select_adjacent(*d),
            EditorEvent::NewNode => {
                let origin = influencer
                    .get(context.ship)
                    .unwrap()
                    .main_influencer
                    .map(|e| bodies.get(e).unwrap().0.id)
                    .unwrap_or(primary.single().0.id);
                let time = context
                    .nodes
                    .last()
                    .map(|n| n.time + GAMETIME_PER_SIMTICK)
                    .unwrap_or_default();
                context.nodes.push(ManeuverNode {
                    name: "Node".into(),
                    time,
                    thrust: DVec3::ZERO,
                    origin,
                });
            }
            EditorEvent::Back => screen.set(AppScreen::Fleet),
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
        List::new(state.nodes.iter().map(|n| &n.name[..]))
            .highlight_symbol(">")
            .block(Block::bordered().title_top("Maneuver nodes"))
            .render(chunks[0], buf, &mut state.list_state);
    }
}
