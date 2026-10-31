mod brand;
mod icons;
mod lang;
mod theme;
mod tray;
mod widgets;

use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use icons::Icon;
use lang::Lang;
use localweb_core::{
    CaStatus, Cache, Database, GlobalConfig, NewProject, Paths, Profile, Project, ProjectStatus,
    Request, Response, Status, SupervisorConfig, WebServer, default_supervisor_command,
    detect_profile, sanitize_name, send_request,
};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};
use theme::{
    ACCENT_SOFT, ACCENT_SOFT_PRESSED, ACCENT_TEXT, BANNER_TITLE_SIZE, BODY_SIZE, BORDER,
    BORDER_STRONG, CANVAS, DANGER, DANGER_SOFT, FOCUS, INK, INK_MUTED, LABEL_SIZE, MONO_SIZE, NAV,
    NAV_MUTED, RADIUS_CARD, RADIUS_CONTROL, RAIL_WIDTH, SECTION_TITLE_SIZE, SMALL_SIZE, STATUS_OFF,
    SUCCESS, SUCCESS_SOFT, SURFACE, SURFACE_HOVER, SURFACE_MUTED, WARNING, configure_style,
    medium_family, strong_family,
};
use tray::{Tray, TrayCommand};
use widgets::{
    CalloutKind, Pill, PillKind, callout, card_frame, check_row, empty_card, focus_ring,
    form_label, icon_button, icon_tile, medium, multi_line_field, page_header, rail_item,
    raised_frame, section_heading, service_chip, single_line_field, state_chip,
    state_chip_with_note, status_mark, strong, truncate_middle,
};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// How long an informational toast stays visible before it fades out.
const TOAST_LIFETIME: Duration = Duration::from_secs(4);
/// How long the window may sit idle before the status refreshes again.
const REFRESH_INTERVAL: Duration = Duration::from_secs(3);
/// Duration in seconds of the cross-fade applied when the centered view changes.
const VIEW_FADE_SECONDS: f32 = 0.16;
/// Repaint cadence while the view cross-fade is still running.
const FADE_REPAINT_INTERVAL: Duration = Duration::from_millis(16);
/// Repaint cadence once the interface is fully settled.
const IDLE_REPAINT_INTERVAL: Duration = Duration::from_millis(250);
/// Reply budget for interactive daemon requests such as start or stop.
const DAEMON_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// Reply budget for background refreshes that must not block the UI.
const DAEMON_BACKGROUND_TIMEOUT: Duration = Duration::from_secs(5);

