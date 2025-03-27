use bevy::{math::DVec3, prelude::*};
use serde::{Deserialize, Serialize};

use super::{
    super::prelude::ClientMode,
    prelude::*,
    time::{SimStepSize, GAMETIME_PER_SIMTICK},
    G,
};
use crate::game::InGame;

// See https://en.wikipedia.org/wiki/Leapfrog_integration#Algorithm
pub fn plugin(app: &mut App) {
    info!("loading leapfrog::plugin");
    info!("configuring sets FixedUpdate : LeapfrogUpdate.run_if(resource_equals(ToggleTime(true))).run_if(in_state(InGame)),");
    app.configure_sets(
        FixedUpdate,
        LeapfrogUpdate
            .run_if(resource_equals(ToggleTime(true)))
            .run_if(in_state(InGame).or_else(in_state(ClientMode::Server))),
    );
    info!("adding systems FixedUpdate :  (update_position, update_acceleration, update_velocity).chain().in_set(LeapfrogUpdate),");
    app.add_systems(
        FixedUpdate,
        (update_position, update_acceleration, update_velocity)
            .chain()
            .in_set(LeapfrogUpdate),
    );
}

#[derive(SystemSet, Debug, PartialEq, Eq, Hash, Clone)]
pub struct LeapfrogUpdate;

#[derive(Component, Debug, Default, Serialize, Deserialize)]
pub struct Acceleration {
    pub current: DVec3,
    pub previous: DVec3,
}

impl Acceleration {
    pub fn new(acc: DVec3) -> Self {
        debug!("new_acceleration");
        Self {
            current: acc,
            ..Default::default()
        }
    }
}

fn update_acceleration(
    mut gravity_bound: Query<(&Position, &mut Acceleration, &Influenced)>,
    bodies: Query<(&Position, &Mass)>,
) {
    debug!("updating accelaration");
    gravity_bound
        .par_iter_mut()
        .for_each(|(object_pos, mut acceleration, influenced)| {
            acceleration.previous = acceleration.current;
            acceleration.current = get_acceleration(
                object_pos.0,
                bodies
                    .iter_many(&influenced.influencers)
                    .map(|(p, m)| (p.0, m.0)),
            );
        });
}

fn update_position(
    mut query: Query<(&mut Position, &Velocity, &Acceleration)>,
    step: Res<SimStepSize>,
) {
    debug!("updating position");
    query.par_iter_mut().for_each(|(mut pos, speed, acc)| {
        pos.0 += get_dx(speed.0, acc.current, GAMETIME_PER_SIMTICK * step.0 as f64)
    });
}

fn update_velocity(mut query: Query<(&mut Velocity, &Acceleration)>, step: Res<SimStepSize>) {
    debug!("updating velocity");
    query.par_iter_mut().for_each(|(mut speed, acc)| {
        speed.0 += get_dv(
            acc.previous,
            acc.current,
            GAMETIME_PER_SIMTICK * step.0 as f64,
        )
    });
}

/// Computes the acceleration from the object's position, and an iterator of the influencers' positions and masses
pub fn get_acceleration(
    object_pos: DVec3,
    influencers: impl Iterator<Item = (DVec3, f64)>,
) -> DVec3 {
    debug!("getting acceleration");
    let mut acc = DVec3::ZERO;
    for (body_pos, mass) in influencers {
        let r = object_pos - body_pos;
        let dist = r.length();
        acc -= r * G * mass / (dist.powi(3));
    }
    acc
}

pub fn get_dx(speed: DVec3, acc: DVec3, dt: f64) -> DVec3 {
    debug!("getting dx");
    (speed + acc * dt / 2.) * dt
}

pub fn get_dv(previous_acc: DVec3, acc: DVec3, dt: f64) -> DVec3 {
    debug!("getting dv");
    (previous_acc + acc) * dt / 2.
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use bevy::app::FixedMain;

    use super::*;

    use crate::{prelude::*, utils::algebra::circular_orbit_around_body};

    #[test]
    fn test_leapfrog() {
        let mut app = App::new();
        app.add_plugins(ClientPlugin::testing().in_mode(ClientMode::Singleplayer));
        app.update();
        let world = app.world_mut();
        let mapping = &world.resource::<BodiesMapping>().0;
        let earth = mapping[&id_from("terre")];
        let (&mass, &spawn_earth_pos, &spawn_earth_speed) = world
            .query::<(&Mass, &mut Position, &Velocity)>()
            .get(world, earth)
            .unwrap();
        let (spawn_pos, spawn_speed) =
            circular_orbit_around_body(1e5, mass.0, spawn_earth_pos.0, spawn_earth_speed.0);
        app.world_mut().send_event(ShipEvent::Create(ShipInfo {
            id: id_from("s"),
            spawn_pos,
            spawn_speed,
        }));
        app.update();
        let period = 2. * PI * (1e5_f64).powf(3. / 2.) / (G * mass.0).sqrt();
        app.world_mut()
            .resource_mut::<NextState<GameStage>>()
            .set(GameStage::Action);
        app.update();
        while app.world().resource::<GameTime>().time() < period {
            app.update();
            FixedMain::run_fixed_main(app.world_mut());
        }
        let world = app.world_mut();
        let pos = world
            .query_filtered::<&mut Position, With<Influenced>>()
            .single(world)
            .0;
        let &earth_pos = world.query::<&mut Position>().get(world, earth).unwrap();
        // dbg!(spawn_pos - spawn_earth_pos.0);
        // dbg!(pos - earth_pos.0);
        assert!(((spawn_pos - spawn_earth_pos.0) - (pos - earth_pos.0)).length() < 2e4);
    }
}
