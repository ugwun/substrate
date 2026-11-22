use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, EventStream, KeyCode, KeyEvent, KeyEventKind};
use futures_util::StreamExt;
use mind_core::config::{load_or_create, ConfigOverrides};
use mind_core::protocol::BaseModuleKind;
use mind_tui::{
    app::{
        App, ConnectionState, ContextAction, OnboardingView, ScrollLimits, SessionMenuAction,
        SessionMenuState, Workspace,
    },
    boot::BootMode,
    connection::{self, ConnectionCommand, ConnectionUpdate},
    ui::{self, RenderOptions},
};
use std::{
    env,
    io::{self, Write},
    net::SocketAddr,
    path::PathBuf,
    time::Duration,
};
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
    #[arg(long, env = "SUBSTRATE_TRUECOLOR")]
    truecolor: bool,
    #[arg(long, env = "SUBSTRATE_ANSI16")]
    ansi16: bool,
    /// Opt-in: update the terminal window/tab title with live mission status.
    #[arg(long, env = "SUBSTRATE_TITLE")]
    title: bool,
    /// Opt-in: ring the terminal bell when a mission or discovery completes.
    #[arg(long, env = "SUBSTRATE_BELL")]
    bell: bool,
    /// Skip the startup session menu and jump directly into the live dashboard.
    #[arg(long, env = "SUBSTRATE_RESUME")]
    resume: bool,
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
    let (ansi16, ansi256) = if options.no_color || term_is_dumb || args.truecolor {
        (false, false)
    } else if args.ansi16 {
        (true, false)
    } else if supports_truecolor() {
        (false, false)
    } else if supports_256color() {
        (false, true)
    } else {
        (true, false)
    };
    let enable_boot_intro = !term_is_dumb && !args.resume;

    let (command_tx, command_rx) = mpsc::channel(8);
    let (update_tx, mut update_rx) = mpsc::channel(32);
    let connection_task = tokio::spawn(connection::run(config.address, command_rx, update_tx));
    let mut terminal = ratatui::try_init().context("failed to initialize terminal")?;
    let result = run_dashboard(
        &mut terminal,
        &command_tx,
        &mut update_rx,
        options,
        ansi16,
        ansi256,
        enable_boot_intro,
        args.title && !term_is_dumb,
        args.bell && !term_is_dumb,
        args.resume,
    )
    .await;
    if args.title && !term_is_dumb {
        let _ = write_osc_title("");
    }
    let _ = command_tx.send(ConnectionCommand::Detach).await;
    let _ = time::timeout(Duration::from_secs(1), connection_task).await;
    let restore_result = ratatui::try_restore().context("failed to restore terminal");
    result.and(restore_result)
}

fn supports_truecolor() -> bool {
    if env::var("COLORTERM").is_ok_and(|value| {
        let lower = value.to_ascii_lowercase();
        lower.contains("truecolor") || lower.contains("24bit")
    }) {
        return true;
    }
    if env::var_os("WT_SESSION").is_some() {
        return true;
    }
    if env::var("TERM_PROGRAM").is_ok_and(|value| {
        matches!(
            value.as_str(),
            "WezTerm" | "iTerm.app" | "vscode" | "Alacritty" | "ghostty" | "kitty" | "Hyper"
        )
    }) {
        return true;
    }
    env::var("TERM").is_ok_and(|value| {
        let lower = value.to_ascii_lowercase();
        lower.contains("truecolor")
            || lower.contains("24bit")
            || lower.contains("kitty")
            || lower.contains("alacritty")
            || lower.contains("wezterm")
            || lower.contains("ghostty")
    })
}

fn supports_256color() -> bool {
    env::var("TERM").is_ok_and(|value| value.to_ascii_lowercase().contains("256color"))
}

fn write_osc_title(title: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    write!(stdout, "\x1b]0;{title}\x07")?;
    stdout.flush()
}

fn ring_terminal_bell() -> io::Result<()> {
    let mut stdout = io::stdout();
    write!(stdout, "\x07")?;
    stdout.flush()
}