fn main() -> eframe::Result {
    let placement = Paths::discover()
        .ok()
        .and_then(|paths| WindowPlacement::load(&paths.config));
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1_280.0, 800.0])
        .with_min_inner_size([840.0, 560.0])
        .with_icon(brand::icon_data());
    if let Some(placement) = placement {
        viewport = viewport.with_position([placement.x, placement.y]);
        if placement.maximized {
            viewport = viewport.with_maximized(true);
        }
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "LocalWeb",
        options,
        Box::new(move |context| Ok(Box::new(LocalWebApp::new(context, placement.is_none())))),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WindowPlacement {
    x: f32,
    y: f32,
    maximized: bool,
}

impl WindowPlacement {
    fn load(config: &Path) -> Option<Self> {
        let text = fs::read_to_string(config.join("gui-window")).ok()?;
        let mut fields = text.split_whitespace();
        let x: f32 = fields.next()?.parse().ok()?;
        let y: f32 = fields.next()?.parse().ok()?;
        let maximized = fields.next() == Some("1");
        (x.is_finite() && y.is_finite()).then_some(Self { x, y, maximized })
    }

    fn save(self, config: &Path) -> std::io::Result<()> {
        fs::create_dir_all(config)?;
        fs::write(
            config.join("gui-window"),
            format!("{:.1} {:.1} {}\n", self.x, self.y, u8::from(self.maximized)),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Dashboard,
    Projects,
    AddProject,
    EditProject,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Connection {
    Checking,
    Connected,
    Unavailable,
    Stopped,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EnvironmentState {
    Checking,
    Ready,
    Unavailable,
    Stopped,
}

#[derive(Clone, Copy)]
enum DaemonTransition {
    Setup,
    Starting,
    Stopping,
}

#[derive(Clone, Copy)]
enum ProjectTransition {
    Starting,
    Stopping,
    Restarting,
}

#[derive(Clone)]
enum Action {
    RefreshStatus,
    LoadProjects,
    LoadCa,
    Setup,
    InstallCa,
    StartDaemon,
    Shutdown,
    AddProject,
    UpdateProject(String),
    SyncHosts,
    StartProject,
    StopProject,
    RestartProject,
    RemoveProject,
    LoadEnvironment(String),
}

impl Action {
    fn background(&self) -> bool {
        matches!(
            self,
            Self::RefreshStatus | Self::LoadProjects | Self::LoadCa | Self::LoadEnvironment(_)
        )
    }
}

enum GuiEvent {
    Response {
        action: Action,
        result: Box<Result<Response, String>>,
    },
    Command {
        action: Action,
        result: Result<String, String>,
    },
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "independent connection, confirmation and window-state flags"
)]
struct LocalWebApp {
    paths: Option<Paths>,
    sender: Sender<GuiEvent>,
    receiver: Receiver<GuiEvent>,
    view: View,
    lang: Lang,
    daemon_online: bool,
    connection: Connection,
    status: Option<Status>,
    ca: Option<CaStatus>,
    projects: Vec<ProjectStatus>,
    projects_loaded: bool,
    projects_error: Option<String>,
    ca_error: Option<String>,
    mcp_path: Option<PathBuf>,
    project_search: String,
    form: AddProjectForm,
    edit_project: Option<Project>,
    queued_add: Option<NewProject>,
    picker: Option<DirectoryPicker>,
    remove_confirmation: Option<String>,
    stop_confirmation: bool,
    details: Option<String>,
    environment: Option<BTreeMap<String, String>>,
    environment_error: Option<String>,
    notice: Option<Notice>,
    pending: usize,
    background_pending: usize,
    daemon_transition: Option<DaemonTransition>,
    project_transition: Option<(String, ProjectTransition)>,
    last_refresh: Instant,
    last_view: View,
    view_changed_at: Option<Instant>,
    tray: Option<Tray>,
    tray_checked: bool,
    tray_sender: Sender<TrayCommand>,
    tray_receiver: Receiver<TrayCommand>,
    quit_requested: bool,
    center_on_first_frame: bool,
    brand_texture: Option<egui::TextureHandle>,
    logo_texture: Option<egui::TextureHandle>,
}

impl LocalWebApp {
    fn new(context: &eframe::CreationContext<'_>, center_on_first_frame: bool) -> Self {
        configure_style(&context.egui_ctx);
        let (paths, notice) = match Paths::discover() {
            Ok(paths) => (Some(paths), None),
            Err(error) => (None, Some(Notice::error(error.to_string()))),
        };
        let lang = paths
            .as_ref()
            .map_or_else(Lang::default, |paths| Lang::load(&paths.config));
        let mut app = Self::state(paths, lang, notice);
        app.center_on_first_frame = center_on_first_frame;
        app.brand_texture = brand::icon_texture(&context.egui_ctx);
        app.logo_texture = brand::logo_texture(&context.egui_ctx);
        app.refresh_all();
        app
    }

    fn state(paths: Option<Paths>, lang: Lang, notice: Option<Notice>) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (tray_sender, tray_receiver) = mpsc::channel();
        Self {
            paths,
            lang,
            sender,
            receiver,
            view: View::Dashboard,
            daemon_online: false,
            connection: Connection::Checking,
            status: None,
            ca: None,
            projects: Vec::new(),
            projects_loaded: false,
            projects_error: None,
            ca_error: None,
            mcp_path: find_program("localweb-mcp"),
            project_search: String::new(),
            form: AddProjectForm::default(),
            edit_project: None,
            queued_add: None,
            picker: None,
            remove_confirmation: None,
            stop_confirmation: false,
            details: None,
            environment: None,
            environment_error: None,
            notice,
            pending: 0,
            background_pending: 0,
            daemon_transition: None,
            project_transition: None,
            last_refresh: Instant::now(),
            last_view: View::Dashboard,
            view_changed_at: None,
            tray: None,
            tray_checked: false,
            tray_sender,
            tray_receiver,
            quit_requested: false,
            center_on_first_frame: false,
            brand_texture: None,
            logo_texture: None,
        }
    }

    fn refresh_all(&mut self) {
        if self.paths.is_none() {
            return;
        }
        self.request(Action::RefreshStatus, Request::Status);
        self.request(Action::LoadProjects, Request::ProjectList);
        self.request(Action::LoadCa, Request::CaStatus);
        self.last_refresh = Instant::now();
    }

    fn request(&mut self, action: Action, request: Request) {
        let Some(paths) = self.paths.clone() else {
            return;
        };
        if action.background() {
            self.background_pending += 1;
        } else {
            self.pending += 1;
        }
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
                .and_then(|runtime| {
                    runtime.block_on(async {
                        let reply_timeout = if action.background() {
                            DAEMON_BACKGROUND_TIMEOUT
                        } else {
                            DAEMON_REQUEST_TIMEOUT
                        };
                        tokio::time::timeout(reply_timeout, send_request(&paths, request))
                        .await
                        .map_err(|_elapsed| {
                            "The daemon did not reply in time. Refresh the state before retrying."
                                .to_owned()
                        })?
                        .map_err(|error| error.to_string())
                    })
                });
            let _ = sender.send(GuiEvent::Response {
                action,
                result: Box::new(result),
            });
        });
    }

    fn run_command(&mut self, action: Action, program: PathBuf, args: Vec<String>) {
        self.pending += 1;
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = Command::new(program)
                .args(args)
                .output()
                .map_err(|error| error.to_string())
                .and_then(|output| {
                    if output.status.success() {
                        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
                    } else {
                        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
                    }
                });
            let _ = sender.send(GuiEvent::Command { action, result });
        });
    }

    fn receive_events(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            let action = match &event {
                GuiEvent::Response { action, .. } | GuiEvent::Command { action, .. } => action,
            };
            if action.background() {
                self.background_pending = self.background_pending.saturating_sub(1);
            } else {
                self.pending = self.pending.saturating_sub(1);
            }
            match event {
                GuiEvent::Response { action, result } => self.handle_response(action, *result),
                GuiEvent::Command { action, result } => self.handle_command(action, result),
            }
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "dispatch mirrors the daemon response variants"
    )]
    fn handle_response(&mut self, action: Action, result: Result<Response, String>) {
        if matches!(
            &action,
            Action::StartProject | Action::StopProject | Action::RestartProject
        ) && result.is_err()
        {
            self.project_transition = None;
        }
        if matches!(&action, Action::Shutdown) {
            self.daemon_transition = None;
        }
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                match action {
                    Action::RefreshStatus => {
                        if self.connection != Connection::Stopped {
                            self.connection = Connection::Unavailable;
                        }
                        self.daemon_online = false;
                    }
                    Action::LoadProjects => self.projects_error = Some(error),
                    Action::LoadCa => self.ca_error = Some(error),
                    Action::LoadEnvironment(name) if self.details.as_ref() == Some(&name) => {
                        self.environment_error = Some(error);
                    }
                    Action::UpdateProject(_) if self.view == View::EditProject => {
                        self.form.error = Some(error);
                    }
                    _ => self.notice = Some(Notice::error(error)),
                }
                return;
            }
        };
        if let Response::Error { code, message } = response {
            self.project_transition = None;
            let error = format!("{code}: {message}");
            match action {
                Action::LoadProjects => self.projects_error = Some(error),
                Action::LoadCa => self.ca_error = Some(error),
                Action::LoadEnvironment(name) if self.details.as_ref() == Some(&name) => {
                    self.environment_error = Some(error);
                }
                Action::UpdateProject(_) if self.view == View::EditProject => {
                    self.form.error = Some(error);
                }
                _ => self.notice = Some(Notice::error(error)),
            }
            return;
        }
        match (action, response) {
            (Action::RefreshStatus, Response::Status(status)) => {
                self.connection = Connection::Connected;
                self.daemon_online = true;
                self.status = Some(status);
            }
            (Action::LoadProjects, Response::Projects(projects)) => {
                self.projects_loaded = true;
                self.projects_error = None;
                self.projects = projects;
                if self.pending == 0 {
                    self.project_transition = None;
                }
            }
            (Action::LoadCa, Response::Ca(ca)) => {
                self.ca_error = None;
                self.ca = Some(ca);
            }
            (Action::LoadEnvironment(name), Response::Environment(values)) => {
                if self.details.as_ref() == Some(&name) {
                    self.environment = Some(values);
                    self.environment_error = None;
                }
            }
            (Action::Shutdown, Response::Message(_)) => {
                self.connection = Connection::Stopped;
                self.daemon_online = false;
                self.status = None;
                self.stop_confirmation = false;
                self.projects
                    .iter_mut()
                    .for_each(|project| project.running = false);
                self.notice = Some(Notice::success(self.lang.notice_stopped()));
            }
            (Action::AddProject, Response::Project(project)) => {
                self.notice = Some(Notice::success(
                    self.lang.notice_project_added(&project.project.name),
                ));
                self.form = AddProjectForm::default();
                self.view = View::Projects;
                self.request(Action::LoadProjects, Request::ProjectList);
                self.sync_hosts();
            }
            (Action::UpdateProject(name), Response::Project(project)) => {
                debug_assert_eq!(project.project.name, name);
                self.notice = Some(Notice::success(self.lang.notice_project_updated(&name)));
                self.form = AddProjectForm::default();
                self.edit_project = None;
                self.details = Some(name);
                self.view = View::Projects;
                self.request(Action::LoadProjects, Request::ProjectList);
                self.request(Action::RefreshStatus, Request::Status);
            }
            (
                action @ (Action::StartProject | Action::StopProject | Action::RestartProject),
                Response::Message(_),
            ) => {
                let message = match action {
                    Action::StartProject => self.lang.notice_project_started(),
                    Action::StopProject => self.lang.notice_project_stopped(),
                    _ => self.lang.notice_project_restarted(),
                };
                self.notice = Some(Notice::success(message));
                self.request(Action::LoadProjects, Request::ProjectList);
                self.request(Action::RefreshStatus, Request::Status);
            }
            (Action::RemoveProject, Response::Message(_)) => {
                self.notice = Some(Notice::success(self.lang.notice_project_removed()));
                self.remove_confirmation = None;
                self.request(Action::LoadProjects, Request::ProjectList);
                self.sync_hosts();
            }
            (_, unexpected) => {
                self.notice = Some(Notice::error(
                    self.lang.notice_unexpected(&format!("{unexpected:?}")),
                ));
            }
        }
    }

    fn handle_command(&mut self, action: Action, result: Result<String, String>) {
        if matches!(&action, Action::StartDaemon) {
            self.daemon_transition = None;
        }
        if matches!(&action, Action::Setup) && result.is_err() {
            self.daemon_transition = None;
        }
        if matches!(&action, Action::StartDaemon | Action::Setup) && result.is_err() {
            self.queued_add = None;
        }
        match (action, result) {
            (Action::Setup, Ok(_)) => {
                self.notice = Some(Notice::success(self.lang.notice_setup()));
                self.start_daemon();
            }
            (Action::InstallCa, Ok(_)) => {
                self.notice = Some(Notice::success(self.lang.notice_ca()));
                self.request(Action::LoadCa, Request::CaStatus);
            }
            (Action::StartDaemon, Ok(_)) => {
                self.notice = Some(Notice::success(self.lang.notice_started()));
                self.refresh_all();
                if let Some(request) = self.queued_add.take() {
                    self.request(Action::AddProject, Request::ProjectAdd(request));
                }
            }
            (Action::SyncHosts, Ok(_)) => {
                self.notice = Some(Notice::success(self.lang.notice_hosts()));
            }
            (_, Err(error)) => self.notice = Some(Notice::error(error)),
            (_, Ok(message)) => self.notice = Some(Notice::success(message)),
        }
    }

    fn open_add_project(&mut self) {
        if self.edit_project.is_some() {
            self.edit_project = None;
            self.form = AddProjectForm::default();
        }
        self.view = View::AddProject;
    }

    fn edit_project_settings(&mut self, project: &Project) {
        self.form = AddProjectForm::from_project(project);
        self.edit_project = Some(project.clone());
        self.details = None;
        self.environment = None;
        self.environment_error = None;
        self.view = View::EditProject;
    }

    fn leave_project_form(&mut self) {
        if self.edit_project.is_some() {
            self.edit_project = None;
            self.form = AddProjectForm::default();
        }
    }

    fn start_daemon(&mut self) {
        self.daemon_transition = Some(DaemonTransition::Starting);
        match localweb_cli_path() {
            Ok(program) => self.run_command(Action::StartDaemon, program, vec!["start".into()]),
            Err(error) => {
                self.daemon_transition = None;
                self.notice = Some(Notice::error(error));
            }
        }
    }

    fn setup(&mut self) {
        self.daemon_transition = Some(DaemonTransition::Setup);
        match localweb_cli_path() {
            Ok(program) => self.run_command(Action::Setup, program, vec!["setup".into()]),
            Err(error) => {
                self.daemon_transition = None;
                self.notice = Some(Notice::error(error));
            }
        }
    }

    fn install_ca(&mut self) {
        match localweb_cli_path() {
            Ok(program) => self.run_command(
                Action::InstallCa,
                program,
                vec!["ca".into(), "install".into()],
            ),
            Err(error) => self.notice = Some(Notice::error(error)),
        }
    }

    fn sync_hosts(&mut self) {
        match localweb_cli_path() {
            Ok(program) => self.run_command(
                Action::SyncHosts,
                program,
                vec!["hosts".into(), "sync".into()],
            ),
            Err(error) => self.notice = Some(Notice::error(error)),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "sidebar owns the navigation rail and its docked footer"
    )]
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let status_label = if self.connection == Connection::Checking {
            lang.checking_status()
        } else if self.daemon_online {
            lang.nav_running()
        } else if self.connection != Connection::Stopped {
            lang.daemon_unavailable()
        } else {
            lang.nav_stopped()
        };
        egui::Panel::left("navigation")
            .exact_size(RAIL_WIDTH)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(NAV)
                    .inner_margin(egui::Margin::symmetric(16, 20)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    brand::app_icon(ui, self.brand_texture.as_ref(), 30.0);
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("LocalWeb")
                            .size(BANNER_TITLE_SIZE)
                            .family(strong_family())
                            .color(Color32::WHITE),
                    );
                });
                ui.add_space(28.0);
                if rail_item(
                    ui,
                    lang.nav_dashboard(),
                    Icon::Overview,
                    self.view == View::Dashboard,
                )
                .clicked()
                {
                    self.leave_project_form();
                    self.view = View::Dashboard;
                }
                ui.add_space(4.0);
                if rail_item(
                    ui,
                    lang.nav_projects(),
                    Icon::Folder,
                    matches!(
                        self.view,
                        View::Projects | View::AddProject | View::EditProject
                    ),
                )
                .clicked()
                {
                    self.leave_project_form();
                    self.view = View::Projects;
                }
                ui.add_space(4.0);
                if rail_item(
                    ui,
                    lang.nav_settings(),
                    Icon::Settings,
                    self.view == View::Settings,
                )
                .clicked()
                {
                    self.leave_project_form();
                    self.view = View::Settings;
                }
                egui::Panel::bottom("rail_footer")
                    .frame(egui::Frame::NONE)
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.add_space(14.0);
                        let mut next_lang = self.lang;
                        ui.scope(|ui| {
                            ui.visuals_mut().override_text_color = Some(NAV_MUTED);
                            let widgets = &mut ui.style_mut().visuals.widgets;
                            widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                            widgets.inactive.bg_stroke = Stroke::NONE;
                            widgets.inactive.fg_stroke.color = NAV_MUTED;
                            widgets.hovered.weak_bg_fill = theme::NAV_HOVER;
                            widgets.hovered.bg_stroke = Stroke::NONE;
                            widgets.hovered.fg_stroke.color = Color32::WHITE;
                            widgets.active.weak_bg_fill = theme::NAV_HOVER;
                            widgets.active.bg_stroke = Stroke::NONE;
                            widgets.active.fg_stroke.color = Color32::WHITE;
                            widgets.open.weak_bg_fill = theme::NAV_HOVER;
                            widgets.open.bg_stroke = Stroke::NONE;
                            widgets.open.fg_stroke.color = Color32::WHITE;
                            ui.spacing_mut().interact_size.y = 36.0;
                            let mut popup_style = (*ui.ctx().style_of(egui::Theme::Light)).clone();
                            popup_style.visuals.window_fill = theme::SURFACE;
                            popup_style.visuals.panel_fill = theme::SURFACE;
                            popup_style.visuals.window_stroke =
                                Stroke::new(1.0, theme::BORDER_STRONG);
                            popup_style.visuals.widgets.hovered.bg_fill = SURFACE_MUTED;
                            popup_style.visuals.widgets.hovered.weak_bg_fill = SURFACE_MUTED;
                            popup_style.visuals.widgets.hovered.bg_stroke =
                                Stroke::new(1.0, theme::BORDER_STRONG);
                            popup_style.visuals.widgets.hovered.fg_stroke.color = INK;
                            popup_style.visuals.widgets.active.bg_fill = ACCENT_SOFT_PRESSED;
                            popup_style.visuals.widgets.active.weak_bg_fill = ACCENT_SOFT_PRESSED;
                            popup_style.visuals.widgets.active.bg_stroke =
                                Stroke::new(1.0, theme::FOCUS);
                            popup_style.visuals.widgets.active.fg_stroke.color = INK;
                            popup_style.visuals.selection.bg_fill = ACCENT_SOFT;
                            popup_style.visuals.selection.stroke = Stroke::new(1.0, theme::FOCUS);
                            egui::ComboBox::from_id_salt("language_select")
                                .selected_text(self.lang.initials())
                                .width(ui.available_width())
                                .popup_style(popup_style.into())
                                .show_ui(ui, |ui| {
                                    for lang in Lang::ALL {
                                        ui.selectable_value(&mut next_lang, lang, lang.initials());
                                    }
                                });
                        });
                        if next_lang != self.lang {
                            self.lang = next_lang;
                            if let Some(tray) = &self.tray {
                                tray.set_lang(self.lang);
                            }
                            if let Some(paths) = &self.paths {
                                self.lang.save(&paths.config);
                            }
                        }
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            status_mark(
                                ui,
                                if self.daemon_online {
                                    SUCCESS
                                } else {
                                    STATUS_OFF
                                },
                            );
                            ui.label(medium(status_label).size(SMALL_SIZE).color(
                                if self.daemon_online {
                                    Color32::WHITE
                                } else {
                                    NAV_MUTED
                                },
                            ));
                        });
                        ui.add_space(8.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(lang.nav_close_note())
                                    .size(SMALL_SIZE)
                                    .color(NAV_MUTED),
                            )
                            .wrap(),
                        );
                    });
            });
    }

    fn daemon_action(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        if self.daemon_online {
            let stopping = matches!(self.daemon_transition, Some(DaemonTransition::Stopping));
            let response = Pill::new(
                if stopping {
                    lang.op_stopping()
                } else {
                    lang.stop_localweb()
                },
                PillKind::Danger,
            )
            .icon(Icon::Stop)
            .min_width(158.0)
            .height(theme::COMMAND_HEIGHT)
            .enabled(self.pending == 0 && !stopping)
            .show(ui)
            .on_hover_text(lang.stop_hint());
            if response.clicked() {
                if self.projects.iter().any(|project| project.running) {
                    self.stop_confirmation = true;
                } else {
                    self.daemon_transition = Some(DaemonTransition::Stopping);
                    self.request(Action::Shutdown, Request::Shutdown);
                }
            }
        } else {
            let (label, icon) = match self.daemon_transition {
                Some(DaemonTransition::Setup) => (lang.op_setup(), Icon::Settings),
                Some(DaemonTransition::Starting) => (lang.op_starting(), Icon::Play),
                Some(DaemonTransition::Stopping) => (lang.op_stopping(), Icon::Stop),
                None if self.setup_complete() => (lang.start_localweb(), Icon::Play),
                None => (lang.configure_localweb(), Icon::Settings),
            };
            let response = Pill::new(label, PillKind::Primary)
                .icon(icon)
                .min_width(158.0)
                .height(theme::COMMAND_HEIGHT)
                .enabled(
                    self.connection != Connection::Checking
                        && self.pending == 0
                        && self.daemon_transition.is_none(),
                )
                .show(ui);
            if response.clicked() {
                if self.setup_complete() {
                    self.start_daemon();
                } else {
                    self.setup();
                }
            }
        }
    }

    fn page_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        show_add_project: bool,
        show_global: bool,
    ) {
        let lang = self.lang;
        let mut open_add = false;
        if ui.available_width() < 700.0 {
            ui.vertical(|ui| {
                page_header(ui, title);
                if show_add_project || show_global {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if show_add_project {
                            open_add |= Pill::new(
                                lang.nav_add(),
                                if self.daemon_online {
                                    PillKind::Primary
                                } else {
                                    PillKind::Secondary
                                },
                            )
                            .icon(Icon::Plus)
                            .show(ui)
                            .clicked();
                        }
                        if show_global {
                            self.daemon_action(ui);
                        }
                    });
                }
            });
        } else {
            ui.horizontal(|ui| {
                page_header(ui, title);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if show_global {
                        self.daemon_action(ui);
                    }
                    if show_add_project {
                        ui.add_space(8.0);
                        open_add |= Pill::new(
                            lang.nav_add(),
                            if self.daemon_online {
                                PillKind::Primary
                            } else {
                                PillKind::Secondary
                            },
                        )
                        .icon(Icon::Plus)
                        .show(ui)
                        .clicked();
                    }
                });
            });
        }
        if open_add {
            self.open_add_project();
        }
    }

    fn dashboard(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        self.page_toolbar(ui, lang.dash_title(), false, self.daemon_online);
        ui.add_space(22.0);

        self.environment_overview(ui);

        ui.add_space(26.0);
        ui.label(
            strong(lang.nav_projects())
                .size(SECTION_TITLE_SIZE)
                .color(INK),
        );
        ui.add_space(12.0);
        if !self.projects_loaded {
            ui.label(if self.daemon_online {
                lang.loading_projects()
            } else {
                lang.stopped_body()
            });
        } else if self.projects.is_empty() {
            if empty_card(
                ui,
                Icon::Folder,
                lang.empty_title(),
                if self.daemon_online {
                    lang.empty_online()
                } else {
                    lang.empty_offline()
                },
                Some((lang.nav_add(), Icon::Plus)),
            ) {
                self.open_add_project();
            }
        } else {
            let preview = self.projects.iter().take(5).cloned().collect::<Vec<_>>();
            self.project_ledger(ui, &preview);
            if self.projects.len() > 5 {
                ui.add_space(6.0);
                if Pill::new(lang.view_all_projects(), PillKind::Ghost)
                    .show(ui)
                    .clicked()
                {
                    self.view = View::Projects;
                }
            }
        }

        if self.daemon_online
            && let Some(error) = &self.ca_error
        {
            ui.colored_label(DANGER, lang.ca_load_failed());
            ui.collapsing(lang.technical_details(), |ui| {
                ui.label(error);
            });
        }
    }

    fn environment_overview(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let state = match self.connection {
            Connection::Checking => EnvironmentState::Checking,
            Connection::Unavailable => EnvironmentState::Unavailable,
            Connection::Stopped => EnvironmentState::Stopped,
            Connection::Connected => EnvironmentState::Ready,
        };
        let ca_untrusted = self.daemon_online
            && matches!(state, EnvironmentState::Ready)
            && self.ca.as_ref().is_some_and(|ca| !ca.trusted);
        let (mark_icon, mark_bg, mark_text) = if ca_untrusted {
            (Icon::Warning, WARNING, Color32::WHITE)
        } else {
            match state {
                EnvironmentState::Ready => (Icon::Check, SUCCESS, Color32::WHITE),
                EnvironmentState::Unavailable => (Icon::Warning, WARNING, Color32::WHITE),
                EnvironmentState::Checking | EnvironmentState::Stopped => {
                    (Icon::Minus, SURFACE_MUTED, INK_MUTED)
                }
            }
        };
        let ca_installable =
            self.daemon_online && self.ca.as_ref().is_some_and(|ca| ca.exists && !ca.trusted);
        let (title, body) = if ca_untrusted {
            (lang.ca_callout_title(), lang.ca_callout_body())
        } else {
            match state {
                EnvironmentState::Checking => (lang.checking_status(), lang.loading_projects()),
                EnvironmentState::Ready => (lang.env_on(), lang.env_on_body()),
                EnvironmentState::Stopped => (lang.env_off(), lang.env_off_body()),
                EnvironmentState::Unavailable => (lang.daemon_unavailable(), lang.env_off_body()),
            }
        };
        raised_frame()
            .inner_margin(egui::Margin::symmetric(24, 20))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icon_tile(ui, mark_icon, mark_bg, mark_text, 52.0);
                    ui.add_space(4.0);
                    let has_action = !self.daemon_online || ca_installable;
                    let right_width = if has_action { 190.0 } else { 0.0 };
                    let text_width = (ui.available_width() - right_width).max(200.0);
                    ui.allocate_ui(Vec2::new(text_width, 0.0), |ui| {
                        ui.set_min_width(text_width);
                        ui.vertical(|ui| {
                            ui.label(strong(title).size(BANNER_TITLE_SIZE).color(INK));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(body).size(BODY_SIZE).color(INK_MUTED),
                                )
                                .wrap(),
                            );
                        });
                    });
                    if !self.daemon_online {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            self.daemon_action(ui);
                        });
                    } else if ca_installable {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if Pill::new(lang.install_ca(), PillKind::Primary)
                                .icon(Icon::Download)
                                .height(36.0)
                                .show(ui)
                                .clicked()
                            {
                                self.install_ca();
                            }
                        });
                    }
                });
            });
    }

    #[expect(
        clippy::too_many_lines,
        reason = "the view owns the search card, empty state and the responsive ledger"
    )]
    fn projects(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        self.page_toolbar(
            ui,
            lang.projects_title(),
            !(self.projects_loaded && self.projects.is_empty()),
            true,
        );
        ui.add_space(22.0);
        if !self.daemon_online {
            callout(ui, CalloutKind::Warning, Icon::Warning, |ui| {
                ui.vertical(|ui| {
                    ui.label(strong(if self.connection == Connection::Checking {
                        lang.checking_status()
                    } else if self.connection == Connection::Stopped {
                        lang.stopped_title()
                    } else {
                        lang.daemon_unavailable()
                    }));
                    ui.label(RichText::new(lang.stopped_body()).color(INK_MUTED));
                });
            });
            ui.add_space(14.0);
        }
        if !self.projects_loaded {
            if self.daemon_online {
                if let Some(error) = &self.projects_error {
                    ui.colored_label(DANGER, lang.projects_load_failed());
                    ui.collapsing(lang.technical_details(), |ui| {
                        ui.label(error);
                    });
                } else {
                    ui.label(lang.loading_projects());
                }
            }
            return;
        }
        if let Some(error) = &self.projects_error {
            ui.colored_label(DANGER, lang.projects_load_failed());
            ui.collapsing(lang.technical_details(), |ui| {
                ui.label(error);
            });
        }
        let show_search = self.projects.len() > 5 || !self.project_search.is_empty();
        if show_search {
            card_frame()
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    let label = ui.label(
                        RichText::new(lang.search_projects())
                            .size(SMALL_SIZE)
                            .color(INK_MUTED),
                    );
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                        icons::paint(ui.painter(), rect, Icon::Search, INK_MUTED);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.project_search)
                                .frame(egui::Frame::NONE)
                                .desired_width(ui.available_width().min(320.0) - 60.0)
                                .hint_text(lang.search_hint()),
                        )
                        .labelled_by(label.id);
                        if !self.project_search.is_empty()
                            && Pill::new(lang.clear_search(), PillKind::Ghost)
                                .height(36.0)
                                .show(ui)
                                .clicked()
                        {
                            self.project_search.clear();
                        }
                    });
                });
            ui.add_space(14.0);
        }
        if self.projects.is_empty() {
            if empty_card(
                ui,
                Icon::Folder,
                lang.empty_title(),
                if self.daemon_online {
                    lang.empty_online()
                } else {
                    lang.empty_offline()
                },
                Some((lang.nav_add(), Icon::Plus)),
            ) {
                self.open_add_project();
            }
            return;
        }
        let needle = self.project_search.trim().to_lowercase();
        let projects = self
            .projects
            .iter()
            .filter(|item| {
                needle.is_empty()
                    || item.project.name.to_lowercase().contains(&needle)
                    || item.project.hostname.to_lowercase().contains(&needle)
                    || item
                        .project
                        .path
                        .display()
                        .to_string()
                        .to_lowercase()
                        .contains(&needle)
            })
            .cloned()
            .collect::<Vec<_>>();
        if projects.is_empty() {
            ui.label(lang.no_search_results(&self.project_search));
            return;
        }
        if show_search {
            ui.label(
                RichText::new(lang.projects_summary(
                    projects.iter().filter(|item| item.running).count(),
                    projects.len(),
                ))
                .color(INK_MUTED),
            );
            ui.add_space(6.0);
        }
        self.project_ledger(ui, &projects);
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        self.page_toolbar(ui, lang.nav_settings(), false, self.daemon_online);
        ui.add_space(22.0);
        self.settings_mcp(ui);
        ui.add_space(24.0);
        self.settings_environment(ui);
        ui.add_space(24.0);
        self.settings_maintenance(ui);
        ui.add_space(28.0);
        brand::logo(ui, self.logo_texture.as_ref(), 168.0);
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("LocalWeb {}", env!("CARGO_PKG_VERSION")))
                .size(SMALL_SIZE)
                .color(INK_MUTED),
        );
    }

    fn settings_mcp(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        section_heading(ui, lang.settings_mcp_title(), lang.settings_mcp_body());
        ui.add_space(10.0);
        card_frame()
            .inner_margin(egui::Margin::symmetric(20, 18))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.mcp_path.is_some() {
                        icon_tile(ui, Icon::Terminal, ACCENT_SOFT, ACCENT_TEXT, 42.0);
                    } else {
                        icon_tile(ui, Icon::Warning, DANGER_SOFT, DANGER, 42.0);
                    }
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(strong("localweb-mcp").size(BODY_SIZE).color(INK));
                        if let Some(path) = &self.mcp_path {
                            let path = path.display().to_string();
                            ui.add(
                                egui::Label::new(
                                    RichText::new(truncate_middle(&path, 58))
                                        .monospace()
                                        .size(MONO_SIZE)
                                        .color(INK_MUTED),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&path);
                        } else {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(lang.settings_mcp_missing())
                                        .size(SMALL_SIZE)
                                        .color(DANGER),
                                )
                                .wrap(),
                            );
                        }
                    });
                });
                ui.add_space(12.0);
                egui::Frame::NONE
                    .fill(SURFACE_MUTED)
                    .stroke(Stroke::new(1.0, BORDER))
                    .corner_radius(RADIUS_CONTROL)
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        ui.set_min_width((ui.available_width() - 28.0).max(260.0));
                        ui.label(
                            RichText::new(mcp_configuration())
                                .monospace()
                                .size(MONO_SIZE)
                                .color(INK),
                        );
                    });
                ui.add_space(12.0);
                if Pill::new(lang.settings_copy_mcp(), PillKind::Secondary)
                    .icon(Icon::Copy)
                    .height(36.0)
                    .show(ui)
                    .clicked()
                {
                    ui.ctx().copy_text(mcp_configuration().to_owned());
                    self.notice = Some(Notice::success(lang.settings_copied()));
                }
            });
    }

    fn settings_environment(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        section_heading(
            ui,
            lang.settings_environment_title(),
            lang.settings_read_only(),
        );
        ui.add_space(10.0);
        let config = self.global_config();
        let suffix = config.general.domain_suffix.trim_start_matches('.');
        let scheme = if config.general.https {
            "https"
        } else {
            "http"
        };
        let example = if suffix.is_empty() {
            format!("{scheme}://my-project")
        } else {
            format!("{scheme}://my-project.{suffix}")
        };
        let defaults = format!(
            "{} · PHP {} · {} · {}",
            web_label(config.defaults.web_server),
            config.defaults.php,
            lang.database_name(config.defaults.database),
            lang.cache_name(config.defaults.cache)
        );
        card_frame()
            .inner_margin(egui::Margin::symmetric(20, 18))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::Grid::new("settings_environment")
                    .num_columns(2)
                    .spacing([28.0, 12.0])
                    .show(ui, |ui| {
                        settings_label(ui, lang.settings_domain_suffix());
                        ui.label(
                            RichText::new(if suffix.is_empty() {
                                "—".to_owned()
                            } else {
                                format!(".{suffix}")
                            })
                            .size(LABEL_SIZE)
                            .color(INK),
                        );
                        ui.end_row();

                        settings_label(ui, lang.settings_example_url());
                        ui.label(
                            RichText::new(example)
                                .monospace()
                                .size(MONO_SIZE)
                                .color(ACCENT_TEXT),
                        );
                        ui.end_row();

                        settings_label(ui, lang.settings_defaults());
                        ui.label(RichText::new(defaults).size(LABEL_SIZE).color(INK));
                        ui.end_row();

                        settings_label(ui, lang.settings_config_folder());
                        if let Some(paths) = &self.paths {
                            let path = paths.config.display().to_string();
                            ui.add(
                                egui::Label::new(
                                    RichText::new(truncate_middle(&path, 54))
                                        .monospace()
                                        .size(MONO_SIZE)
                                        .color(INK_MUTED),
                                )
                                .truncate(),
                            )
                            .on_hover_text(path);
                        } else {
                            ui.label(RichText::new("—").size(LABEL_SIZE).color(INK_MUTED));
                        }
                        ui.end_row();
                    });
            });
    }

    fn settings_maintenance(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        section_heading(ui, lang.maintenance_title(), lang.maintenance_body());
        ui.add_space(10.0);
        card_frame()
            .inner_margin(egui::Margin::symmetric(20, 18))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    if Pill::new(lang.refresh(), PillKind::Secondary)
                        .icon(Icon::Refresh)
                        .height(36.0)
                        .enabled(self.background_pending == 0 && self.paths.is_some())
                        .show(ui)
                        .clicked()
                    {
                        self.refresh_all();
                    }
                    if Pill::new(lang.sync_domains(), PillKind::Secondary)
                        .icon(Icon::Globe)
                        .height(36.0)
                        .enabled(self.pending == 0)
                        .show(ui)
                        .clicked()
                    {
                        self.sync_hosts();
                    }
                    if Pill::new(lang.repair_setup(), PillKind::Secondary)
                        .icon(Icon::Settings)
                        .height(36.0)
                        .enabled(self.pending == 0)
                        .show(ui)
                        .clicked()
                    {
                        self.setup();
                    }
                });
            });
    }

    #[expect(
        clippy::too_many_lines,
        reason = "wide and stacked ledger share one row implementation"
    )]
    fn project_ledger(&mut self, ui: &mut egui::Ui, items: &[ProjectStatus]) {
        let lang = self.lang;
        if ui.available_width() >= 940.0 {
            card_frame()
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| {
                    ui.set_min_width(900.0);
                    ledger_header(ui, lang);
                    for (index, item) in items.iter().enumerate() {
                        if index > 0 {
                            let (line, _) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 1.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(line, 0, BORDER);
                        }
                        self.project_row_wide(ui, item);
                        if self.details.as_deref() == Some(&item.project.name) {
                            egui::Frame::NONE
                                .inner_margin(egui::Margin {
                                    left: 16,
                                    right: 16,
                                    top: 2,
                                    bottom: 14,
                                })
                                .show(ui, |ui| {
                                    self.project_details(ui, item);
                                });
                        }
                    }
                });
        } else {
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    ui.add_space(10.0);
                }
                let hover_id = ui.id().with(("project-card", &item.project.name));
                let previous: bool = ui.data(|data| data.get_temp(hover_id).unwrap_or(false));
                let tint = ui.ctx().animate_bool_with_time(hover_id, previous, 0.15);
                let fill = SURFACE.lerp_to_gamma(SURFACE_HOVER, tint);
                let transition = self
                    .project_transition
                    .as_ref()
                    .filter(|(name, _)| name == &item.project.name)
                    .map(|(_, transition)| *transition);
                let inner = card_frame()
                    .fill(fill)
                    .inner_margin(egui::Margin::symmetric(18, 16))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(strong(&item.project.name).size(BODY_SIZE).color(INK));
                            if let Some(transition) = transition {
                                transition_status(ui, lang, transition);
                            } else if self.daemon_online {
                                state_chip(
                                    ui,
                                    item.running,
                                    lang.badge_active(),
                                    lang.badge_stopped(),
                                );
                            } else {
                                state_chip_with_note(
                                    ui,
                                    item.running,
                                    lang.badge_active(),
                                    lang.badge_stopped(),
                                    lang.last_known_state(),
                                );
                            }
                        });
                        ui.add_space(2.0);
                        let domain = item.public_url.as_deref().unwrap_or(&item.project.hostname);
                        ui.add(
                            egui::Label::new(
                                medium(truncate_middle(domain, 56))
                                    .size(LABEL_SIZE)
                                    .color(ACCENT_TEXT),
                            )
                            .truncate(),
                        )
                        .on_hover_text(domain);
                        ui.label(
                            RichText::new(format!(
                                "PHP {} · {} · {}",
                                item.project.php,
                                web_label(item.project.web_server),
                                lang.database_name(item.project.database)
                            ))
                            .size(SMALL_SIZE)
                            .color(INK_MUTED),
                        );
                        let path = item.project.path.display().to_string();
                        ui.label(
                            RichText::new(truncate_middle(&path, 56))
                                .size(MONO_SIZE)
                                .color(INK_MUTED)
                                .monospace(),
                        )
                        .on_hover_text(&path);
                        ui.add_space(10.0);
                        self.project_actions(ui, item, false);
                    });
                let hovered = ui.rect_contains_pointer(inner.response.rect);
                ui.data_mut(|data| data.insert_temp(hover_id, hovered));
                if self.details.as_deref() == Some(&item.project.name) {
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: 18,
                            right: 18,
                            top: 0,
                            bottom: 10,
                        })
                        .show(ui, |ui| {
                            self.project_details(ui, item);
                        });
                }
            }
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "wide row keeps its column layout in one place"
    )]
    fn project_row_wide(&mut self, ui: &mut egui::Ui, item: &ProjectStatus) {
        let lang = self.lang;
        let hover_id = ui.id().with(("project-row", &item.project.name));
        let previous: bool = ui.data(|data| data.get_temp(hover_id).unwrap_or(false));
        let tint = ui.ctx().animate_bool_with_time(hover_id, previous, 0.12);
        let fill = SURFACE.lerp_to_gamma(SURFACE_HOVER, tint);
        let cache = if item.project.supervisor.enabled {
            format!(
                "{} · {}",
                lang.cache_name(item.project.cache),
                Lang::supervisor_tag()
            )
        } else {
            lang.cache_name(item.project.cache).to_owned()
        };
        let inner = egui::Frame::NONE
            .fill(fill)
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_min_width(840.0);
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(140.0, 42.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(140.0);
                            ui.label(strong(&item.project.name).color(INK));
                            ui.label(
                                RichText::new(format!(
                                    "PHP {} · {}",
                                    item.project.php,
                                    web_label(item.project.web_server)
                                ))
                                .size(SMALL_SIZE)
                                .color(INK_MUTED),
                            );
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(150.0, 42.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(150.0);
                            let domain =
                                item.public_url.as_deref().unwrap_or(&item.project.hostname);
                            ui.add(
                                egui::Label::new(
                                    medium(truncate_middle(domain, 24))
                                        .size(LABEL_SIZE)
                                        .color(ACCENT_TEXT),
                                )
                                .truncate(),
                            )
                            .on_hover_text(domain);
                            let path = item.project.path.display().to_string();
                            ui.add(
                                egui::Label::new(
                                    RichText::new(truncate_middle(&path, 18))
                                        .size(MONO_SIZE)
                                        .color(INK_MUTED)
                                        .monospace(),
                                )
                                .truncate(),
                            )
                            .on_hover_text(path);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(108.0, 42.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(108.0);
                            ui.label(
                                medium(lang.database_name(item.project.database)).size(LABEL_SIZE),
                            );
                            ui.label(RichText::new(cache).size(SMALL_SIZE).color(INK_MUTED));
                        },
                    );
                    ui.allocate_ui_with_layout(
                        Vec2::new(88.0, 42.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(88.0);
                            let transition = self
                                .project_transition
                                .as_ref()
                                .filter(|(name, _)| name == &item.project.name)
                                .map(|(_, transition)| *transition);
                            if let Some(transition) = transition {
                                transition_status(ui, lang, transition);
                            } else if self.daemon_online {
                                state_chip(
                                    ui,
                                    item.running,
                                    lang.badge_active(),
                                    lang.badge_stopped(),
                                );
                            } else {
                                state_chip(
                                    ui,
                                    item.running,
                                    lang.badge_active(),
                                    lang.badge_stopped(),
                                )
                                .on_hover_text(lang.last_known_state());
                            }
                        },
                    );
                    self.project_actions(ui, item, true);
                });
            });
        let hovered = ui.rect_contains_pointer(inner.response.rect);
        ui.data_mut(|data| data.insert_temp(hover_id, hovered));
    }

    fn project_actions(&mut self, ui: &mut egui::Ui, item: &ProjectStatus, wide: bool) {
        if wide {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.project_crud_actions(ui, item, true);
                self.project_lifecycle_actions(ui, item, true);
            });
        } else {
            ui.horizontal_wrapped(|ui| {
                self.project_lifecycle_actions(ui, item, false);
                self.project_crud_actions(ui, item, false);
            });
        }
    }

    fn project_crud_actions(&mut self, ui: &mut egui::Ui, item: &ProjectStatus, wide: bool) {
        if wide {
            self.project_delete_action(ui, item, true);
            ui.add_space(6.0);
            self.project_edit_action(ui, item, true);
            self.project_details_action(ui, item, true);
        } else {
            self.project_details_action(ui, item, false);
            self.project_edit_action(ui, item, false);
            ui.add_space(6.0);
            self.project_delete_action(ui, item, false);
        }
    }

    fn project_details_action(&mut self, ui: &mut egui::Ui, item: &ProjectStatus, wide: bool) {
        let lang = self.lang;
        let expanded = self.details.as_deref() == Some(&item.project.name);
        let chevron = if expanded {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        };
        let details = if wide {
            Pill::new("", PillKind::Ghost)
                .icon_only(chevron, lang.project_details())
                .height(36.0)
                .show(ui)
                .on_hover_text(lang.project_details())
        } else {
            Pill::new(lang.project_details(), PillKind::Ghost)
                .icon(chevron)
                .height(36.0)
                .show(ui)
        };
        if details.clicked() {
            if expanded {
                self.details = None;
            } else {
                self.details = Some(item.project.name.clone());
                self.environment = None;
                self.environment_error = None;
            }
        }
    }

    fn project_edit_action(&mut self, ui: &mut egui::Ui, item: &ProjectStatus, wide: bool) {
        let lang = self.lang;
        let enabled = self.daemon_online && self.pending == 0;
        let edit = if wide {
            Pill::new("", PillKind::Ghost)
                .icon_only(Icon::Pencil, lang.action_edit())
                .height(36.0)
                .enabled(enabled)
                .show(ui)
                .on_hover_text(lang.action_edit())
        } else {
            Pill::new(lang.action_edit(), PillKind::Secondary)
                .icon(Icon::Pencil)
                .height(36.0)
                .enabled(enabled)
                .show(ui)
        };
        if edit.clicked() {
            self.edit_project_settings(&item.project);
        }
    }

    fn project_delete_action(&mut self, ui: &mut egui::Ui, item: &ProjectStatus, wide: bool) {
        let lang = self.lang;
        let enabled = self.daemon_online && self.pending == 0;
        let delete = if wide {
            Pill::new("", PillKind::GhostDanger)
                .icon_only(Icon::Trash, lang.action_delete())
                .height(36.0)
                .enabled(enabled)
                .show(ui)
                .on_hover_text(lang.action_delete())
        } else {
            Pill::new(lang.action_delete(), PillKind::GhostDanger)
                .icon(Icon::Trash)
                .height(36.0)
                .enabled(enabled)
                .show(ui)
        };
        if delete.clicked() {
            self.remove_confirmation = Some(item.project.name.clone());
        }
    }

    fn project_lifecycle_actions(&mut self, ui: &mut egui::Ui, item: &ProjectStatus, wide: bool) {
        let lang = self.lang;
        if !self.daemon_online {
            return;
        }
        if let Some((name, _)) = &self.project_transition
            && name == &item.project.name
        {
            return;
        }
        if item.running {
            self.project_stop_action(ui, item, wide);
            self.project_restart_action(ui, item, wide);
            self.project_open_action(ui, item, wide);
            self.project_mail_action(ui, item, wide);
        } else {
            let start = Pill::new(lang.action_start(), PillKind::Primary)
                .icon(Icon::Play)
                .height(36.0)
                .min_width(96.0)
                .enabled(self.pending == 0)
                .show(ui);
            if start.clicked() {
                self.project_transition =
                    Some((item.project.name.clone(), ProjectTransition::Starting));
                self.request(
                    Action::StartProject,
                    Request::ProjectStart {
                        name: item.project.name.clone(),
                    },
                );
            }
        }
    }

    fn project_stop_action(&mut self, ui: &mut egui::Ui, item: &ProjectStatus, wide: bool) {
        let lang = self.lang;
        let stop = if wide {
            Pill::new("", PillKind::Secondary)
                .icon_only(Icon::Stop, lang.action_stop())
                .height(36.0)
                .enabled(self.pending == 0)
                .show(ui)
                .on_hover_text(lang.action_stop())
        } else {
            Pill::new(lang.action_stop(), PillKind::Secondary)
                .icon(Icon::Stop)
                .height(36.0)
                .enabled(self.pending == 0)
                .show(ui)
        };
        if stop.clicked() {
            self.project_transition =
                Some((item.project.name.clone(), ProjectTransition::Stopping));
            self.request(
                Action::StopProject,
                Request::ProjectStop {
                    name: item.project.name.clone(),
                },
            );
        }
    }

    fn project_restart_action(&mut self, ui: &mut egui::Ui, item: &ProjectStatus, wide: bool) {
        let lang = self.lang;
        let restart = if wide {
            Pill::new("", PillKind::Secondary)
                .icon_only(Icon::Refresh, lang.action_restart())
                .height(36.0)
                .enabled(self.pending == 0)
                .show(ui)
                .on_hover_text(lang.action_restart())
        } else {
            Pill::new(lang.action_restart(), PillKind::Secondary)
                .icon(Icon::Refresh)
                .height(36.0)
                .enabled(self.pending == 0)
                .show(ui)
        };
        if restart.clicked() {
            self.project_transition =
                Some((item.project.name.clone(), ProjectTransition::Restarting));
            self.request(
                Action::RestartProject,
                Request::ProjectRestart {
                    name: item.project.name.clone(),
                },
            );
        }
    }

    fn project_open_action(&mut self, ui: &mut egui::Ui, item: &ProjectStatus, wide: bool) {
        let lang = self.lang;
        let open = if wide {
            Pill::new("", PillKind::Ghost)
                .icon_only(Icon::External, lang.action_open())
                .height(36.0)
                .show(ui)
                .on_hover_text(lang.action_open())
        } else {
            Pill::new(lang.action_open(), PillKind::Secondary)
                .icon(Icon::External)
                .height(36.0)
                .show(ui)
        };
        if open.clicked() {
            open_browser(
                &item.project.hostname,
                item.project.web_port,
                item.public_url.as_deref(),
                lang,
                &mut self.notice,
            );
        }
    }

    fn project_mail_action(&mut self, ui: &mut egui::Ui, item: &ProjectStatus, wide: bool) {
        let Some(url) = item.mail_web_url.as_deref() else {
            return;
        };
        let mail = if wide {
            Pill::new("", PillKind::Ghost)
                .icon_only(Icon::Mail, "Mailpit")
                .height(36.0)
                .show(ui)
                .on_hover_text("Mailpit")
        } else {
            Pill::new("Mailpit", PillKind::Secondary)
                .icon(Icon::Mail)
                .height(36.0)
                .show(ui)
        };
        if mail.clicked() {
            open_url(url, self.lang, &mut self.notice);
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "metadata, environment disclosure and the destructive action share one panel"
    )]
    fn project_details(&mut self, ui: &mut egui::Ui, item: &ProjectStatus) {
        let lang = self.lang;
        let project = &item.project;
        ui.label(
            RichText::new(format!(
                "{} · {}",
                lang.profile_name(project.profile),
                project.document_root.display()
            ))
            .size(SMALL_SIZE)
            .color(INK_MUTED),
        );
        ui.label(
            RichText::new(format!(
                "Node.js {} · Composer {}",
                project.node, project.composer
            ))
            .size(SMALL_SIZE)
            .color(INK_MUTED),
        );
        ui.label(
            RichText::new(lang.project_options(
                project.supervisor.enabled,
                project.cron.len(),
                project.mail,
                project.autostart,
            ))
            .size(SMALL_SIZE)
            .color(INK_MUTED),
        );
        ui.label(
            RichText::new(format!(
                "HTTP 127.0.0.1:{} · DB 127.0.0.1:{}",
                project
                    .web_port
                    .map_or_else(|| "-".to_owned(), |port| port.to_string()),
                project
                    .database_port
                    .map_or_else(|| "-".to_owned(), |port| port.to_string()),
            ))
            .size(SMALL_SIZE)
            .color(INK_MUTED),
        );
        ui.add_space(8.0);
        if self.environment.is_none() && self.environment_error.is_none() {
            if Pill::new(lang.reveal_environment(), PillKind::Secondary)
                .height(36.0)
                .enabled(self.daemon_online && self.background_pending == 0)
                .show(ui)
                .clicked()
            {
                self.request(
                    Action::LoadEnvironment(project.name.clone()),
                    Request::ProjectEnvironment {
                        name: project.name.clone(),
                    },
                );
            }
        } else if let Some(values) = &self.environment {
            ui.label(
                RichText::new(lang.environment_help())
                    .size(SMALL_SIZE)
                    .color(INK_MUTED),
            );
            let mut dotenv = String::new();
            for (key, value) in values {
                let _ = writeln!(dotenv, "{key}={value}");
            }
            if Pill::new(lang.copy_environment(), PillKind::Secondary)
                .icon(Icon::Copy)
                .height(36.0)
                .show(ui)
                .clicked()
            {
                ui.ctx().copy_text(dotenv.clone());
            }
            ui.add_space(6.0);
            egui::Frame::NONE
                .fill(SURFACE_MUTED)
                .corner_radius(RADIUS_CONTROL)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_min_width((ui.available_width() - 24.0).max(240.0));
                    ui.add(
                        egui::TextEdit::multiline(&mut dotenv)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(values.len().min(12))
                            .interactive(false)
                            .frame(egui::Frame::NONE),
                    );
                });
        } else if let Some(error) = &self.environment_error {
            ui.label(
                RichText::new(lang.environment_failed())
                    .size(SMALL_SIZE)
                    .color(DANGER),
            );
            ui.collapsing(lang.technical_details(), |ui| {
                ui.label(error);
            });
            if Pill::new(lang.refresh(), PillKind::Secondary)
                .height(36.0)
                .show(ui)
                .clicked()
            {
                self.environment_error = None;
            }
        }
        ui.add_space(6.0);
    }

    #[expect(
        clippy::too_many_lines,
        reason = "adding and editing share one validated submission path"
    )]
    fn project_form(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let editing = self.edit_project.is_some();
        self.page_toolbar(
            ui,
            if editing {
                lang.edit_title()
            } else {
                lang.add_title()
            },
            false,
            true,
        );
        ui.add_space(22.0);
        if !self.daemon_online {
            callout(ui, CalloutKind::Warning, Icon::Warning, |ui| {
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(strong(if self.connection == Connection::Checking {
                            lang.checking_status()
                        } else if matches!(self.daemon_transition, Some(DaemonTransition::Starting))
                        {
                            lang.op_starting()
                        } else if matches!(self.daemon_transition, Some(DaemonTransition::Setup)) {
                            lang.op_setup()
                        } else if self.connection == Connection::Stopped {
                            lang.stopped_title()
                        } else {
                            lang.daemon_unavailable()
                        }))
                        .wrap(),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(lang.stopped_add_body()).color(INK_MUTED))
                            .wrap(),
                    );
                });
            });
            if editing {
                return;
            }
            ui.add_space(14.0);
        }
        let mut submit = false;
        self.project_source_form(ui);
        ui.add_space(14.0);
        self.project_services_form(ui);
        ui.add_space(16.0);
        if editing {
            ui.add(
                egui::Label::new(
                    RichText::new(lang.edit_restart_note())
                        .size(SMALL_SIZE)
                        .color(INK_MUTED),
                )
                .wrap(),
            );
            ui.add_space(8.0);
        }
        ui.horizontal(|ui| {
            if Pill::new(lang.cancel(), PillKind::Ghost)
                .height(theme::COMMAND_HEIGHT)
                .show(ui)
                .clicked()
            {
                self.form = AddProjectForm::default();
                self.edit_project = None;
                self.view = View::Projects;
            }
            submit = Pill::new(
                if editing {
                    lang.save_changes()
                } else {
                    lang.create_project()
                },
                PillKind::Primary,
            )
            .icon(Icon::Check)
            .height(theme::COMMAND_HEIGHT)
            .min_width(150.0)
            .enabled(self.pending == 0)
            .show(ui)
            .clicked();
            if self.pending > 0 {
                ui.spinner();
                ui.label(RichText::new(lang.operation_busy()).color(INK_MUTED));
            }
        });
        if submit {
            match self.form.to_request(lang) {
                Ok(request) => {
                    self.form.error = None;
                    if let Some(project) = self.edit_project.clone() {
                        self.request(
                            Action::UpdateProject(project.name.clone()),
                            Request::ProjectUpdate {
                                name: project.name,
                                input: request,
                            },
                        );
                    } else if self.daemon_online {
                        self.request(Action::AddProject, Request::ProjectAdd(request));
                    } else {
                        self.queued_add = Some(request);
                        if self.daemon_transition.is_none() {
                            if self.setup_complete() {
                                self.start_daemon();
                            } else {
                                self.setup();
                            }
                        }
                    }
                }
                Err(error) => self.form.error = Some(error),
            }
        }
        if let Some(error) = &self.form.error {
            let anchored = error == lang.error_no_path() || error == lang.error_no_executable();
            if !anchored {
                ui.add_space(10.0);
                callout(ui, CalloutKind::Danger, Icon::Warning, |ui| {
                    ui.label(strong(format!("{}: {error}", lang.check_form())).color(DANGER));
                });
            }
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "locked identity rows and editable source settings share one grid"
    )]
    fn project_source_form(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let editing = self.edit_project.is_some();
        let card_width = (ui.available_width() - 40.0).max(300.0);
        card_frame().show(ui, |ui| {
            ui.set_min_width(card_width);
            let field_width = (ui.available_width() - 290.0).clamp(180.0, 380.0);
            section_heading(ui, lang.source_title(), lang.source_body());
            if editing {
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(lang.edit_identity_note())
                            .size(SMALL_SIZE)
                            .color(INK_MUTED),
                    )
                    .wrap(),
                );
            }
            ui.add_space(12.0);
            egui::Grid::new("add_project_form")
                .num_columns(2)
                .spacing([22.0, 13.0])
                .min_col_width(138.0)
                .show(ui, |ui| {
                    if editing {
                        let label = form_label(ui, lang.path_label());
                        let path = self.form.path.clone();
                        ui.add(
                            egui::Label::new(
                                RichText::new(truncate_middle(&path, 58))
                                    .size(MONO_SIZE)
                                    .color(INK_MUTED)
                                    .monospace(),
                            )
                            .truncate(),
                        )
                        .labelled_by(label.id)
                        .on_hover_text(path);
                        ui.end_row();

                        let label = form_label(ui, lang.name_label());
                        ui.add(egui::Label::new(
                            medium(&self.form.name).size(LABEL_SIZE).color(INK),
                        ))
                        .labelled_by(label.id);
                        ui.end_row();

                        let label = form_label(ui, lang.host_label());
                        ui.add(egui::Label::new(
                            medium(&self.form.hostname)
                                .size(LABEL_SIZE)
                                .color(ACCENT_TEXT),
                        ))
                        .labelled_by(label.id);
                        ui.end_row();
                    } else {
                        let label = form_label(ui, lang.path_label());
                        ui.horizontal(|ui| {
                            let response = single_line_field(
                                ui,
                                &mut self.form.path,
                                field_width,
                                "~/Code/acme",
                            )
                            .labelled_by(label.id);
                            if response.lost_focus() {
                                let path = self.form.source_path();
                                if path.is_dir() {
                                    self.form.detect_path(&path, &self.domain_suffix());
                                }
                            }
                            if response.changed() {
                                self.form.error = None;
                            }
                            if Pill::new(lang.browse(), PillKind::Secondary)
                                .height(36.0)
                                .show(ui)
                                .clicked()
                            {
                                self.picker = Some(DirectoryPicker::new(&self.form.path));
                            }
                        });
                        ui.end_row();
                        if self.form.error.as_deref() == Some(lang.error_no_path()) {
                            ui.label("");
                            ui.label(
                                RichText::new(lang.error_no_path())
                                    .size(SMALL_SIZE)
                                    .color(DANGER),
                            );
                            ui.end_row();
                        }

                        let label = form_label(ui, lang.name_label());
                        single_line_field(ui, &mut self.form.name, field_width, "")
                            .labelled_by(label.id);
                        ui.end_row();

                        let label = form_label(ui, lang.host_label());
                        single_line_field(ui, &mut self.form.hostname, field_width, "")
                            .labelled_by(label.id);
                        ui.end_row();
                    }

                    let label = form_label(ui, lang.docroot_label());
                    single_line_field(ui, &mut self.form.document_root, field_width, "")
                        .labelled_by(label.id);
                    ui.end_row();

                    let label = form_label(ui, lang.profile_label());
                    egui::ComboBox::from_id_salt("profile")
                        .width(field_width)
                        .selected_text(lang.profile_name(self.form.profile))
                        .show_ui(ui, |ui| {
                            for profile in [
                                Profile::GenericPhp,
                                Profile::Wordpress,
                                Profile::Laravel,
                                Profile::Symfony,
                                Profile::Drupal,
                                Profile::ProcessWire,
                            ] {
                                ui.selectable_value(
                                    &mut self.form.profile,
                                    profile,
                                    lang.profile_name(profile),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id);
                    ui.end_row();
                });
        });
    }

    fn project_services_form(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let card_width = (ui.available_width() - 40.0).max(300.0);
        card_frame().show(ui, |ui| {
            ui.set_min_width(card_width);
            let field_width = (ui.available_width() - 290.0).clamp(180.0, 380.0);
            section_heading(ui, lang.services_title(), lang.services_body());
            ui.add_space(12.0);
            egui::Grid::new("add_project_services")
                .num_columns(2)
                .spacing([22.0, 13.0])
                .min_col_width(138.0)
                .show(ui, |ui| {
                    self.basic_service_fields(ui, field_width);
                    self.supervisor_fields(ui, field_width);
                    self.cron_fields(ui, field_width);

                    let label = form_label(ui, lang.autostart_label());
                    check_row(ui, &mut self.form.autostart, lang.autostart_check())
                        .labelled_by(label.id);
                    ui.end_row();
                });
        });
    }

    fn basic_service_fields(&mut self, ui: &mut egui::Ui, field_width: f32) {
        let lang = self.lang;
        let label = form_label(ui, lang.web_server_label());
        egui::ComboBox::from_id_salt("web_server")
            .width(field_width)
            .selected_text(web_label(self.form.web_server))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.form.web_server, WebServer::Nginx, "nginx");
                ui.selectable_value(&mut self.form.web_server, WebServer::Apache, "Apache");
            })
            .response
            .labelled_by(label.id);
        ui.end_row();

        let label = form_label(ui, "PHP");
        egui::ComboBox::from_id_salt("php_version")
            .width(field_width)
            .selected_text(format!("PHP {}", self.form.php))
            .show_ui(ui, |ui| {
                for version in localweb_core::PHP_VERSIONS {
                    let bundled = localweb_core::runtime::RuntimeLayout::discover()
                        .php(version)
                        .is_ok();
                    ui.selectable_value(
                        &mut self.form.php,
                        version.into(),
                        format!(
                            "PHP {version} · {}",
                            lang.php_version_availability(version, bundled)
                        ),
                    );
                }
            })
            .response
            .labelled_by(label.id);
        ui.end_row();

        let label = form_label(ui, "Node.js");
        egui::ComboBox::from_id_salt("node_version")
            .width(field_width)
            .selected_text(format!("Node.js {}", self.form.node))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.form.node, "24".into(), "Node.js 24 LTS");
                ui.selectable_value(&mut self.form.node, "22".into(), "Node.js 22 LTS");
                ui.selectable_value(&mut self.form.node, "20".into(), "Node.js 20 (EOL)");
            })
            .response
            .labelled_by(label.id);
        ui.end_row();

        let label = form_label(ui, lang.database_label());
        egui::ComboBox::from_id_salt("database")
            .width(field_width)
            .selected_text(lang.database_name(self.form.database))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.form.database, Database::None, lang.db_none());
                ui.selectable_value(&mut self.form.database, Database::Mariadb, "MariaDB");
                ui.selectable_value(&mut self.form.database, Database::Mysql, "MySQL");
                ui.selectable_value(&mut self.form.database, Database::Postgres, "PostgreSQL");
                ui.selectable_value(&mut self.form.database, Database::Sqlite, "SQLite");
            })
            .response
            .labelled_by(label.id);
        ui.end_row();

        let label = form_label(ui, lang.cache_label());
        egui::ComboBox::from_id_salt("cache")
            .width(field_width)
            .selected_text(lang.cache_name(self.form.cache))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.form.cache, Cache::None, lang.cache_none());
                ui.selectable_value(&mut self.form.cache, Cache::Redis, "Redis");
            })
            .response
            .labelled_by(label.id);
        ui.end_row();

        let _ = form_label(ui, lang.mail_title());
        let _ = check_row(ui, &mut self.form.mail, lang.mail_check());
        ui.end_row();
    }

    fn supervisor_fields(&mut self, ui: &mut egui::Ui, field_width: f32) {
        let lang = self.lang;
        let _ = form_label(ui, lang.supervisor_title());
        let _ = check_row(ui, &mut self.form.supervisor, lang.supervisor_check());
        ui.end_row();
        if !self.form.supervisor {
            return;
        }

        if self.form.has_worker_preset() {
            let _ = form_label(ui, lang.supervisor_preset_label());
            ui.vertical(|ui| {
                service_chip(
                    ui,
                    lang.supervisor_title(),
                    if self.form.profile == Profile::Laravel {
                        "Laravel queue:work"
                    } else {
                        "Symfony messenger:consume async"
                    },
                    ACCENT_TEXT,
                );
                let _ = check_row(
                    ui,
                    &mut self.form.supervisor_custom,
                    lang.supervisor_custom(),
                );
            });
            ui.end_row();
        }

        if !self.form.has_worker_preset() || self.form.supervisor_custom {
            self.custom_supervisor_fields(ui, field_width);
        }
    }

    fn custom_supervisor_fields(&mut self, ui: &mut egui::Ui, field_width: f32) {
        let lang = self.lang;
        let label = form_label(ui, lang.supervisor_executable());
        let response = single_line_field(
            ui,
            &mut self.form.supervisor_command,
            field_width,
            lang.supervisor_executable_hint(),
        )
        .labelled_by(label.id);
        if response.changed() {
            self.form.error = None;
        }
        ui.end_row();
        if self.form.error.as_deref() == Some(lang.error_no_executable()) {
            ui.label("");
            ui.label(
                RichText::new(lang.error_no_executable())
                    .size(SMALL_SIZE)
                    .color(DANGER),
            );
            ui.end_row();
        }

        let label = form_label(ui, lang.supervisor_args());
        ui.vertical(|ui| {
            multi_line_field(
                ui,
                &mut self.form.supervisor_args,
                field_width,
                4,
                lang.supervisor_args_hint(),
            )
            .labelled_by(label.id);
            ui.label(
                RichText::new(lang.supervisor_args_note())
                    .size(SMALL_SIZE)
                    .color(INK_MUTED),
            );
        });
        ui.end_row();
    }

    fn cron_fields(&mut self, ui: &mut egui::Ui, field_width: f32) {
        let lang = self.lang;
        let _ = form_label(ui, lang.cron_title());
        let _ = check_row(ui, &mut self.form.cron, lang.cron_check());
        ui.end_row();
        if !self.form.cron {
            return;
        }
        let label = form_label(ui, lang.cron_lines());
        ui.vertical(|ui| {
            multi_line_field(
                ui,
                &mut self.form.cron_lines,
                field_width,
                3,
                "*/5 * * * * @php artisan schedule:run",
            )
            .labelled_by(label.id);
            ui.add(
                egui::Label::new(
                    RichText::new(lang.cron_note())
                        .size(SMALL_SIZE)
                        .color(INK_MUTED),
                )
                .wrap(),
            );
        });
        ui.end_row();
    }

    fn show_notice(&mut self, context: &egui::Context) {
        let Some(notice) = self.notice.take() else {
            return;
        };
        if !notice.error && notice.created.elapsed() > TOAST_LIFETIME {
            return;
        }
        let mut dismissed = false;
        egui::Area::new(egui::Id::new("notice_toast"))
            .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-18.0, 18.0))
            .order(egui::Order::Foreground)
            .show(context, |ui| {
                dismissed = notice.toast(ui, self.lang);
            });
        if !dismissed {
            self.notice = Some(notice);
        }
    }

    fn remove_dialog(&mut self, context: &egui::Context) {
        let lang = self.lang;
        let Some(name) = self.remove_confirmation.clone() else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::new("remove_project")).show(context, |ui| {
            ui.label(strong(lang.delete_title()).size(SECTION_TITLE_SIZE));
            ui.add_space(4.0);
            ui.label(RichText::new(lang.delete_body(&name)).color(INK_MUTED));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if Pill::new(lang.cancel(), PillKind::Ghost)
                    .height(36.0)
                    .show(ui)
                    .clicked()
                {
                    self.remove_confirmation = None;
                }
                if Pill::new(lang.delete_confirm(), PillKind::Danger)
                    .icon(Icon::Trash)
                    .height(36.0)
                    .enabled(self.pending == 0)
                    .show(ui)
                    .clicked()
                {
                    self.request(
                        Action::RemoveProject,
                        Request::ProjectRemove { name: name.clone() },
                    );
                }
            });
        });
        if modal.should_close() && self.pending == 0 {
            self.remove_confirmation = None;
        }
    }

    fn stop_dialog(&mut self, context: &egui::Context) {
        if !self.stop_confirmation {
            return;
        }
        let lang = self.lang;
        let modal = egui::Modal::new(egui::Id::new("stop_localweb")).show(context, |ui| {
            ui.label(strong(lang.stop_localweb()).size(SECTION_TITLE_SIZE));
            ui.add_space(4.0);
            ui.label(RichText::new(lang.stop_hint()).color(INK_MUTED));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if Pill::new(lang.cancel(), PillKind::Ghost)
                    .height(36.0)
                    .show(ui)
                    .clicked()
                {
                    self.stop_confirmation = false;
                }
                if Pill::new(lang.stop_localweb(), PillKind::Danger)
                    .icon(Icon::Stop)
                    .height(36.0)
                    .enabled(self.pending == 0)
                    .show(ui)
                    .clicked()
                {
                    self.stop_confirmation = false;
                    self.daemon_transition = Some(DaemonTransition::Stopping);
                    self.request(Action::Shutdown, Request::Shutdown);
                }
            });
        });
        if modal.should_close() && self.pending == 0 {
            self.stop_confirmation = false;
        }
    }

    fn setup_complete(&self) -> bool {
        self.paths
            .as_ref()
            .is_some_and(|paths| paths.config.join("config.toml").is_file())
    }

    fn domain_suffix(&self) -> String {
        self.global_config().general.domain_suffix
    }

    fn global_config(&self) -> GlobalConfig {
        self.paths
            .as_ref()
            .filter(|paths| paths.config.join("config.toml").is_file())
            .and_then(|paths| GlobalConfig::load_or_create(paths).ok())
            .unwrap_or_default()
    }

    fn save_window_placement(&self, ctx: &egui::Context) {
        let Some(paths) = &self.paths else {
            return;
        };
        let (rect, minimized, maximized) = ctx.input(|input| {
            let viewport = input.viewport();
            (viewport.outer_rect, viewport.minimized, viewport.maximized)
        });
        if minimized == Some(true) {
            return;
        }
        let Some(rect) = rect else {
            return;
        };
        let placement = WindowPlacement {
            x: rect.min.x,
            y: rect.min.y,
            maximized: maximized.unwrap_or(false),
        };
        if let Err(error) = placement.save(&paths.config) {
            eprintln!("could not save the LocalWeb window position: {error}");
        }
    }
}

