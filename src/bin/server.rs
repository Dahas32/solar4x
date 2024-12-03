use bevy::app::App;
use bevy::prelude::*;
use bevy::tasks::block_on;
use bevy::tasks::{poll_once, AsyncComputeTaskPool, Task};
use bevy::utils::hashbrown::HashMap;
use rust_space_trading::prelude::*;
use std::io::{self, BufRead};
use std::net::{IpAddr, Ipv4Addr};

fn main() {
    App::new()
        .add_plugins((
            ServerPlugin {
                server_address: ServerNetworkInfo(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6000),
                config: BodiesConfig::default(),
            },
            bevy::app::ScheduleRunnerPlugin::default(),
        ))
        .insert_resource(TaskCommand::default())
        .insert_state(Reading::default())
        .add_systems(Update, (handle_stdin, read_stdin))
        .run();
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

fn handle_stdin(mut command: ResMut<TaskCommand>, mut next_state: ResMut<NextState<Reading>>) {
    command.command.retain(|string, task| {
        let status = block_on(poll_once(task));
        let retain = status.is_none();
        if let Some(res) = status {
            println!("la");
            println!("{} , {:?}", res, string);
            next_state.set(Reading::NotReading);
        }

        retain
    })
}
