use {
    clap::Parser,
    crossterm::{ cursor, event, execute, terminal, event::KeyCode, },
    std::{
        io::{ Error as IOError, stdout as get_stdout, Stdout, },
        path::PathBuf,
        thread::spawn as thread_spawn,
        time::{ Duration, Instant, },
        sync::mpsc::{ channel, Receiver, },
    },
    todo_core::{
        Container, GetPath, Item, ItemStatus, ItemType, ItemActor,
        ItemAction,
    },
    tui::{ backend::CrosstermBackend, Terminal, widgets, layout, text, style, },
};
#[derive(Parser, Clone)]
struct PathArgs {
    #[clap()]
    list_path: String,
}
#[derive(Parser, Clone)]
enum Mode {
    New(PathArgs),
    Open(PathArgs),
}
#[derive(Parser, Clone)]
struct Args {
    #[clap(short, long)]
    debug: bool,
    #[clap(subcommand)]
    mode: Mode,
}
#[derive(Clone)]
struct Ctx {
    args: Args,
    path: PathBuf,
}
impl Ctx {
    fn new(args: Args) -> Self {
        Self { args, path: PathBuf::new(), }
    }
    fn construct_path(&mut self) {
        let path = match &self.args.mode {
            Mode::Open(args) => {
                &args.list_path
            },
            Mode::New(args) => {
                &args.list_path
            },
        };
        let tmp_path = PathBuf::from(format!("{}", &path));
        match tmp_path.extension() {
            Some(ext) => {
                if !ext.eq("json") {
                    self.path.push(format!("{}.json", &path));
                } else {
                    self.path.push(format!("{}", &path));
                }
            },
            None => self.path.push(format!("{}.json", &path)),
        }
    }
}
impl GetPath for Ctx {
    fn get_path(&self) -> &PathBuf {
        &self.path
    }
    fn get_path_mut(&mut self) -> &mut PathBuf {
        &mut self.path
    }
}
enum TermEvent {
    Key(event::KeyEvent),
    Tick,
}
#[derive(PartialEq)]
enum NavMode {
    Navigate,
    Input,
}
#[derive(PartialEq)]
enum NavAction {
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
    Initial,
    PreAddItem,
    AddItem,
    PreAddRootItem,
    AddRootItem,
    ToggleItemType,
    RemoveItem,
}
impl NavAction {
    fn from_event(event: TermEvent, initial: bool) -> Self {
        if initial {
            return Self::Initial;
        }
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
                    _ => Self::NoAction,
                }
            },
            TermEvent::Tick => Self::NoAction,
        }
    }
}
trait PrintCoords {
    fn to_coords(&self) -> String;
}
impl PrintCoords for Vec<usize> {
    fn to_coords(&self) -> String {
        let mut out = String::new();
        let mut delim = "";
        for item in self {
            out.push_str(&format!("{}{}", delim, item));
            delim = ",";
        }
        out
    }
}
#[derive(Clone)]
enum LogType {
    Log,
    Warning,
    Error,
}
#[derive(Clone)]
struct LogMsg {
    log_type: LogType,
    message: String,
}
impl LogMsg {
    fn log(msg: impl AsRef<str>) -> Self {
        let message = msg.as_ref().to_string();
        let log_type = LogType::Log;
        Self { log_type, message, }
    }
    fn warn(msg: impl AsRef<str>) -> Self {
        let message = msg.as_ref().to_string();
        let log_type = LogType::Warning;
        Self { log_type, message, }
    }
    fn error(msg: impl AsRef<str>) -> Self {
        let message = msg.as_ref().to_string();
        let log_type = LogType::Error;
        Self { log_type, message, }
    }
}
trait NavigateMap {
    fn set_nearest_pos(&mut self);
    fn move_action(&mut self);
    fn through_items(
        the_map: &mut Vec<Vec<usize>>, sub_positions: &mut Vec<usize>,
        items: &Vec<Item>, display_hidden: bool
    );
    fn from_items(items: &Vec<Item>, display_hidden: bool) -> Vec<Vec<usize>>;
}
struct NavigationMap {
    position: Vec<usize>,
    valid_positions: Vec<Vec<usize>>,
}
struct Navigator {
    height: u16,
    map: NavigationMap,
    d_buffer: Vec<LogMsg>,
    display_hidden: bool,
    debug: bool,
    action: NavAction,
    mode: NavMode,
    container: Container,
    i_buffer: String,
}
impl Navigator {
    fn new(ctx: &mut Ctx) -> Self {
        let display_hidden = false;
        let container = Container::load(ctx)
            .unwrap_or_else(|_| panic!("Failed to load list"));
        let valid_pos = Self::from_items(&container.list.items, display_hidden);
        let nav_map = NavigationMap { valid_positions: valid_pos, position: vec![0], };
        Self {
            height: 0,
            debug: ctx.args.debug, d_buffer: Vec::new(), display_hidden, map: nav_map,
            action: NavAction::NoAction, container, mode: NavMode::Navigate,
            i_buffer: String::new()
        }
    }
    fn save_and_reload(&mut self, ctx: &mut Ctx) {
        self.container.save()
            .unwrap_or_else(|_| panic!("Failed to save list"));
        let container = Container::load(ctx)
            .unwrap_or_else(|_| panic!("Failed to load list"));
        let nav_map = Self::from_items(&container.list.items, self.display_hidden);
        self.container = container;
        self.map.valid_positions = nav_map;
        self.set_nearest_pos();
    }
    fn push_log(&mut self, msg: impl AsRef<str>) {
        self.d_buffer.push(LogMsg::log(msg));
    }
    fn push_warn(&mut self, msg: impl AsRef<str>) {
        self.d_buffer.push(LogMsg::warn(msg));
    }
    fn push_error(&mut self, msg: impl AsRef<str>) {
        self.d_buffer.push(LogMsg::error(msg));
    }
    fn render_buffer(&mut self) -> Vec<widgets::ListItem> {
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
    fn get_todo_item_location(&mut self) -> Vec<usize> {
        let mut action_vec = self.map.position.clone();
        action_vec.reverse();
        action_vec.iter_mut().for_each(|item| {
            *item = *item + 1;
        });
        action_vec
    }
    fn take_movement_action(&mut self, ctx: &mut Ctx) -> bool {
        let keep_run;
        let is_movement;
        match self.action {
            NavAction::ToggleDebug => {
                self.push_log("Toggling debug mode");
                self.debug = !self.debug;
                keep_run = true;
                is_movement = false;
            },
            NavAction::NoAction | NavAction::Initial => {
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
        };
        if is_movement {
            self.move_action();
        }
        keep_run
    }
    fn take_action(&mut self, ctx: &mut Ctx) -> bool {
        match self.mode {
            NavMode::Navigate => self.take_movement_action(ctx),
            NavMode::Input => true,
        }
    }
    fn handle_input(&mut self, event: TermEvent, initial: bool) {
        match self.mode {
            NavMode::Navigate => {
                self.action = NavAction::from_event(event, initial);
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
    fn item_as_widget(&self, item: &Item, items: &mut Vec<widgets::ListItem>, pos: &mut Vec<usize>) {
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
    fn get_line_no_from_map(&self) -> usize {
        let mut y = 0;
        self.map.position.iter().for_each(|item| {
            y = y + item + 1;
        });
        y
    }
    fn item_to_list_items(
        &self, item: &Item, items: &mut Vec<widgets::ListItem>,
        iter_state: &mut Vec<usize>, mut lines: usize
    ) {
        if !self.display_hidden && item.hidden {
            return;
        }
        lines = lines + 1;
        // TODO: Handle scrolling via height and y_pos
        //let y_pos = self.get_line_no_from_map();
        //let rel_pos = self.height
        // add item to list of widgets
        self.item_as_widget(&item, items, iter_state);
        let mut i = 0;
        for sub_item in item.sub_items.iter() {
            iter_state.push(i);
            self.item_to_list_items(sub_item, items, iter_state, lines);
            iter_state.pop().unwrap();
            i = i + 1;
        }
    }
    fn get_list(&self) -> Vec<widgets::ListItem> {
        let mut iter_state = Vec::new();
        let mut items = Vec::new();
        // TODO: Handle empty list
        let mut i = 0;
        let lines = 0;
        for item in self.container.list.items.iter() {
            iter_state.push(i);
            self.item_to_list_items(item, &mut items, &mut iter_state, lines);
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
struct TerminalManager {
    term: Terminal<CrosstermBackend<Stdout>>,
    event_rx: Receiver<TermEvent>,
    navigator: Navigator,
}
impl TerminalManager {
    fn init(
        ctx: &mut Ctx, mut out: Stdout, event_rx: Receiver<TermEvent>,
    ) -> Result<Self, IOError> {
        execute!(
            &mut out,
            cursor::Hide,
        )?;
        execute!(
            &mut out,
            terminal::EnterAlternateScreen,
        )?;
        terminal::enable_raw_mode()?;
        let term = tui::Terminal::new(CrosstermBackend::new(out))?;
        Ok(Self {
            term, event_rx,
            navigator: Navigator::new(ctx),
        })
    }
    fn run(&mut self, ctx: &mut Ctx) {
        let mut initial = true;
        loop {
            self.navigator.handle_input(self.event_rx.recv().unwrap(), initial);
            if initial {
                initial = false;
            }
            if !self.navigator.take_action(ctx) {
                break;
            }
            let is_input_mode = if self.navigator.mode.eq(&NavMode::Input) {
                true
            } else {
                false
            };
            self.term.draw(|rect| {
                let constraints = if self.navigator.debug && is_input_mode {
                    [
                        layout::Constraint::Min(3),
                        layout::Constraint::Length(3),
                        layout::Constraint::Length(6),
                    ].as_ref()
                } else if is_input_mode {
                    [
                        layout::Constraint::Min(3),
                        layout::Constraint::Length(3),
                    ].as_ref()
                } else if self.navigator.debug {
                    [
                        layout::Constraint::Min(3),
                        layout::Constraint::Length(6),
                    ].as_ref()
                } else {
                    [
                        layout::Constraint::Min(3),
                    ].as_ref()
                };
                let layout = layout::Layout::default()
                    .direction(layout::Direction::Vertical)
                    .margin(1)
                    .constraints(constraints)
                    .split(rect.size());
                let list_items = self.navigator.get_list();
                let list = widgets::List::new(list_items).block(
                    widgets::Block::default()
                        .borders(widgets::Borders::all())
                        .title(ctx.get_path().to_str().unwrap())
                        .title_alignment(layout::Alignment::Left)
                );
                let text_box = widgets::Paragraph::new(
                    self.navigator.i_buffer.clone()
                ).block(
                    widgets::Block::default()
                        .borders(widgets::Borders::all())
                        .title("Text")
                        .title_alignment(layout::Alignment::Left)
                );
                if self.navigator.debug && is_input_mode  {
                    rect.render_widget(list, layout[0]);
                    rect.render_widget(text_box, layout[1]);
                    let buf_items = self.navigator.render_buffer();
                    let buf_list = widgets::List::new(buf_items).block(
                        widgets::Block::default()
                        .borders(widgets::Borders::all())
                        .title("Debug")
                        .title_alignment(layout::Alignment::Left)
                    );
                    rect.render_widget(buf_list, layout[2]);
                } else if is_input_mode {
                    rect.render_widget(list, layout[0]);
                    rect.render_widget(text_box, layout[1]);
                } else if self.navigator.debug {
                    rect.render_widget(list, layout[0]);
                    let buf_items = self.navigator.render_buffer();
                    let buf_list = widgets::List::new(buf_items).block(
                        widgets::Block::default()
                        .borders(widgets::Borders::all())
                        .title("Debug")
                        .title_alignment(layout::Alignment::Left)
                    );
                    rect.render_widget(buf_list, layout[1]);
                } else {
                    rect.render_widget(list, layout[0]);
                }
                if !layout[0].height.eq(&self.navigator.height) {
                    let y = layout[0].height.clone();
                    self.navigator.height = y;
                    self.navigator.push_log(format!("Height set to {}", y));
                }
            }).unwrap();
        }
    }
    fn exit(&mut self) -> Result<(), IOError> {
        terminal::disable_raw_mode()?;
        execute!(
            self.term.backend_mut(),
            terminal::LeaveAlternateScreen,
        )?;
        execute!(
            get_stdout(),
            cursor::Show,
        )?;
        Ok(())
    }
}
fn main() -> Result<(), IOError> {
    let tick_rate = Duration::from_millis(200);
    let mut ctx;
    { // construct ctx
        let args = Args::parse();
        ctx = Ctx::new(args);
    }
    ctx.construct_path();
    match ctx.args.mode.clone() {
        Mode::New(_) => {
            Container::create(&mut ctx).unwrap_or_else(|e| panic!("{}", e));
        },
        Mode::Open(_) => {},
    }
    // main vars
    let (tx, rx) = channel();
    thread_spawn(move || {
        let mut last_tick = Instant::now();
        loop {
            let timeout = tick_rate.checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));
            if event::poll(timeout).unwrap() {
                if let event::Event::Key(key) = event::read().unwrap() {
                    tx.send(TermEvent::Key(key)).unwrap();
                }
            }
            if last_tick.elapsed() >= tick_rate {
                if let Ok(_) = tx.send(TermEvent::Tick) {
                    last_tick = Instant::now();
                }
            }
        }
    });
    let mut tman = TerminalManager::init(&mut ctx, get_stdout(), rx)?;
    tman.run(&mut ctx);
    tman.exit()?;
    Ok(())
}