impl eframe::App for LocalWebApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.center_on_first_frame
            && let Some(command) = egui::ViewportCommand::center_on_screen(ctx)
        {
            ctx.send_viewport_cmd(command);
            self.center_on_first_frame = false;
        }
        if !self.tray_checked {
            self.tray_checked = true;
            self.tray = Tray::new(self.lang, ctx.clone(), self.tray_sender.clone());
        }
        while let Ok(command) = self.tray_receiver.try_recv() {
            match command {
                TrayCommand::Open => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayCommand::Quit => {
                    self.quit_requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if close_requested {
            self.save_window_placement(ctx);
        }
        if self.tray.is_some() && !self.quit_requested && close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.receive_events();
        if self.pending == 0
            && self.background_pending == 0
            && self.last_refresh.elapsed() > REFRESH_INTERVAL
        {
            if self.daemon_online {
                self.request(Action::RefreshStatus, Request::Status);
                self.request(Action::LoadProjects, Request::ProjectList);
            } else {
                self.request(Action::RefreshStatus, Request::Status);
            }
            self.last_refresh = Instant::now();
        }
        if self.last_view != self.view {
            self.last_view = self.view;
            self.view_changed_at = Some(Instant::now());
        }
        let fade = self.view_changed_at.map_or(1.0, |changed| {
            (changed.elapsed().as_secs_f32() / VIEW_FADE_SECONDS).clamp(0.0, 1.0)
        });
        self.sidebar(ui);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(CANVAS)
                    .inner_margin(egui::Margin::symmetric(30, 26)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.scope(|ui| {
                            ui.multiply_opacity(fade);
                            match self.view {
                                View::Dashboard => self.dashboard(ui),
                                View::Projects => self.projects(ui),
                                View::AddProject | View::EditProject => self.project_form(ui),
                                View::Settings => self.settings(ui),
                            }
                        });
                        ui.add_space(24.0);
                    });
            });
        self.show_notice(&context);
        let suffix = self.domain_suffix();
        if let Some(picker) = self.picker.as_mut() {
            if let Some(path) = picker.show(&context, self.lang) {
                self.form.detect_path(&path, &suffix);
            }
            if !picker.open {
                self.picker = None;
            }
        }
        self.remove_dialog(&context);
        self.stop_dialog(&context);
        if fade < 1.0 {
            context.request_repaint_after(FADE_REPAINT_INTERVAL);
        } else {
            context.request_repaint_after(IDLE_REPAINT_INTERVAL);
        }
    }
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "form mirrors independent checkboxes"
)]
struct AddProjectForm {
    error: Option<String>,
    detected_path: Option<PathBuf>,
    detected_suffix: String,
    path: String,
    name: String,
    hostname: String,
    document_root: String,
    profile: Profile,
    php: String,
    database: Database,
    cache: Cache,
    supervisor: bool,
    supervisor_custom: bool,
    supervisor_command: String,
    supervisor_args: String,
    cron: bool,
    cron_lines: String,
    mail: bool,
    web_server: WebServer,
    node: String,
    autostart: bool,
}

