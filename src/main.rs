mod app;
#[allow(dead_code)]
mod highlight;
mod input;
#[allow(dead_code)]
mod source;
mod ui;

use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc;

use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;
use source::SourceMsg;

#[derive(Parser, Debug)]
#[command(
    name = "log-lens",
    about = "Streaming log viewer TUI with filtering and highlighting"
)]
struct Args {
    /// Log files to view (reads stdin if none given)
    #[arg()]
    files: Vec<PathBuf>,

    /// Follow mode: keep reading new data (like tail -f)
    #[arg(short, long)]
    follow: bool,

    /// Maximum lines to buffer (ring buffer)
    #[arg(long, default_value = "100000")]
    buffer_size: usize,
}

fn is_stdin_pipe() -> bool {
    use std::os::unix::io::AsRawFd;
    let fd = io::stdin().as_raw_fd();
    unsafe { libc::isatty(fd) == 0 }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let is_stdin = args.files.is_empty();
    let follow = args.follow || is_stdin;

    // Build source names
    let source_names: Vec<String> = if is_stdin {
        vec!["stdin".to_string()]
    } else {
        args.files
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.display().to_string())
            })
            .collect()
    };

    let mut app = App::new(args.buffer_size, source_names, follow);

    // Set up source reader channels
    let (tx, rx) = mpsc::channel::<SourceMsg>();

    if is_stdin {
        if !is_stdin_pipe() {
            eprintln!("log-lens: no files given and stdin is a terminal");
            eprintln!("Usage: log-lens [OPTIONS] [FILES...]");
            eprintln!("       command | log-lens");
            std::process::exit(1);
        }
        source::read_stdin(tx);
    } else {
        for (idx, path) in args.files.iter().enumerate() {
            source::read_file(path.clone(), idx, follow, tx.clone());
        }
        drop(tx);
    }

    // When stdin is piped, we need to use /dev/tty for terminal I/O
    // so the TUI can draw to the real terminal
    if is_stdin {
        let tty = File::options().read(true).write(true).open("/dev/tty")?;
        let tty_write = tty.try_clone()?;

        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ = crossterm::execute!(std::io::stdout(), LeaveAlternateScreen);
            original_hook(info);
        }));

        enable_raw_mode()?;

        let mut tty_writer = io::BufWriter::new(tty_write);
        execute!(tty_writer, EnterAlternateScreen, EnableMouseCapture)?;
        tty_writer.flush()?;

        let backend = CrosstermBackend::new(tty_writer);
        let mut terminal = Terminal::new(backend)?;

        let result = run_loop(&mut terminal, &mut app, &rx);

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        if let Err(e) = result {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    } else {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = run_loop(&mut terminal, &mut app, &rx);

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        if let Err(e) = result {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn run_loop<W: Write>(
    terminal: &mut Terminal<CrosstermBackend<W>>,
    app: &mut App,
    rx: &mpsc::Receiver<SourceMsg>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        // Drain incoming lines (non-blocking)
        let mut got_new = false;
        loop {
            match rx.try_recv() {
                Ok(SourceMsg::Line {
                    content,
                    source_idx,
                }) => {
                    app.add_line(content, source_idx);
                    got_new = true;
                }
                Ok(SourceMsg::Error { msg, source_idx: _ }) => {
                    app.add_line(format!("[log-lens error] {}", msg), 0);
                    got_new = true;
                }
                Ok(SourceMsg::Eof { source_idx: _ }) => {}
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        // Refilter if needed
        if app.needs_refilter || got_new {
            if app.needs_refilter {
                app.refilter();
            } else if got_new && app.search.regex.is_some() {
                let fi_len = app.filtered_indices.len();
                let start_fi = app
                    .search
                    .match_indices
                    .last()
                    .copied()
                    .map(|l| l + 1)
                    .unwrap_or(0);
                if let Some(ref regex) = app.search.regex {
                    for fi in start_fi..fi_len {
                        if let Some(&li) = app.filtered_indices.get(fi) {
                            if let Some(line) = app.lines.get(li) {
                                if regex.is_match(&line.content) {
                                    app.search.match_indices.push(fi);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Auto-scroll
        app.maybe_auto_scroll();

        // Draw
        terminal.draw(|frame| {
            ui::draw(frame, app);
        })?;

        // Handle input
        input::handle_events(app)?;

        // Refilter after input might have changed filters
        if app.needs_refilter {
            app.refilter();
            app.maybe_auto_scroll();
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
