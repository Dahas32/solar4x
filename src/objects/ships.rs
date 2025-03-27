//! A "Ship" is an object whose movement is governed by the gravitationnal
//! attraction of the celestial bodies, along with custom trajectories

use arrayvec::ArrayString;
use bevy::{math::DVec3, prelude::*, utils::HashMap};
use bevy_quinnet::client::QuinnetClient;
use serde::{Deserialize, Serialize};

use crate::game::{ClearOnUnload, Loaded};
use crate::network::{ClientChannel, ClientMessage};
use crate::physics::influence::HillRadius;
use crate::physics::leapfrog::get_acceleration;
use crate::physics::prelude::*;
use crate::prelude::ClientMode;

use super::id::MAX_ID_LENGTH;
use super::prelude::{BodiesMapping, BodyInfo, PrimaryBody};
use super::ObjectsUpdate;

pub mod trajectory;

// pub(crate) struct ShipID(u64);

// #[derive(Resource, Default)]
// struct ShipIDBuilder(NumberIncrementer);

// impl IDBuilder for ShipIDBuilder {
//     type ID = ShipID;

//     fn incrementer(&mut self) -> &mut NumberIncrementer {
//         &mut self.0
//     }

//     fn id_from_u64(u: u64) -> Self::ID {
//         ShipID(u)
//     }
// }

pub struct ShipsPlugin;

impl Plugin for ShipsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(trajectory::plugin)
            .add_event::<ShipEvent>()
            .add_systems(Update, handle_ship_events.in_set(ObjectsUpdate))
            .add_systems(OnEnter(Loaded), create_ships.in_set(ObjectsUpdate));
    }
}

pub type ShipID = ArrayString<MAX_ID_LENGTH>;

#[derive(Component, Clone, Default, PartialEq, Serialize, Deserialize, Debug, Copy)]
pub struct ShipInfo {
    pub id: ShipID,
    pub spawn_pos: DVec3,
    pub spawn_speed: DVec3,
}

#[derive(Resource, Default)]
pub struct ShipsMapping(pub HashMap<ShipID, Entity>);

#[derive(Event)]
pub enum ShipEvent {
    Create(ShipInfo),
    Remove(ShipID),
}

fn create_ships(mut commands: Commands) {
    commands.insert_resource(ShipsMapping::default());
}

#[derive(Serialize, Deserialize)]
pub struct CreateShipMsg {
    pub info: ShipInfo,
    pub acceleration: Acceleration,
    pub pos: Position,
    pub velocity: Velocity,
    // pub transform: TransformBundle,
    // pub clear_on_unload: ClearOnUnload,
}

fn handle_ship_events(
    mut commands: Commands,
    mut reader: EventReader<ShipEvent>,
    mut ships: ResMut<ShipsMapping>,
    mut client: Option<ResMut<QuinnetClient>>,
    client_mode: Option<Res<State<ClientMode>>>,
    bodies: Query<(&Position, &HillRadius, &BodyInfo)>,
    mapping: Res<BodiesMapping>,
    main_body: Query<&BodyInfo, With<PrimaryBody>>,
) {
    let multiplayer = in_state(ClientMode::Multiplayer)(client_mode);
    for event in reader.read() {
        match event {
            ShipEvent::Create(info) => {
                let pos = Position(info.spawn_pos);
                let influence =
                    Influenced::new(&pos, &bodies, mapping.as_ref(), main_body.single().0.id);
                ships.0.entry(info.id).or_insert({
                    commands
                        .spawn((
                            info.clone(),
                            Acceleration::new(get_acceleration(
                                info.spawn_pos,
                                bodies
                                    .iter_many(&influence.influencers)
                                    .map(|(p, _, i)| (p.0, i.0.mass)),
                            )),
                            influence.clone(),
                            pos,
                            Velocity(info.spawn_speed),
                            TransformBundle::from_transform(Transform::from_xyz(0., 0., 1.)),
                            ClearOnUnload,
                        ))
                        .id()
                });
                if multiplayer {
                    let msg = CreateShipMsg {
                        info: info.clone(),
                        acceleration: Acceleration::new(get_acceleration(
                            info.spawn_pos,
                            bodies
                                .iter_many(&influence.influencers)
                                .map(|(p, _, i)| (p.0, i.0.mass)),
                        )),
                        pos: pos,
                        velocity: Velocity(info.spawn_speed),
                    };
                    match client {
                        Some(ref mut client) => client
                            .connection_mut()
                            .send_message_on(ClientChannel::Once, ClientMessage::CreateShipMsg(msg))
                            .unwrap(),
                        None => (),
                    }
                };
            }
            ShipEvent::Remove(id) => {
                if let Some(e) = ships.0.remove(id) {
                    commands.entity(e).despawn()
                }
            }
        }
    }
}
