use std::net::IpAddr;
use std::result::Result::Ok;

use crate::client::ClientMode;
use crate::game::ClearOnUnload;
use crate::network::PeriodicUpdate;
use crate::physics::influence::HillRadius;
use crate::physics::time::{SimStepSize, ToggleTime};
use crate::physics::{PhysicsUpdate, Position, Velocity};
use crate::prelude::{
    Acceleration, BodiesMapping, BodyInfo, Influenced, PrimaryBody, ShipID, ShipInfo, ShipsMapping,
};
use bevy::prelude::*;
use bevy::tasks::block_on;
use bevy::tasks::{poll_once, AsyncComputeTaskPool, Task};
use bevy::utils::hashbrown::HashMap;
use bevy_quinnet::{
    server::{
        certificate::CertificateRetrievalMode, QuinnetServer, QuinnetServerPlugin,
        ServerEndpointConfiguration,
    },
    shared::ClientId,
};
use std::io::{self, BufRead};
pub mod prelude {
    pub use super::{ServerNetworkInfo, ServerPlugin};
}
#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct CommandSet;

use crate::{
    game::GamePlugin,
    network::{ClientMessage, InitialData, ServerChannel, ServerMessage},
    prelude::{BodiesConfig, GameTime},
    utils::ecs::exit_on_error_if_app,
};

pub struct ServerPlugin {
    pub server_address: ServerNetworkInfo,
    pub config: BodiesConfig,
}

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((GamePlugin::default(), QuinnetServerPlugin::default()))
            .add_event::<ClientConnectionEvent>()
            .insert_state(ClientMode::Server)
            .insert_resource(TaskCommand::default())
            .insert_state(Reading::default())
            .insert_state(Command::default())
            .add_systems(Update, (handle_stdin, read_stdin))
            .add_systems(FixedUpdate, handle_client_messages.in_set(PhysicsUpdate))
            .add_systems(OnExit(Command::None), handle_command.in_set(CommandSet))
            .add_systems(OnEnter(Command::TestSetPos), test_set_pos)
            .insert_resource(self.server_address.clone())
            .insert_resource(self.config.clone())
            .insert_resource(Clients::default())
            .insert_resource(PeriodicUpdatesTimer(Timer::from_seconds(
                1. / 60.,
                TimerMode::Repeating,
            )))
            .insert_resource(Arguments(String::new()))
            .add_systems(Startup, start_endpoint.pipe(exit_on_error_if_app))
            .add_systems(
                Update,
                (
                    update_clients,
                    handle_connection_events.pipe(exit_on_error_if_app),
                    send_periodic_updates,
                ),
            );
    }
}

#[derive(Clone, Resource)]
pub struct ServerNetworkInfo(pub IpAddr, pub u16);

#[derive(Resource, Default)]
struct Clients(Vec<ClientId>);

#[derive(Event)]
enum ClientConnectionEvent {
    Connected(ClientId),
    Disconnected(ClientId),
}

#[derive(Resource)]
struct PeriodicUpdatesTimer(Timer);

fn start_endpoint(
    mut server: ResMut<QuinnetServer>,
    network_info: Res<ServerNetworkInfo>,
) -> color_eyre::Result<()> {
    server.start_endpoint(
        ServerEndpointConfiguration::from_ip(network_info.0, network_info.1),
        CertificateRetrievalMode::GenerateSelfSigned {
            server_hostname: "rust_space_trading_server".into(),
        },
        ServerChannel::channels_configuration(),
    )?;
    Ok(())
}

fn update_clients(
    mut clients: ResMut<Clients>,
    server: ResMut<QuinnetServer>,
    mut writer: EventWriter<ClientConnectionEvent>,
) {
    let updated_clients = server.endpoint().clients();
    for client in &updated_clients {
        if !clients.0.contains(client) {
            writer.send(ClientConnectionEvent::Connected(*client));
        }
    }
    for client in &clients.0 {
        if !updated_clients.contains(client) {
            writer.send(ClientConnectionEvent::Disconnected(*client));
        }
    }
    clients.0 = updated_clients;
}

fn handle_connection_events(
    mut reader: EventReader<ClientConnectionEvent>,
    mut server: ResMut<QuinnetServer>,
    time_toggle: Res<ToggleTime>,
    bodies_config: Res<BodiesConfig>,
) -> color_eyre::Result<()> {
    let endpoint = server.endpoint_mut();
    for event in reader.read() {
        match event {
            ClientConnectionEvent::Connected(id) => {
                info!("Client connected with id {id}");
                endpoint.send_message_on(
                    *id,
                    ServerChannel::Once,
                    ServerMessage::InitialData(InitialData {
                        bodies_config: bodies_config.clone(),
                        toggle_time: time_toggle.0,
                    }),
                )?
            }
            ClientConnectionEvent::Disconnected(id) => {
                info!("Client disconnected with id {id}");
            }
        }
    }
    Ok(())
}