#[allow(clippy::too_many_arguments)]
async fn run_dashboard(
    terminal: &mut ratatui::DefaultTerminal,
    commands: &mpsc::Sender<ConnectionCommand>,
    updates: &mut mpsc::Receiver<ConnectionUpdate>,
    options: RenderOptions,
    ansi16: bool,
    ansi256: bool,
    enable_boot_intro: bool,
    enable_title: bool,
    enable_bell: bool,
    auto_resume: bool,
) -> Result<()> {
    let mut app = App::default();
    app.ansi16 = ansi16;
    app.ansi256 = ansi256;
    app.animations_enabled = enable_boot_intro;
    if enable_boot_intro {
        app.enable_boot_intro();
    }
    if !auto_resume {
        app.open_session_menu();
    }
    let mut initial_snapshot_seen = false;
    let mut events = EventStream::new();
    let mut dirty = true;
    let mut last_window_title = String::new();
    let mut last_bell_sig: Option<(usize, usize, usize, u64)> = None;

    loop {
        if dirty {
            let now = std::time::Instant::now();
            app.advance_globe_animation(now);
            let draw_start = std::time::Instant::now();
            terminal
                .draw(|frame| ui::render(frame, &app, options, now))
                .context("failed to draw dashboard")?;
            app.record_frame_metrics(draw_start.elapsed());
            if enable_title {
                let next_title = app.companion_window_title(now);
                if next_title != last_window_title {
                    let _ = write_osc_title(&next_title);
                    last_window_title = next_title;
                }
            }
            if let Some(ack_id) = app.pending_live_activity_ack(now) {
                let _ = commands.try_send(ConnectionCommand::AcknowledgeActivity(ack_id));
            }
            dirty = false;
        }

        let now = std::time::Instant::now();
        let next_deadline = app.next_frame_deadline(now);
        let animation_sleep = async {
            if let Some(deadline) = next_deadline {
                time::sleep_until(time::Instant::from_std(deadline)).await;
            } else {
                std::future::pending::<()>().await;
            }
        };

        // Process one event per select! iteration directly from EventStream (never calling
        // now_or_never(), which leaves crossterm's internal wake task holding a no-op waker).
        tokio::select! {
            biased;
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(event)) => {
                        match event {
                            Event::Key(key) => {
                                if handle_key(&mut app, key, commands).await? {
                                    if let Some(ack_id) = app.pending_live_activity_ack(std::time::Instant::now()) {
                                        let _ = commands
                                            .send(ConnectionCommand::AcknowledgeActivity(ack_id))
                                            .await;
                                    }
                                    return Ok(());
                                }
                                dirty = true;
                            }
                            Event::Resize(_, _) => {
                                dirty = true;
                            }
                            _ => {}
                        }
                    }
                    Some(Err(error)) => {
                        return Err(error).context("failed to read terminal input");
                    }
                    None => return Ok(()),
                }
            }
            update = updates.recv() => {
                match update {
                    Some(ConnectionUpdate::Connecting) => app.connecting(),
                    Some(ConnectionUpdate::Connected) => app.connected(),
                    Some(ConnectionUpdate::Snapshot(snapshot)) => {
                        let onboarding_incomplete = !snapshot.onboarding.complete;
                        app.apply_snapshot(snapshot, std::time::Instant::now());
                        // On a brand-new world, enter Awakening -> Coherence directly rather than
                        // presenting a management menu offering "CONTINUE".
                        if !initial_snapshot_seen {
                            initial_snapshot_seen = true;
                            if onboarding_incomplete
                                && app.session_menu == Some(SessionMenuState::Selecting)
                            {
                                app.close_session_menu();
                            }
                        }
                    }
                    Some(ConnectionUpdate::Planet(planet)) => {
                        let had_planet = app.planet.is_some();
                        let now = std::time::Instant::now();
                        app.apply_planet(*planet, now);
                        if let Some(ack_id) = app.pending_live_activity_ack(now) {
                            let _ = commands
                                .try_send(ConnectionCommand::AcknowledgeActivity(ack_id));
                        }
                        if let Some(sig) = app.bell_milestone_signature() {
                            if enable_bell
                                && had_planet
                                && last_bell_sig.is_some_and(|prev| {
                                    sig.0 > prev.0
                                        || sig.1 > prev.1
                                        || sig.2 > prev.2
                                        || sig.3 > prev.3
                                })
                            {
                                let _ = ring_terminal_bell();
                            }
                            last_bell_sig = Some(sig);
                        }
                    }
                    Some(ConnectionUpdate::Disconnected(error)) => app.disconnected(error),
                    Some(ConnectionUpdate::DiagnosticCompleted(result)) => {
                        app.diagnostic_completed(&result);
                    }
                    Some(ConnectionUpdate::DiagnosticFailed(error)) => app.diagnostic_failed(error),
                    Some(ConnectionUpdate::OnboardingCompleted(result)) => {
                        app.onboarding_completed(result);
                    }
                    Some(ConnectionUpdate::NewPlaythroughCompleted(result)) => {
                        app.new_playthrough_completed(result);
                    }
                    Some(ConnectionUpdate::DaemonStopped) => return Ok(()),
                    Some(ConnectionUpdate::LifecycleFailed(error)) => {
                        app.lifecycle_failed(error);
                    }
                    Some(ConnectionUpdate::PlanetActionCompleted(kind, result)) => {
                        app.planet_action_completed(kind, result);
                    }
                    None => app.disconnected("connection worker stopped"),
                }
                dirty = true;
            }
            _ = animation_sleep => {
                dirty = true;
            }
        }
    }
}

