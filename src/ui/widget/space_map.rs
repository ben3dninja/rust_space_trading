use bevy::{
    math::{DVec2, DVec3},
    prelude::*,
    utils::HashMap,
};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Stylize},
    widgets::{
        block::Title,
        canvas::{Canvas, Circle},
        Block, StatefulWidgetRef, WidgetRef,
    },
};

use crate::{prelude::*, utils::algebra::project_onto_plane};

pub const OFFSET_STEP: f64 = 1e8;
pub const ZOOM_STEP: f64 = 1.5;

#[derive(Debug)]
pub enum SpaceMapEvent {
    Zoom(Direction2),
    MapOffset(Direction4),
    MapOffsetReset,
    FocusBody,
    Autoscale,
}

#[derive(Debug, Resource)]
pub struct SpaceMap {
    pub offset_amount: DVec2,
    pub zoom_level: f64,
    pub system_size: f64,
    pub focus_body: Option<Entity>,
    pub selected: Option<Entity>,
}

impl SpaceMap {
    pub fn new(system_size: f64, focus_body: Option<Entity>, selected: Option<Entity>) -> SpaceMap {
        SpaceMap {
            offset_amount: DVec2::ZERO,
            zoom_level: 1.,
            system_size,
            focus_body,
            selected,
        }
    }

    pub fn zoom_in(&mut self) {
        self.zoom_level *= ZOOM_STEP;
    }

    pub fn zoom_out(&mut self) {
        self.zoom_level /= ZOOM_STEP;
    }

    pub fn zoom(&mut self, direction: Direction2) {
        match direction {
            Direction2::Up => self.zoom_in(),
            Direction2::Down => self.zoom_out(),
        }
    }

    pub fn offset(&mut self, direction: Direction4) {
        use Direction4::*;
        self.offset_amount += (match direction {
            Front | Right => 1.,
            _ => -1.,
        } * OFFSET_STEP
            / self.zoom_level)
            * match direction {
                Front | Back => DVec2::Y,
                _ => DVec2::X,
            }
    }

    pub fn reset_offset(&mut self) {
        self.offset_amount = DVec2::ZERO;
    }

    pub fn autoscale(&mut self, id_mapping: &HashMap<BodyID, Entity>, bodies: &Query<&BodyInfo>) {
        if let Some(focus_data) = self.focus_body.map(|f| &bodies.get(f).unwrap().0) {
            if let Some(max_dist) = focus_data
                .orbiting_bodies
                .iter()
                .filter_map(|id| {
                    id_mapping
                        .get(id)
                        .and_then(|&e| bodies.get(e).ok())
                        .map(|body| body.0.semimajor_axis)
                })
                .max_by(|a, b| a.total_cmp(b))
            {
                self.zoom_level = self.system_size / max_dist;
            }
        }
    }

    pub fn focus(&mut self, entity: Entity) {
        self.reset_offset();
        self.focus_body = Some(entity);
    }
}

#[derive(Default)]
pub struct SpaceMapWidget {
    circles: Vec<Circle>,
}

impl SpaceMapWidget {
    pub fn update_map(
        &mut self,
        space_map: &SpaceMap,
        query: &Query<(Entity, &Position, &BodyInfo)>,
    ) {
        let mut circles = Vec::new();
        let &Position(focus_pos) = space_map
            .focus_body
            .map_or(&Position::default(), |f| query.get(f).unwrap().1);
        for (entity, &Position(pos), BodyInfo(data)) in query.iter() {
            let proj =
                project_onto_plane(pos - focus_pos, (DVec3::X, DVec3::Y)) - space_map.offset_amount;
            let color = match data.body_type {
                _ if Some(entity) == space_map.selected => Color::Red,
                BodyType::Star => Color::Yellow,
                BodyType::Planet => Color::Blue,
                _ => Color::DarkGray,
            };
            let radius = data.radius;
            circles.push(Circle {
                x: proj.x,
                y: proj.y,
                radius,
                color,
            });
        }
        self.circles = circles;
    }
}

impl StatefulWidgetRef for SpaceMapWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State)
    where
        Self: Sized,
    {
        let (width, height) = (area.width as f64, area.height as f64);
        let scale = state.system_size / (width.min(height) * state.zoom_level);
        let (width, height) = (width * scale, height * scale);
        Canvas::default()
            .block(
                Block::bordered()
                    .title(Title::from("Space map".bold()).alignment(Alignment::Center)),
            )
            .x_bounds([-width / 2., width / 2.])
            .y_bounds([-height, height])
            .paint(|ctx| {
                for circle in &self.circles {
                    ctx.draw(circle);
                }
            })
            .render_ref(area, buf)
    }

    type State = SpaceMap;
}

#[cfg(test)]
mod tests {
    use bevy::app::App;

    use super::*;
    use crate::ui::screen::explorer::{ExplorerContext, ExplorerEvent};

    fn new_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            ClientPlugin::testing().in_mode(ClientMode::Explorer),
            TuiPlugin::testing(),
        ));
        app.update();
        app.update();
        app.update();
        app
    }

    #[test]
    fn test_update_space_map() {
        let app = new_app();
        let ctx = app.world().resource::<ExplorerContext>();
        let map_widget = &ctx.space_map;
        let map = app.world().resource::<SpaceMap>();
        assert_eq!(map_widget.circles.len(), 9);
        assert!(4459753056. < map.system_size);
        assert!(map.system_size < 4537039826.);
    }

    #[test]
    fn test_change_focus_body() {
        let mut app = new_app();
        let earth = id_from("terre");
        let mut ctx = app.world_mut().resource_mut::<ExplorerContext>();
        ctx.tree_state.select_body(earth);

        app.update();

        app.world_mut()
            .send_event(ExplorerEvent::SpaceMap(SpaceMapEvent::FocusBody));
        app.update();
        let map = app.world().resource::<SpaceMap>();
        let focus = map.focus_body.unwrap();

        let world = &mut app.world_mut();

        assert_eq!(
            world.query::<&BodyInfo>().get(world, focus).unwrap().0.id,
            earth
        );
    }
}