fn handle_client_messages(
    mut server: ResMut<QuinnetServer>,
    mut ships: ResMut<ShipsMapping>,
    mut command: Commands,
    bodies: Query<(&Position, &HillRadius, &BodyInfo)>,
    main_body: Query<&BodyInfo, With<PrimaryBody>>,
    mapping: Res<BodiesMapping>,
) {
    let mut endpoint = server.endpoint_mut();
    for client_id in endpoint.clients() {
        while let Some(message) = endpoint.try_receive_message_from::<ClientMessage>(client_id) {
            match message.1 {
                ClientMessage::CreateShipMsg(msg) => {
                    ships.0.entry(msg.info.id).or_insert({
                        let alpha = main_body.single().0.id;
                        println!("{:#?}", alpha);
                        let influence = Influenced::new(&msg.pos, &bodies, mapping.as_ref(), alpha);
                        command
                            .spawn((
                                msg.info.clone(),
                                msg.acceleration,
                                influence,
                                msg.pos,
                                msg.velocity,
                                TransformBundle::from_transform(Transform::from_xyz(0., 0., 1.)),
                                ClearOnUnload,
                            ))
                            .id()
                    });
                }
            }
        }
    }
}

fn send_periodic_updates(
    mut timer: ResMut<PeriodicUpdatesTimer>,
    time: Res<Time>,
    mut server: ResMut<QuinnetServer>,
    game_time: Res<GameTime>,
    query: Query<(&ShipInfo, &Position, &Velocity)>,
) {
    timer.0.tick(time.delta());
    if timer.0.finished() {
        let mut alpha = Vec::<(ShipID, Position, Velocity)>::new();
        for (id, pos, velocity) in query.iter() {
            alpha.push((id.id, *pos, *velocity));
        }
        server.endpoint_mut().try_broadcast_message_on(
            ServerChannel::PeriodicUpdates,
            ServerMessage::PeriodicUpdate(PeriodicUpdate {
                time: game_time.simtick,
                ships: alpha,
            }), //ServerMessage::UpdateTime(game_time.simtick),
        );
    }
}
#[derive(Resource)]
struct TaskCommand {
    command: HashMap<bool, Task<String>>,
}
impl Default for TaskCommand {
    fn default() -> Self {
        Self {
            command: HashMap::new(),
        }
    }
}

#[derive(Default, States, Debug, PartialEq, Eq, Clone, Hash, Copy)]
enum Reading {
    #[default]
    NotReading,
    Reading,
}

#[derive(Default, States, Debug, PartialEq, Eq, Clone, Hash, Copy)]
enum Command {
    #[default]
    None,
    Help,
    TimeStart,
    TimeScale,
    ListShips,
    GetShipData,
    GetBodysData,
    Test,
    TestSetPos,
}

#[derive(Resource)]
struct Arguments(String);

fn read_stdin(
    mut command: ResMut<TaskCommand>,
    state: Res<State<Reading>>,
    mut next_state: ResMut<NextState<Reading>>,
) {
    if *state.get() == Reading::NotReading {
        next_state.set(Reading::Reading);
        let task_pool = AsyncComputeTaskPool::get();
        let mut buffer = String::new();
        let cur_task = task_pool.spawn(async move {
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            handle
                .read_line(&mut buffer)
                .expect("reading from stdin won't fail");
            buffer
        });
        command.command.insert(true, cur_task);
    }
}

fn handle_stdin(
    mut command: ResMut<TaskCommand>,
    mut next_state: ResMut<NextState<Reading>>,
    mut next_command: ResMut<NextState<Command>>,
    mut arg: ResMut<Arguments>,
) {
    command.command.retain(|_b, task| {
        let status = block_on(poll_once(task));
        let retain = status.is_none();
        if let Some(res) = status {
            let res = res.strip_suffix("\n").unwrap();
            let mut res = res.split_whitespace();
            let command = res.next();
            let command = match command {
                None => "help",
                Some(command) => command,
            };
            arg.0 = {
                let mut b = true;
                let mut arguments = String::new();
                while b {
                    let a = res.next();
                    match a {
                        None => b = false,
                        Some(a) => {
                            arguments.push_str(a);
                            arguments.push(' ');
                        }
                    }
                }
                arguments
            };
            match command {
                "help" => next_command.set(Command::Help),
                "toggle_time" => next_command.set(Command::TimeStart),
                "time_scale" => next_command.set(Command::TimeScale),
                "list_ships" => next_command.set(Command::ListShips),
                "get_ship_data" => next_command.set(Command::GetShipData),
                "get_bodys_data" => next_command.set(Command::GetBodysData),
                "test" => next_command.set(Command::Test),
                "test_set_pos" => next_command.set(Command::TestSetPos),
                _ => next_command.set(Command::None),
            }
            next_state.set(Reading::NotReading);
        }

        retain
    })
}