impl Default for AddProjectForm {
    fn default() -> Self {
        Self {
            error: None,
            detected_path: None,
            detected_suffix: "dev".into(),
            path: String::new(),
            name: String::new(),
            hostname: String::new(),
            document_root: ".".into(),
            profile: Profile::GenericPhp,
            php: "8.5".into(),
            database: Database::None,
            cache: Cache::None,
            supervisor: false,
            supervisor_custom: false,
            supervisor_command: String::new(),
            supervisor_args: String::new(),
            cron: false,
            cron_lines: String::new(),
            mail: false,
            web_server: WebServer::Nginx,
            node: "24".into(),
            autostart: false,
        }
    }
}

impl AddProjectForm {
    fn from_project(project: &Project) -> Self {
        let preset = default_supervisor_command(project.profile);
        let supervisor_custom =
            project.supervisor.command != preset && !project.supervisor.command.is_empty();
        let (supervisor_command, supervisor_args) = if supervisor_custom {
            let mut command = project.supervisor.command.iter();
            (
                command.next().cloned().unwrap_or_default(),
                command.cloned().collect::<Vec<_>>().join("\n"),
            )
        } else {
            (String::new(), String::new())
        };
        let cron_entries = project
            .cron
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| {
                std::iter::once(entry.schedule.as_str())
                    .chain(entry.command.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>();
        Self {
            error: None,
            detected_path: None,
            detected_suffix: "dev".into(),
            path: project.path.display().to_string(),
            name: project.name.clone(),
            hostname: project.hostname.clone(),
            document_root: project.document_root.display().to_string(),
            profile: project.profile,
            php: project.php.clone(),
            database: project.database,
            cache: project.cache,
            supervisor: project.supervisor.enabled,
            supervisor_custom,
            supervisor_command,
            supervisor_args,
            cron: !cron_entries.is_empty(),
            cron_lines: cron_entries.join("\n"),
            mail: project.mail,
            web_server: project.web_server,
            node: project.node.clone(),
            autostart: project.autostart,
        }
    }

    fn detect_path(&mut self, path: &Path, suffix: &str) {
        self.path = path.display().to_string();
        if self.detected_path.as_deref() == Some(path) {
            return;
        }
        let old_name = self
            .detected_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(sanitize_name)
            .unwrap_or_default();
        let old_domain = format!("{old_name}.{}", self.detected_suffix);
        if let Some(basename) = path.file_name().and_then(|name| name.to_str()) {
            if self.name.is_empty() || self.name == old_name {
                self.name = sanitize_name(basename);
            }
            if self.hostname.is_empty() || self.hostname == old_domain {
                self.hostname = format!("{}.{suffix}", self.name);
            }
        }
        self.profile = detect_profile(path);
        self.document_root = if matches!(self.profile, Profile::Laravel | Profile::Symfony)
            && path.join("public").is_dir()
        {
            "public".into()
        } else {
            ".".into()
        };
        self.detected_path = Some(path.to_path_buf());
        self.detected_suffix = suffix.into();
    }

    fn source_path(&self) -> PathBuf {
        self.path
            .trim()
            .strip_prefix("~/")
            .and_then(|relative| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(relative))
            })
            .unwrap_or_else(|| PathBuf::from(self.path.trim()))
    }

