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
    Initial,
}
impl NavAction {
    fn from_event(event: TermEvent, initial: bool) -> Self {
        if initial {
            return Self::Initial;
        }
        match event {
            TermEvent::Key(key) => {
                match key.code {
                    KeyCode::Char('h') => Self::MoveOut,
                    KeyCode::Char('j') => Self::Next,
                    KeyCode::Char('k') => Self::Prev,
                    KeyCode::Char('l') => Self::MoveIn,
                    KeyCode::Char('q') => Self::Exit,
                    KeyCode::Char('H') => Self::ToggleShowHidden,
                    KeyCode::Char('c') => Self::CycleItemStatus,
                    KeyCode::Char('s') => Self::ToggleItemHidden,
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
struct Navigator {
    d_buffer: Vec<LogMsg>,
    display_hidden: bool,
    position: Vec<usize>,
    action: NavAction,
}
impl Navigator {
    fn new() -> Self {
        Self {
            d_buffer: Vec::new(), display_hidden: false, position: vec![0],
            action: NavAction::NoAction
        }
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
    fn toggle_show_hidden(&mut self) {
        self.display_hidden = !self.display_hidden;
        self.position = vec![0];
    }
    fn next_item(&mut self) {
        let item = self.position.pop().unwrap();
        self.position.push(item + 1);
    }
    fn prev_item(&mut self) {
        let item = self.position.pop().unwrap();
        if item == 0 {
            self.push_warn("Underflow in vertical position corrected (nav)");
            self.position.push(item);
            return;
        }
        self.position.push(item - 1);
    }
    fn inner_item(&mut self) {
        self.position.push(0);
    }
    fn outer_item(&mut self) {
        if self.position.len() == 1 {
            self.push_warn("Underflow in horizontal position corrected (nav)");
            return;
        }
        self.position.pop().unwrap();
    }
    fn get_todo_item_location(&mut self) -> Vec<usize> {
        let mut action_vec = self.position.clone();
        action_vec.reverse();
        action_vec.iter_mut().for_each(|item| {
            *item = *item + 1;
        });
        action_vec
    }
    fn cycle_status_at(&mut self, container: &mut Container) {
        let mut action_vec = self.get_todo_item_location();
        container.act_on_item_at(&mut action_vec, ItemAction::CycleStatus);
    }
    fn toggle_item_hidden(&mut self, container: &mut Container) {
        let mut action_vec = self.get_todo_item_location();
        container.act_on_item_at(&mut action_vec, ItemAction::ToggleHidden);
        self.position = vec![0];
    }
    fn take_action(&mut self, container: &mut Container) -> bool {
        match self.action {
            NavAction::NoAction | NavAction::Initial => {
                true
            },
            NavAction::MoveOut => {
                self.push_log("Moving out");
                self.outer_item();
                true
            },
            NavAction::MoveIn => {
                self.push_log("Moving in");
                self.inner_item();
                true
            },
            NavAction::Next => {
                self.push_log("Next item");
                self.next_item();
                true
            },
            NavAction::Prev => {
                self.push_log("Prev item");
                self.prev_item();
                true
            },
            NavAction::Exit => {
                false
            },
            NavAction::ToggleShowHidden => {
                self.push_log("Hidden toggled");
                self.toggle_show_hidden();
                true
            },
            NavAction::CycleItemStatus => {
                self.push_log("Cycling status");
                self.cycle_status_at(container);
                true
            },
            NavAction::ToggleItemHidden => {
                self.push_log("Toggling item hidden");
                self.toggle_item_hidden(container);
                true
            },
        }
    }
    fn handle_input(&mut self, event: TermEvent, initial: bool) {
        self.action = NavAction::from_event(event, initial);
    }
    fn item_as_widget(&mut self, item: &Item, items: &mut Vec<widgets::ListItem>, pos: &mut Vec<usize>) {
        let mut indent_str = String::new();
        for _ in 0..pos.len() - 1 {
            indent_str.push_str("    ");
        }
        let indent = text::Span::from(indent_str);
        let hidden_icon = if self.display_hidden {
            if item.hidden {
                "H "
            } else {
                "  "
            }
        } else {
            ""
        };
        let hidden = text::Span::styled(
            hidden_icon,
            style::Style::default().fg(style::Color::Blue)
        );
        let status = match item.item_type {
            ItemType::Todo => {
                match item.status {
                    ItemStatus::Complete => {
                        text::Span::styled(
                            "[x] ",
                            style::Style::default()
                                .fg(style::Color::Green)
                        )
                    },
                    ItemStatus::Incomplete => {
                        text::Span::styled(
                            "[ ] ",
                            style::Style::default()
                                .fg(style::Color::Red)
                        )
                    },
                    ItemStatus::Disabled => {
                        text::Span::styled(
                            "[-] ",
                            style::Style::default()
                                .fg(style::Color::Yellow)
                        )
                    },
                }
            },
            ItemType::Note => {
                text::Span::styled(
                    "-   ",
                    style::Style::default()
                        .fg(style::Color::Cyan)
                )
            },
        };
        let text = if (*pos).eq(&self.position) {
            text::Span::styled(
                item.text.clone(),
                style::Style::default().fg(style::Color::Cyan)
            )
        } else {
            text::Span::from(item.text.clone())
        };
        items.push(widgets::ListItem::new(
            text::Spans::from(vec![ indent, hidden, status, text, ])
        ));
    }
    fn fix_position_by_items(&mut self, iter_state: &mut Vec<usize>, items: &Vec<Item>) {
        // correct vertical position overflow
        if self.position.len() > iter_state.len() {
            let cur_pos = self.position.get(iter_state.len()).unwrap();
            if cur_pos > &(items.len() - 1) {
                self.position.push(items.len() - 1);
                self.position[iter_state.len()] = items.len() - 1;
                self.push_warn("Overflow in vertical position corrected");
            }
        }
        // translate vertical position if not displaying hidden and last action
        // was vertical movement or last action alters the position vector
        if !self.display_hidden && (
            self.action.eq(&NavAction::Initial) ||
            self.action.eq(&NavAction::Next) ||
            self.action.eq(&NavAction::Prev) ||
            self.action.eq(&NavAction::ToggleShowHidden) ||
            self.action.eq(&NavAction::ToggleItemHidden) ||
            self.action.eq(&NavAction::MoveIn) ||
            self.action.eq(&NavAction::MoveOut)
        ) {
            let pos_in_list = self.position.get(iter_state.len()).unwrap();
            // match protects against overflow
            let message;
            let index = match items.iter()
                .filter(|item| !item.hidden)
                .nth(pos_in_list.clone())
            {
                Some(hid_item) => {
                    message = "Position translated from found item";
                    items.iter()
                        .position(|item| item.eq(hid_item))
                        .unwrap()
                },
                None => {
                    message = "Position translated from corrected vertical overflow";
                    let mut displayed_items = items.iter()
                        .filter(|item| !item.hidden)
                        .collect::<Vec<&Item>>();
                    let last_displayed = displayed_items.pop().unwrap();
                    items.iter()
                        .position(|item| item.eq(last_displayed))
                        .unwrap()
                },
            };
            if !index.eq(pos_in_list) {
                self.position[iter_state.len()] = index;
                self.push_warn(message);
            }
        }
        let mut moment = 0;
        for item in items.into_iter() {
            // correct horizontal/vertical position overflow for item
            if self.position.len() > iter_state.len() {
                let last_nav_pos = self.position.pop().unwrap();
                let cur_nav_pos = self.position.pop().unwrap();
                if cur_nav_pos == moment {
                    let sub_len = if self.display_hidden {
                        items.len()
                    } else {
                        items.iter()
                            .filter(|item| !item.hidden)
                            .collect::<Vec<&Item>>()
                            .len()
                    };
                    if sub_len > 0 && last_nav_pos > sub_len - 1 {
                        self.position.push(cur_nav_pos);
                        self.position.push(sub_len - 1);
                        self.push_warn(
                            "Overflow in vertical position corrected, > sub-items"
                        );
                    } else if sub_len == 0 {
                        self.position.push(cur_nav_pos);
                        self.push_warn(
                            "Overflow in horizontal position corrected, 0 sub-items"
                        );
                    } else {
                        self.position.push(cur_nav_pos);
                        self.position.push(last_nav_pos);
                    }
                } else {
                    self.position.push(cur_nav_pos);
                    self.position.push(last_nav_pos);
                }
            }
            self.fix_position_by_items(iter_state, &item.sub_items);
            moment = moment + 1;
        }
    }
    fn item_to_list_items(&mut self, item: Item, items: &mut Vec<widgets::ListItem>, pos: &mut Vec<usize>) {
        // add item to list of widgets
        self.item_as_widget(&item, items, pos);
        let sub_items = item.sub_items;
        let mut i = 0;
        for sub_item in sub_items.into_iter() {
            pos.push(i);
            self.item_to_list_items(sub_item, items, pos);
            pos.pop().unwrap();
            i = i + 1;
        }
    }
    fn get_list(&mut self, container: Container) -> Vec<widgets::ListItem> {
        let mut iter_state = Vec::new();
        let mut items = Vec::new();
        let mut list_items = container.list.items;
        if list_items.len() == 0 {
            list_items.push(
                Item::new(
                    ItemType::Note,
                    "There are no items in this list with current display settings"
                )
            );
        }
        self.fix_position_by_items(&mut Vec::new(), &list_items);
        let mut i = 0;
        for item in list_items {
            iter_state.push(i);
            self.item_to_list_items(item, &mut items, &mut iter_state);
            iter_state.pop().unwrap();
            i = i + 1;
        }
        items
    }
}
struct TerminalManager {
    ctx: Ctx,
    term: Terminal<CrosstermBackend<Stdout>>,
    event_rx: Receiver<TermEvent>,
    navigator: Navigator,
}
impl TerminalManager {
    fn init(
        ctx: Ctx, mut out: Stdout, event_rx: Receiver<TermEvent>,
    ) -> Result<Self, IOError> {
        execute!(
            &mut out,
            cursor::Hide,
        )?;
        execute!(
            &mut out,
            terminal::EnterAlternateScreen,
            event::EnableMouseCapture,
        )?;
        terminal::enable_raw_mode()?;
        let term = tui::Terminal::new(CrosstermBackend::new(out))?;
        Ok(Self {
            ctx, term,
            event_rx,
            navigator: Navigator::new(),
        })
    }
    fn run(&mut self) {
        let mut initial = true;
        loop {
            self.navigator.handle_input(self.event_rx.recv().unwrap(), initial);
            if initial {
                initial = false;
            }
            let mut container = Container::load(&mut self.ctx).unwrap_or_else(|e| {
                panic!("{}", e);
            });
            if !self.navigator.take_action(&mut container) {
                break;
            }
            // save/reload container if need be
            match self.navigator.action {
                NavAction::CycleItemStatus | NavAction::ToggleItemHidden => {
                    container.save().unwrap_or_else(|_| {
                        self.navigator.push_error("Failed to save list");
                    });
                    match Container::load(&mut self.ctx) {
                        Ok(c) => {
                            container = c;
                        },
                        Err(_) => {
                            self.navigator.push_error("Failed to load list");
                        },
                    };
                },
                _ => {},
            }
            self.term.draw(|rect| {
                let constraints = if self.ctx.args.debug {
                    [
                        layout::Constraint::Ratio(3, 5),
                        layout::Constraint::Ratio(2, 5)
                    ].as_ref()
                } else {
                    [
                        layout::Constraint::Length(3),
                    ].as_ref()
                };
                let layout = layout::Layout::default()
                    .direction(layout::Direction::Vertical)
                    .margin(1)
                    .constraints(constraints)
                    .split(rect.size());
                let list_items = self.navigator.get_list(container);
                let list = widgets::List::new(list_items).block(
                    widgets::Block::default()
                        .borders(widgets::Borders::all())
                        .title(self.ctx.get_path().to_str().unwrap())
                        .title_alignment(layout::Alignment::Left)
                );
                if self.ctx.args.debug {
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
    let mut tman = TerminalManager::init(ctx, get_stdout(), rx)?;
    tman.run();
    tman.exit()?;
    Ok(())
}