fn normalize_letter_case(mut key: KeyEvent) -> KeyEvent {
    if let KeyCode::Char(c) = key.code {
        if c.is_ascii_uppercase() {
            key.code = KeyCode::Char(c.to_ascii_lowercase());
            key.modifiers.remove(event::KeyModifiers::SHIFT);
        }
    }
    key
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
    // Hints display uppercase letters; accept either case (Shift or Caps Lock) for every command.
    let key = normalize_letter_case(key);
    let now = std::time::Instant::now();
    app.record_input(now);
    app.clear_transition();
    if app.session_menu.is_some() {
        return handle_session_menu(app, key, commands).await;
    }
    if app.boot_intro_active(now) {
        let is_quick = app
            .boot_intro
            .is_some_and(|boot| boot.mode == BootMode::QuickPowerOn);
        app.skip_boot_intro();
        // On the brief QuickPowerOn reattach wipe, only Space/Esc merely skips; intentional
        // keys fall straight through so user input is never swallowed on reattach.
        if !is_quick || matches!(key.code, KeyCode::Char(' ') | KeyCode::Esc) {
            return Ok(false);
        }
    }
    if key.code == KeyCode::Char('q') {
        app.open_session_menu();
        return Ok(false);
    }
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
    if app.active_transmission_record().is_some() {
        match key.code {
            KeyCode::Char(' ') => {
                app.transmission_revealed = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.scroll_transmission(-1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.scroll_transmission(1);
            }
            KeyCode::PageUp => {
                app.scroll_transmission(-5);
            }
            KeyCode::PageDown => {
                app.scroll_transmission(5);
            }
            KeyCode::Enter | KeyCode::Esc
                if crossterm::terminal::size()
                    .is_ok_and(|(width, height)| width < 44 || height < 14) =>
            {
                // Too small to read the record: keep it pending rather than filing it unseen.
                app.notice = Some("ENLARGE THE TERMINAL TO READ THIS RECORD".into());
            }
            KeyCode::Enter | KeyCode::Esc => {
                if let Some(record_id) =
                    app.reveal_or_dismiss_transmission(std::time::Instant::now())
                {
                    commands
                        .send(ConnectionCommand::AcknowledgeArchivePresentation(record_id))
                        .await
                        .context("connection worker stopped")?;
                }
            }
            _ => {}
        }
        return Ok(false);
    }
    if app.show_chapter_card {
        if matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('c')) {
            app.show_chapter_card = false;
        }
        return Ok(false);
    }
    if app.show_help {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') => {
                app.show_help = false;
                app.help_scroll = 0;
            }
            KeyCode::Up => app.scroll_help(-1),
            KeyCode::Down => app.scroll_help(1),
            KeyCode::PageUp => app.scroll_help(-isize::from(ScrollLimits::page(
                &app.scroll_limits.help_rows,
            ))),
            KeyCode::PageDown => app.scroll_help(isize::from(ScrollLimits::page(
                &app.scroll_limits.help_rows,
            ))),
            _ => {}
        }
        return Ok(false);
    }

    if app.snapshot.is_none() {
        match key.code {
            KeyCode::Esc => app.open_session_menu(),
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
            KeyCode::Esc => app.open_session_menu(),
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
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Char('r') if !matches!(app.connection, ConnectionState::Connected) => {
            commands
                .send(ConnectionCommand::Retry)
                .await
                .context("connection worker stopped")?;
        }
        KeyCode::Tab => {
            app.cycle_workspace(key.modifiers.contains(event::KeyModifiers::SHIFT));
            app.trigger_transition(now);
        }
        KeyCode::BackTab => {
            app.cycle_workspace(true);
            app.trigger_transition(now);
        }
        KeyCode::Char(digit @ '1'..='7') if app.pending_action.is_none() => {
            let idx = (digit as u8 - b'0') as usize;
            if app.select_visible_workspace(idx) {
                app.trigger_transition(now);
            }
        }
        KeyCode::Esc if app.pending_action.is_some() => app.cancel_confirmation_action(),
        KeyCode::Enter
            if app.pending_action.is_some()
                && crossterm::terminal::size().is_ok_and(|(width, height)| {
                    !ui::confirmation_visible(app, width, height, false)
                }) =>
        {
            // The confirmation is not fully on screen: refuse rather than confirm unseen.
            app.notice = Some("ENLARGE THE TERMINAL TO CONFIRM".into());
        }
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
            Some(ContextAction::ExtractSubregion {
                region_id,
                subregion_id,
            }) => {
                commands
                    .send(ConnectionCommand::DeployExtractorAt(
                        region_id,
                        subregion_id,
                    ))
                    .await
                    .context("connection worker stopped")?;
            }
            Some(ContextAction::EstablishBase(subregion_id)) => {
                commands
                    .send(ConnectionCommand::EstablishBase(subregion_id))
                    .await
                    .context("connection worker stopped")?;
            }
            Some(ContextAction::MaintainBase(subregion_id)) => {
                commands
                    .send(ConnectionCommand::MaintainBase(subregion_id))
                    .await
                    .context("connection worker stopped")?;
            }
            Some(ContextAction::ConstructBaseModule {
                subregion_id,
                module,
            }) => {
                commands
                    .send(ConnectionCommand::ConstructBaseModule(subregion_id, module))
                    .await
                    .context("connection worker stopped")?;
            }
            Some(ContextAction::SetBaseModuleEnabled {
                subregion_id,
                module,
                enabled,
            }) => {
                commands
                    .send(ConnectionCommand::SetBaseModuleEnabled(
                        subregion_id,
                        module,
                        enabled,
                    ))
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
        // While confirming, the inspector can still be paged so its full detail is reachable.
        KeyCode::PageUp if app.pending_action.is_some() => {
            app.scroll_region_panel(-ScrollLimits::page(&app.scroll_limits.region_rows));
        }
        KeyCode::PageDown if app.pending_action.is_some() => {
            app.scroll_region_panel(ScrollLimits::page(&app.scroll_limits.region_rows));
        }
        _ if app.pending_action.is_some() => {}
        KeyCode::Esc if app.workspace == Workspace::Planet && app.local_view => {
            app.leave_local_view();
            app.trigger_transition(now);
        }
        KeyCode::Esc => app.open_session_menu(),
        KeyCode::Char('v') if app.workspace == Workspace::Planet => {
            let was_local = app.local_view;
            if app.local_view {
                app.leave_local_view();
            } else {
                app.enter_local_view();
            }
            if app.local_view != was_local {
                app.trigger_transition(now);
            }
        }
        KeyCode::Char('n') => {
            app.focus_campaign_lead();
        }
        KeyCode::Char('c') if !(app.workspace == Workspace::Planet && app.local_view) => {
            app.toggle_chapter_card();
        }
        KeyCode::Left if app.workspace == Workspace::Planet && app.local_view => {
            app.move_subregion_selection(0, -1);
        }
        KeyCode::Right if app.workspace == Workspace::Planet && app.local_view => {
            app.move_subregion_selection(0, 1);
        }
        KeyCode::Up if app.workspace == Workspace::Planet && app.local_view => {
            app.move_subregion_selection(-1, 0);
        }
        KeyCode::Down if app.workspace == Workspace::Planet && app.local_view => {
            app.move_subregion_selection(1, 0);
        }
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
            app.scroll_region_panel(-ScrollLimits::page(&app.scroll_limits.region_rows));
        }
        KeyCode::PageDown if app.workspace == Workspace::Planet => {
            app.scroll_region_panel(ScrollLimits::page(&app.scroll_limits.region_rows));
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
            Some(ContextAction::SurveySubregion(subregion_id)) => {
                commands
                    .send(ConnectionCommand::SurveySubregion(subregion_id))
                    .await
                    .context("connection worker stopped")?;
            }
            Some(action @ ContextAction::ExtractSubregion { .. })
            | Some(action @ ContextAction::EstablishBase(_)) => {
                app.request_confirmation(action);
            }
            _ => {}
        },
        KeyCode::Char('s') if app.workspace == Workspace::Planet => {
            if app.local_view {
                if let Some(ContextAction::SurveySubregion(subregion_id)) =
                    app.subregion_survey_action()
                {
                    commands
                        .send(ConnectionCommand::SurveySubregion(subregion_id))
                        .await
                        .context("connection worker stopped")?;
                }
            } else if let Some(ContextAction::Deploy(region_id)) = app.scout_action() {
                commands
                    .send(ConnectionCommand::DeployScout(region_id))
                    .await
                    .context("connection worker stopped")?;
            }
        }
        KeyCode::Char('x') if app.workspace == Workspace::Planet => {
            if app.local_view {
                if let Some(action @ ContextAction::ExtractSubregion { .. }) =
                    app.subregion_extraction_action()
                {
                    app.request_confirmation(action);
                } else {
                    app.notice = Some("EXTRACTION UNAVAILABLE HERE // SEE SITE OPERATIONS".into());
                }
            } else if let Some(ContextAction::Extract(region_id)) = app.extraction_action() {
                app.request_confirmation(ContextAction::Extract(region_id));
            } else {
                app.notice = Some("EXTRACTION UNAVAILABLE HERE // SEE REGION OPERATIONS".into());
            }
        }
        KeyCode::Char('g') if app.workspace == Workspace::Planet => {
            if let Some(ContextAction::ConfigureExtractor(loadout)) = app.extractor_loadout_action()
            {
                commands
                    .send(ConnectionCommand::ConfigureExtractor(loadout))
                    .await
                    .context("connection worker stopped")?;
            }
        }
        KeyCode::Char('b') if app.workspace == Workspace::Planet => {
            if app.local_view {
                if let Some(action @ ContextAction::EstablishBase(_)) =
                    app.base_establishment_action()
                {
                    app.request_confirmation(action);
                } else {
                    app.notice = Some("FOOTHOLD UNAVAILABLE HERE // SEE SITE OPERATIONS".into());
                }
            } else if let Some(ContextAction::ConstructRelay(region_id)) = app.construction_action()
            {
                app.request_confirmation(ContextAction::ConstructRelay(region_id));
            } else {
                app.notice = Some("RELAY UNAVAILABLE HERE // SEE REGION OPERATIONS".into());
            }
        }
        KeyCode::Char(key @ ('p' | 'c' | 'd' | 'w' | 'l' | 'o'))
            if app.workspace == Workspace::Planet && app.local_view =>
        {
            let module = match key {
                'p' => BaseModuleKind::PowerArray,
                'c' => BaseModuleKind::Communications,
                'd' => BaseModuleKind::Depot,
                'w' => BaseModuleKind::Workshop,
                'l' => BaseModuleKind::Laboratory,
                'o' => BaseModuleKind::Observatory,
                _ => unreachable!(),
            };
            if let Some(
                action @ ContextAction::SetBaseModuleEnabled {
                    enabled: false,
                    module: BaseModuleKind::PowerArray | BaseModuleKind::Communications,
                    ..
                },
            ) = app.base_module_mode_action(module)
            {
                // Suspending power or communications can isolate nodes: confirm first.
                app.request_confirmation(action);
            } else if let Some(ContextAction::SetBaseModuleEnabled {
                subregion_id,
                module,
                enabled,
            }) = app.base_module_mode_action(module)
            {
                commands
                    .send(ConnectionCommand::SetBaseModuleEnabled(
                        subregion_id,
                        module,
                        enabled,
                    ))
                    .await
                    .context("connection worker stopped")?;
            } else if let Some(action) = app.base_module_action(module) {
                app.request_confirmation(action);
            } else {
                app.notice = Some("MODULE UNAVAILABLE HERE // SEE SITE OPERATIONS".into());
            }
        }
        KeyCode::Char('m') if app.workspace == Workspace::Planet && app.local_view => {
            if let Some(action) = app.base_maintenance_action() {
                app.request_confirmation(action);
            } else {
                app.notice = Some("MAINTENANCE UNAVAILABLE HERE // SEE SITE OPERATIONS".into());
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
                _ => {
                    app.notice = Some("ROUTINE UNAVAILABLE HERE // SEE REGION OPERATIONS".into());
                    Ok(())
                }
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
            app.scroll_overview_status(-ScrollLimits::page(&app.scroll_limits.overview_rows));
        }
        KeyCode::PageDown if app.workspace == Workspace::Overview => {
            app.scroll_overview_status(ScrollLimits::page(&app.scroll_limits.overview_rows));
        }
        KeyCode::Left | KeyCode::Up | KeyCode::Char('k') if app.workspace == Workspace::Probes => {
            app.move_probe_selection(-1);
        }
        KeyCode::Right | KeyCode::Down | KeyCode::Char('j')
            if app.workspace == Workspace::Probes =>
        {
            app.move_probe_selection(1);
        }
        KeyCode::Char('g') if app.workspace == Workspace::Probes => {
            app.probe_selection = 1;
            if let Some(ContextAction::ConfigureExtractor(loadout)) = app.extractor_loadout_action()
            {
                commands
                    .send(ConnectionCommand::ConfigureExtractor(loadout))
                    .await
                    .context("connection worker stopped")?;
            } else {
                app.notice = Some("EXTRACTOR RIG LOCKED UNTIL PROBE IS READY".into());
            }
        }
        KeyCode::PageUp if app.workspace == Workspace::Probes => {
            app.scroll_probe_panel(-ScrollLimits::page(&app.scroll_limits.probe_rows));
        }
        KeyCode::PageDown if app.workspace == Workspace::Probes => {
            app.scroll_probe_panel(ScrollLimits::page(&app.scroll_limits.probe_rows));
        }
        KeyCode::Enter if app.workspace == Workspace::Probes => {
            if app.focus_selected_probe_region() {
                app.trigger_transition(now);
            }
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
        KeyCode::Up | KeyCode::Char('k') if app.workspace == Workspace::Archive => {
            app.move_archive_selection(-1);
        }
        KeyCode::Down | KeyCode::Char('j') if app.workspace == Workspace::Archive => {
            app.move_archive_selection(1);
        }
        KeyCode::PageUp if app.workspace == Workspace::Archive => {
            app.scroll_archive_detail(-ScrollLimits::page(&app.scroll_limits.archive_rows));
        }
        KeyCode::PageDown if app.workspace == Workspace::Archive => {
            app.scroll_archive_detail(ScrollLimits::page(&app.scroll_limits.archive_rows));
        }
        KeyCode::Char('v') if app.workspace == Workspace::Archive => {
            app.open_archive_replay(now);
        }
        KeyCode::Enter if app.workspace == Workspace::Archive => {
            if matches!(
                app.archive_experiment_action(),
                Some(ContextAction::StartArchiveExperiment)
            ) {
                commands
                    .send(ConnectionCommand::StartArchiveExperiment)
                    .await
                    .context("connection worker stopped")?;
            } else {
                app.open_archive_replay(now);
            }
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

async fn handle_session_menu(
    app: &mut App,
    key: KeyEvent,
    commands: &mpsc::Sender<ConnectionCommand>,
) -> Result<bool> {
    match app.session_menu {
        Some(SessionMenuState::Selecting) => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.close_session_menu(),
            KeyCode::Up | KeyCode::Char('k') => app.move_session_menu_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => app.move_session_menu_selection(1),
            KeyCode::Enter => match app.selected_session_menu_action() {
                SessionMenuAction::Resume => app.close_session_menu(),
                SessionMenuAction::Detach => return Ok(true),
                SessionMenuAction::StopDaemon => {
                    if matches!(app.connection, ConnectionState::Connected) {
                        app.begin_daemon_stop();
                        commands
                            .send(ConnectionCommand::StopDaemon)
                            .await
                            .context("connection worker stopped")?;
                    } else {
                        app.session_action_unavailable();
                    }
                }
                SessionMenuAction::NewPlaythrough => {
                    if matches!(app.connection, ConnectionState::Connected) {
                        app.confirm_new_playthrough();
                    } else {
                        app.session_action_unavailable();
                    }
                }
            },
            _ => {}
        },
        Some(SessionMenuState::ConfirmingNewPlaythrough) => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.return_to_session_menu(),
            KeyCode::Enter => {
                app.begin_new_playthrough();
                commands
                    .send(ConnectionCommand::StartNewPlaythrough)
                    .await
                    .context("connection worker stopped")?;
            }
            _ => {}
        },
        Some(SessionMenuState::StartingNewPlaythrough | SessionMenuState::StoppingDaemon)
        | None => {}
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[test]
    fn letter_commands_accept_either_case() {
        let upper = normalize_letter_case(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT));
        assert_eq!(upper.code, KeyCode::Char('n'));
        assert!(!upper.modifiers.contains(KeyModifiers::SHIFT));
        let other = normalize_letter_case(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT));
        assert_eq!(other.code, KeyCode::Char('?'));
        assert!(other.modifiers.contains(KeyModifiers::SHIFT));
    }

    #[tokio::test]
    async fn aligned_onboarding_requires_observe_then_awaken() {
        let mut app = App::default();
        app.apply_snapshot(
            mind_core::engine::MindEngine::new(20).status(),
            std::time::Instant::now(),
        );
        app.connected();
        app.band_positions = [4, 4, 4];
        let (commands, mut received) = mpsc::channel(3);

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
    async fn session_menu_exposes_detach_stop_and_confirmed_new_playthrough() {
        let mut app = App::default();
        app.connected();
        let (commands, mut received) = mpsc::channel(4);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert_eq!(app.session_menu, Some(SessionMenuState::Selecting));

        app.move_session_menu_selection(2);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert_eq!(app.session_menu, Some(SessionMenuState::StoppingDaemon));
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::StopDaemon)
        ));

        app.return_to_session_menu();
        app.session_menu_selection = 3;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert_eq!(
            app.session_menu,
            Some(SessionMenuState::ConfirmingNewPlaythrough)
        );
        assert!(received.try_recv().is_err());
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert_eq!(
            app.session_menu,
            Some(SessionMenuState::StartingNewPlaythrough)
        );
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::StartNewPlaythrough)
        ));

        app.return_to_session_menu();
        app.session_menu_selection = 1;
        assert!(handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap());
    }

    #[tokio::test]
    async fn disconnected_session_menu_does_not_issue_lifecycle_actions() {
        let mut app = App::default();
        app.open_session_menu();
        app.session_menu_selection = 2;
        let (commands, mut received) = mpsc::channel(1);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();

        assert_eq!(app.session_menu, Some(SessionMenuState::Selecting));
        assert!(app.session_menu_message.is_some());
        assert!(received.try_recv().is_err());
    }

    #[tokio::test]
    async fn planet_region_keys_send_typed_actions() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([4, 4, 4]);
        engine.acknowledge_archive_presentation("observation_001");
        let mut app = App::default();
        app.apply_snapshot(engine.status(), std::time::Instant::now());
        app.apply_planet(engine.planet().unwrap(), std::time::Instant::now());
        app.connected();
        app.show_return_summary = false;
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
    async fn extractor_rig_key_sends_typed_configuration() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([4, 4, 4]);
        engine.acknowledge_archive_presentation("observation_001");
        let mut app = App::default();
        app.apply_snapshot(engine.status(), std::time::Instant::now());
        app.apply_planet(engine.planet().unwrap(), std::time::Instant::now());
        app.connected();
        app.show_return_summary = false;
        let (commands, mut received) = mpsc::channel(1);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::ConfigureExtractor(
                mind_core::protocol::ExtractorLoadout::Cargo
            ))
        ));
    }

    #[tokio::test]
    async fn archive_enter_starts_only_the_visible_typed_experiment() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([4, 4, 4]);
        engine.acknowledge_archive_presentation("observation_001");
        let mut planet = engine.planet().unwrap();
        planet.archive.experiment = Some(mind_core::protocol::ArchiveExperimentSnapshot {
            available: true,
            active: false,
            running: false,
            remaining_ms: 75_000,
            phase_total_ms: 75_000,
            status: "Recovered timing blocks are ready for a repeatable comparison.".into(),
        });
        let mut app = App::default();
        app.apply_snapshot(engine.status(), std::time::Instant::now());
        app.apply_planet(planet, std::time::Instant::now());
        app.connected();
        app.show_return_summary = false;
        app.workspace = Workspace::Archive;
        let (commands, mut received) = mpsc::channel(1);

        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::StartArchiveExperiment)
        ));
    }

    #[tokio::test]
    async fn overview_enter_returns_to_the_selected_operation_region() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([4, 4, 4]);
        engine.acknowledge_archive_presentation("observation_001");
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
        app.show_return_summary = false;
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
        engine.acknowledge_archive_presentation("observation_001");
        let mut app = App::default();
        app.apply_snapshot(engine.status(), std::time::Instant::now());
        app.apply_planet(engine.planet().unwrap(), std::time::Instant::now());
        app.connected();
        app.show_return_summary = false;
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

    #[tokio::test]
    async fn confirmed_local_operations_send_only_typed_commands() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([4, 4, 4]);
        engine.acknowledge_archive_presentation("observation_001");
        let mut app = App::default();
        app.apply_snapshot(engine.status(), std::time::Instant::now());
        app.apply_planet(engine.planet().unwrap(), std::time::Instant::now());
        app.connected();
        app.show_return_summary = false;
        let (commands, mut received) = mpsc::channel(2);

        app.pending_action = Some(ContextAction::ExtractSubregion {
            region_id: "region_082".into(),
            subregion_id: "region_082_site_2".into(),
        });
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::DeployExtractorAt(region, site))
                if region == "region_082" && site == "region_082_site_2"
        ));

        app.pending_action = Some(ContextAction::MaintainBase("region_082_site_2".into()));
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::MaintainBase(site)) if site == "region_082_site_2"
        ));

        app.pending_action = Some(ContextAction::SetBaseModuleEnabled {
            subregion_id: "region_082_site_2".into(),
            module: BaseModuleKind::Communications,
            enabled: false,
        });
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::SetBaseModuleEnabled(site, BaseModuleKind::Communications, false))
                if site == "region_082_site_2"
        ));
    }

    #[tokio::test]
    async fn transmission_keys_reveal_then_acknowledge_and_lead_focus_jumps_globe() {
        let engine = mind_core::engine::MindEngine::new(20);
        engine.complete_onboarding([3, 3, 3]);
        let mut app = App::default();
        app.apply_snapshot(engine.status(), std::time::Instant::now());
        app.apply_planet(engine.planet().unwrap(), std::time::Instant::now());
        app.connected();
        app.show_return_summary = false;
        assert!(app.active_transmission_record().is_some());

        let (commands, mut received) = mpsc::channel(4);
        // First Enter snaps typewriter reveal to complete without dismissing yet.
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(app.transmission_revealed);
        assert!(received.try_recv().is_err());

        // Second Enter acknowledges the transmission via typed IPC command.
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(matches!(
            received.recv().await,
            Some(ConnectionCommand::AcknowledgeArchivePresentation(id)) if id == "observation_001"
        ));
        assert!(app.active_transmission_record().is_none());

        // 'c' toggles chapter briefing card.
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(app.show_chapter_card);
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert!(!app.show_chapter_card);

        // 'n' jumps globe selection to the current campaign lead region.
        let lead = app
            .planet
            .as_ref()
            .unwrap()
            .campaign
            .lead_region_id
            .clone()
            .unwrap();
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
            &commands,
        )
        .await
        .unwrap();
        assert_eq!(app.selected_region_id.as_deref(), Some(lead.as_str()));
    }
}
