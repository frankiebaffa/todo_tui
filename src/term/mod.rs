use {
    crate::{
        ctx::Ctx,
        nav::{ NavigateMap, Navigator, NavMode, },
    },
    crossterm::{ cursor, event::KeyEvent, execute, terminal },
    std::{
        io::{ Error as IOError, stdout as get_stdout, Stdout, },
        sync::mpsc::Receiver,
    },
    todo_core::GetPath,
    tui::{ backend::CrosstermBackend, layout, Terminal, widgets, },
};
pub enum TermEvent {
    Key(KeyEvent),
    Tick,
}
pub struct TerminalManager {
    term: Terminal<CrosstermBackend<Stdout>>,
    event_rx: Receiver<TermEvent>,
    navigator: Navigator,
}
impl TerminalManager {
    pub fn init(
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
        let mut navigator = Navigator::new(ctx);
        navigator.push_log(
            format!("Hash initialized {}", navigator.file_hash)
        );
        Ok(Self {
            term, event_rx,
            navigator,
        })
    }
    pub fn run(&mut self, ctx: &mut Ctx) {
        let mut initial = true;
        let mut is_running = true;
        loop {
            self.term.draw(|rect| {
                self.navigator.handle_input(self.event_rx.recv().unwrap());
                // initialize possible movements / locations
                if initial {
                    self.navigator.set_nearest_pos();
                }
                if !self.navigator.take_action(ctx) {
                    is_running = false;
                    return;
                }
                let is_input_mode = if self.navigator.mode.eq(&NavMode::Input) {
                    if initial {
                        false
                    } else {
                        true
                    }
                } else {
                    false
                };
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
                if !layout[0].height.eq(&(self.navigator.height)) {
                    let y = layout[0].height.clone();
                    self.navigator.height = y;
                    self.navigator.push_log(format!("Height set to {}", self.navigator.height));
                    self.navigator.handle_win_buf(initial);
                }
                let list_items = self.navigator.get_list();
                let list = widgets::List::new(list_items).block(
                    widgets::Block::default()
                        .borders(widgets::Borders::all())
                        .title(ctx.get_path().to_str().unwrap())
                        .title_alignment(layout::Alignment::Left)
                );
                let text_box = widgets::Paragraph::new(
                    format!("{}|", self.navigator.i_buffer.clone())
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
            }).unwrap();
            initial = false;
            if !is_running {
                break;
            }
        }
    }
    pub fn exit(&mut self) -> Result<(), IOError> {
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
