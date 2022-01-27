use clap::Parser;
use todo_core::Container;
use todo_core::GetPath;
use crossterm::event;
use crossterm::event::DisableMouseCapture;
use crossterm::event::EnableMouseCapture;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::execute;
use crossterm::terminal::disable_raw_mode;
use crossterm::terminal::enable_raw_mode;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use std::io::Error as IOError;
use std::io::stdout as get_stdout;
use std::io::Stdout;
use std::path::PathBuf;
use std::thread::sleep as thread_sleep;
use std::thread::spawn as thread_spawn;
use std::time::Duration;
use std::time::Instant;
use std::sync::Mutex;
use std::sync::Arc;
use tui::backend::CrosstermBackend;
use tui::widgets::Block;
use tui::widgets::Borders;
use tui::Terminal;
#[derive(Parser, Clone)]
struct Args {
    #[clap(short, long)]
    list_path: String,
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
        let tmp_path = PathBuf::from(format!("{}", &self.args.list_path));
        match tmp_path.extension() {
            Some(ext) => {
                if !ext.eq("json") {
                    self.path.push(format!("{}.json", &self.args.list_path));
                } else {
                    self.path.push(format!("{}", &self.args.list_path));
                }
            },
            None => self.path.push(format!("{}.json", &self.args.list_path)),
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
struct TerminalManager {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    ctx: Ctx,
}
impl TerminalManager {
    fn init(ctx: Ctx) -> Result<TerminalManager, IOError> {
        enable_raw_mode()?;
        let mut stdout = get_stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(Self { terminal, ctx, })
    }
    fn close(&mut self) -> Result<(), IOError> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture,
        )?;
        self.terminal.show_cursor()?;
        Ok(())
    }
    fn redraw(&mut self) -> Result<(), IOError> {
        let container = Container::load(&mut self.ctx);
        self.terminal.draw(|f| {
            let size = f.size();
            let block = Block::default()
                .title("Block")
                .borders(Borders::ALL);
            f.render_widget(block, size);
        })?;
        Ok(())
    }
}
const TICK_MS: u128 = 200;
fn wait_if_lt_tick(instant: Instant) {
    if instant.elapsed().as_millis().lt(&TICK_MS) {
        let ms = instant.elapsed().as_millis();
        thread_sleep(Duration::from_millis((TICK_MS - ms) as u64));
    }
}
fn main() -> Result<(), IOError> {
    let mut ctx;
    { // construct ctx
        let args = Args::parse();
        ctx = Ctx::new(args);
    }
    ctx.construct_path();
    // main vars
    let terminal = Arc::new(Mutex::new(TerminalManager::init(ctx)?));
    let running = Arc::new(Mutex::new(true));
    let t1_terminal = terminal.clone();
    let t1_running = running.clone();
    // terminal thread
    thread_spawn(move || { // t1
        let terminal = t1_terminal;
        let running = t1_running;
        loop {
            let loop_start = Instant::now();
            { // check running lock
                let running_lock = match running.lock() {
                    Ok(l) => l,
                    Err(_) => return,
                };
                if !*running_lock {
                    return;
                }
            }
            let mut lock = match terminal.lock() {
                Ok(l) => l,
                Err(_) => {
                    let mut running_lock = match running.lock() {
                        Ok(l) => l,
                        Err(_) => return,
                    };
                    *running_lock = false;
                    return;
                },
            };
            match lock.redraw() {
                Ok(_) => {},
                Err(_) => {
                    let mut running_lock = match running.lock() {
                        Ok(l) => l,
                        Err(_) => return,
                    };
                    *running_lock = false;
                    return;
                },
            }
            wait_if_lt_tick(loop_start);
        }
    });
    // check loop
    loop {
        let loop_start = Instant::now();
        let mut running_lock = match running.lock() {
            Ok(l) => l,
            Err(_) => return Ok(()),
        };
        if !*running_lock {
            let mut term_lock = match terminal.lock() {
                Ok(t) => t,
                Err(_) => return Ok(()),
            };
            match term_lock.close() {
                Ok(_) => {},
                Err(_) => {
                    *running_lock = false;
                    return Ok(());
                },
            }
            return Ok(());
        }
        wait_if_lt_tick(loop_start);
    }
}
