use bevy::prelude::*;
use bevy_ratatui::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::{
    layout::{Constraint, Flex, Layout},
    text::Line,
    widgets::{List, ListState, Paragraph, StatefulWidget, Widget},
};

use crate::prelude::*;

use super::AppScreen;

const SCREENS: [(ClientMode, &str); 3] = [
    (ClientMode::Singleplayer, "Singleplayer"),
    (ClientMode::Multiplayer, "Multiplayer"),
    (ClientMode::Explorer, "Explore"),
];

pub struct StartMenuPlugin;

#[derive(Event)]
pub enum StartMenuEvent {
    Quit,
    Select(Direction2),
    Validate,
}

#[derive(Resource)]
pub struct StartMenuContext {
    list_state: ListState,
}

pub struct StartMenu;
pub fn plugin(app: &mut App) {
    app.insert_resource(StartMenuContext::default())
        .add_event::<StartMenuEvent>()
        .add_systems(
            Update,
            (
                read_input.in_set(InputReading),
                handle_events.in_set(EventHandling),
            )
                .run_if(in_state(AppScreen::StartMenu)),
        )
        .add_systems(OnEnter(ClientMode::None), create_screen);
}

fn create_screen(mut next_screen: ResMut<NextState<AppScreen>>) {
    next_screen.set(AppScreen::StartMenu);
}

fn read_input(
    mut key_event: EventReader<KeyEvent>,
    keymap: Res<Keymap>,
    mut internal_event: EventWriter<StartMenuEvent>,
) {
    for KeyEvent(event) in key_event.read() {
        if event.kind == KeyEventKind::Release {
            return;
        }
        use Direction2::*;
        use StartMenuEvent::*;

        let keymap = &keymap.start_menu;
        internal_event.send(match event {
            e if keymap.select_next.matches(e) => Select(Down),
            e if keymap.select_previous.matches(e) => Select(Up),
            e if keymap.quit.matches(e) => Quit,
            e if keymap.validate.matches(e) => Validate,
            _ => return,
        });
    }
}

impl StartMenuContext {
    fn get_next_mode(&self) -> ClientMode {
        match self.list_state.selected().unwrap() {
            i if i < SCREENS.len() => SCREENS[i].0,
            _ => unreachable!(),
        }
    }
}

impl Default for StartMenuContext {
    fn default() -> Self {
        Self {
            list_state: ListState::default().with_selected(Some(0)),
        }
    }
}

impl ClampedList for StartMenuContext {
    fn list_state(&mut self) -> &mut ListState {
        &mut self.list_state
    }

    fn len(&self) -> usize {
        SCREENS.len()
    }
}

pub fn handle_events(
    mut next_mode: ResMut<NextState<ClientMode>>,
    mut context: ResMut<StartMenuContext>,
    mut events: EventReader<StartMenuEvent>,
    mut quit: EventWriter<AppExit>,
) {
    for event in events.read() {
        match event {
            StartMenuEvent::Quit => {
                quit.send_default();
            }
            StartMenuEvent::Select(d) => context.select_adjacent(*d),
            StartMenuEvent::Validate => next_mode.set(context.get_next_mode()),
        }
    }
}

impl StatefulWidget for StartMenu {
    type State = StartMenuContext;

    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        let title = r#"
 _______  _____         _______  ______           _     _
 |______ |     | |      |_____| |_____/  /_____|   \___/ 
 ______| |_____| |_____ |     | |    \_        |  _/   \_
        "#;
        let split = title.split('\n');
        // let title_width = split.clone().next().unwrap().len();
        let title_height = split.count();
        let chunks = Layout::vertical([
            Constraint::Length(title_height as u16),
            Constraint::Max(3),
            Constraint::Length(SCREENS.len() as u16),
        ])
        .flex(Flex::Center)
        .split(area);
        Paragraph::new(title).centered().render(chunks[0], buf);
        let (_, entries): (Vec<_>, Vec<&str>) = SCREENS.into_iter().unzip();
        let list_width = entries.iter().map(|s| s.len()).max().unwrap();
        let entries = entries.into_iter().map(|s| Line::from(s).centered());
        let list = List::new(entries).highlight_symbol(">");
        let [list_area] = Layout::horizontal([Constraint::Length(list_width as u16 + 1)])
            .flex(Flex::Center)
            .areas(chunks[2]);
        StatefulWidget::render(list, list_area, buf, &mut state.list_state);
    }
}
