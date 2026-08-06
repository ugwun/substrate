use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use mind_core::config::{load_or_create, ConfigOverrides};
use mind_tui::{
    app::{App, ConnectionState, OnboardingView},
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
                    Some(ConnectionUpdate::OnboardingCompleted(result)) => {
                        app.onboarding_completed(result);
                    }
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
    if app.show_help {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') => app.show_help = false,
            _ => {}
        }
        return Ok(false);
    }

    if app.snapshot.is_none() {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
            KeyCode::Char('?') => app.show_help = true,
            KeyCode::Char('r') => {
                commands
                    .send(ConnectionCommand::Retry)
                    .await
                    .context("connection worker stopped")?;
            }
            _ => {}
        }
        return Ok(false);
    }

    if app.onboarding_pending() {
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('?') => app.show_help = true,
            KeyCode::Char('r') => {
                commands
                    .send(ConnectionCommand::Retry)
                    .await
                    .context("connection worker stopped")?;
            }
            KeyCode::Esc if app.onboarding_view == OnboardingView::Confirming => {
                app.cancel_confirmation();
            }
            KeyCode::Up if app.onboarding_view == OnboardingView::Adjusting => {
                app.select_previous_band();
            }
            KeyCode::Down if app.onboarding_view == OnboardingView::Adjusting => {
                app.select_next_band();
            }
            KeyCode::Left if app.onboarding_view == OnboardingView::Adjusting => {
                app.rotate_selected(-1);
            }
            KeyCode::Right if app.onboarding_view == OnboardingView::Adjusting => {
                app.rotate_selected(1);
            }
            KeyCode::Enter if app.onboarding_view == OnboardingView::Adjusting => {
                app.observe();
            }
            KeyCode::Enter
                if app.onboarding_view == OnboardingView::Confirming
                    && matches!(app.connection, ConnectionState::Connected) =>
            {
                app.begin_submission();
                commands
                    .send(ConnectionCommand::CompleteOnboarding(app.band_positions))
                    .await
                    .context("connection worker stopped")?;
            }
            KeyCode::Enter if app.onboarding_view == OnboardingView::Confirming => {
                app.notice = Some("A daemon connection is required to awaken.".into());
            }
            _ => {}
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Char('q') => return Ok(true),
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
        _ => {}
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[tokio::test]
    async fn aligned_onboarding_requires_observe_then_awaken() {
        let mut app = App::default();
        app.apply_snapshot(
            mind_core::engine::MindEngine::new(20).status(),
            std::time::Instant::now(),
        );
        app.connected();
        app.band_positions = [4, 4, 4];
        let (commands, mut received) = mpsc::channel(2);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert_eq!(app.onboarding_view, OnboardingView::Confirming);
        assert!(received.try_recv().is_err());

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert_eq!(app.onboarding_view, OnboardingView::Submitting);
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::CompleteOnboarding([4, 4, 4]))
        ));
    }

    #[tokio::test]
    async fn onboarding_input_is_blocked_until_status_arrives() {
        let mut app = App::default();
        app.band_positions = [4, 4, 4];
        let (commands, mut received) = mpsc::channel(1);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert_eq!(app.onboarding_view, OnboardingView::Adjusting);
        assert!(received.try_recv().is_err());
    }
}