    fn has_worker_preset(&self) -> bool {
        !default_supervisor_command(self.profile).is_empty()
    }

    fn to_request(&self, lang: Lang) -> Result<NewProject, String> {
        if self.path.trim().is_empty() {
            return Err(lang.error_no_path().into());
        }
        let mut cron_entries = Vec::new();
        for line in self
            .cron_lines
            .lines()
            .map(str::trim)
            .filter(|line| self.cron && !line.is_empty())
        {
            cron_entries
                .push(localweb_core::parse_cron_line(line).map_err(|error| error.to_string())?);
        }
        let mut supervisor_command = Vec::new();
        let custom_supervisor = !self.has_worker_preset() || self.supervisor_custom;
        if self.supervisor && custom_supervisor && !self.supervisor_command.trim().is_empty() {
            supervisor_command.push(self.supervisor_command.trim().to_owned());
            supervisor_command.extend(
                self.supervisor_args
                    .lines()
                    .map(str::trim)
                    .filter(|argument| !argument.is_empty())
                    .map(str::to_owned),
            );
        }
        if self.supervisor && custom_supervisor && supervisor_command.is_empty() {
            return Err(lang.error_no_executable().into());
        }
        let request = NewProject {
            path: self.source_path(),
            name: nonempty(&self.name),
            hostname: nonempty(&self.hostname),
            document_root: nonempty(&self.document_root).map(PathBuf::from),
            profile: Some(self.profile),
            web_server: Some(self.web_server),
            php: Some(self.php.clone()),
            node: Some(self.node.clone()),
            composer: Some("2".into()),
            database: Some(self.database),
            cache: Some(self.cache),
            supervisor: SupervisorConfig {
                enabled: self.supervisor,
                command: supervisor_command,
            },
            cron: cron_entries,
            cron_preset: self.cron,
            clear_cron: false,
            mail: self.mail,
            web_port: None,
            database_port: None,
            autostart: self.autostart,
        };
        localweb_core::Project::from_new(request.clone(), &GlobalConfig::default())
            .map_err(|error| error.to_string())?;
        Ok(request)
    }
}

