use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde::{Deserialize, Serialize};
use std::{
    env,
    error::Error,
    fs::{File, OpenOptions},
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph,
        Scrollbar, ScrollbarState, ScrollbarOrientation,
    },
    Frame, Terminal,
};

#[derive(Serialize, Deserialize, Clone)]
struct Page {
    name: String,
    todos: Vec<Todo>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Todo {
    name: String,
    completed: bool,
}

const TICK_RATE_MS: u64 = 250;

struct App {
    pages: Vec<Page>,
    current_page_index: usize,

    selected_todo_index: usize,

    is_creating_todo: bool,
    new_todo_input: String,

    is_creating_page: bool,
    new_page_name_input: String,

    is_renaming_page: bool,
    rename_page_input: String,

    is_renaming_todo: bool,
    rename_todo_input: String,

    should_quit: bool,

    scrollbar_state: ScrollbarState,

    context_prefix: String,

    cursor_position: usize,
}

impl App {
    fn new() -> App {
        let mut pages = load_app_data().unwrap_or_else(|_| vec![]);
        if pages.is_empty() {
            pages.push(Page {
                name: "main".to_string(),
                todos: vec![],
            });
        }

        let context_prefix = get_context_prefix();

        App {
            pages,
            current_page_index: 0,

            selected_todo_index: 0,

            is_creating_todo: false,
            new_todo_input: String::new(),

            is_creating_page: false,
            new_page_name_input: String::new(),

            is_renaming_page: false,
            rename_page_input: String::new(),

            is_renaming_todo: false,
            rename_todo_input: String::new(),

            should_quit: false,

            scrollbar_state: ScrollbarState::default(),

            context_prefix,
            cursor_position: 0,
        }
    }

    fn current_page(&self) -> &Page {
        &self.pages[self.current_page_index]
    }

    fn current_todos(&self) -> &Vec<Todo> {
        &self.pages[self.current_page_index].todos
    }

    fn current_todos_mut(&mut self) -> &mut Vec<Todo> {
        &mut self.pages[self.current_page_index].todos
    }

    fn save_app_data(&self) -> Result<(), Box<dyn Error>> {
        save_app_data(&self.pages)
    }

    fn update_scrollbar(&mut self, list_height: usize) {
        let current_todos_len = self.current_todos().len();
        if current_todos_len > 0 {
            self.scrollbar_state = ScrollbarState::default()
                .content_length(current_todos_len)
                .viewport_content_length(list_height)
                .position(self.selected_todo_index);
        }
    }

