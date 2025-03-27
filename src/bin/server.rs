use bevy::app::App;

use rust_space_trading::prelude::*;
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
        .run();
}