struct DirectoryPicker {
    current: PathBuf,
    history: Vec<PathBuf>,
    history_position: usize,
    selected: Option<PathBuf>,
    open: bool,
    error: Option<String>,
}

impl DirectoryPicker {
    fn new(current: &str) -> Self {
        let requested = PathBuf::from(current);
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let current = if requested.is_dir() { requested } else { home };
        Self {
            history: vec![current.clone()],
            current,
            history_position: 0,
            selected: None,
            open: true,
            error: None,
        }
    }

    fn navigate_to(&mut self, directory: PathBuf) {
        if directory != self.current {
            self.history.truncate(self.history_position + 1);
            self.history.push(directory.clone());
            self.history_position = self.history.len() - 1;
            self.current = directory;
        }
        self.selected = None;
        self.error = None;
    }

    fn go_back(&mut self) {
        if self.history_position > 0 {
            self.history_position -= 1;
            self.restore_from_history();
        }
    }

    fn go_forward(&mut self) {
        if self.history_position + 1 < self.history.len() {
            self.history_position += 1;
            self.restore_from_history();
        }
    }

    fn restore_from_history(&mut self) {
        self.current = self.history[self.history_position].clone();
        self.selected = None;
        self.error = None;
    }

    fn show(&mut self, context: &egui::Context, lang: Lang) -> Option<PathBuf> {
        let mut selected = None;
        let mut navigate = None;
        let mut request_close = false;
        let mut accept = false;
        let modal = egui::Modal::new(egui::Id::new("source_folder_picker")).show(context, |ui| {
            ui.set_width(context.viewport_rect().width().min(760.0) - 40.0);
            ui.label(strong(lang.picker_title()).size(SECTION_TITLE_SIZE));
            ui.add_space(10.0);
            ui.horizontal_top(|ui| {
                if let Some(path) = self.places_sidebar(ui, lang) {
                    navigate = Some(path);
                }
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if icon_button(ui, Icon::ChevronLeft, 36.0, lang.picker_back())
                            .on_hover_text(lang.picker_back())
                            .clicked()
                        {
                            self.go_back();
                        }
                        if icon_button(ui, Icon::ChevronRight, 36.0, lang.picker_forward())
                            .on_hover_text(lang.picker_forward())
                            .clicked()
                        {
                            self.go_forward();
                        }
                        if icon_button(ui, Icon::ChevronUp, 36.0, lang.picker_parent())
                            .on_hover_text(lang.picker_parent())
                            .clicked()
                        {
                            navigate = self.current.parent().map(Path::to_path_buf);
                        }
                    });
                    ui.add_space(6.0);
                    if let Some(path) = self.breadcrumb(ui, lang) {
                        navigate = Some(path);
                    }
                    ui.add_space(8.0);
                    if let Some(error) = &self.error {
                        ui.colored_label(DANGER, error);
                        ui.add_space(4.0);
                    }
                    if let Some(path) = self.folder_list(ui, lang) {
                        navigate = Some(path);
                    }
                });
            });
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if Pill::new(lang.picker_use(), PillKind::Primary)
                    .icon(Icon::Check)
                    .height(36.0)
                    .show(ui)
                    .clicked()
                {
                    accept = true;
                    request_close = true;
                }
                if Pill::new(lang.cancel(), PillKind::Ghost)
                    .height(36.0)
                    .show(ui)
                    .clicked()
                {
                    request_close = true;
                }
            });
            if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                accept = true;
                request_close = true;
            }
        });
        if let Some(directory) = navigate {
            self.navigate_to(directory);
        }
        if accept {
            selected = Some(self.current.clone());
        }
        if request_close || modal.should_close() {
            self.open = false;
        }
        selected
    }

    fn places_sidebar(&self, ui: &mut egui::Ui, lang: Lang) -> Option<PathBuf> {
        let mut navigate = None;
        egui::Frame::new()
            .fill(SURFACE_MUTED)
            .corner_radius(RADIUS_CONTROL)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(156.0);
                ui.set_min_height(392.0);
                ui.vertical(|ui| {
                    for (path, label) in picker_places(lang) {
                        if picker_row(ui, &label, self.current == path, None).clicked() {
                            navigate = Some(path);
                        }
                    }
                });
            });
        navigate
    }

    fn breadcrumb(&self, ui: &mut egui::Ui, lang: Lang) -> Option<PathBuf> {
        let mut navigate = None;
        ui.horizontal_wrapped(|ui| {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            let segments = picker_breadcrumb(&self.current, home.as_deref(), lang);
            let count = segments.len();
            for (index, (path, label)) in segments.into_iter().enumerate() {
                if index > 0 {
                    ui.label(RichText::new("/").size(LABEL_SIZE).color(BORDER));
                }
                if index + 1 == count {
                    ui.label(
                        RichText::new(label)
                            .size(LABEL_SIZE)
                            .color(INK)
                            .family(strong_family()),
                    );
                } else if Pill::new(&label, PillKind::Ghost)
                    .height(36.0)
                    .show(ui)
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    navigate = Some(path);
                }
            }
        });
        navigate
    }

    fn folder_list(&mut self, ui: &mut egui::Ui, lang: Lang) -> Option<PathBuf> {
        let mut navigate = None;
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match child_directories(&self.current) {
                    Ok(directories) => {
                        self.error = None;
                        if directories.is_empty() {
                            ui.label(RichText::new(lang.picker_empty()).color(INK_MUTED));
                        }
                        for directory in directories {
                            let name = directory.file_name().map_or_else(
                                || directory.display().to_string(),
                                |name| name.to_string_lossy().into_owned(),
                            );
                            let chosen = self.selected.as_deref() == Some(directory.as_path());
                            let response = picker_row(ui, &name, chosen, Some(Icon::Folder))
                                .on_hover_text(lang.picker_open(&name));
                            if response.double_clicked() {
                                navigate = Some(directory);
                            } else if response.clicked() {
                                self.selected = Some(directory);
                            }
                        }
                    }
                    Err(error) => self.error = Some(error),
                }
            });
        navigate
    }
}

