//! A "Body" is a celestial body whose position is entirely determined by the
//! current simtick, following orbital mechanics.
use arrayvec::ArrayString;
use bevy::{prelude::*, utils::HashMap};
use bodies_config::BodiesConfig;
use body_data::BodyData;
use main_bodies::read_main_bodies;

use crate::game::{ClearOnUnload, Loaded};
use crate::physics::prelude::*;

use super::id::MAX_ID_LENGTH;
use super::ObjectsUpdate;

pub mod bodies_config;
pub mod body_data;
mod main_bodies;

pub type BodyID = ArrayString<MAX_ID_LENGTH>;
// #[derive(Serialize, Deserialize)]
// pub(crate) struct BodyID(u64);

// #[derive(Resource, Default)]
// struct BodyIDBuilder(NumberIncrementer);

// impl IDBuilder for BodyIDBuilder {
//     type ID = BodyID;

//     fn incrementer(&mut self) -> &mut NumberIncrementer {
//         &mut self.0
//     }

//     fn id_from_u64(u: u64) -> Self::ID {
//         BodyID(u)
//     }
// }

#[derive(Component)]
pub struct PrimaryBody;

#[derive(Component, Debug, Clone)]
pub struct BodyInfo(pub BodyData);

#[derive(Resource)]
pub struct BodiesMapping(pub HashMap<BodyID, Entity>);

pub struct BodiesPlugin;

impl Plugin for BodiesPlugin {
    fn build(&self, app: &mut App) {
        info!("loading BodiesPlugin");
        info!("adding system OnEnter(Loaded) : build_system.in_set(ObjectsUpdate)");
        app.add_systems(OnEnter(Loaded), build_system.in_set(ObjectsUpdate));
    }
}

pub fn build_system(mut commands: Commands, config: Res<BodiesConfig>) {
    info!("building system");
    let bodies: Vec<_> = read_main_bodies()
        .expect("Failed to read bodies")
        .into_iter()
        .filter(config.clone().into_filter())
        .collect();
    let primary_body = bodies
        .iter()
        .find(|data| data.host_body.is_none())
        .expect("no primary body found")
        .id;
    let mut id_mapping = HashMap::new();
    for data in bodies {
        let id = data.id;
        let mut entity = commands.spawn((
            Position::default(),
            EllipticalOrbit::from(&data),
            Mass(data.mass),
            BodyInfo(data),
            Velocity::default(),
            ClearOnUnload,
        ));
        if id == primary_body {
            entity.insert(PrimaryBody);
        }
        id_mapping.insert(id, entity.id());
    }
    commands.insert_resource(BodiesMapping(id_mapping));
}

#[cfg(test)]
mod tests {
    use bevy::{app::App, ecs::query::With};

    use crate::prelude::*;

    use super::BodyInfo;

    #[test]
    fn test_build_system() {
        let mut app = App::new();
        app.add_plugins(ClientPlugin::testing().in_mode(ClientMode::Explorer));
        app.update();
        app.update();

        let world = app.world_mut();
        assert_eq!(world.resource::<BodiesMapping>().0.len(), 9);
        assert_eq!(world.query::<&BodyInfo>().iter(world).len(), 9);
        let (orbit, BodyInfo(data)) = world
            .query::<(&EllipticalOrbit, &BodyInfo)>()
            .iter(world)
            .find(|(_, BodyInfo(data))| data.id == id_from("terre"))
            .unwrap();
        assert_eq!(orbit.semimajor_axis, 149598023.);
        assert_eq!(data.semimajor_axis, 149598023.);
        assert_eq!(
            world
                .query_filtered::<&BodyInfo, With<PrimaryBody>>()
                .single(world)
                .0
                .id,
            id_from("soleil")
        )
    }
}
