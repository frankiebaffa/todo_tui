mod args;
mod ctx;
mod log;
mod nav;
mod term;
mod win;
use {
    args::{ Args, Mode },
    clap::Parser,
    crossterm::event,
    ctx::Ctx,
    std::{
        io::{ Error as IOError, stdout as get_stdout, },
        thread::spawn as thread_spawn,
        time::{ Duration, Instant, },
        sync::mpsc::channel,
    },
    term::{ TermEvent, TerminalManager, },
    todo_core::Container,
};
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
fn main() -> Result<(), IOError> {
    const TICK: u64 = 200;
    let tick_rate = Duration::from_millis(TICK);
    let mut ctx;
    { // construct ctx
        let args = Args::parse();
        ctx = Ctx::new(args);
    }
    ctx.construct_path();
    match ctx.args.mode.clone() {
        Mode::New(_) => {
            let mut c = Container::create(&mut ctx).unwrap_or_else(|e| panic!("{}", e));
            c.save().unwrap_or_else(|e| panic!("{}", e));
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