fn picker_places(lang: Lang) -> Vec<(PathBuf, String)> {
    let mut places = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        places.push((home.clone(), lang.picker_home().to_owned()));
        for (name, label) in [
            ("Desktop", lang.picker_desktop()),
            ("Documents", lang.picker_documents()),
            ("Downloads", lang.picker_downloads()),
        ] {
            let path = home.join(name);
            if path.is_dir() {
                places.push((path, label.to_owned()));
            }
        }
    }
    places.push((PathBuf::from("/"), "/".to_owned()));
    places
}

fn picker_breadcrumb(current: &Path, home: Option<&Path>, lang: Lang) -> Vec<(PathBuf, String)> {
    let mut ancestors = current.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    if let Some(index) =
        home.and_then(|home| ancestors.iter().position(|ancestor| *ancestor == home))
    {
        ancestors.drain(..index);
    }
    ancestors
        .into_iter()
        .map(|ancestor| {
            let label = if ancestor.parent().is_none() {
                "/".to_owned()
            } else if home == Some(ancestor) {
                lang.picker_home().to_owned()
            } else {
                ancestor.file_name().map_or_else(
                    || ancestor.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                )
            };
            (ancestor.to_path_buf(), label)
        })
        .collect()
}

fn picker_row(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    icon: Option<Icon>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), theme::COMPACT_CONTROL_HEIGHT),
        egui::Sense::click(),
    );
    let pressed = response.is_pointer_button_down_on();
    let t = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() && !selected,
        0.12,
    );
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if selected {
            painter.rect_filled(rect, RADIUS_CONTROL, ACCENT_SOFT);
        } else if pressed {
            painter.rect_filled(rect, RADIUS_CONTROL, theme::ACCENT_SOFT_PRESSED);
        } else if t > 0.0 {
            painter.rect_filled(
                rect,
                RADIUS_CONTROL,
                SURFACE.lerp_to_gamma(SURFACE_HOVER, t),
            );
        }
        let mut label_left = rect.left() + 10.0;
        if let Some(icon) = icon {
            crate::icons::paint(
                painter,
                egui::Rect::from_center_size(
                    egui::pos2(rect.left() + 20.0, rect.center().y),
                    Vec2::splat(15.0),
                ),
                icon,
                if selected { ACCENT_TEXT } else { INK_MUTED },
            );
            label_left = rect.left() + 38.0;
        }
        painter.text(
            egui::pos2(label_left, rect.center().y),
            egui::Align2::LEFT_CENTER,
            picker_ellipsize(label, 52),
            egui::FontId::new(LABEL_SIZE, medium_family()),
            if selected { ACCENT_TEXT } else { INK },
        );
        if response.has_focus() {
            focus_ring(ui, rect, FOCUS);
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label.to_owned())
    });
    response
}

fn picker_ellipsize(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        return label.to_owned();
    }
    let mut shortened = label
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    shortened.push('…');
    shortened
}

struct Notice {
    text: String,
    error: bool,
    created: Instant,
}

impl Notice {
    fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: false,
            created: Instant::now(),
        }
    }

    fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: true,
            created: Instant::now(),
        }
    }

    fn toast(&self, ui: &mut egui::Ui, lang: Lang) -> bool {
        let (fill, color, icon) = if self.error {
            (DANGER_SOFT, DANGER, Icon::Warning)
        } else {
            (SUCCESS_SOFT, SUCCESS, Icon::Check)
        };
        let mut dismissed = false;
        egui::Frame::NONE
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, BORDER_STRONG))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_max_width(380.0);
                ui.horizontal_top(|ui| {
                    icon_tile(ui, icon, fill, color, 26.0);
                    ui.add_space(2.0);
                    ui.vertical(|ui| {
                        ui.set_max_width(280.0);
                        if self.error {
                            ui.label(strong(lang.operation_failed()).size(LABEL_SIZE).color(INK));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&self.text).size(SMALL_SIZE).color(INK_MUTED),
                                )
                                .wrap(),
                            );
                        } else {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&self.text).size(LABEL_SIZE).color(INK),
                                )
                                .wrap(),
                            );
                        }
                    });
                    ui.add_space(4.0);
                    if icon_button(ui, Icon::Close, 36.0, lang.dismiss()).clicked() {
                        dismissed = true;
                    }
                });
            });
        dismissed
    }
}

fn settings_label(ui: &mut egui::Ui, label: &str) {
    ui.allocate_ui(Vec2::new(170.0, 20.0), |ui| {
        ui.label(RichText::new(label).size(SMALL_SIZE).color(INK_MUTED));
    });
}

fn ledger_header(ui: &mut egui::Ui, lang: Lang) {
    egui::Frame::NONE
        .fill(SURFACE_MUTED)
        .corner_radius(RADIUS_CARD)
        .inner_margin(egui::Margin::symmetric(16, 9))
        .show(ui, |ui| {
            ui.set_min_width(840.0);
            ui.horizontal(|ui| {
                header_cell(ui, lang.col_project(), 140.0);
                header_cell(ui, lang.col_domain(), 150.0);
                header_cell(ui, lang.col_services(), 108.0);
                header_cell(ui, lang.col_state(), 88.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(lang.col_actions())
                            .size(SMALL_SIZE)
                            .family(strong_family())
                            .color(INK_MUTED),
                    );
                });
            });
        });
}

fn header_cell(ui: &mut egui::Ui, label: &str, width: f32) {
    ui.allocate_ui(Vec2::new(width, 18.0), |ui| {
        ui.set_min_width(width);
        ui.label(
            RichText::new(label)
                .size(SMALL_SIZE)
                .family(strong_family())
                .color(INK_MUTED),
        );
    });
}
fn child_directories(path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut directories = fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry| entry.is_dir())
        .collect::<Vec<_>>();
    directories.sort_by_key(|directory| {
        directory
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    Ok(directories)
}

fn open_browser(
    hostname: &str,
    web_port: Option<u16>,
    public_url: Option<&str>,
    lang: Lang,
    notice: &mut Option<Notice>,
) {
    let url = public_url.map_or_else(
        || {
            web_port.map_or_else(
                || format!("https://{hostname}"),
                |port| format!("http://127.0.0.1:{port}"),
            )
        },
        str::to_owned,
    );
    open_url(&url, lang, notice);
}

fn open_url(url: &str, lang: Lang, notice: &mut Option<Notice>) {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(windows)]
    let program = "cmd";
    #[cfg(not(any(target_os = "macos", windows)))]
    let program = "xdg-open";
    let mut command = Command::new(program);
    #[cfg(windows)]
    command.args(["/C", "start", ""]).arg(url);
    #[cfg(not(windows))]
    command.arg(url);
    if let Err(error) = command.spawn() {
        *notice = Some(Notice::error(lang.notice_browser(&error.to_string())));
    }
}

fn program_filename(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn localweb_cli_path() -> Result<PathBuf, String> {
    let sibling = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .with_file_name(program_filename("localweb"));
    if sibling.is_file() {
        Ok(sibling)
    } else {
        Ok(PathBuf::from(program_filename("localweb")))
    }
}

fn find_program(name: &str) -> Option<PathBuf> {
    let name = program_filename(name);
    if let Ok(executable) = std::env::current_exe() {
        let sibling = executable.with_file_name(&name);
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|directory| directory.join(&name))
            .find(|candidate| candidate.is_file())
    })
}

