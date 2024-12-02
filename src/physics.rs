use bevy::{math::DVec3, prelude::*};
use influence::InfluenceUpdate;
use leapfrog::LeapfrogUpdate;
use orbit::OrbitsUpdate;
use time::{TimeUpdate, ToggleTime};

use crate::objects::ships::trajectory::TrajectoryUpdate;

pub mod influence;
pub mod leapfrog;
pub mod orbit;
pub mod predictions;
pub mod time;

const SECONDS_PER_DAY: f64 = 24. * 3600.;

/// Gravitationnal constant in km3kg-1d-2
pub const G: f64 = 6.6743e-11 * SECONDS_PER_DAY * SECONDS_PER_DAY * 1e-9;

pub(crate) mod prelude {
    pub use super::{
        influence::Influenced,
        leapfrog::Acceleration,
        orbit::{EllipticalOrbit, SystemSize},
        predictions::Prediction,
        time::{GameTime, ToggleTime},
        Mass, Position, Velocity,
    };
}

#[derive(Component, Default, Debug, Clone, Copy)]
pub struct Position(pub DVec3);

#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Velocity(pub DVec3);

#[derive(Component, Clone, Copy)]
pub struct Mass(pub f64);

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        info!("loading PhysicsPlugin");
        info!("adding plugins : orbit::plugin , inflence::plugin, leapfrog::plugin, time::plugin");
        app.add_plugins((
            orbit::plugin,
            influence::plugin,
            leapfrog::plugin,
            time::plugin,
        ));
        info!("configuring sets : (TimeUpdate,OrbitsUpdate,InfluenceUpdate,TrajectoryUpdate,LeapfrogUpdate,).chain().in_set(PhysicsUpdate).run_if(resource_equals(ToggleTime(true)))");
        app.configure_sets(
            FixedUpdate,
            (
                TimeUpdate,
                OrbitsUpdate,
                InfluenceUpdate,
                TrajectoryUpdate,
                LeapfrogUpdate,
            )
                .chain()
                .in_set(PhysicsUpdate)
                .run_if(resource_equals(ToggleTime(true))),
        );
    }
}

#[derive(SystemSet, Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct PhysicsUpdate;