fn handle_command(
    command: Res<State<Command>>,
    mut next_state: ResMut<NextState<Command>>,
    mut toggle_time: ResMut<ToggleTime>,
    mut server: ResMut<QuinnetServer>,
    mut sim_step_size: ResMut<SimStepSize>,
    mut arg: ResMut<Arguments>,
    ships: Res<ShipsMapping>,
    bodies: Query<(&Position, &HillRadius, &BodyInfo)>,
    query: Query<(&Position, &Velocity, &Acceleration, &Influenced)>,
    pos_query_mut: Query<(&Position, &ShipInfo, Entity)>,
) {
    match command.get() {
        Command::Help => help_command(),
        Command::TimeStart => toggle_time_command(toggle_time, server),
        Command::TimeScale => set_time_scale(sim_step_size, arg),
        Command::ListShips => list_ships_command(ships),
        Command::GetShipData => get_ship_data(ships, arg, query),
        Command::GetBodysData => get_bodys_data(bodies),
        Command::Test => test(pos_query_mut),
        //Command::TestSetPos => test_set_pos(pos_query_mut, ships, arg),
        _ => println!("Command is not implemented"),
    }
    next_state.set(Command::None);
}

fn help_command() {
    println!(
        "list of commands:
    help : print the list of all available command
    toggle_time : start the simulation or pause it if already started
    time_scale : set the timescale to first argument, if no argument print current timescale (stepsize)
    list_ships : print the list of ships
    get_ship_data ID : print the data of the ship with id ID
    get_bodies_data : print data of all bodys
    test
    test_set_pos"
    );
}

fn toggle_time_command(mut toggle_time: ResMut<ToggleTime>, mut server: ResMut<QuinnetServer>) {
    println!("toggling time");
    toggle_time.0 = !toggle_time.0;
    let _ = server.endpoint_mut().broadcast_message_on(
        ServerChannel::Once,
        ServerMessage::ToggleTime(toggle_time.0),
    );
}

fn set_time_scale(mut sim_step_size: ResMut<SimStepSize>, mut arguments: ResMut<Arguments>) {
    let mut arg = arguments.0.split_whitespace();
    match arg.next() {
        Some(arg1) => {
            let tmp: Result<u64, _> = arg1.parse();
            match tmp {
                Ok(tmp) => sim_step_size.0 = tmp,
                Err(error) => println!("timescale is a u64, Error : {}", error),
            }
        }
        None => {}
    }
    println!("Current timescale = {}", sim_step_size.0)
}

fn list_ships_command(ships: Res<ShipsMapping>) {
    println!("ships list : {:?}", ships.0.keys())
}

fn get_ship_data(
    ships: Res<ShipsMapping>,
    arguments: ResMut<Arguments>,
    query: Query<(&Position, &Velocity, &Acceleration, &Influenced)>,
) {
    let mut arg = arguments.0.split_whitespace();
    match arg.next() {
        Some(arg1) => {
            let tmp: Result<ShipID, _> = arg1.parse();
            match tmp {
                Ok(tmp) => {
                    match ships.0.get(&tmp) {
                        Some(thing) => println!("data : {:#?}", query.get(*thing)),
                        None => {
                            println!("wrong ID");
                        }
                    };
                }
                Err(error) => println!("not an id, Error : {}", error),
            }
        }
        None => {}
    }
}

fn get_bodys_data(bodies: Query<(&Position, &HillRadius, &BodyInfo)>) {
    for (i, (pos, hill, bodyinfo)) in bodies.iter().enumerate() {
        println!("{} - {:#?}", i, bodyinfo)
    }
}

fn test(query: Query<(&Position, &ShipInfo, Entity)>) {
    let mut alpha = Vec::<(ShipID, Position)>::new();
    for (a, b, c) in query.iter() {
        alpha.push((b.id, *a));
        //println!("{:#?} , {:#?} , {:#?}", *a, *b, c)
    }
    println!("{:#?}", alpha);
    println!("test")
}

fn test_set_pos(
    mut query: Query<(&mut Position, &ShipInfo, Entity)>,
    ships: Res<ShipsMapping>,
    arguments: ResMut<Arguments>,
) {
    let mut argument = arguments.0.split_whitespace();
    let id = match argument.next() {
        Some(arg) => ships.0.get(arg),
        None => {
            println!("wrong ID");
            None
        }
    };
    let pos1 = match argument.next() {
        Some(arg) => match arg.parse() {
            Ok(pos) => pos,
            Err(err) => {
                println!("err : {}", err);
                0.
            }
        },
        None => {
            println!("wrong pos");
            0.
        }
    };
    let pos2 = match argument.next() {
        Some(arg) => match arg.parse() {
            Ok(pos) => pos,
            Err(err) => {
                println!("err : {}", err);
                0.
            }
        },
        None => {
            println!("wrong pos");
            0.
        }
    };
    let pos3 = match argument.next() {
        Some(arg) => match arg.parse() {
            Ok(pos) => pos,
            Err(err) => {
                println!("err : {}", err);
                0.
            }
        },
        None => {
            println!("wrong pos");
            0.
        }
    };
    match id {
        Some(entity) => {
            let mut alpha = query.get_mut(*entity).unwrap();
            alpha.0 .0[0] = pos1;
            alpha.0 .0[1] = pos2;
            alpha.0 .0[2] = pos3;
            println!("{:#?}", alpha);
        }
        None => (),
    }
}
