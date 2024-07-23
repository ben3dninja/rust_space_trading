use crate::{
    physics::predictions::Prediction,
    prelude::*,
    ui::{
        screen::editor::{EditorContext, PREDICTIONS_NUMBER},
        widget::space_map::SpaceMap,
        RenderSet,
    },
};
use bevy::{color::palettes::css::RED, prelude::*};

use super::MAX_HEIGHT;

pub fn plugin(app: &mut App) {
    app.add_systems(
        PostUpdate,
        (
            draw_predictions,
            draw_maneuver_node.run_if(resource_exists::<EditorContext>),
        )
            .in_set(RenderSet)
            .run_if(in_state(Loaded).and_then(resource_exists::<SpaceMap>)),
    );
}

fn draw_predictions(
    mut gizmos: Gizmos,
    predictions: Query<(&Transform, &Prediction)>,
    space_map: Res<SpaceMap>,
) {
    for (t, p) in predictions.iter() {
        gizmos.circle_2d(
            t.translation.xy(),
            (1. - p.index as f32 / PREDICTIONS_NUMBER as f32) * MAX_HEIGHT
                / (500. * space_map.zoom_level as f32),
            Color::srgba(1., 1., 1., 0.1),
        );
    }
}

fn draw_maneuver_node(
    mut gizmos: Gizmos,
    space_map: Res<SpaceMap>,
    context: Res<EditorContext>,
    positions: Query<&Transform>,
) {
    if let Some(e) = context.selected_prediction_entity() {
        gizmos.circle_2d(
            positions.get(e).unwrap().translation.xy(),
            MAX_HEIGHT / (100. * space_map.zoom_level as f32),
            RED,
        );
    }
}
