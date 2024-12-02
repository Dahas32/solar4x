use std::time::Duration;

use crate::{
    game::GameFiles,
    objects::ships::trajectory::{read_ship_trajectory, Trajectory, TrajectoryEvent},
    physics::{
        influence::HillRadius,
        predictions::{Prediction, PredictionStart},
    },
    prelude::*,
    ui::gui::SelectionRadius,
};
use bevy::{math::DVec3, prelude::*};

use super::{ClearOnEditorExit, EditorContext};

pub const PREDICTIONS_NUMBER: usize = 250_000;
const PREDICTION_DELAY: Duration = Duration::from_millis(200);
const PREDICTIONS_ADD_STEP: isize = 100;
const TICK_ADD_STEP: isize = 1;

pub fn plugin(app: &mut App) {
    app.add_event::<UpdateThrust>()
        .add_event::<ConfirmThrust>()
        .add_event::<PredictionDelayEvent>()
        .add_event::<ChangePredictionsNumber>()
        .add_event::<ChangeNodeTick>()
        .add_event::<ReloadPredictions>()
        .init_resource::<PredictionDelay>()
        .init_resource::<NumberOfPredictions>()
        .add_systems(
            OnEnter(super::InEditor),
            (
                read_nodes.pipe(exit_on_error_if_app),
                create_predictions,
                update_temp_predictions,
                copy_predictions,
            )
                .chain()
                .after(super::create_screen),
        )
        .add_systems(
            OnExit(super::InEditor),
            save_trajectory.before(super::clear_screen),
        )
        .add_systems(
            Update,
            (
                (
                    handle_change_predictions_number.run_if(on_event::<ChangePredictionsNumber>()),
                    handle_change_node_tick.run_if(on_event::<ChangeNodeTick>()),
                    handle_change_focus.run_if(
                        resource_exists::<Events<SelectObjectEvent>>
                            .and_then(on_event::<SelectObjectEvent>()),
                    ),
                    (
                        explicitly_clear_predictions,
                        create_predictions,
                        update_temp_predictions,
                        copy_predictions,
                    )
                        .chain()
                        .run_if(on_event::<ReloadPredictions>()),
                )
                    .chain(),
                handle_update_thrust.run_if(on_event::<UpdateThrust>()),
                (
                    tick_prediction_delay,
                    update_temp_predictions.run_if(on_event::<PredictionDelayEvent>()),
                )
                    .chain(),
                (
                    handle_confirm_thrust,
                    update_temp_predictions,
                    copy_predictions,
                )
                    .chain()
                    .run_if(on_event::<ConfirmThrust>()),
            )
                .run_if(resource_exists::<EditorContext>)
                .in_set(EventHandling),
        );
}

fn read_nodes(
    mut context: ResMut<EditorContext>,
    gamefiles: Res<GameFiles>,
) -> color_eyre::Result<()> {
    if let Ok(traj) = read_ship_trajectory(&gamefiles.trajectories, context.ship_info.id) {
        context.nodes = traj.nodes;
    }
    Ok(())
}

fn save_trajectory(mut traj_event: EventWriter<TrajectoryEvent>, ctx: Res<EditorContext>) {
    let ship = ctx.ship_info.id;
    traj_event.send_batch([
        TrajectoryEvent::Delete(ship),
        TrajectoryEvent::Create {
            ship,
            trajectory: Trajectory {
                nodes: ctx.nodes.clone(),
            },
        },
    ]);
}

#[derive(Bundle, Clone)]
struct PredictionBundle {
    prediction: Prediction,
    pos: Position,
    speed: Velocity,
    transform: TransformBundle,
    clear: ClearOnEditorExit,
}
impl PredictionBundle {
    fn from_prediction(prediction: Prediction) -> Self {
        Self {
            prediction,
            transform: TransformBundle::from_transform(Transform::from_xyz(0., 0., -3.)),
            pos: Position::default(),
            speed: Velocity::default(),
            clear: ClearOnEditorExit,
        }
    }
}
#[derive(Component)]
pub struct TempPrediction;

fn create_predictions(
    mut commands: Commands,
    mut ctx: ResMut<EditorContext>,
    predictions_number: Res<NumberOfPredictions>,
) {
    let (ship, tick) = (ctx.ship, ctx.simtick);
    (0..predictions_number.0).for_each(|i| {
        let pred = PredictionBundle::from_prediction(Prediction {
            ship,
            index: i,
            simtick: tick + i as u64,
        });
        ctx.predictions.push(
            commands
                .spawn((
                    pred.clone(),
                    SelectionRadius {
                        min_radius: MAX_HEIGHT / 100.,
                        actual_radius: 0.,
                    },
                ))
                .id(),
        );
        ctx.temp_predictions
            .push(commands.spawn((pred, TempPrediction)).id())
    });
}

