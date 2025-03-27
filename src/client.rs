use std::net::{IpAddr, Ipv4Addr};

use bevy::{ecs::query, prelude::*};
use bevy_quinnet::client::{
    certificate::CertificateVerificationMode, connection::ClientEndpointConfiguration,
    QuinnetClient, QuinnetClientPlugin,
};

use crate::{
    game::GamePlugin,
    network::{ClientChannel, ServerMessage},
    objects::prelude::BodiesConfig,
    physics::{prelude::Position, Velocity},
    prelude::{GameTime, Influenced, ShipInfo, ShipsMapping, ToggleTime},
    utils::ecs::exit_on_error_if_app,
};

pub mod prelude {
    pub use super::{ClientMode, ClientPlugin};
}

#[derive(Default)]
pub struct ClientPlugin {
    pub network_info: ClientNetworkInfo,
    pub server_info: ServerNetworkInfo,
    pub singleplayer_bodies_config: BodiesConfig,
    pub initial_mode: ClientMode,
    pub testing: bool,
}

#[derive(Resource)]
pub struct Testing;

impl ClientPlugin {
    pub fn testing() -> Self {
        Self {
            testing: true,
            ..Default::default()
        }
    }

    pub fn with_bodies(self, singleplayer_bodies_config: BodiesConfig) -> Self {
        Self {
            singleplayer_bodies_config,
            ..self
        }
    }

    pub fn in_mode(self, initial_mode: ClientMode) -> Self {
        Self {
            initial_mode,
            ..self
        }
    }
}

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        if self.testing {
            app.insert_resource(Testing);
        }
        app.add_plugins((
            GamePlugin {
                testing: self.testing,
            },
            QuinnetClientPlugin::default(),
        ))
        .insert_resource(self.network_info.clone())
        .insert_resource(self.server_info.clone())
        .insert_state(SyncStatus::NotSynced)
        .insert_resource(self.singleplayer_bodies_config.clone())
        .insert_state(self.initial_mode)
        .add_systems(
            OnEnter(ClientMode::Multiplayer),
            start_connection.pipe(exit_on_error_if_app),
        )
        .add_systems(
            OnEnter(ClientMode::Explorer),
            move |mut toggle: ResMut<ToggleTime>, mut time: ResMut<GameTime>| {
                toggle.0 = true;
                time.simtick = 0;
            },
        )
        .add_systems(
            OnEnter(ClientMode::None),
            move |mut toggle: ResMut<ToggleTime>| toggle.0 = false,
        )
        .add_systems(
            FixedUpdate,
            handle_server_messages.run_if(in_state(ClientMode::Multiplayer)),
        );
    }
}

#[derive(Default, States, Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub enum ClientMode {
    #[default]
    None,
    Singleplayer,
    Multiplayer,
    Explorer,
    Server,
}

#[derive(Clone, Resource)]
pub struct ClientNetworkInfo(pub IpAddr, pub u16);
impl Default for ClientNetworkInfo {
    fn default() -> Self {
        Self(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 0)
    }
}

#[derive(Clone, Resource)]
pub struct ServerNetworkInfo(pub IpAddr, pub u16);
impl Default for ServerNetworkInfo {
    fn default() -> Self {
        Self(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6000)
    }
}

fn start_connection(
    mut client: ResMut<QuinnetClient>,
    client_info: Res<ClientNetworkInfo>,
    server_info: Res<ServerNetworkInfo>,
) -> color_eyre::Result<()> {
    let ClientNetworkInfo(ca, cp) = *client_info;
    let ServerNetworkInfo(sa, sp) = *server_info;
    client.open_connection(
        ClientEndpointConfiguration::from_ips(sa, sp, ca, cp),
        CertificateVerificationMode::SkipVerification,
        ClientChannel::channels_configuration(),
    )?;
    Ok(())
}

#[derive(States, Debug, Default, Clone, PartialEq, Eq, Hash)]
pub enum SyncStatus {
    #[default]
    NotSynced,
    Synced,
}

fn handle_server_messages(
    mut client: ResMut<QuinnetClient>,
    mut commands: Commands,
    mut time: ResMut<GameTime>,
    mut sync: ResMut<NextState<SyncStatus>>,
    mut toggle_time: ResMut<ToggleTime>,
    mut query: Query<(&ShipInfo, &mut Position, &mut Velocity)>,
    ships: Res<ShipsMapping>,
) {
    while let Some((_, message)) = client
        .connection_mut()
        .try_receive_message::<ServerMessage>()
    {
        match message {
            ServerMessage::BodiesConfig(bodies) => {
                commands.insert_resource(bodies);
                sync.set(SyncStatus::Synced);
            }
            ServerMessage::UpdateTime(simtick) => time.simtick = simtick,
            ServerMessage::InitialData(initial_data) => {
                commands.insert_resource(initial_data.bodies_config);
                toggle_time.0 = initial_data.toggle_time;
                sync.set(SyncStatus::Synced);
            }
            ServerMessage::ToggleTime(b) => toggle_time.0 = b,
            ServerMessage::PeriodicUpdate(periodic_update) => {
                time.simtick = periodic_update.time;
                let new_ships = periodic_update.ships;
                for (id, pos, velocity) in new_ships {
                    let entity = ships.0.get(&id);
                    match entity {
                        Some(entity) => {
                            let mut tmp = query.get_mut(*entity).unwrap();
                            tmp.1 .0 = pos.0;
                            tmp.2 .0 = velocity.0;
                        }
                        None => (),
                    }
                }
            }
            _ => warn!("message not implemented on client side"),
        }
    }
}
