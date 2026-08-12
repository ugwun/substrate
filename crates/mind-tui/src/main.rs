use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use mind_core::config::{load_or_create, ConfigOverrides};
use mind_tui::{
    app::{App, ConnectionState, ContextAction, OnboardingView, Workspace},
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
    let mut draw_interval = time::interval(Duration::from_millis(160));
    draw_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = draw_interval.tick() => {
                let now = std::time::Instant::now();
                app.advance_globe_animation(now);
                terminal
                    .draw(|frame| ui::render(frame, &app, options, now))
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
                    Some(ConnectionUpdate::Planet(planet)) => {
                        app.apply_planet(*planet, std::time::Instant::now());
                    }
                    Some(ConnectionUpdate::Disconnected(error)) => app.disconnected(error),
                    Some(ConnectionUpdate::DiagnosticCompleted(result)) => {
                        app.diagnostic_completed(&result);
                    }
                    Some(ConnectionUpdate::DiagnosticFailed(error)) => app.diagnostic_failed(error),
                    Some(ConnectionUpdate::OnboardingCompleted(result)) => {
                        app.onboarding_completed(result);
                    }
                    Some(ConnectionUpdate::PlanetActionCompleted(kind, result)) => {
                        app.planet_action_completed(kind, result);
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
    app.record_input(std::time::Instant::now());
    if app.show_return_summary {
        if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
            if let Some(activity_id) = app.dismiss_return_summary() {
                commands
                    .send(ConnectionCommand::AcknowledgeActivity(activity_id))
                    .await
                    .context("connection worker stopped")?;
            }
        }
        return Ok(false);
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
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Char('r') if !matches!(app.connection, ConnectionState::Connected) => {
            commands
                .send(ConnectionCommand::Retry)
                .await
                .context("connection worker stopped")?;
        }
        KeyCode::Tab => app.cycle_workspace(key.modifiers.contains(event::KeyModifiers::SHIFT)),
        KeyCode::BackTab => app.cycle_workspace(true),
        KeyCode::Esc if app.pending_action.is_some() => app.cancel_confirmation_action(),
        KeyCode::Enter if app.pending_action.is_some() => match app.pending_action.clone() {
            Some(ContextAction::Extract(region_id)) => {
                commands
                    .send(ConnectionCommand::DeployExtractor(region_id))
                    .await
                    .context("connection worker stopped")?;
            }
            Some(ContextAction::ConstructRelay(region_id)) => {
                commands
                    .send(ConnectionCommand::ConstructRelay(region_id))
                    .await
                    .context("connection worker stopped")?;
            }
            Some(
                ContextAction::QueueSurvey(_)
                | ContextAction::RemoveQueuedSurvey(_)
                | ContextAction::SetExtractionRoute(_)
                | ContextAction::ClearExtractionRoute
                | ContextAction::StartResearch(_)
                | ContextAction::ActivateProfile(_),
            ) => app.cancel_confirmation_action(),
            _ => app.cancel_confirmation_action(),
        },
        _ if app.pending_action.is_some() => {}
        KeyCode::Left if app.workspace == Workspace::Planet => {
            app.move_selection(
                0,
                if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                    -5
                } else {
                    -1
                },
            );
        }
        KeyCode::Right if app.workspace == Workspace::Planet => {
            app.move_selection(
                0,
                if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                    5
                } else {
                    1
                },
            );
        }
        KeyCode::Up if app.workspace == Workspace::Planet => {
            app.move_selection(
                if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                    -2
                } else {
                    -1
                },
                0,
            );
        }
        KeyCode::Down if app.workspace == Workspace::Planet => {
            app.move_selection(
                if key.modifiers.contains(event::KeyModifiers::SHIFT) {
                    2
                } else {
                    1
                },
                0,
            );
        }
        KeyCode::PageUp if app.workspace == Workspace::Planet => {
            app.scroll_region_panel(-6);
        }
        KeyCode::PageDown if app.workspace == Workspace::Planet => {
            app.scroll_region_panel(6);
        }
        KeyCode::Enter if app.workspace == Workspace::Planet => match app.primary_region_action() {
            Some(ContextAction::Deploy(region_id)) => {
                commands
                    .send(ConnectionCommand::DeployScout(region_id))
                    .await
                    .context("connection worker stopped")?;
            }
            Some(ContextAction::Recall) => {
                commands
                    .send(ConnectionCommand::RecallScout)
                    .await
                    .context("connection worker stopped")?;
            }
            Some(ContextAction::Extract(region_id)) => {
                app.request_confirmation(ContextAction::Extract(region_id));
            }
            Some(ContextAction::ConstructRelay(region_id)) => {
                app.request_confirmation(ContextAction::ConstructRelay(region_id));
            }
            _ => {}
        },
        KeyCode::Char('s') if app.workspace == Workspace::Planet => {
            if let Some(ContextAction::Deploy(region_id)) = app.scout_action() {
                commands
                    .send(ConnectionCommand::DeployScout(region_id))
                    .await
                    .context("connection worker stopped")?;
            }
        }
        KeyCode::Char('x') if app.workspace == Workspace::Planet => {
            if let Some(ContextAction::Extract(region_id)) = app.extraction_action() {
                app.request_confirmation(ContextAction::Extract(region_id));
            }
        }
        KeyCode::Char('b') if app.workspace == Workspace::Planet => {
            if let Some(ContextAction::ConstructRelay(region_id)) = app.construction_action() {
                app.request_confirmation(ContextAction::ConstructRelay(region_id));
            }
        }
        KeyCode::Char('r') if app.workspace == Workspace::Planet => {
            if matches!(app.scout_recall_action(), Some(ContextAction::Recall)) {
                commands
                    .send(ConnectionCommand::RecallScout)
                    .await
                    .context("connection worker stopped")?;
            }
        }
        KeyCode::Char('a') if app.workspace == Workspace::Planet => {
            match app.automation_action() {
                Some(ContextAction::QueueSurvey(region_id)) => {
                    commands
                        .send(ConnectionCommand::QueueSurvey(region_id))
                        .await
                }
                Some(ContextAction::RemoveQueuedSurvey(region_id)) => {
                    commands
                        .send(ConnectionCommand::RemoveQueuedSurvey(region_id))
                        .await
                }
                Some(ContextAction::SetExtractionRoute(region_id)) => {
                    commands
                        .send(ConnectionCommand::SetExtractionRoute(region_id))
                        .await
                }
                Some(ContextAction::ClearExtractionRoute) => {
                    commands.send(ConnectionCommand::ClearExtractionRoute).await
                }
                _ => Ok(()),
            }
            .context("connection worker stopped")?;
        }
        KeyCode::Up | KeyCode::Char('k') if app.workspace == Workspace::Overview => {
            app.move_overview_selection(-1);
        }
        KeyCode::Down | KeyCode::Char('j') if app.workspace == Workspace::Overview => {
            app.move_overview_selection(1);
        }
        KeyCode::Enter if app.workspace == Workspace::Overview => {
            app.open_selected_overview_region();
        }
        KeyCode::PageUp if app.workspace == Workspace::Overview => {
            app.scroll_overview_status(-6);
        }
        KeyCode::PageDown if app.workspace == Workspace::Overview => {
            app.scroll_overview_status(6);
        }
        KeyCode::Up | KeyCode::Char('k') if app.workspace == Workspace::Research => {
            app.move_research_selection(-1);
        }
        KeyCode::Down | KeyCode::Char('j') if app.workspace == Workspace::Research => {
            app.move_research_selection(1);
        }
        KeyCode::Enter if app.workspace == Workspace::Research => {
            match app.selected_research_action() {
                Some(ContextAction::StartResearch(project)) => {
                    commands
                        .send(ConnectionCommand::StartResearch(project))
                        .await
                }
                Some(ContextAction::ActivateProfile(profile)) => {
                    commands
                        .send(ConnectionCommand::ActivateProfile(profile))
                        .await
                }
                _ => Ok(()),
            }
            .context("connection worker stopped")?;
        }
        KeyCode::Char('d')
            if app.workspace == Workspace::System
                && matches!(app.connection, ConnectionState::Connected) =>
        {
            app.notice = Some("DIAGNOSTIC PULSE REQUESTED".to_owned());
            commands
                .send(ConnectionCommand::DiagnosticPulse)
                .await
                .context("connection worker stopped")?;
        }
        KeyCode::Up | KeyCode::Char('k') if app.workspace == Workspace::System => app.scroll_up(1),
        KeyCode::Down | KeyCode::Char('j') if app.workspace == Workspace::System => {
            app.scroll_down(1);
        }
        KeyCode::PageUp if app.workspace == Workspace::System => app.scroll_up(10),
        KeyCode::PageDown if app.workspace == Workspace::System => app.scroll_down(10),
        KeyCode::Home if app.workspace == Workspace::System => app.scroll_home(),
        KeyCode::End if app.workspace == Workspace::System => app.scroll_end(),
        KeyCode::Char('f') if app.workspace == Workspace::System => app.toggle_follow(),
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

    #[tokio::test]
    async fn planet_region_keys_send_typed_actions() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([4, 4, 4]);
        let mut app = App::default();
        app.apply_snapshot(engine.status(), std::time::Instant::now());
        app.apply_planet(engine.planet().unwrap(), std::time::Instant::now());
        app.connected();
        app.move_selection(0, 1);
        let target = app.selected_region_id.clone().unwrap();
        let (commands, mut received) = mpsc::channel(2);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::DeployScout(region)) if region == target
        ));

        engine.deploy_scout(&target);
        app.apply_planet(engine.planet().unwrap(), std::time::Instant::now());
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::RecallScout)
        ));
    }

    #[tokio::test]
    async fn overview_enter_returns_to_the_selected_operation_region() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([4, 4, 4]);
        let target = engine
            .planet()
            .unwrap()
            .regions
            .into_iter()
            .find(|region| region.within_scout_range && !region.core)
            .unwrap();
        engine.deploy_scout(&target.id);

        let mut app = App::default();
        app.apply_snapshot(engine.status(), std::time::Instant::now());
        app.apply_planet(engine.planet().unwrap(), std::time::Instant::now());
        app.workspace = Workspace::Overview;
        let (commands, mut received) = mpsc::channel(1);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();

        assert_eq!(app.workspace, Workspace::Planet);
        assert_eq!(app.selected_region_id.as_deref(), Some(target.id.as_str()));
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn shifted_arrows_jump_across_the_expanded_grid() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([4, 4, 4]);
        let mut app = App::default();
        app.apply_snapshot(engine.status(), std::time::Instant::now());
        app.apply_planet(engine.planet().unwrap(), std::time::Instant::now());
        app.connected();
        let start = app.selected_region().unwrap().position.clone();
        let (commands, _received) = mpsc::channel(1);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT),
            &commands,
        )
        .await
        .unwrap();
        let selected = &app.selected_region().unwrap().position;
        assert_eq!(selected.latitude_band, start.latitude_band);
        assert_eq!(selected.longitude_sector, (start.longitude_sector + 5) % 20);
    }
}