fn explicitly_clear_predictions(mut commands: Commands, mut context: ResMut<EditorContext>) {
    let EditorContext {
        predictions,
        temp_predictions,
        ..
    } = context.as_mut();
    predictions
        .drain(0..)
        .chain(temp_predictions.drain(0..))
        .for_each(|e| commands.entity(e).despawn());
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct NumberOfPredictions(pub usize);

#[derive(Event, Clone, Copy)]
pub struct ChangePredictionsNumber {
    pub is_step: bool,
    pub amount: f32,
}

impl Default for NumberOfPredictions {
    fn default() -> Self {
        Self(PREDICTIONS_NUMBER)
    }
}

#[derive(Event, Default)]
pub struct ReloadPredictions;

fn handle_change_predictions_number(
    mut events: EventReader<ChangePredictionsNumber>,
    mut number: ResMut<NumberOfPredictions>,
    mut reload: EventWriter<ReloadPredictions>,
) {
    for event in events.read() {
        number.0 = number.0.saturating_add_signed(
            (if event.is_step {
                PREDICTIONS_ADD_STEP as f32
            } else {
                1.
            } * event.amount) as isize,
        );
        reload.send_default();
    }
}

#[derive(Event, Clone, Copy)]
pub struct ChangeNodeTick {
    pub is_step: bool,
    pub amount: f32,
}

fn handle_change_node_tick(
    mut events: EventReader<ChangeNodeTick>,
    mut ctx: ResMut<EditorContext>,
    mut reload: EventWriter<ReloadPredictions>,
) {
    for event in events.read() {
        if let Some(tick) = ctx.selected_tick() {
            let newtick = tick.saturating_add_signed(
                (if event.is_step {
                    TICK_ADD_STEP as f32
                } else {
                    1.
                } * event.amount) as i64,
            );
            ctx.change_tick(tick, newtick);
        }
        reload.send_default();
    }
}

fn handle_change_focus(
    mut events: EventReader<SelectObjectEvent>,
    mut reload: EventWriter<ReloadPredictions>,
    bodies: Query<&BodyInfo>,
    mut space_map: ResMut<SpaceMap>,
) {
    for event in events.read() {
        let e = event.entity;
        if bodies.get(e).is_ok() {
            space_map.focus(e);
            reload.send_default();
        }
    }
}

#[derive(Event, Clone)]
pub struct ConfirmThrust;

#[derive(Event, Clone)]
pub struct UpdateThrust(pub DVec3);

fn handle_confirm_thrust(
    mut context: ResMut<EditorContext>,
    mut traj_event: EventWriter<TrajectoryEvent>,
) {
    if let Some(thrust) = context.editing_data {
        let ship = context.ship_info.id;
        if let Some((&tick, node)) = context.selected_entry_mut() {
            node.thrust += thrust;
            traj_event.send(TrajectoryEvent::AddNode {
                ship,
                node: node.clone(),
                tick,
            });
        }
    }
    context.editing_data = None;
}

fn handle_update_thrust(
    mut thrust_updates: EventReader<UpdateThrust>,
    mut context: ResMut<EditorContext>,
) {
    for &UpdateThrust(thrust) in thrust_updates.read() {
        context.editing_data = Some(thrust);
    }
}

#[derive(Event, Default)]
struct PredictionDelayEvent;

#[derive(Resource)]
struct PredictionDelay(Timer);

impl Default for PredictionDelay {
    fn default() -> Self {
        Self(Timer::new(PREDICTION_DELAY, TimerMode::Repeating))
    }
}

fn tick_prediction_delay(
    mut timer: ResMut<PredictionDelay>,
    time: Res<Time>,
    mut event: EventWriter<PredictionDelayEvent>,
    ctx: Res<EditorContext>,
) {
    if ctx.editing_data.is_some() {
        timer.0.tick(time.delta());
        if timer.0.just_finished() {
            event.send_default();
        }
    }
}

fn update_temp_predictions(
    ctx: Res<EditorContext>,
    predictions_number: Res<NumberOfPredictions>,
    query: Query<(&Acceleration, &Influenced)>,
    mut bodies: Query<(&EllipticalOrbit, &BodyInfo, &HillRadius)>,
    bodies_mapping: Res<BodiesMapping>,
    mut coords: Query<(&mut Position, &mut Velocity), With<TempPrediction>>,
    space_map: Res<SpaceMap>,
) {
    let (&Acceleration { current: acc, .. }, influence) = query.get(ctx.ship).unwrap();
    let start = PredictionStart {
        pos: ctx.pos,
        speed: ctx.speed,
        simtick: ctx.simtick,
        acc,
    };
    let thrust = ctx.editing_data.unwrap_or_default();
    let mut nodes = ctx.nodes.clone();
    if let Some(tick) = ctx.selected_tick() {
        nodes.get_mut(&tick).unwrap().thrust += thrust;
    }
    let reference = space_map.focus_body.or(influence.main_influencer);
    let predictions = start.compute_predictions(
        predictions_number.0,
        influence,
        reference,
        &mut bodies.as_query_lens(),
        &bodies_mapping.0,
        &nodes,
    );
    let mut i = 0;
    let mut iter = coords.iter_many_mut(&ctx.temp_predictions);
    while let Some((mut pos, mut speed)) = iter.fetch_next() {
        (pos.0, speed.0) = predictions[i];
        i += 1;
    }
}

fn copy_predictions(
    ctx: Res<EditorContext>,
    new_coords: Query<(&Position, &Velocity), With<TempPrediction>>,
    mut coords: Query<(&mut Position, &mut Velocity), Without<TempPrediction>>,
) {
    let mut new_coords = new_coords.iter_many(&ctx.temp_predictions);
    let mut iter = coords.iter_many_mut(&ctx.predictions);
    while let (Some((mut pos, mut speed)), Some((new_pos, new_speed))) =
        (iter.fetch_next(), new_coords.next())
    {
        (pos.0, speed.0) = (new_pos.0, new_speed.0);
    }
}