    fn process_input_event(&mut self, key: KeyEvent) -> bool {
        if self.is_creating_todo {
            match key.code {
                KeyCode::Down => {
                    if !self.current_todos().is_empty() {
                        self.selected_todo_index = (self.selected_todo_index + 1) % self.current_todos().len();
                    }
                    return true;
                }
                KeyCode::Up => {
                    if !self.current_todos().is_empty() {
                        self.selected_todo_index = (self.selected_todo_index + self.current_todos().len() - 1) % self.current_todos().len();
                    }
                    return true;
                }
                _ => {}
            }

            match Self::edit_buffer(&mut self.new_todo_input, &mut self.cursor_position, key) {
                EditResult::Enter => {
                    let name: String = self.new_todo_input.drain(..).collect();
                    if !name.is_empty() {
                        self.current_todos_mut().push(Todo { name, completed: false });
                        self.selected_todo_index = self.current_todos().len() - 1;
                        self.save_app_data().ok();
                    }
                    self.is_creating_todo = false;
                    self.cursor_position = 0;
                }
                EditResult::Esc => {
                    self.is_creating_todo = false;
                    self.new_todo_input.clear();
                    self.cursor_position = 0;
                }
                EditResult::None => {}
            }

            return true;
        }

        if self.is_creating_page {
            match Self::edit_buffer(&mut self.new_page_name_input, &mut self.cursor_position, key) {
                EditResult::Enter => {
                    let page_name = if self.new_page_name_input.is_empty() {
                        "".to_string()
                    } else {
                        self.new_page_name_input.drain(..).collect()
                    };
                    self.pages.push(Page { name: page_name, todos: vec![] });
                    self.current_page_index = self.pages.len() - 1;
                    self.selected_todo_index = 0;
                    self.is_creating_page = false;
                    self.cursor_position = 0;
                    self.save_app_data().ok();
                }
                EditResult::Esc => {
                    self.is_creating_page = false;
                    self.new_page_name_input.clear();
                    self.cursor_position = 0;
                }
                EditResult::None => {}
            }
            return true;
        }

        if self.is_renaming_page {
            match Self::edit_buffer(&mut self.rename_page_input, &mut self.cursor_position, key) {
                EditResult::Enter => {
                    if self.rename_page_input.is_empty() {
                        if self.pages.len() > 1 {
                            self.pages.remove(self.current_page_index);
                            if self.current_page_index >= self.pages.len() {
                                self.current_page_index = self.pages.len() - 1;
                            }
                            self.selected_todo_index = 0;
                        }
                    } else {
                        self.pages[self.current_page_index].name = self.rename_page_input.drain(..).collect();
                    }
                    self.is_renaming_page = false;
                    self.rename_page_input.clear();
                    self.save_app_data().ok();
                }
                EditResult::Esc => {
                    self.is_renaming_page = false;
                    self.rename_page_input.clear();
                }
                EditResult::None => {}
            }
            return true;
        }

        if self.is_renaming_todo {
            match Self::edit_buffer(&mut self.rename_todo_input, &mut self.cursor_position, key) {
                EditResult::Enter => {
                    if !self.current_todos().is_empty() {
                        let index = self.selected_todo_index;
                        if self.rename_todo_input.is_empty() {
                            self.current_todos_mut().remove(index);
                            if self.current_todos().is_empty() {
                                self.selected_todo_index = 0;
                            } else if self.selected_todo_index >= self.current_todos().len() {
                                self.selected_todo_index = self.current_todos().len() - 1;
                            }
                        } else {
                            self.current_todos_mut()[index].name = self.rename_todo_input.drain(..).collect();
                        }
                        self.save_app_data().ok();
                    }
                    self.is_renaming_todo = false;
                    self.rename_todo_input.clear();
                }
                EditResult::Esc => {
                    self.is_renaming_todo = false;
                    self.rename_todo_input.clear();
                }
                EditResult::None => {}
            }
            return true;
        }

        false
    }

    fn edit_buffer(buf: &mut String, cursor_pos: &mut usize, key: KeyEvent) -> EditResult {
        match key.code {
            KeyCode::Enter => EditResult::Enter,
            KeyCode::Esc => EditResult::Esc,
            KeyCode::Char(c) => {
                buf.insert(*cursor_pos, c);
                *cursor_pos += 1;
                EditResult::None
            }
            KeyCode::Backspace => {
                if *cursor_pos > 0 {
                    *cursor_pos -= 1;
                    buf.remove(*cursor_pos);
                }
                EditResult::None
            }
            KeyCode::Left => {
                if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                    if *cursor_pos > 0 {
                        let bytes = buf.as_bytes();
                        let mut pos = *cursor_pos - 1;
                        while pos > 0 && bytes[pos].is_ascii_whitespace() { pos -= 1; }
                        while pos > 0 && !bytes[pos].is_ascii_whitespace() { pos -= 1; }
                        if pos > 0 || bytes[0].is_ascii_whitespace() { pos += 1; }
                        *cursor_pos = pos;
                    }
                } else if *cursor_pos > 0 {
                    *cursor_pos -= 1;
                }
                EditResult::None
            }
            KeyCode::Right => {
                if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                    let len = buf.len();
                    if *cursor_pos < len {
                        let bytes = buf.as_bytes();
                        let mut pos = *cursor_pos;
                        while pos < len && !bytes[pos].is_ascii_whitespace() { pos += 1; }
                        while pos < len && bytes[pos].is_ascii_whitespace() { pos += 1; }
                        *cursor_pos = pos;
                    }
                } else if *cursor_pos < buf.len() {
                    *cursor_pos += 1;
                }
                EditResult::None
            }
            _ => EditResult::None,
        }
    }
}

