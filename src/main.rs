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
    todo_core::{ Container, GetPath, Item, ItemStatus, ItemType, },
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
enum NavAction {
    MoveOut,
    Next,
    Prev,
    MoveIn,
    NoAction,
    Exit,
    ToggleHidden,
}
impl NavAction {
    fn from_event(event: TermEvent) -> Self {
        match event {
            TermEvent::Key(key) => {
                match key.code {
                    KeyCode::Char('h') => Self::MoveOut,
                    KeyCode::Char('j') => Self::Next,
                    KeyCode::Char('k') => Self::Prev,
                    KeyCode::Char('l') => Self::MoveIn,
                    KeyCode::Char('q') => Self::Exit,
                    KeyCode::Char('H') => Self::ToggleHidden,
                    _ => Self::NoAction,
                }
            },
            TermEvent::Tick => Self::NoAction,
        }
    }
}
struct Navigator {
    display_hidden: bool,
    position: Vec<usize>,
    action: NavAction,
}
impl Navigator {
    fn new() -> Self {
        Self { display_hidden: false, position: vec![0], action: NavAction::NoAction }
    }
    fn toggle_hidden(&mut self) {
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
            return;
        }
        self.position.pop().unwrap();
    }
    fn take_action(&mut self) -> bool {
        match self.action {
            NavAction::NoAction => {
                true
            },
            NavAction::MoveOut => {
                self.outer_item();
                true
            },
            NavAction::MoveIn => {
                self.inner_item();
                true
            },
            NavAction::Next => {
                self.next_item();
                true
            },
            NavAction::Prev => {
                self.prev_item();
                true
            },
            NavAction::Exit => {
                false
            },
            NavAction::ToggleHidden => {
                self.toggle_hidden();
                true
            },
        }
    }
    fn handle_input(&mut self, event: TermEvent) {
        self.action = NavAction::from_event(event);
    }
    fn item_to_list_items(&mut self, item: Item, items: &mut Vec<widgets::ListItem>, pos: &mut Vec<usize>) {
        let mut indent_str = String::new();
        for _ in 0..pos.len() - 1 {
            indent_str.push_str("    ");
        }
        let indent = text::Span::from(indent_str);
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
            text::Spans::from(vec![ indent, status, text, ])
        ));
        let sub_items = if self.display_hidden {
            item.sub_items
        } else {
            item.sub_items.into_iter()
                .filter(|item| !item.hidden)
                .collect::<Vec<Item>>()
        };
        // don't allow position to overrun sub items
        if self.position.len() == pos.len() + 1 && sub_items.len() > 0 {
            let after_pos = self.position.pop().unwrap();
            if self.position.eq(pos) && after_pos > sub_items.len() - 1 {
                self.position.push(sub_items.len() - 1);
            } else {
                self.position.push(after_pos);
            }
        }
        let mut i = 0;
        for item in sub_items.into_iter() {
            pos.push(i);
            self.item_to_list_items(item, items, pos);
            pos.pop().unwrap();
            i = i + 1;
        }
    }
    fn get_list(&mut self, container: Container) -> Vec<widgets::ListItem> {
        let mut pos = Vec::new();
        let mut items = Vec::new();
        let list_items = if self.display_hidden {
            container.list.items
        } else {
            container.list.items.into_iter()
                .filter(|item| !item.hidden)
                .collect::<Vec<Item>>()
        };
        // don't allow position to overrun items
        if self.position.len() == 1 {
            let cur_pos = self.position.pop().unwrap();
            if cur_pos > list_items.len() - 1 {
                self.position.push(list_items.len() - 1);
            } else {
                self.position.push(cur_pos);
            }
        }
        let mut i = 0;
        for item in list_items.into_iter() {
            pos.push(i);
            self.item_to_list_items(item, &mut items, &mut pos);
            pos.pop().unwrap();
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
        loop {
            self.navigator.handle_input(self.event_rx.recv().unwrap());
            if !self.navigator.take_action() {
                break;
            }
            self.term.draw(|f| {
                let layout = layout::Layout::default()
                    .direction(layout::Direction::Vertical)
                    .margin(1)
                    .constraints(
                        [
                            layout::Constraint::Length(3)
                        ]
                    )
                    .split(f.size());
                let container = Container::load(&mut self.ctx).unwrap_or_else(|e| {
                    panic!("{}", e);
                });
                let items = self.navigator.get_list(container);
                let list = widgets::List::new(items).block(
                    widgets::Block::default()
                        .borders(widgets::Borders::all())
                        .title(self.ctx.get_path().to_str().unwrap())
                        .title_alignment(layout::Alignment::Left)
                );
                f.render_widget(list, layout[0]);
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
