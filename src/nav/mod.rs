use {
    crate::{
        ctx::Ctx, log::{ LogMsg, LogType }, term::TermEvent,
        win::WindowBufferBounds,
    },
    crossterm::event::KeyCode,
    md5::{Md5, Digest},
    std::{
        fs::File,
        io::Read,
    },
    todo_core::{
        Container, GetPath, Item, ItemAction, ItemActor, ItemStatus, ItemType,
    },
    tui::{ widgets, style, text, },
};
pub trait NavigateMap {
    fn set_nearest_pos(&mut self);
    fn move_action(&mut self);
    fn through_items(
        the_map: &mut Vec<Vec<usize>>, sub_positions: &mut Vec<usize>,
        items: &Vec<Item>, display_hidden: bool
    );
    fn from_items(items: &Vec<Item>, display_hidden: bool) -> Vec<Vec<usize>>;
}
#[derive(PartialEq)]
pub enum NavMode {
    Navigate,
    Input,
}
#[derive(PartialEq)]
pub enum NavAction {
    MoveOut,
    Next,
    Prev,
    MoveIn,
    NoAction,
    Exit,
    ToggleShowHidden,
    CycleItemStatus,
    ToggleItemHidden,
    ToggleDebug,
    PreAddItem,
    AddItem,
    PreAddRootItem,
    AddRootItem,
    ToggleItemType,
    RemoveItem,
    GoToBottom,
    GoToTop,
    GoToRootLevel,
    //GoToInnerLevel,
}
impl NavAction {
    pub fn from_event(event: TermEvent) -> Self {
        match event {
            TermEvent::Key(key) => {
                match key.code {
                    KeyCode::Char('D') => Self::ToggleDebug,
                    KeyCode::Char('h') => Self::MoveOut,
                    KeyCode::Char('j') => Self::Next,
                    KeyCode::Char('k') => Self::Prev,
                    KeyCode::Char('l') => Self::MoveIn,
                    KeyCode::Char('q') => Self::Exit,
                    KeyCode::Char('H') => Self::ToggleShowHidden,
                    KeyCode::Char('c') => Self::CycleItemStatus,
                    KeyCode::Char('s') => Self::ToggleItemHidden,
                    KeyCode::Char('t') => Self::ToggleItemType,
                    KeyCode::Char('a') => Self::PreAddItem,
                    KeyCode::Char('A') => Self::PreAddRootItem,
                    KeyCode::Char('R') | KeyCode::Delete => Self::RemoveItem,
                    KeyCode::Char('g') => Self::GoToTop,
                    KeyCode::Char('G') => Self::GoToBottom,
                    //KeyCode::Char('$') => Self::GoToInnerLevel,
                    KeyCode::Char('0') => Self::GoToRootLevel,
                    _ => Self::NoAction,
                }
            },
            TermEvent::Tick => Self::NoAction,
        }
    }
}
pub struct NavigationMap {
    position: Vec<usize>,
    valid_positions: Vec<Vec<usize>>,
    window_buffer: WindowBufferBounds,
}
pub struct Navigator {
    pub height: u16,
    map: NavigationMap,
    d_buffer: Vec<LogMsg>,
    display_hidden: bool,
    pub debug: bool,
    action: NavAction,
    pub mode: NavMode,
    container: Container,
    pub file_hash: String,
    pub i_buffer: String,
}
impl Navigator {
    pub fn get_file_hash(ctx: &mut Ctx) -> String {
        let path = ctx.get_path();
        if !path.exists() {
            panic!("Failed to find path \"{}\"", path.to_str().unwrap());
        }
        let mut contents = String::new();
        { // file lock
            let mut file = File::open(path).unwrap();
            file.read_to_string(&mut contents).unwrap();
        }
        let mut hasher = Md5::new();
        hasher.update(contents);
        let result = hasher.finalize();
        format!("{:x}", result)
    }
    pub fn new(ctx: &mut Ctx) -> Self {
        let display_hidden = ctx.args.display_hidden;
        let container = Container::load(ctx)
            .unwrap_or_else(|_| panic!("Failed to load list"));
        let hash = Self::get_file_hash(ctx);
        let valid_pos = Self::from_items(&container.list.items, display_hidden);
        let nav_map = NavigationMap {
            valid_positions: valid_pos,
            position: vec![0],
            window_buffer: WindowBufferBounds::init(),
        };
        Self {
            height: 0,
            debug: ctx.args.debug, d_buffer: Vec::new(), display_hidden, map: nav_map,
            action: NavAction::NoAction, container, mode: NavMode::Navigate,
            i_buffer: String::new(),
            file_hash: hash,
        }
    }
    pub fn reload(&mut self, ctx: &mut Ctx) {
        let container = match Container::load(ctx) {
            Ok(c) => c,
            Err(e) => {
                self.push_error(format!("Failed to load list: {}", e));
                return;
            },
        };
        let nav_map = Self::from_items(&container.list.items, self.display_hidden);
        self.container = container;
        self.map.valid_positions = nav_map;
        self.set_nearest_pos();
        self.file_hash = Self::get_file_hash(ctx);
        self.push_log(
            format!("Hash reloaded {}", self.file_hash)
        );
    }
    pub fn save_and_reload(&mut self, ctx: &mut Ctx) {
        match self.container.save() {
            Ok(_) => {},
            Err(e) => {
                self.push_error(format!("Failed to save list: {}", e));
            },
        }
        self.reload(ctx);
    }
    pub fn push_log(&mut self, msg: impl AsRef<str>) {
        self.d_buffer.push(LogMsg::log(msg));
    }
    pub fn push_warn(&mut self, msg: impl AsRef<str>) {
        self.d_buffer.push(LogMsg::warn(msg));
    }
    pub fn push_error(&mut self, msg: impl AsRef<str>) {
        self.d_buffer.push(LogMsg::error(msg));
    }
    pub fn render_buffer(&mut self) -> Vec<widgets::ListItem> {
        let mut items = Vec::new();
        let mut rev_buf = self.d_buffer.clone();
        rev_buf.reverse();
        for msg in rev_buf.iter() {
            let s_color;
            let s_prefix;
            match msg.log_type {
                LogType::Log => {
                    s_color = style::Style::default().fg(style::Color::Cyan);
                    s_prefix = "LOG : ";
                },
                LogType::Warning => {
                    s_color = style::Style::default().fg(style::Color::Yellow);
                    s_prefix = "WARN: ";
                },
                LogType::Error => {
                    s_color = style::Style::default().fg(style::Color::Red);
                    s_prefix = "DANG: ";
                },
            }
            let pre_span = text::Span::styled(
                s_prefix,
                s_color,
            );
            let span =text::Span::styled(
                msg.message.clone(),
                style::Style::default(),
            );
            let spans = text::Spans::from(vec![pre_span, span]);
            items.push(widgets::ListItem::new(spans));
        }
        items
    }
    pub fn get_todo_item_location(&mut self) -> Vec<usize> {
        let mut action_vec = self.map.position.clone();
        action_vec.reverse();
        action_vec.iter_mut().for_each(|item| {
            *item = *item + 1;
        });
        action_vec
    }
    pub fn handle_win_buf(&mut self, initial: bool) {
        let changed = self.map.window_buffer.set_size(
            self.height, &self.map.position, &self.map.valid_positions, initial
        );
        if changed {
            self.push_log(format!(
                "WinBuf set to height: {}, min: {}, max: {}, pos: {}",
                self.map.window_buffer.size,
                self.map.window_buffer.min,
                self.map.window_buffer.max,
                self.map.window_buffer.pos,
            ));
        }
    }
    pub fn take_movement_action(&mut self, ctx: &mut Ctx) -> bool {
        let new_hash = Self::get_file_hash(ctx);
        if !self.file_hash.eq(&new_hash) {
            self.reload(ctx);
        }
        let keep_run;
        let is_movement;
        match self.action {
            NavAction::ToggleDebug => {
                self.push_log("Toggling debug mode");
                self.debug = !self.debug;
                keep_run = true;
                is_movement = false;
            },
            NavAction::NoAction => {
                keep_run = true;
                is_movement = false;
            },
            NavAction::MoveOut => {
                self.push_log("Moving out");
                keep_run = true;
                is_movement = true;
            },
            NavAction::MoveIn => {
                self.push_log("Moving in");
                keep_run = true;
                is_movement = true;
            },
            NavAction::Next => {
                self.push_log("Next item");
                keep_run = true;
                is_movement = true;
            },
            NavAction::Prev => {
                self.push_log("Prev item");
                keep_run = true;
                is_movement = true;
            },
            NavAction::Exit => {
                keep_run = false;
                is_movement = false;
            },
            NavAction::ToggleShowHidden => {
                self.push_log("Hidden toggled");
                self.display_hidden = !self.display_hidden;
                // TODO: Do not save and reload here, just reinit map
                self.save_and_reload(ctx);
                keep_run = true;
                is_movement = true;
            },
            NavAction::CycleItemStatus => {
                self.push_log("Cycling status");
                let mut action_vec = self.get_todo_item_location();
                self.container.act_on_item_at(&mut action_vec, ItemAction::CycleStatus);
                self.save_and_reload(ctx);
                keep_run = true;
                is_movement = false;
            },
            NavAction::ToggleItemHidden => {
                self.push_log("Toggling item hidden");
                let mut action_vec = self.get_todo_item_location();
                self.container.act_on_item_at(&mut action_vec, ItemAction::ToggleHidden);
                self.save_and_reload(ctx);
                keep_run = true;
                is_movement = false;
            },
            NavAction::ToggleItemType => {
                self.push_log("Toggling item type at position");
                let mut action_vec = self.get_todo_item_location();
                self.container.act_on_item_at(&mut action_vec, ItemAction::ToggleType);
                self.save_and_reload(ctx);
                keep_run = true;
                is_movement = false;
            },
            NavAction::PreAddItem => {
                self.push_log("Preparing to add item");
                self.mode = NavMode::Input;
                keep_run = true;
                is_movement = false;
            },
            NavAction::AddItem => {
                self.push_log("Adding item");
                let mut action_vec = self.get_todo_item_location();
                self.container.act_on_item_at(
                    &mut action_vec,
                    ItemAction::Add(ItemType::Todo, self.i_buffer.clone()),
                );
                self.i_buffer = String::new();
                self.save_and_reload(ctx);
                keep_run = true;
                is_movement = false;
            },
            NavAction::PreAddRootItem => {
                self.push_log("Preparing to add root item");
                self.mode = NavMode::Input;
                keep_run = true;
                is_movement = false;
            },
            NavAction::AddRootItem => {
                self.push_log("Adding root item");
                self.container.act_on_item_at(
                    &mut Vec::new(),
                    ItemAction::Add(ItemType::Todo, self.i_buffer.clone()),
                );
                self.i_buffer = String::new();
                self.save_and_reload(ctx);
                keep_run = true;
                is_movement = false;
            },
            NavAction::RemoveItem => {
                self.push_log("Removing item at position");
                let mut action_vec = self.get_todo_item_location();
                self.container.act_on_item_at(
                    &mut action_vec,
                    ItemAction::Remove,
                );
                self.save_and_reload(ctx);
                keep_run = true;
                is_movement = true;
            },
            NavAction::GoToTop => {
                self.push_log("Top");
                keep_run = true;
                is_movement = true;
            },
            NavAction::GoToBottom => {
                self.push_log("Bottom");
                keep_run = true;
                is_movement = true;
            },
            NavAction::GoToRootLevel => {
                self.push_log("Root");
                keep_run = true;
                is_movement = true;
            },
            //NavAction::GoToInnerLevel => {
            //    self.push_log("Innermost");
            //    keep_run = true;
            //    is_movement = true;
            //},
        };
        if is_movement {
            self.move_action();
            self.handle_win_buf(false);
        }
        keep_run
    }
    pub fn take_action(&mut self, ctx: &mut Ctx) -> bool {
        match self.mode {
            NavMode::Navigate => self.take_movement_action(ctx),
            NavMode::Input => true,
        }
    }
    pub fn handle_input(&mut self, event: TermEvent) {
        match self.mode {
            NavMode::Navigate => {
                self.action = NavAction::from_event(event);
            },
            NavMode::Input => {
                match event {
                    TermEvent::Key(key) => match key.code {
                        KeyCode::Enter => {
                            // TODO: Finish implementing typing
                            self.mode = NavMode::Navigate;
                            match self.action {
                                NavAction::PreAddRootItem => {
                                    self.action = NavAction::AddRootItem;
                                },
                                NavAction::PreAddItem => {
                                    self.action = NavAction::AddItem;
                                },
                                _ => {},
                            }
                        },
                        KeyCode::Esc => {
                            self.mode = NavMode::Navigate;
                            self.action = NavAction::NoAction;
                        },
                        KeyCode::Char(c) => {
                            self.i_buffer.push(c);
                        },
                        KeyCode::Backspace => {
                            self.i_buffer.pop();
                        },
                        _ => {},
                    },
                    _ => {},
                }
            },
        }
    }
    pub fn item_as_widget(&self, item: &Item, items: &mut Vec<widgets::ListItem>, pos: &mut Vec<usize>) {
        let mut indent_str = String::new();
        for _ in 0..pos.len() - 1 {
            indent_str.push_str("    ");
        }
        let indent = text::Span::from(indent_str);
        let status = match item.item_type {
            ItemType::Todo => {
                match item.status {
                    ItemStatus::Complete => {
                        let color = if item.hidden {
                            style::Color::DarkGray
                        } else {
                            style::Color::Green
                        };
                        text::Span::styled(
                            "[x] ",
                            style::Style::default().fg(color)
                        )
                    },
                    ItemStatus::Incomplete => {
                        let color = if item.hidden {
                            style::Color::DarkGray
                        } else {
                            style::Color::Red
                        };
                        text::Span::styled(
                            "[ ] ",
                            style::Style::default().fg(color)
                        )
                    },
                    ItemStatus::Disabled => {
                        let color = if item.hidden {
                            style::Color::DarkGray
                        } else {
                            style::Color::Yellow
                        };
                        text::Span::styled(
                            "[-] ",
                            style::Style::default().fg(color)
                        )
                    },
                }
            },
            ItemType::Note => {
                let color = if item.hidden {
                    style::Color::DarkGray
                } else {
                    style::Color::Cyan
                };
                text::Span::styled(
                    "-   ",
                    style::Style::default().fg(color)
                )
            },
        };
        // is item selected?
        let text = if (*pos).eq(&self.map.position) {
            text::Span::styled(
                item.text.clone(),
                style::Style::default().fg(style::Color::Cyan)
            )
        } else {
            if item.hidden {
                text::Span::styled(
                    item.text.clone(),
                    style::Style::default().fg(style::Color::DarkGray)
                )
            } else {
                text::Span::styled(
                    item.text.clone(),
                    style::Style::default().fg(style::Color::White)
                )
            }
        };
        items.push(widgets::ListItem::new(
            text::Spans::from(vec![ indent, status, text, ])
        ));
    }
    pub fn item_to_list_items(
        &self, item: &Item, items: &mut Vec<widgets::ListItem>,
        iter_state: &mut Vec<usize>, lines: &mut u16
    ) {
        if !self.display_hidden && item.hidden {
            return;
        }
        *lines = (*lines) + 1;
        // TODO: Handle scrolling via height and y_pos
        if self.map.window_buffer.is_in_view(*lines) {
            self.item_as_widget(&item, items, iter_state);
        }
        let mut i = 0;
        for sub_item in item.sub_items.iter() {
            iter_state.push(i);
            self.item_to_list_items(sub_item, items, iter_state, lines);
            iter_state.pop().unwrap();
            i = i + 1;
        }
    }
    pub fn get_list(&self) -> Vec<widgets::ListItem> {
        let mut iter_state = Vec::new();
        let mut items = Vec::new();
        // TODO: Handle empty list
        let mut i = 0;
        let mut lines = 0;
        for item in self.container.list.items.iter() {
            iter_state.push(i);
            self.item_to_list_items(item, &mut items, &mut iter_state, &mut lines);
            iter_state.pop().unwrap();
            i = i + 1;
        }
        items
    }
}
impl NavigateMap for Navigator {
    fn set_nearest_pos(&mut self) {
        // get nearest valid position
        // check if same position exists
        let same_pos = self.map.valid_positions.iter()
            .filter(|pos| pos == &&self.map.position)
            .collect::<Vec<&Vec<usize>>>();
        if same_pos.len() > 0 {
            let new_pos = *same_pos.first().unwrap();
            self.map.position = new_pos.clone();
            return;
        }
        // check same/previous levels
        let mut i = self.map.position.len();
        while i > 0 {
            let mut valid_positions_at_level = self.map.valid_positions.iter()
                .filter(|pos| {
                    pos.len() == i &&
                    pos[0..(i - 1)] == self.map.position[0..(i - 1)]
                })
                .collect::<Vec<&Vec<usize>>>();
            if valid_positions_at_level.len() > 0 {
                let num_to_match = self.map.position.get(i - 1).unwrap().clone() as i32;
                // sort by closest number to num_to_match to furthest
                valid_positions_at_level.sort_by(|x, y| {
                    let x_pos = x.get(i - 1).unwrap().clone() as i32;
                    let y_pos = y.get(i - 1).unwrap().clone() as i32;
                    (num_to_match - x_pos).abs()
                        .cmp(&(num_to_match - y_pos).abs())
                });
                self.map.position = (*valid_positions_at_level.first().unwrap())
                    .clone();
                break;
            }
            let valid_positions_prev_level = self.map.valid_positions.iter()
                .filter(|pos| {
                    pos.len() == (i - 1) &&
                    pos[0..(i - 1)] == self.map.position[0..(i - 1)]
                })
                .collect::<Vec<&Vec<usize>>>();
            if valid_positions_prev_level.len() > 0 {
                self.map.position = (*valid_positions_prev_level.first().unwrap())
                    .clone();
                break;
            }
            i = i - 1;
        }
        if i == 0 {
            self.map.position = vec![0];
        }
        self.push_warn(
            "Position could not be maintained, corrected to nearest available position"
        );
    }
    fn move_action(&mut self) {
        match self.action {
            NavAction::ToggleItemHidden | NavAction::ToggleShowHidden => {
                self.set_nearest_pos();
            },
            NavAction::MoveIn => {
                let valid_in_positions = self.map.valid_positions.iter()
                    .filter(|pos| {
                        // valid position length equals current position length + 1
                        pos.len().eq(&(self.map.position.len() + 1)) &&
                            // valid position without last value equals current position
                            pos[0..self.map.position.len()].eq(&self.map.position)
                    })
                    .collect::<Vec<&Vec<usize>>>();
                if valid_in_positions.len() > 0 {
                    let new_pos = *valid_in_positions.get(0).unwrap();
                    self.map.position = new_pos.clone();
                } else {
                    self.push_warn("Horizontal positional overflow avoided");
                }
            },
            NavAction::MoveOut => {
                if self.map.position.len() > 1 {
                    self.map.position.pop().unwrap();
                } else {
                    self.push_warn("Horizontal positional underflow avoided");
                }
            },
            NavAction::Next => {
                let valid_next_positions = self.map.valid_positions.iter()
                    .filter(|pos| {
                        pos.len() == self.map.position.len() &&
                            pos[0..pos.len() - 1] == self.map.position[0..self.map.position.len() - 1] &&
                            pos.get(pos.len() - 1).unwrap() > self.map.position.get(self.map.position.len() - 1).unwrap()
                    })
                    .collect::<Vec<&Vec<usize>>>();
                if valid_next_positions.len() > 0 {
                    let new_pos = *valid_next_positions.get(0).unwrap();
                    self.map.position = new_pos.clone();
                } else {
                    self.push_warn("Vertical position overflow avoided");
                }
            },
            NavAction::Prev => {
                if self.map.position.get(self.map.position.len() - 1).unwrap().eq(&0) {
                    self.push_warn("Vertical position underflow avoided");
                    return;
                }
                let valid_prev_positions = self.map.valid_positions.iter()
                    .filter(|pos| {
                        pos.len() == self.map.position.len() &&
                            pos[0..pos.len() - 1] == self.map.position[0..self.map.position.len() - 1] &&
                            pos.get(pos.len() - 1).unwrap() < self.map.position.get(self.map.position.len() - 1).unwrap()
                    })
                    .collect::<Vec<&Vec<usize>>>();
                if valid_prev_positions.len() > 0 {
                    let new_pos = *valid_prev_positions
                        .get(valid_prev_positions.len() - 1)
                        .unwrap();
                    self.map.position = new_pos.clone();
                } else {
                    self.push_warn("Vertical position underflow avoided");
                }
            },
            NavAction::GoToTop => {
                let next_position = self.map.valid_positions.iter().filter(|pos| {
                    pos[0..pos.len() - 1] == self.map.position[0..self.map.position.len() - 1]
                }).next().unwrap();
                self.map.position = next_position.clone();
            },
            NavAction::GoToBottom => {
                let next_position = self.map.valid_positions.iter().filter(|pos| {
                    pos[0..pos.len() - 1] == self.map.position[0..self.map.position.len() - 1]
                }).last().unwrap();
                self.map.position = next_position.clone();
            },
            NavAction::GoToRootLevel => {
                let next_position = self.map.valid_positions.iter().filter(|pos| {
                    pos[0] == self.map.position[0]
                }).next().unwrap();
                self.map.position = next_position.clone();
            },
            _ => {},
        }
    }
    fn through_items(
        the_map: &mut Vec<Vec<usize>>, sub_positions: &mut Vec<usize>, items: &Vec<Item>, display_hidden: bool
    ) {
        let mut moment = 0;
        for item in items.into_iter() {
            sub_positions.push(moment);
            // perform checks
            if display_hidden || !item.hidden {
                the_map.push(sub_positions.clone());
                Self::through_items(the_map, sub_positions, &item.sub_items, display_hidden);
            }
            sub_positions.pop();
            moment = moment + 1;
        }
    }
    fn from_items(items: &Vec<Item>, display_hidden: bool) -> Vec<Vec<usize>> {
        let mut the_map = Vec::new();
        let mut sub_positions = Vec::new();
        Self::through_items(&mut the_map, &mut sub_positions, items, display_hidden);
        the_map
    }
}