#[derive(PartialEq, Eq, Debug)]
enum EditResult {
    Enter,
    Esc,
    None,
}

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    let tick_rate = Duration::from_millis(TICK_RATE_MS);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui(f, app))?;

        if app.should_quit {
            return Ok(());
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                } else {
                    if app.process_input_event(key) {
                    } else {
                        match key.code {
                            KeyCode::Char('q') => {
                                app.should_quit = true;
                            }
                            KeyCode::Char('n') => {
                                app.is_creating_todo = true;
                                app.cursor_position = app.new_todo_input.len();
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                                    if !app.current_todos().is_empty() && app.current_todos().len() > 1 {
                                        let current = app.selected_todo_index;
                                        let next = (current + 1) % app.current_todos().len();
                                        app.current_todos_mut().swap(current, next);
                                        app.selected_todo_index = next;
                                        app.save_app_data().unwrap();
                                    }
                                } else {
                                    if !app.current_todos().is_empty() {
                                        app.selected_todo_index = (app.selected_todo_index + 1) % app.current_todos().len();
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                                    if !app.current_todos().is_empty() && app.current_todos().len() > 1 {
                                        let current = app.selected_todo_index;
                                        let prev = (current + app.current_todos().len() - 1) % app.current_todos().len();
                                        app.current_todos_mut().swap(current, prev);
                                        app.selected_todo_index = prev;
                                        app.save_app_data().unwrap();
                                    }
                                } else {
                                    if !app.current_todos().is_empty() {
                                        app.selected_todo_index = (app.selected_todo_index + app.current_todos().len() - 1) % app.current_todos().len();
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                if !app.current_todos().is_empty() {
                                    let index = app.selected_todo_index;
                                    let todo = &mut app.current_todos_mut()[index];
                                    todo.completed = !todo.completed;
                                    app.save_app_data().unwrap();
                                }
                            }
                            KeyCode::Char('d') => {
                                if !app.current_todos().is_empty() {
                                    let index = app.selected_todo_index;
                                    app.current_todos_mut().remove(index);
                                    if app.current_todos().is_empty() {
                                        app.selected_todo_index = 0;
                                    } else if app.selected_todo_index >= app.current_todos().len() {
                                        app.selected_todo_index = app.current_todos().len() - 1;
                                    }
                                    app.save_app_data().unwrap();
                                }
                            }
                            KeyCode::Char('r') => {
                                if !app.current_todos().is_empty() {
                                    let index = app.selected_todo_index;
                                    app.rename_todo_input = app.current_todos()[index].name.clone();
                                    app.cursor_position = app.rename_todo_input.len();
                                    app.is_renaming_todo = true;
                                }
                            }
                            KeyCode::Right | KeyCode::Char('l') => {
                                if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                                    if app.pages.len() > 1 {
                                        let current = app.current_page_index;
                                        let next = (current + 1) % app.pages.len();
                                        app.pages.swap(current, next);
                                        app.current_page_index = next;
                                        app.save_app_data().unwrap();
                                    }
                                } else {
                                    if !app.pages.is_empty() {
                                        app.current_page_index = (app.current_page_index + 1) % app.pages.len();
                                        app.selected_todo_index = 0;
                                    }
                                }
                            }
                            KeyCode::Left | KeyCode::Char('h') => {
                                if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                                    if app.pages.len() > 1 {
                                        let current = app.current_page_index;
                                        let prev = (current + app.pages.len() - 1) % app.pages.len();
                                        app.pages.swap(current, prev);
                                        app.current_page_index = prev;
                                        app.save_app_data().unwrap();
                                    }
                                } else {
                                    if !app.pages.is_empty() {
                                        app.current_page_index = (app.current_page_index + app.pages.len() - 1) % app.pages.len();
                                        app.selected_todo_index = 0;
                                    }
                                }
                            }
                            KeyCode::Char(c @ '1'..='9') => {
                                if let Some(digit) = c.to_digit(10) {
                                    let page_index = (digit - 1) as usize;
                                    if page_index < app.pages.len() {
                                        if page_index == app.current_page_index {
                                            app.rename_page_input = app.current_page().name.clone();
                                            app.cursor_position = app.rename_page_input.len();
                                            app.is_renaming_page = true;
                                        } else {
                                            app.current_page_index = page_index;
                                            app.selected_todo_index = 0;
                                        }
                                    } else {
                                        app.is_creating_page = true;
                                        app.new_page_name_input.clear();
                                        app.cursor_position = 0;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let is_in_input_mode = app.is_creating_todo || app.is_creating_page || app.is_renaming_page || app.is_renaming_todo;
    
    let top_needed: u16 = if is_in_input_mode { 3 } else { 0 };

    let list_min_height: u16 = 5;

    let constraints = if top_needed > 0 {
        vec![
            Constraint::Length(top_needed),
            Constraint::Min(list_min_height),
        ]   
    } else {
        vec![Constraint::Min(list_min_height)]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(&constraints)
        .split(f.area());

    let (top_chunk_opt, main_chunk) = if top_needed > 0 {
        (Some(chunks[0]), chunks[1])
    } else {
        (None, chunks[0])
    };

    let neon_orange = Color::Rgb(255, 140, 0);
    let bright_orange = Color::Rgb(255, 165, 0);
    let dark_orange = Color::Rgb(180, 82, 0);
    let selected_style = Style::default().fg(Color::White);
    let done_style = Style::default().fg(dark_orange);
    let default_style = Style::default().fg(bright_orange);
    let preview_style = Style::default().fg(Color::Rgb(100, 100, 100));
    let todo_border_style = Style::default().fg(neon_orange);
    let input_border_style = Style::default().fg(bright_orange);
    let page_active_style = Style::default().fg(Color::Black).bg(neon_orange);
    let page_inactive_style = Style::default().fg(bright_orange);

    let list_height = (main_chunk.height.saturating_sub(2)) as usize;

    let mut items: Vec<ListItem> = app
    .current_todos()
    .iter()
    .enumerate()
    .flat_map(|(i, todo)| {
        let checkbox = if todo.completed { "[X] " } else { "[ ] " };
        let style = if todo.completed { done_style } else { default_style };
        
        let line_style = if i == app.selected_todo_index && !app.is_creating_todo {
            selected_style
        } else {
            style
        };

        let selector = if i == app.selected_todo_index && !app.is_creating_todo { ">> " } else { "   " };
        let mut result = vec![ListItem::new(format!("{}{}{}", selector, checkbox, todo.name)).style(line_style)];
        
        if app.is_creating_todo && i == app.selected_todo_index {
            let preview_text = format!(">> [ ] {}", app.new_todo_input);
            result.push(ListItem::new(preview_text).style(preview_style));
        }
        
        result
    })
    .collect();

    if app.current_todos().is_empty() && app.is_creating_todo {
        let preview_text = format!(">> [ ] {}", app.new_todo_input);
        items.push(ListItem::new(preview_text).style(preview_style));
    }

    let page_spans: Vec<Span> = app.pages.iter().enumerate().map(|(i, page)| {
        let style = if i == app.current_page_index {
            page_active_style
        } else {
            page_inactive_style
        };
        Span::styled(format!(" {}: {} ", i + 1, page.name), style)
    }).collect();

    let mut title_spans = vec![
        Span::styled(format!(" {} ", app.context_prefix), Style::default().fg(neon_orange))
    ];
    title_spans.extend(page_spans);
    let page_title = Line::from(title_spans);
    
    let help_text = " new: [n] | rename: [r] | complete: [↵] | delete: [d] | nav: [↑↓→←],[hjkl] | new/rename page: [1-9] | quit: [q] ";
    
    let list = List::new(items)
        .block(
            Block::default()
                .title_top(page_title)
                .title_bottom(help_text)
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(todo_border_style),
        )
        .highlight_style(selected_style);

    let mut state = ListState::default();
    if !(app.is_creating_todo && app.current_todos().is_empty()) {
        state.select(Some(app.selected_todo_index));
    }

    f.render_stateful_widget(list, main_chunk, &mut state);

    app.update_scrollbar(list_height);

    let current_todos_len = app.current_todos().len();
    if !app.current_todos().is_empty() && list_height < current_todos_len {
        let scrollbar_area = Rect::new(
            main_chunk.x + main_chunk.width - 1,
            main_chunk.y + 1,
            1,
            main_chunk.height.saturating_sub(2),
        );

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(None)
            .thumb_symbol("▐")
            .thumb_style(neon_orange);

        f.render_stateful_widget(scrollbar, scrollbar_area, &mut app.scrollbar_state);
    }

    if let Some(top_chunk) = top_chunk_opt {
        let prefix_len: u16 = 2;
        
        if app.is_creating_page {
            let input_title = " new page - [↵]: save | [ESC]: cancel ";
            let display_text = format!("* {}", app.new_page_name_input);
            let input = Paragraph::new(display_text.as_str())
                .style(default_style)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .title(input_title)
                        .border_style(input_border_style),
                );
            f.render_widget(input, top_chunk);
            f.set_cursor_position(
                ratatui::layout::Position::new(
                    top_chunk.x + 1 + prefix_len + app.cursor_position as u16,
                    top_chunk.y + 1,
                ),
            );
        } else if app.is_renaming_page {
            let input_title = " rename page - [↵]: save | {EMPTY}: delete page | [ESC]: cancel ";
            let display_text = format!("* {}", app.rename_page_input);
            let input = Paragraph::new(display_text.as_str())
                .style(default_style)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .title(input_title)
                        .border_style(input_border_style),
                );
            f.render_widget(input, top_chunk);
            f.set_cursor_position(
                ratatui::layout::Position::new(
                    top_chunk.x + 1 + prefix_len + app.cursor_position as u16,
                    top_chunk.y + 1,
                ),
            );
        } else if app.is_renaming_todo {
            let input_title = " rename todo - [↵]: save | {EMPTY}: delete todo | [ESC]: cancel ";
            let display_text = format!("* {}", app.rename_todo_input);
            let input = Paragraph::new(display_text.as_str())
                .style(default_style)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .title(input_title)
                        .border_style(input_border_style),
                );
            f.render_widget(input, top_chunk);
            f.set_cursor_position(
                ratatui::layout::Position::new(
                    top_chunk.x + 1 + prefix_len + app.cursor_position as u16,
                    top_chunk.y + 1,
                ),
            );
        } else if app.is_creating_todo {
            let input_title = " new todo - [↵]: save | [ESC]: cancel ";
            let display_text = format!("* {}", app.new_todo_input);
            let input = Paragraph::new(display_text.as_str())
                .style(default_style)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .title(input_title)
                        .border_style(input_border_style),
                );
            f.render_widget(input, top_chunk);
            f.set_cursor_position(
                ratatui::layout::Position::new(
                    top_chunk.x + 1 + prefix_len + app.cursor_position as u16,
                    top_chunk.y + 1,
                ),
            );
        }
    }
}

fn get_context_prefix() -> String {
    let path = get_data_path().unwrap_or_else(|_| PathBuf::from("todo.json"));
    
    if let Some(home_dir) = home::home_dir() {
        let home_todo = home_dir.join(".todo.json");
        if path == home_todo {
            return "[global]: ".to_string();
        }
    }
    
    if let Ok(current_dir) = env::current_dir() {
        if let Some(dir_name) = current_dir.file_name() {
            if let Some(name_str) = dir_name.to_str() {
                return format!("[{}]:", name_str);
            }
        }
    }
    
    "[local]: ".to_string()
}

fn get_data_path() -> Result<PathBuf, Box<dyn Error>> {
    let local_path = Path::new("todo.json");
    if local_path.exists() {
        return Ok(local_path.to_path_buf());
    }

    let home_dir = home::home_dir().ok_or("could not find home directory")?;
    let home_path = home_dir.join(".todo.json");
    Ok(home_path)
}

fn load_app_data() -> Result<Vec<Page>, Box<dyn Error>> {
    let path = get_data_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut contents = String::new();
    reader.read_to_string(&mut contents)?;

    let pages: Vec<Page> = serde_json::from_str(&contents)?;
    Ok(pages)
}

fn save_app_data(pages: &[Page]) -> Result<(), Box<dyn Error>> {
    let path = get_data_path()?;
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    let mut writer = BufWriter::new(file);

    let json = serde_json::to_string_pretty(pages)?;
    writer.write_all(json.as_bytes())?;

    Ok(())
}