fn mcp_configuration() -> &'static str {
    "{\n  \"mcpServers\": {\n    \"localweb\": {\n      \"command\": \"localweb-mcp\"\n    }\n  }\n}"
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn transition_label(lang: Lang, transition: ProjectTransition) -> &'static str {
    match transition {
        ProjectTransition::Starting => lang.transition_starting(),
        ProjectTransition::Stopping => lang.transition_stopping(),
        ProjectTransition::Restarting => lang.transition_restarting(),
    }
}

fn transition_status(ui: &mut egui::Ui, lang: Lang, transition: ProjectTransition) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(
            RichText::new(transition_label(lang, transition))
                .size(SMALL_SIZE)
                .color(INK_MUTED),
        );
    });
}

fn web_label(server: WebServer) -> &'static str {
    match server {
        WebServer::Nginx => "nginx",
        WebServer::Apache => "Apache",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_folder_derives_project_fields() {
        let root = std::env::temp_dir().join(format!("localweb-gui-{}", std::process::id()));
        let project = root.join("My Project");
        fs::create_dir_all(project.join("public")).unwrap();
        fs::write(project.join("artisan"), "").unwrap();
        let mut form = AddProjectForm::default();
        assert_eq!(form.node, "24");
        form.detect_path(&project, "dev");
        assert_eq!(form.name, "my-project");
        assert_eq!(form.hostname, "my-project.dev");
        assert_eq!(form.document_root, "public");
        assert_eq!(form.profile, Profile::Laravel);
        fs::remove_dir_all(root).unwrap();
    }

    fn test_source(label: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("localweb-gui-{label}-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn window_placement_round_trips() {
        let config = test_source("window-placement");
        let placement = WindowPlacement {
            x: 123.5,
            y: -45.0,
            maximized: true,
        };
        placement.save(&config).unwrap();
        assert_eq!(WindowPlacement::load(&config), Some(placement));
        fs::write(config.join("gui-window"), "not a position").unwrap();
        assert_eq!(WindowPlacement::load(&config), None);
        fs::remove_dir_all(config).unwrap();
    }

    #[test]
    fn add_form_is_usable_while_localweb_is_stopped() {
        let path = test_source("offline-add");
        let form = AddProjectForm {
            path: path.display().to_string(),
            ..Default::default()
        };
        let mut app = LocalWebApp::state(None, Lang::English, None);
        app.connection = Connection::Stopped;
        app.view = View::AddProject;
        app.form = form;
        assert_view_fits(
            &mut app,
            View::AddProject,
            4000.0,
            &[Lang::English.create_project(), Lang::English.cancel()],
        );
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn offline_add_is_queued_until_the_daemon_starts() {
        let path = test_source("offline-queue");
        let form = AddProjectForm {
            path: path.display().to_string(),
            ..Default::default()
        };
        let mut app = LocalWebApp::state(None, Lang::English, None);
        app.connection = Connection::Stopped;
        app.queued_add = Some(form.to_request(Lang::English).unwrap());
        app.handle_command(Action::StartDaemon, Ok("started".into()));
        assert!(app.queued_add.is_none());
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn edit_form_round_trips_editable_settings() {
        let path = test_source("edit-roundtrip");
        fs::create_dir_all(path.join("public")).unwrap();
        let project = Project::from_new(
            NewProject {
                path: path.clone(),
                name: Some("edit-roundtrip".into()),
                hostname: Some("edit-roundtrip.test".into()),
                web_port: Some(9_201),
                database_port: Some(33_201),
                document_root: Some(PathBuf::from("public")),
                profile: Some(Profile::Laravel),
                web_server: Some(WebServer::Apache),
                php: Some("8.4".into()),
                composer: Some("2".into()),
                node: Some("22".into()),
                database: Some(Database::Postgres),
                cache: Some(Cache::Redis),
                supervisor: SupervisorConfig {
                    enabled: true,
                    command: vec![
                        "@php".into(),
                        "artisan".into(),
                        "queue:work".into(),
                        "--queue=high".into(),
                    ],
                },
                cron: vec![
                    localweb_core::CronEntry {
                        enabled: true,
                        schedule: "0 3 * * *".into(),
                        command: vec!["@php".into(), "artisan".into(), "schedule:run".into()],
                    },
                    localweb_core::CronEntry {
                        enabled: false,
                        schedule: "0 4 * * *".into(),
                        command: vec!["@php".into(), "artisan".into(), "queue:prune".into()],
                    },
                ],
                cron_preset: false,
                clear_cron: false,
                mail: true,
                autostart: true,
            },
            &GlobalConfig::default(),
        )
        .unwrap();
        assert_eq!(project.path, path.canonicalize().unwrap());
        let form = AddProjectForm::from_project(&project);
        assert_eq!(form.path, project.path.display().to_string());
        assert_eq!(form.name, "edit-roundtrip");
        assert_eq!(form.hostname, "edit-roundtrip.test");
        assert_eq!(form.php, "8.4");
        assert_eq!(form.database, Database::Postgres);
        assert_eq!(form.web_server, WebServer::Apache);
        assert_eq!(form.node, "22");
        assert!(form.supervisor && form.supervisor_custom);
        assert_eq!(form.supervisor_command, "@php");
        assert_eq!(form.supervisor_args, "artisan\nqueue:work\n--queue=high");
        assert!(form.cron);
        assert_eq!(form.cron_lines, "0 3 * * * @php artisan schedule:run");

        let request = form.to_request(Lang::English).unwrap();
        let updated = Project::from_update(&project, request, &GlobalConfig::default()).unwrap();
        assert_eq!(updated.id, project.id);
        assert_eq!(updated.php, project.php);
        assert_eq!(updated.database, project.database);
        assert_eq!(updated.web_server, project.web_server);
        assert_eq!(updated.supervisor.command, project.supervisor.command);
        assert_eq!(updated.cron.iter().filter(|entry| entry.enabled).count(), 1);
        assert!(updated.cron.iter().any(|entry| !entry.enabled));
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn disabling_cron_omits_hidden_commands() {
        let path = test_source("cron");
        let form = AddProjectForm {
            path: path.display().to_string(),
            cron: false,
            cron_lines: "not even a valid schedule".into(),
            ..Default::default()
        };
        let request = form.to_request(Lang::English).unwrap();
        assert_eq!(request.cron, Vec::new());
        assert!(!request.cron_preset);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn symfony_worker_uses_the_shared_preset() {
        let path = test_source("symfony");
        let form = AddProjectForm {
            path: path.display().to_string(),
            profile: Profile::Symfony,
            supervisor: true,
            ..Default::default()
        };
        let request = form.to_request(Lang::English).unwrap();
        let project = localweb_core::Project::from_new(request, &GlobalConfig::default()).unwrap();
        assert_eq!(
            project.supervisor.command,
            default_supervisor_command(Profile::Symfony)
        );
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn folder_detection_respects_suffix_and_keeps_custom_identity() {
        let path = test_source("custom");
        let mut form = AddProjectForm::default();
        form.detect_path(&path, "test");
        assert_eq!(form.hostname.rsplit('.').next(), Some("test"));
        form.name = "custom-app".into();
        form.hostname = "custom-app.test".into();
        let next = path.join("another-folder");
        fs::create_dir_all(&next).unwrap();
        form.detect_path(&next, "test");
        assert_eq!(form.name, "custom-app");
        assert_eq!(form.hostname, "custom-app.test");
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn transport_failure_preserves_known_project_state() {
        let path = test_source("connection");
        let form = AddProjectForm {
            path: path.display().to_string(),
            ..Default::default()
        };
        let project = localweb_core::Project::from_new(
            form.to_request(Lang::English).unwrap(),
            &GlobalConfig::default(),
        )
        .unwrap();
        let mut app = LocalWebApp::state(None, Lang::English, None);
        app.connection = Connection::Connected;
        app.daemon_online = true;
        app.projects.push(ProjectStatus {
            project,
            running: true,
            mail_web_url: None,
            public_url: Some("https://connection.test:8443".into()),
        });
        app.handle_response(Action::RefreshStatus, Err("socket unavailable".into()));
        assert!(app.projects[0].running);
        assert!(app.connection == Connection::Unavailable);
        assert!(!app.daemon_online);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn reading_domain_suffix_does_not_perform_setup() {
        let base = test_source("first-run");
        let paths = Paths {
            config: base.join("config"),
            data: base.join("data"),
            state: base.join("state"),
            runtime: base.join("runtime"),
        };
        let app = LocalWebApp::state(Some(paths.clone()), Lang::English, None);
        assert_eq!(app.domain_suffix(), "dev");
        assert!(!app.setup_complete());
        assert!(!paths.config.join("config.toml").exists());
        fs::remove_dir_all(base).unwrap();
    }

    fn render_minimum_view(
        app: &mut LocalWebApp,
        view: View,
        height: f32,
    ) -> (egui::Rect, egui::FullOutput) {
        let ctx = egui::Context::default();
        configure_style(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(840.0, height));
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                app.sidebar(ui);
                egui::CentralPanel::default().show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match view {
                            View::Dashboard => app.dashboard(ui),
                            View::Projects => app.projects(ui),
                            View::AddProject | View::EditProject => app.project_form(ui),
                            View::Settings => app.settings(ui),
                        });
                });
            },
        );
        (screen, output)
    }

    fn assert_view_fits(app: &mut LocalWebApp, view: View, height: f32, required: &[&str]) {
        let (screen, output) = render_minimum_view(app, view, height);
        for label in required {
            let matches = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == *label => {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(
                !matches.is_empty(),
                "required action {label:?} must be rendered in {view:?}; rendered: {:?}",
                output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            );
            assert!(
                matches
                    .iter()
                    .all(|rect| rect.left() >= screen.left() && rect.right() <= screen.right()),
                "required action {label:?} must fit without horizontal scrolling in {view:?}"
            );
        }
        for shape in &output.shapes {
            let egui::Shape::Text(text) = &shape.shape else {
                continue;
            };
            let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
            assert!(
                rect.left() >= screen.left() && rect.right() <= screen.right(),
                "text {:?} overflows the minimum window horizontally in {view:?}",
                text.galley.job.text
            );
        }
    }

    #[test]
    fn primary_actions_fit_the_minimum_window_in_all_languages() {
        let path = test_source("layout");
        let form = AddProjectForm {
            path: path.display().to_string(),
            ..Default::default()
        };
        let project = localweb_core::Project::from_new(
            form.to_request(Lang::English).unwrap(),
            &GlobalConfig::default(),
        )
        .unwrap();
        for lang in Lang::ALL {
            let mut app = LocalWebApp::state(None, lang, None);
            app.connection = Connection::Connected;
            app.daemon_online = true;
            app.projects_loaded = true;
            app.projects.push(ProjectStatus {
                project: project.clone(),
                running: false,
                mail_web_url: None,
                public_url: None,
            });
            let stop = lang.stop_localweb();
            assert_view_fits(
                &mut app,
                View::Dashboard,
                560.0,
                &[lang.action_start(), stop],
            );
            assert_view_fits(
                &mut app,
                View::Projects,
                560.0,
                &[
                    lang.action_start(),
                    lang.project_details(),
                    lang.action_edit(),
                    lang.action_delete(),
                    stop,
                ],
            );
            app.details = Some(project.name.clone());
            assert_view_fits(
                &mut app,
                View::Projects,
                4000.0,
                &[lang.reveal_environment()],
            );
            assert_view_fits(&mut app, View::AddProject, 560.0, &[stop]);
            assert_view_fits(
                &mut app,
                View::AddProject,
                4000.0,
                &[lang.create_project(), lang.cancel()],
            );
            app.edit_project = Some(project.clone());
            app.form = AddProjectForm::from_project(&project);
            assert_view_fits(&mut app, View::EditProject, 560.0, &[stop]);
            assert_view_fits(
                &mut app,
                View::EditProject,
                4000.0,
                &[lang.save_changes(), lang.cancel()],
            );
            app.edit_project = None;
            app.form = AddProjectForm::default();
            app.details = None;
            assert_view_fits(&mut app, View::Settings, 560.0, &[stop]);
            assert_view_fits(
                &mut app,
                View::Settings,
                4000.0,
                &[
                    lang.maintenance_title(),
                    lang.refresh(),
                    lang.sync_domains(),
                    lang.repair_setup(),
                ],
            );
        }
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn picker_history_walks_back_and_forward() {
        let mut picker = DirectoryPicker {
            current: PathBuf::from("/a"),
            history: vec![PathBuf::from("/a")],
            history_position: 0,
            selected: None,
            open: true,
            error: None,
        };
        picker.navigate_to(PathBuf::from("/a/b"));
        picker.navigate_to(PathBuf::from("/a/b/c"));
        picker.go_back();
        assert_eq!(picker.current, Path::new("/a/b"));
        picker.go_forward();
        assert_eq!(picker.current, Path::new("/a/b/c"));
        picker.go_back();
        picker.navigate_to(PathBuf::from("/a/x"));
        picker.go_forward();
        assert_eq!(picker.current, Path::new("/a/x"));
        picker.go_back();
        picker.go_back();
        picker.go_back();
        assert_eq!(picker.current, Path::new("/a"));
    }

    #[test]
    fn picker_breadcrumb_collapses_to_home() {
        let home = Path::new("/home/dev");
        let labels = picker_breadcrumb(Path::new("/home/dev/sites/app"), Some(home), Lang::English)
            .into_iter()
            .map(|(_, label)| label)
            .collect::<Vec<_>>();
        assert_eq!(labels, ["Home", "sites", "app"]);
        let outside = picker_breadcrumb(Path::new("/srv/www"), Some(home), Lang::English);
        assert_eq!(outside.len(), 3);
        let at_home = picker_breadcrumb(Path::new("/home/dev"), Some(home), Lang::English);
        assert_eq!(at_home.len(), 1);
    }
}
