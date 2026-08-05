use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use mind_core::config::{load_or_create, ConfigOverrides};
use mind_tui::{
    app::{App, ConnectionState, Workspace},
    connection::{self, ConnectionCommand, ConnectionUpdate},
    ui::{self, RenderOptions},
};
use std::{env, net::SocketAddr, path::PathBuf, time::Duration};
use tokio::{sync::mpsc, time};

#[derive(Debug, Parser)]
#[command(version, about = "Attach to the Substrate mind daemon")]
struct Args {
    #[arg(long, env = "SUBSTRATE_CONFIG")]
    config: Option<PathBuf>,
    #[arg(long, env = "SUBSTRATE_ADDR")]
    addr: Option<SocketAddr>,
    #[arg(long)]
    ascii: bool,
    #[arg(long)]
    no_color: bool,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = Args::parse();
    let (config, _) = load_or_create(ConfigOverrides {
        config_path: args.config,
        address: args.addr,
        mcp_address: None,
    })?;
    let term_is_dumb = env::var("TERM").is_ok_and(|term| term == "dumb");
    let no_color_env = env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
    let options = RenderOptions {
        ascii: args.ascii || term_is_dumb,
        no_color: args.no_color || no_color_env || term_is_dumb,
    };

    let (command_tx, command_rx) = mpsc::channel(8);
    let (update_tx, mut update_rx) = mpsc::channel(32);
    let connection_task = tokio::spawn(connection::run(config.address, command_rx, update_tx));
    let mut terminal = ratatui::try_init().context("failed to initialize terminal")?;
    let result = run_dashboard(&mut terminal, &command_tx, &mut update_rx, options).await;
    let _ = command_tx.send(ConnectionCommand::Detach).await;
    let _ = time::timeout(Duration::from_secs(1), connection_task).await;
    let restore_result = ratatui::try_restore().context("failed to restore terminal");
    result.and(restore_result)
}

async fn run_dashboard(
    terminal: &mut ratatui::DefaultTerminal,
    commands: &mpsc::Sender<ConnectionCommand>,
    updates: &mut mpsc::Receiver<ConnectionUpdate>,
    options: RenderOptions,
) -> Result<()> {
    let mut app = App::default();
    let mut draw_interval = time::interval(Duration::from_millis(100));
    draw_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = draw_interval.tick() => {
                terminal
                    .draw(|frame| ui::render(frame, &app, options, std::time::Instant::now()))
                    .context("failed to draw dashboard")?;
                while event::poll(Duration::ZERO).context("failed to poll terminal input")? {
                    if let Event::Key(key) = event::read().context("failed to read terminal input")? {
                        if handle_key(&mut app, key, commands).await? {
                            return Ok(());
                        }
                    }
                }
            }
            update = updates.recv() => {
                match update {
                    Some(ConnectionUpdate::Connecting) => app.connecting(),
                    Some(ConnectionUpdate::Connected) => app.connected(),
                    Some(ConnectionUpdate::Snapshot(snapshot)) => {
                        app.apply_snapshot(snapshot, std::time::Instant::now());
                    }
                    Some(ConnectionUpdate::Disconnected(error)) => app.disconnected(error),
                    Some(ConnectionUpdate::DiagnosticCompleted(result)) => {
                        app.diagnostic_completed(&result);
                    }
                    Some(ConnectionUpdate::DiagnosticFailed(error)) => app.diagnostic_failed(error),
                    Some(ConnectionUpdate::DslCompleted(result)) => app.apply_dsl_result(result),
                    Some(ConnectionUpdate::Graph(graph)) => app.apply_graph(graph),
                    None => app.disconnected("connection worker stopped"),
                }
            }
        }
    }
}

async fn handle_key(
    app: &mut App,
    key: KeyEvent,
    commands: &mpsc::Sender<ConnectionCommand>,
) -> Result<bool> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return Ok(false);
    }
    if matches!(key.code, KeyCode::Char('c'))
        && key.modifiers.contains(event::KeyModifiers::CONTROL)
    {
        return Ok(true);
    }
    match key.code {
        KeyCode::F(1) => {
            app.set_workspace(Workspace::Dashboard);
            return Ok(false);
        }
        KeyCode::F(2) => {
            app.set_workspace(Workspace::Assembly);
            return Ok(false);
        }
        _ => {}
    }

    if app.workspace == Workspace::Assembly && app.prompt_focused {
        match key.code {
            KeyCode::Enter => {
                if let Some(command) = app.take_prompt_command() {
                    commands
                        .send(ConnectionCommand::ExecuteDsl(command))
                        .await
                        .context("connection worker stopped")?;
                }
            }
            KeyCode::Esc => app.blur_prompt(),
            KeyCode::Tab => app.complete_prompt(),
            KeyCode::Backspace => app.backspace_prompt(),
            KeyCode::Delete => app.delete_prompt(),
            KeyCode::Left => app.move_prompt_left(),
            KeyCode::Right => app.move_prompt_right(),
            KeyCode::Home => app.prompt_cursor = 0,
            KeyCode::End => app.prompt_cursor = app.prompt.len(),
            KeyCode::Up => app.history_previous(),
            KeyCode::Down => app.history_next(),
            KeyCode::Char(character)
                if key.modifiers.is_empty() || key.modifiers == event::KeyModifiers::SHIFT =>
            {
                app.insert_prompt_char(character)
            }
            _ => {}
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Esc if app.show_help => app.show_help = false,
        KeyCode::Esc => return Ok(true),
        KeyCode::Char('?') => app.show_help = !app.show_help,
        KeyCode::Char('d') if matches!(app.connection, ConnectionState::Connected) => {
            app.notice = Some("DIAGNOSTIC PULSE REQUESTED".to_owned());
            commands
                .send(ConnectionCommand::DiagnosticPulse)
                .await
                .context("connection worker stopped")?;
        }
        KeyCode::Char('r') => {
            commands
                .send(ConnectionCommand::Retry)
                .await
                .context("connection worker stopped")?;
        }
        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(1),
        KeyCode::Down | KeyCode::Char('j') => app.scroll_down(1),
        KeyCode::PageUp => app.scroll_up(10),
        KeyCode::PageDown => app.scroll_down(10),
        KeyCode::Home => app.scroll_home(),
        KeyCode::End => app.scroll_end(),
        KeyCode::Char('f') => app.toggle_follow(),
        KeyCode::Char('i') if app.workspace == Workspace::Assembly => app.focus_prompt(),
        _ => {}
    }
    Ok(false)
}
