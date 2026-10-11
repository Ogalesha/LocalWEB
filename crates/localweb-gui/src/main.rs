mod lang;

use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use lang::Lang;
use localweb_core::{
    CaStatus, Cache, Database, GlobalConfig, NewProject, Paths, Profile, ProjectStatus, Request,
    Response, Status, SupervisorConfig, WebServer, default_supervisor_command, detect_profile,
    sanitize_name, send_request,
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

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const CANVAS: Color32 = Color32::from_rgb(247, 245, 239);
const SURFACE: Color32 = Color32::from_rgb(255, 254, 250);
const SURFACE_MUTED: Color32 = Color32::from_rgb(240, 241, 236);
const INK: Color32 = Color32::from_rgb(18, 28, 37);
const INK_MUTED: Color32 = Color32::from_rgb(82, 94, 105);
const NAV: Color32 = Color32::from_rgb(23, 35, 31);
const NAV_MUTED: Color32 = Color32::from_rgb(171, 187, 180);
const ACCENT: Color32 = Color32::from_rgb(8, 122, 77);
const ACCENT_SOFT: Color32 = Color32::from_rgb(224, 243, 233);
const SUCCESS: Color32 = Color32::from_rgb(8, 122, 77);
const WARNING: Color32 = Color32::from_rgb(154, 91, 10);
const DANGER: Color32 = Color32::from_rgb(199, 39, 53);
const BORDER: Color32 = Color32::from_rgb(216, 220, 214);

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1_080.0, 720.0])
            .with_min_inner_size([840.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "LocalWeb",
        options,
        Box::new(|context| Ok(Box::new(LocalWebApp::new(context)))),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Dashboard,
    Projects,
    AddProject,
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
    GatewayStopped,
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
    project_search: String,
    form: AddProjectForm,
    picker: Option<DirectoryPicker>,
    remove_confirmation: Option<String>,
    details: Option<String>,
    environment: Option<BTreeMap<String, String>>,
    environment_error: Option<String>,
    notice: Option<Notice>,
    pending: usize,
    background_pending: usize,
    daemon_transition: Option<DaemonTransition>,
    project_transition: Option<(String, ProjectTransition)>,
    last_refresh: Instant,
}

impl LocalWebApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        configure_style(&context.egui_ctx);
        let (paths, notice) = match Paths::discover() {
            Ok(paths) => (Some(paths), None),
            Err(error) => (None, Some(Notice::error(error.to_string()))),
        };
        let lang = paths
            .as_ref()
            .map_or_else(Lang::default, |paths| Lang::load(&paths.config));
        let mut app = Self::state(paths, lang, notice);
        app.refresh_all();
        app
    }

    fn state(paths: Option<Paths>, lang: Lang, notice: Option<Notice>) -> Self {
        let (sender, receiver) = mpsc::channel();
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
            project_search: String::new(),
            form: AddProjectForm::default(),
            picker: None,
            remove_confirmation: None,
            details: None,
            environment: None,
            environment_error: None,
            notice,
            pending: 0,
            background_pending: 0,
            daemon_transition: None,
            project_transition: None,
            last_refresh: Instant::now(),
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
                        let seconds = if action.background() { 5 } else { 120 };
                        tokio::time::timeout(
                            Duration::from_secs(seconds),
                            send_request(&paths, request),
                        )
                        .await
                        .map_err(|_| {
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
            }
            (Action::SyncHosts, Ok(_)) => {
                self.notice = Some(Notice::success(self.lang.notice_hosts()));
            }
            (_, Err(error)) => self.notice = Some(Notice::error(error)),
            (_, Ok(message)) => self.notice = Some(Notice::success(message)),
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
        reason = "sidebar owns the navigation and the fixed-height footer layout"
    )]
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        egui::Panel::left("navigation")
            .exact_size(224.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(NAV)
                    .inner_margin(egui::Margin::symmetric(18, 22)),
            )
            .show(ui, |ui| {
                let sidebar_size = Vec2::new(
                    ui.available_width(),
                    (ui.ctx().content_rect().height() - 44.0).max(480.0),
                );
                ui.allocate_ui_with_layout(
                    sidebar_size,
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.visuals_mut().widgets.active.bg_stroke = Stroke::new(2.0, CANVAS);
                        ui.horizontal(|ui| {
                            brand_mark(ui, 26.0);
                            ui.add_space(5.0);
                            ui.label(
                                RichText::new("LocalWeb")
                                    .size(22.0)
                                    .strong()
                                    .color(Color32::WHITE),
                            );
                        });
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(lang.nav_tagline())
                                .size(12.0)
                                .color(NAV_MUTED),
                        );
                        ui.add_space(30.0);
                        nav_button(ui, &mut self.view, View::Dashboard, lang.nav_dashboard());
                        ui.add_space(5.0);
                        nav_button(ui, &mut self.view, View::Projects, lang.nav_projects());
                        ui.add_space(5.0);
                        nav_button(ui, &mut self.view, View::AddProject, lang.nav_add());
                        let panel_rect = ui.max_rect();
                        let footer_height = 196.0;
                        let footer_rect = egui::Rect::from_min_size(
                            egui::pos2(panel_rect.left(), panel_rect.bottom() - footer_height),
                            Vec2::new(panel_rect.width(), footer_height),
                        );
                        ui.scope_builder(egui::UiBuilder::new().max_rect(footer_rect), |ui| {
                            ui.vertical(|ui| {
                                for (code, label) in
                                    [("en", "English"), ("it", "Italiano"), ("sq", "Shqip")]
                                {
                                    let selected = self.lang.code() == code;
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 36.0],
                                            egui::Button::new(
                                                RichText::new(label).size(12.0).color(
                                                    if selected {
                                                        Color32::WHITE
                                                    } else {
                                                        NAV_MUTED
                                                    },
                                                ),
                                            )
                                            .selected(selected)
                                            .fill(if selected {
                                                ACCENT
                                            } else {
                                                Color32::TRANSPARENT
                                            })
                                            .corner_radius(6.0),
                                        )
                                        .clicked()
                                    {
                                        self.lang = Lang::from_code(code);
                                        if let Some(paths) = &self.paths {
                                            self.lang.save(&paths.config);
                                        }
                                    }
                                }
                            });
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(lang.nav_close_note())
                                    .size(12.0)
                                    .color(NAV_MUTED),
                            );
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                status_dot(ui, self.daemon_online);
                                ui.label(
                                    RichText::new(if self.connection == Connection::Checking {
                                        lang.checking_status()
                                    } else if self.daemon_online {
                                        lang.nav_running()
                                    } else if self.connection != Connection::Stopped {
                                        lang.daemon_unavailable()
                                    } else {
                                        lang.nav_stopped()
                                    })
                                    .strong()
                                    .color(Color32::WHITE),
                                );
                            });
                            ui.add_space(12.0);
                            ui.separator();
                        });
                    },
                );
            });
    }

    fn global_action(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        if self.daemon_online {
            let stopping = matches!(self.daemon_transition, Some(DaemonTransition::Stopping));
            let stop = egui::Button::new(
                RichText::new(if stopping {
                    lang.op_stopping()
                } else {
                    lang.stop_localweb()
                })
                .color(Color32::WHITE)
                .strong(),
            )
            .fill(DANGER)
            .corner_radius(9.0)
            .min_size(Vec2::new(154.0, 42.0));
            if ui
                .add_enabled(self.pending == 0 && !stopping, stop)
                .on_hover_text(lang.stop_hint())
                .clicked()
            {
                self.daemon_transition = Some(DaemonTransition::Stopping);
                self.request(Action::Shutdown, Request::Shutdown);
            }
        } else {
            let label = match self.daemon_transition {
                Some(DaemonTransition::Setup) => lang.op_setup(),
                Some(DaemonTransition::Starting) => lang.op_starting(),
                Some(DaemonTransition::Stopping) => lang.op_stopping(),
                None if self.setup_complete() => lang.start_localweb(),
                None => lang.configure_localweb(),
            };
            if ui
                .add_enabled(
                    self.connection != Connection::Checking
                        && self.pending == 0
                        && self.daemon_transition.is_none(),
                    primary_button(label),
                )
                .clicked()
            {
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
        description: &str,
        show_add_project: bool,
    ) {
        let lang = self.lang;
        if ui.available_width() < 700.0 {
            ui.vertical(|ui| {
                page_header(ui, title, description);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if show_add_project && ui.add(primary_button(lang.nav_add())).clicked() {
                        self.view = View::AddProject;
                    }
                    self.global_action(ui);
                });
            });
        } else {
            ui.horizontal(|ui| {
                page_header(ui, title, description);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.global_action(ui);
                    if show_add_project {
                        ui.add_space(8.0);
                        if ui.add(primary_button(lang.nav_add())).clicked() {
                            self.view = View::AddProject;
                        }
                    }
                });
            });
        }
    }

    fn dashboard(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        self.page_toolbar(ui, lang.dash_title(), lang.dash_sub(), false);
        ui.add_space(22.0);

        self.environment_overview(ui);

        ui.add_space(26.0);
        ui.horizontal(|ui| {
            ui.heading(RichText::new(lang.nav_projects()).size(25.0).color(INK));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(primary_button(lang.nav_add())).clicked() {
                    self.view = View::AddProject;
                }
            });
        });
        ui.add_space(10.0);
        if !self.projects_loaded {
            ui.label(if self.daemon_online {
                lang.loading_projects()
            } else {
                lang.stopped_body()
            });
        } else if self.projects.is_empty() {
            empty_projects(ui, lang, self.daemon_online);
        } else {
            let preview = self.projects.iter().take(5).cloned().collect::<Vec<_>>();
            if ui.available_width() >= 1_120.0 {
                project_ledger_header(ui, lang);
            }
            for project in preview {
                self.project_ledger_row(ui, &project);
            }
            if self.projects.len() > 5 && ui.button(lang.view_all_projects()).clicked() {
                self.view = View::Projects;
            }
        }

        if self.daemon_online && self.ca.as_ref().is_some_and(|ca| !ca.trusted) {
            ui.add_space(16.0);
            callout(ui, WARNING, |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(lang.ca_callout_title()).strong());
                    ui.label(lang.ca_callout_body());
                    if ui
                        .add_enabled(self.pending == 0, secondary_button(lang.install_ca()))
                        .clicked()
                    {
                        self.install_ca();
                    }
                });
            });
        }
        ui.add_space(12.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.background_pending == 0,
                    secondary_button(lang.refresh()),
                )
                .clicked()
            {
                self.refresh_all();
            }
            if ui
                .add_enabled(
                    self.pending == 0 && self.daemon_online,
                    secondary_button(lang.sync_domains()),
                )
                .clicked()
            {
                self.sync_hosts();
            }
            if ui
                .add_enabled(self.pending == 0, secondary_button(lang.repair_setup()))
                .clicked()
            {
                self.setup();
            }
        });
        if let Some(error) = &self.ca_error {
            ui.label(lang.ca_load_failed());
            ui.collapsing(lang.technical_details(), |ui| {
                ui.label(error);
            });
        }
    }

    fn environment_overview(&self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let gateway_running = self
            .status
            .as_ref()
            .is_some_and(|status| self.daemon_online && status.gateway_running);
        let active = self
            .projects
            .iter()
            .filter(|project| project.running)
            .count();
        let supervised = self
            .projects
            .iter()
            .filter(|project| project.project.supervisor.enabled)
            .count();
        let (ca_label, ca_active) = match self.ca.as_ref() {
            Some(ca) if ca.trusted => (lang.ca_trusted(), true),
            Some(ca) if ca.exists => (lang.ca_pending(), false),
            Some(_) => (lang.ca_missing(), false),
            None => (lang.checking_status(), false),
        };
        environment_banner(
            ui,
            lang,
            match self.connection {
                Connection::Checking => EnvironmentState::Checking,
                Connection::Unavailable => EnvironmentState::Unavailable,
                Connection::Stopped => EnvironmentState::Stopped,
                Connection::Connected if gateway_running => EnvironmentState::Ready,
                Connection::Connected => EnvironmentState::GatewayStopped,
            },
            active,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    service_card(
                        ui,
                        Lang::svc_daemon(),
                        if self.connection == Connection::Checking {
                            lang.checking_status()
                        } else if self.daemon_online {
                            lang.val_running()
                        } else if self.connection != Connection::Stopped {
                            lang.daemon_unavailable()
                        } else {
                            lang.val_stopped()
                        },
                        self.daemon_online,
                    );
                    service_card(
                        ui,
                        Lang::svc_gateway(),
                        if gateway_running {
                            lang.gateway_on()
                        } else {
                            lang.val_stopped()
                        },
                        gateway_running,
                    );
                    service_card(ui, Lang::svc_ca(), ca_label, ca_active);
                    service_card(
                        ui,
                        lang.svc_projects(),
                        &lang.projects_summary(active, self.projects.len()),
                        active > 0,
                    );
                    service_card(
                        ui,
                        lang.configured_workers(),
                        &lang.supervised_summary(supervised),
                        false,
                    );
                });
            },
        );
    }

    fn projects(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        self.page_toolbar(ui, lang.projects_title(), lang.projects_sub(), true);
        ui.add_space(22.0);
        if !self.daemon_online {
            callout(ui, DANGER, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(if self.connection == Connection::Checking {
                            lang.checking_status()
                        } else if self.connection == Connection::Stopped {
                            lang.stopped_title()
                        } else {
                            lang.daemon_unavailable()
                        })
                        .strong(),
                    );
                    ui.label(lang.stopped_body());
                });
            });
            if !self.projects_loaded {
                return;
            }
        }
        if !self.projects_loaded {
            ui.label(lang.loading_projects());
            return;
        }
        if let Some(error) = &self.projects_error {
            ui.label(lang.projects_load_failed());
            ui.collapsing(lang.technical_details(), |ui| {
                ui.label(error);
            });
        }
        egui::Frame::NONE
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(9.0)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                let label = ui.label(lang.search_projects());
                ui.horizontal_wrapped(|ui| {
                    search_mark(ui);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.project_search)
                            .desired_width(ui.available_width().min(320.0) - 60.0)
                            .hint_text(lang.search_hint()),
                    )
                    .labelled_by(label.id);
                    if !self.project_search.is_empty() && ui.button(lang.clear_search()).clicked() {
                        self.project_search.clear();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(lang.configured_count(self.projects.len()))
                                .color(INK_MUTED),
                        );
                    });
                });
            });
        ui.add_space(14.0);
        if self.projects.is_empty() {
            empty_projects(ui, lang, true);
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
        } else {
            ui.label(lang.projects_summary(
                projects.iter().filter(|item| item.running).count(),
                projects.len(),
            ));
        }
        if ui.available_width() >= 1_120.0 {
            project_ledger_header(ui, lang);
        }
        for item in projects {
            self.project_ledger_row(ui, &item);
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "wide and stacked layouts share one project action implementation"
    )]
    fn project_ledger_row(&mut self, ui: &mut egui::Ui, item: &ProjectStatus) {
        let lang = self.lang;
        if ui.available_width() < 1_120.0 {
            egui::Frame::NONE
                .fill(SURFACE)
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&item.project.name).strong());
                        if self.daemon_online {
                            state_badge(ui, item.running, lang);
                        } else {
                            ui.label(lang.last_known_state());
                        }
                        ui.label(format!(
                            "PHP {} · {} · {}",
                            item.project.php,
                            web_label(item.project.web_server),
                            lang.database_name(item.project.database)
                        ));
                    });
                    ui.label(RichText::new(&item.project.hostname).monospace());
                    ui.label(
                        RichText::new(item.project.path.display().to_string())
                            .size(12.0)
                            .color(INK_MUTED),
                    )
                    .on_hover_text(item.project.path.display().to_string());
                    ui.horizontal_wrapped(|ui| self.project_actions(ui, item));
                });
            if self.details.as_deref() == Some(&item.project.name) {
                self.project_details(ui, item);
            }
            ui.separator();
            return;
        }
        let cache = if item.project.supervisor.enabled {
            format!(
                "{} · {}",
                lang.cache_name(item.project.cache),
                Lang::supervisor_tag()
            )
        } else {
            lang.cache_name(item.project.cache).to_owned()
        };
        egui::Frame::NONE
            .fill(SURFACE)
            .inner_margin(egui::Margin::symmetric(14, 11))
            .show(ui, |ui| {
                ui.set_min_width(730.0);
                ui.horizontal(|ui| {
                    ui.allocate_ui(Vec2::new(150.0, 42.0), |ui| {
                        ui.label(RichText::new(&item.project.name).strong().color(INK));
                        ui.label(
                            RichText::new(format!(
                                "PHP {} · {}",
                                item.project.php,
                                web_label(item.project.web_server)
                            ))
                            .size(12.0)
                            .color(INK_MUTED),
                        );
                    });
                    ui.allocate_ui(Vec2::new(184.0, 42.0), |ui| {
                        ui.label(RichText::new(&item.project.hostname).color(ACCENT));
                        ui.label(
                            RichText::new(item.project.path.display().to_string())
                                .size(12.0)
                                .color(INK_MUTED),
                        );
                    });
                    ui.allocate_ui(Vec2::new(132.0, 42.0), |ui| {
                        ui.label(lang.database_name(item.project.database));
                        ui.label(RichText::new(cache).size(12.0).color(INK_MUTED));
                    });
                    ui.allocate_ui(Vec2::new(92.0, 42.0), |ui| {
                        let transition = self
                            .project_transition
                            .as_ref()
                            .filter(|(name, _)| name == &item.project.name)
                            .map(|(_, transition)| *transition);
                        if let Some(transition) = transition {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new(match transition {
                                        ProjectTransition::Starting => lang.transition_starting(),
                                        ProjectTransition::Stopping => lang.transition_stopping(),
                                        ProjectTransition::Restarting => {
                                            lang.transition_restarting()
                                        }
                                    })
                                    .size(12.0)
                                    .color(INK_MUTED),
                                );
                            });
                        } else {
                            state_badge(ui, item.running, lang);
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.project_actions(ui, item);
                    });
                });
            });
        ui.separator();
        if self.details.as_deref() == Some(&item.project.name) {
            self.project_details(ui, item);
        }
    }

    fn project_actions(&mut self, ui: &mut egui::Ui, item: &ProjectStatus) {
        let lang = self.lang;
        if ui.add(compact_button(lang.project_details())).clicked() {
            if self.details.as_deref() == Some(&item.project.name) {
                self.details = None;
            } else {
                self.details = Some(item.project.name.clone());
                self.environment = None;
                self.environment_error = None;
            }
        }
        if !self.daemon_online {
            return;
        }
        if let Some((name, transition)) = &self.project_transition
            && name == &item.project.name
        {
            ui.spinner();
            ui.label(match transition {
                ProjectTransition::Starting => lang.transition_starting(),
                ProjectTransition::Stopping => lang.transition_stopping(),
                ProjectTransition::Restarting => lang.transition_restarting(),
            });
            return;
        }
        if item.running {
            if ui
                .add_enabled(self.pending == 0, compact_button(lang.action_stop()))
                .clicked()
            {
                self.project_transition =
                    Some((item.project.name.clone(), ProjectTransition::Stopping));
                self.request(
                    Action::StopProject,
                    Request::ProjectStop {
                        name: item.project.name.clone(),
                    },
                );
            }
            if ui
                .add_enabled(self.pending == 0, compact_button(lang.action_restart()))
                .clicked()
            {
                self.project_transition =
                    Some((item.project.name.clone(), ProjectTransition::Restarting));
                self.request(
                    Action::RestartProject,
                    Request::ProjectRestart {
                        name: item.project.name.clone(),
                    },
                );
            }
            if ui.add(compact_button(lang.action_open())).clicked() {
                open_browser(&item.project.hostname, lang, &mut self.notice);
            }
            if let Some(url) = item.mail_web_url.as_deref()
                && ui.add(compact_button("Mailpit")).clicked()
            {
                open_url(url, lang, &mut self.notice);
            }
        } else {
            if ui
                .add_enabled(self.pending == 0, compact_button(lang.action_remove()))
                .clicked()
            {
                self.remove_confirmation = Some(item.project.name.clone());
            }
            if ui
                .add_enabled(
                    self.pending == 0,
                    compact_primary_button(lang.action_start()),
                )
                .clicked()
            {
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

    fn project_details(&mut self, ui: &mut egui::Ui, item: &ProjectStatus) {
        let lang = self.lang;
        let project = &item.project;
        ui.label(format!(
            "{} · {} · Node.js {} · Composer {}",
            lang.profile_name(project.profile),
            project.document_root.display(),
            project.node,
            project.composer
        ));
        ui.label(format!(
            "{} · {}",
            lang.database_name(project.database),
            lang.cache_name(project.cache)
        ));
        ui.label(lang.project_options(
            project.supervisor.enabled,
            project.cron.len(),
            project.mail,
            project.autostart,
        ));
        if self.environment.is_none() && self.environment_error.is_none() {
            if ui
                .add_enabled(
                    self.daemon_online && self.background_pending == 0,
                    secondary_button(lang.reveal_environment()),
                )
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
            ui.label(lang.environment_help());
            let mut dotenv = String::new();
            for (key, value) in values {
                let _ = writeln!(dotenv, "{key}={value}");
            }
            if ui.button(lang.copy_environment()).clicked() {
                ui.ctx().copy_text(dotenv.clone());
            }
            ui.add(
                egui::TextEdit::multiline(&mut dotenv)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(values.len().min(12))
                    .interactive(false),
            );
        } else if let Some(error) = &self.environment_error {
            ui.label(lang.environment_failed());
            ui.collapsing(lang.technical_details(), |ui| {
                ui.label(error);
            });
            if ui.button(lang.refresh()).clicked() {
                self.environment_error = None;
            }
        }
    }

    fn add_project(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        self.page_toolbar(ui, lang.add_title(), lang.add_sub(), false);
        ui.add_space(22.0);
        if !self.daemon_online {
            callout(ui, DANGER, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(if self.connection == Connection::Checking {
                            lang.checking_status()
                        } else if self.connection == Connection::Stopped {
                            lang.stopped_title()
                        } else {
                            lang.daemon_unavailable()
                        })
                        .strong(),
                    );
                    ui.label(lang.stopped_add_body());
                });
            });
            return;
        }
        let mut submit = false;
        self.project_source_form(ui);
        ui.add_space(14.0);
        self.project_services_form(ui);
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if ui.add(secondary_button(lang.cancel())).clicked() {
                self.form = AddProjectForm::default();
                self.view = View::Projects;
            }
            submit = ui
                .add_enabled(self.pending == 0, primary_button(lang.create_project()))
                .clicked();
            if self.pending > 0 {
                ui.spinner();
                ui.label(RichText::new(lang.operation_busy()).color(INK_MUTED));
            }
        });
        if submit {
            match self.form.to_request(lang) {
                Ok(request) => self.request(Action::AddProject, Request::ProjectAdd(request)),
                Err(error) => self.form.error = Some(error),
            }
        }
        if let Some(error) = &self.form.error {
            ui.colored_label(DANGER, format!("{}: {error}", lang.check_form()));
        }
    }

    fn project_source_form(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let card_width = (ui.available_width() - 40.0).max(300.0);
        card().show(ui, |ui| {
            ui.set_min_width(card_width);
            let field_width = (ui.available_width() - 290.0).clamp(180.0, 380.0);
            section_heading(ui, lang.source_title(), lang.source_body());
            ui.add_space(12.0);
            egui::Grid::new("add_project_form")
                .num_columns(2)
                .spacing([22.0, 13.0])
                .min_col_width(138.0)
                .show(ui, |ui| {
                    let label = form_label(ui, lang.path_label());
                    ui.horizontal(|ui| {
                        let response = ui
                            .add(
                                egui::TextEdit::singleline(&mut self.form.path)
                                    .desired_width(field_width)
                                    .hint_text("~/Code/acme"),
                            )
                            .labelled_by(label.id);
                        if response.lost_focus() {
                            let path = self.form.source_path();
                            if path.is_dir() {
                                self.form.detect_path(&path, &self.domain_suffix());
                            }
                        }
                        if ui.add(secondary_button(lang.browse())).clicked() {
                            self.picker = Some(DirectoryPicker::new(&self.form.path));
                        }
                    });
                    ui.end_row();

                    let label = form_label(ui, lang.name_label());
                    ui.add(
                        egui::TextEdit::singleline(&mut self.form.name).desired_width(field_width),
                    )
                    .labelled_by(label.id);
                    ui.end_row();

                    let label = form_label(ui, lang.host_label());
                    ui.add(
                        egui::TextEdit::singleline(&mut self.form.hostname)
                            .desired_width(field_width),
                    )
                    .labelled_by(label.id);
                    ui.end_row();

                    let label = form_label(ui, Lang::docroot_label());
                    ui.add(
                        egui::TextEdit::singleline(&mut self.form.document_root)
                            .desired_width(field_width),
                    )
                    .labelled_by(label.id);
                    ui.end_row();

                    let label = form_label(ui, lang.profile_label());
                    egui::ComboBox::from_id_salt("profile")
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
        card().show(ui, |ui| {
            ui.set_min_width(card_width);
            let field_width = (ui.available_width() - 290.0).clamp(180.0, 380.0);
            section_heading(ui, lang.services_title(), lang.services_body());
            ui.add_space(12.0);
            egui::Grid::new("add_project_services")
                .num_columns(2)
                .spacing([22.0, 13.0])
                .min_col_width(138.0)
                .show(ui, |ui| {
                    self.basic_service_fields(ui);
                    self.supervisor_fields(ui, field_width);
                    self.cron_fields(ui, field_width);

                    let label = form_label(ui, lang.autostart_label());
                    ui.checkbox(&mut self.form.autostart, lang.autostart_check())
                        .labelled_by(label.id);
                    ui.end_row();
                });
        });
    }

    fn basic_service_fields(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let label = form_label(ui, lang.web_server_label());
        egui::ComboBox::from_id_salt("web_server")
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

        let _ = form_label(ui, lang.runtime_note_label());
        ui.label(lang.php_versions_note());
        ui.end_row();

        let label = form_label(ui, "Node.js");
        egui::ComboBox::from_id_salt("node_version")
            .selected_text(format!("Node.js {}", self.form.node))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.form.node, "24".into(), "Node.js 24 LTS");
                ui.selectable_value(&mut self.form.node, "22".into(), "Node.js 22 LTS");
                ui.selectable_value(&mut self.form.node, "20".into(), "Node.js 20 (EOL)");
            })
            .response
            .labelled_by(label.id);
        ui.end_row();

        let _ = form_label(ui, lang.runtime_note_label());
        ui.label(lang.node_value());
        ui.end_row();

        let label = form_label(ui, "Database");
        egui::ComboBox::from_id_salt("database")
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

        let label = form_label(ui, "Cache");
        egui::ComboBox::from_id_salt("cache")
            .selected_text(lang.cache_name(self.form.cache))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.form.cache, Cache::None, lang.cache_none());
                ui.selectable_value(&mut self.form.cache, Cache::Redis, "Redis");
            })
            .response
            .labelled_by(label.id);
        ui.end_row();

        let _ = form_label(ui, "Composer");
        ui.label(lang.composer_value());
        ui.end_row();

        let _ = form_label(ui, lang.mail_title());
        ui.checkbox(&mut self.form.mail, lang.mail_check());
        ui.end_row();
    }

    fn supervisor_fields(&mut self, ui: &mut egui::Ui, field_width: f32) {
        let lang = self.lang;
        let _ = form_label(ui, lang.supervisor_title());
        ui.checkbox(&mut self.form.supervisor, lang.supervisor_check());
        ui.end_row();
        if !self.form.supervisor {
            return;
        }

        if self.form.has_worker_preset() {
            let _ = form_label(ui, lang.supervisor_preset_label());
            ui.vertical(|ui| {
                soft_badge(
                    ui,
                    if self.form.profile == Profile::Laravel {
                        "Laravel queue:work"
                    } else {
                        "Symfony messenger:consume async"
                    },
                );
                ui.checkbox(&mut self.form.supervisor_custom, lang.supervisor_custom());
            });
            ui.end_row();
        }

        if !self.form.has_worker_preset() || self.form.supervisor_custom {
            self.custom_supervisor_fields(ui, field_width);
        }

        let _ = form_label(ui, lang.supervisor_restart());
        ui.label(lang.supervisor_restart_body());
        ui.end_row();
    }

    fn custom_supervisor_fields(&mut self, ui: &mut egui::Ui, field_width: f32) {
        let lang = self.lang;
        let label = form_label(ui, lang.supervisor_executable());
        ui.add(
            egui::TextEdit::singleline(&mut self.form.supervisor_command)
                .desired_width(field_width)
                .hint_text(lang.supervisor_executable_hint()),
        )
        .labelled_by(label.id);
        ui.end_row();

        let label = form_label(ui, lang.supervisor_args());
        ui.vertical(|ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.form.supervisor_args)
                    .desired_width(field_width)
                    .desired_rows(4)
                    .hint_text(lang.supervisor_args_hint()),
            )
            .labelled_by(label.id);
            ui.label(
                RichText::new(lang.supervisor_args_note())
                    .size(11.0)
                    .color(INK_MUTED),
            );
        });
        ui.end_row();
    }

    fn cron_fields(&mut self, ui: &mut egui::Ui, field_width: f32) {
        let lang = self.lang;
        let _ = form_label(ui, lang.cron_title());
        ui.checkbox(&mut self.form.cron, lang.cron_check());
        ui.end_row();
        if !self.form.cron {
            return;
        }
        let label = form_label(ui, lang.cron_lines());
        ui.vertical(|ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.form.cron_lines)
                    .desired_width(field_width)
                    .desired_rows(3)
                    .hint_text("*/5 * * * * @php artisan schedule:run"),
            )
            .labelled_by(label.id);
            ui.label(RichText::new(lang.cron_note()).size(11.0).color(INK_MUTED));
        });
        ui.end_row();
    }

    fn remove_dialog(&mut self, context: &egui::Context) {
        let lang = self.lang;
        let Some(name) = self.remove_confirmation.clone() else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::new("remove_project")).show(context, |ui| {
            ui.heading(lang.remove_title());
            ui.label(lang.remove_body(&name));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(lang.cancel()).clicked() {
                    self.remove_confirmation = None;
                }
                let remove =
                    egui::Button::new(RichText::new(lang.remove_confirm()).color(Color32::WHITE))
                        .fill(DANGER);
                if ui.add_enabled(self.pending == 0, remove).clicked() {
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

    fn setup_complete(&self) -> bool {
        self.paths
            .as_ref()
            .is_some_and(|paths| paths.config.join("config.toml").is_file())
    }

    fn domain_suffix(&self) -> String {
        self.paths
            .as_ref()
            .filter(|paths| paths.config.join("config.toml").is_file())
            .and_then(|paths| GlobalConfig::load_or_create(paths).ok())
            .map_or_else(
                || GlobalConfig::default().general.domain_suffix,
                |config| config.general.domain_suffix,
            )
    }
}

impl eframe::App for LocalWebApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.receive_events();
        if self.pending == 0
            && self.background_pending == 0
            && self.last_refresh.elapsed() > Duration::from_secs(3)
        {
            if self.daemon_online {
                self.request(Action::RefreshStatus, Request::Status);
                self.request(Action::LoadProjects, Request::ProjectList);
            } else {
                self.request(Action::RefreshStatus, Request::Status);
            }
            self.last_refresh = Instant::now();
        }
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
                        if let Some(notice) = &self.notice {
                            notice.show(ui, self.lang);
                            ui.add_space(12.0);
                        }
                        match self.view {
                            View::Dashboard => self.dashboard(ui),
                            View::Projects => self.projects(ui),
                            View::AddProject => self.add_project(ui),
                        }
                        ui.add_space(24.0);
                    });
            });
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
        context.request_repaint_after(Duration::from_millis(250));
    }
}

#[allow(
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
            mail: self.mail,
            autostart: self.autostart,
        };
        localweb_core::Project::from_new(request.clone(), &GlobalConfig::default())
            .map_err(|error| error.to_string())?;
        Ok(request)
    }
}

struct DirectoryPicker {
    current: PathBuf,
    open: bool,
    error: Option<String>,
}

impl DirectoryPicker {
    fn new(current: &str) -> Self {
        let requested = PathBuf::from(current);
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let current = if requested.is_dir() { requested } else { home };
        Self {
            current,
            open: true,
            error: None,
        }
    }

    fn show(&mut self, context: &egui::Context, lang: Lang) -> Option<PathBuf> {
        let mut selected = None;
        let mut navigate = None;
        let mut request_close = false;
        let modal = egui::Modal::new(egui::Id::new("source_folder_picker")).show(context, |ui| {
            ui.set_width(context.viewport_rect().width().min(660.0) - 40.0);
            ui.heading(lang.picker_title());
            ui.horizontal_wrapped(|ui| {
                if ui.button(lang.picker_home()).clicked() {
                    navigate = std::env::var_os("HOME").map(PathBuf::from);
                }
                if ui.button(lang.picker_parent()).clicked()
                    && let Some(parent) = self.current.parent()
                {
                    navigate = Some(parent.to_path_buf());
                }
                ui.label(RichText::new(self.current.display().to_string()).monospace());
            });
            ui.separator();
            if let Some(error) = &self.error {
                ui.colored_label(DANGER, error);
            }
            egui::ScrollArea::vertical()
                .max_height(340.0)
                .show(ui, |ui| match child_directories(&self.current) {
                    Ok(directories) => {
                        self.error = None;
                        if directories.is_empty() {
                            ui.label(lang.picker_empty());
                        }
                        for directory in directories {
                            let name = directory.file_name().map_or_else(
                                || directory.display().to_string(),
                                |name| name.to_string_lossy().into_owned(),
                            );
                            if ui
                                .add_sized(
                                    [ui.available_width(), 36.0],
                                    egui::Button::new(lang.picker_open(&name))
                                        .fill(SURFACE)
                                        .stroke(Stroke::new(1.0, BORDER)),
                                )
                                .clicked()
                            {
                                navigate = Some(directory);
                            }
                        }
                    }
                    Err(error) => self.error = Some(error),
                });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(lang.cancel()).clicked() {
                    request_close = true;
                }
                let choose =
                    egui::Button::new(RichText::new(lang.picker_use()).color(Color32::WHITE))
                        .fill(ACCENT);
                if ui.add(choose).clicked() {
                    selected = Some(self.current.clone());
                    request_close = true;
                }
            });
        });
        if let Some(directory) = navigate {
            self.current = directory;
        }
        if request_close || modal.should_close() {
            self.open = false;
        }
        selected
    }
}

struct Notice {
    text: String,
    error: bool,
}

impl Notice {
    fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: false,
        }
    }

    fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: true,
        }
    }

    fn show(&self, ui: &mut egui::Ui, lang: Lang) {
        let color = if self.error { DANGER } else { SUCCESS };
        egui::Frame::group(ui.style()).show(ui, |ui| {
            if self.error {
                ui.colored_label(color, lang.operation_failed());
                ui.collapsing(lang.technical_details(), |ui| {
                    ui.label(&self.text);
                });
            } else {
                ui.colored_label(color, &self.text);
            }
        });
    }
}

fn configure_style(context: &egui::Context) {
    context.set_theme(egui::Theme::Light);
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = CANVAS;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = SURFACE;
    visuals.faint_bg_color = SURFACE_MUTED;
    visuals.selection.bg_fill = ACCENT_SOFT;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.hovered.bg_fill = ACCENT_SOFT;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.active.bg_fill = ACCENT_SOFT;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, INK);
    visuals.widgets.active.bg_stroke = Stroke::new(2.0, Color32::from_rgb(16, 28, 22));
    visuals.window_corner_radius = 12.0.into();
    context.set_visuals(visuals);
    context.style_mut_of(egui::Theme::Light, |style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.spacing.interact_size.y = 36.0;
        style.visuals.override_text_color = Some(INK);
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(25.0));
    });
}

fn nav_button(ui: &mut egui::Ui, current: &mut View, target: View, label: &str) {
    let selected = *current == target;
    if ui
        .add_sized(
            [ui.available_width(), 42.0],
            egui::Button::new(RichText::new(label).size(15.0).color(if selected {
                Color32::WHITE
            } else {
                NAV_MUTED
            }))
            .selected(selected)
            .fill(if selected {
                ACCENT
            } else {
                Color32::TRANSPARENT
            })
            .corner_radius(8.0),
        )
        .clicked()
    {
        *current = target;
    }
}

fn page_header(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.vertical(|ui| {
        ui.heading(RichText::new(title).size(32.0).strong().color(INK));
        ui.label(RichText::new(description).size(15.0).color(INK_MUTED));
    });
}

fn card() -> egui::Frame {
    egui::Frame::NONE
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::same(18))
}

fn primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).strong().color(Color32::WHITE))
        .fill(ACCENT)
        .corner_radius(8.0)
        .min_size(Vec2::new(132.0, 40.0))
}

fn compact_primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).strong().color(Color32::WHITE))
        .fill(ACCENT)
        .corner_radius(7.0)
}

fn secondary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(INK))
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(8.0)
        .min_size(Vec2::new(104.0, 36.0))
}

fn compact_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(INK))
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(7.0)
}

fn environment_banner(
    ui: &mut egui::Ui,
    lang: Lang,
    state: EnvironmentState,
    active_projects: usize,
    services: impl FnOnce(&mut egui::Ui),
) {
    let ready = state == EnvironmentState::Ready;
    let tint = if ready { ACCENT_SOFT } else { SURFACE_MUTED };
    egui::Frame::NONE
        .fill(tint)
        .stroke(Stroke::new(
            1.0,
            if ready {
                Color32::from_rgb(181, 220, 199)
            } else {
                BORDER
            },
        ))
        .corner_radius(11.0)
        .inner_margin(egui::Margin::symmetric(22, 18))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                status_mark(ui, ready);
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(match state {
                            EnvironmentState::Checking => lang.checking_status(),
                            EnvironmentState::Ready => lang.env_on(),
                            EnvironmentState::GatewayStopped => lang.gateway_stopped(),
                            EnvironmentState::Stopped => lang.env_off(),
                            EnvironmentState::Unavailable => lang.daemon_unavailable(),
                        })
                        .size(22.0)
                        .strong()
                        .color(INK),
                    );
                    ui.label(
                        RichText::new(if state == EnvironmentState::Checking {
                            lang.loading_projects()
                        } else if ready {
                            lang.env_on_body()
                        } else if state == EnvironmentState::GatewayStopped {
                            lang.gateway_stopped_body()
                        } else {
                            lang.env_off_body()
                        })
                        .color(INK_MUTED),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(lang.active_projects())
                                .size(12.0)
                                .color(INK_MUTED),
                        );
                        ui.label(
                            RichText::new(active_projects.to_string())
                                .size(24.0)
                                .strong()
                                .color(INK),
                        );
                    });
                });
            });
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);
            services(ui);
        });
}

fn service_card(ui: &mut egui::Ui, title: &str, value: &str, active: bool) {
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_min_size(Vec2::new(118.0, 48.0));
            ui.horizontal(|ui| {
                status_dot(ui, active);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).strong().color(INK));
                    ui.label(RichText::new(value).size(12.0).color(INK_MUTED));
                });
            });
        });
}

fn project_ledger_header(ui: &mut egui::Ui, lang: Lang) {
    egui::Frame::NONE
        .fill(SURFACE_MUTED)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius {
            nw: 9,
            ne: 9,
            sw: 0,
            se: 0,
        })
        .inner_margin(egui::Margin::symmetric(14, 9))
        .show(ui, |ui| {
            ui.set_min_width(730.0);
            ui.horizontal(|ui| {
                header_cell(ui, lang.col_project(), 150.0);
                header_cell(ui, lang.col_domain(), 184.0);
                header_cell(ui, lang.col_services(), 132.0);
                header_cell(ui, lang.col_state(), 92.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(lang.col_actions())
                            .size(12.0)
                            .strong()
                            .color(INK_MUTED),
                    );
                });
            });
        });
}

fn header_cell(ui: &mut egui::Ui, label: &str, width: f32) {
    ui.allocate_ui(Vec2::new(width, 18.0), |ui| {
        ui.label(RichText::new(label).size(12.0).strong().color(INK_MUTED));
    });
}

fn empty_projects(ui: &mut egui::Ui, lang: Lang, daemon_online: bool) {
    let content_width = (ui.available_width() - 40.0).max(240.0);
    card().show(ui, |ui| {
        ui.set_min_size(Vec2::new(content_width, 110.0));
        ui.vertical_centered(|ui| {
            ui.add_space(13.0);
            ui.label(RichText::new(lang.empty_title()).size(18.0).strong());
            ui.label(
                RichText::new(if daemon_online {
                    lang.empty_online()
                } else {
                    lang.empty_offline()
                })
                .color(INK_MUTED),
            );
        });
    });
}

fn callout(ui: &mut egui::Ui, color: Color32, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, color))
        .corner_radius(9.0)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(4.0, 34.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, color);
                ui.add_space(6.0);
                content(ui);
            });
        });
}

fn section_heading(ui: &mut egui::Ui, title: &str, description: &str) {
    ui.label(RichText::new(title).size(19.0).strong().color(INK));
    ui.label(RichText::new(description).size(13.0).color(INK_MUTED));
}

fn form_label(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.label(RichText::new(label).strong().color(INK))
}

fn soft_badge(ui: &mut egui::Ui, label: &str) {
    egui::Frame::NONE
        .fill(ACCENT_SOFT)
        .corner_radius(12.0)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(12.0).strong().color(ACCENT));
        });
}

fn status_dot(ui: &mut egui::Ui, active: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
    ui.painter().circle_filled(
        rect.center(),
        5.0,
        if active {
            SUCCESS
        } else {
            Color32::from_rgb(154, 162, 163)
        },
    );
}

fn status_mark(ui: &mut egui::Ui, active: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.0), egui::Sense::hover());
    let color = if active {
        SUCCESS
    } else {
        Color32::from_rgb(154, 162, 163)
    };
    ui.painter().circle_filled(rect.center(), 24.0, color);
    if active {
        ui.painter().line_segment(
            [
                rect.center() + egui::vec2(-10.0, 0.0),
                rect.center() + egui::vec2(-3.0, 8.0),
            ],
            Stroke::new(4.0, Color32::WHITE),
        );
        ui.painter().line_segment(
            [
                rect.center() + egui::vec2(-3.0, 8.0),
                rect.center() + egui::vec2(12.0, -9.0),
            ],
            Stroke::new(4.0, Color32::WHITE),
        );
    } else {
        ui.painter().line_segment(
            [
                rect.center() + egui::vec2(-10.0, 0.0),
                rect.center() + egui::vec2(10.0, 0.0),
            ],
            Stroke::new(4.0, Color32::WHITE),
        );
    }
}

fn brand_mark(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    ui.painter()
        .circle_stroke(rect.center(), size * 0.42, Stroke::new(3.0, Color32::WHITE));
    ui.painter().circle_stroke(
        rect.center(),
        size * 0.32,
        Stroke::new(3.0, Color32::from_rgb(62, 222, 156)),
    );
}

fn search_mark(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
    let center = rect.center() + egui::vec2(-2.0, -2.0);
    ui.painter()
        .circle_stroke(center, 5.5, Stroke::new(1.6, INK_MUTED));
    ui.painter().line_segment(
        [center + egui::vec2(4.0, 4.0), center + egui::vec2(9.0, 9.0)],
        Stroke::new(1.6, INK_MUTED),
    );
}

fn state_badge(ui: &mut egui::Ui, running: bool, lang: Lang) {
    ui.horizontal(|ui| {
        status_dot(ui, running);
        ui.label(
            RichText::new(if running {
                lang.badge_active()
            } else {
                lang.badge_stopped()
            })
            .size(12.0)
            .color(if running { SUCCESS } else { INK_MUTED }),
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

fn open_browser(hostname: &str, lang: Lang, notice: &mut Option<Notice>) {
    open_url(&format!("https://{hostname}"), lang, notice);
}

fn open_url(url: &str, lang: Lang, notice: &mut Option<Notice>) {
    if let Err(error) = Command::new("xdg-open").arg(url).spawn() {
        *notice = Some(Notice::error(lang.notice_browser(&error.to_string())));
    }
}

fn localweb_cli_path() -> Result<PathBuf, String> {
    let sibling = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .with_file_name("localweb");
    if sibling.is_file() {
        Ok(sibling)
    } else {
        Ok(PathBuf::from("localweb"))
    }
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
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

    #[test]
    fn primary_project_actions_fit_the_minimum_window_in_all_languages() {
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
        for lang in [Lang::English, Lang::Italiano, Lang::Shqip] {
            let ctx = egui::Context::default();
            configure_style(&ctx);
            let mut app = LocalWebApp::state(None, lang, None);
            app.connection = Connection::Connected;
            app.daemon_online = true;
            app.projects_loaded = true;
            app.projects.push(ProjectStatus {
                project: project.clone(),
                running: false,
                mail_web_url: None,
            });
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(840.0, 560.0));
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| {
                    app.sidebar(ui);
                    egui::CentralPanel::default().show(ui, |ui| {
                        app.projects(ui);
                    });
                },
            );
            let action = output
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape {
                        (text.galley.job.text == lang.action_start()).then_some(text)
                    } else {
                        None
                    }
                })
                .expect("Start action must be rendered");
            assert!(
                screen.contains_rect(egui::Rect::from_min_size(action.pos, action.galley.size())),
                "Start action must fit without horizontal scrolling"
            );
        }
        fs::remove_dir_all(path).unwrap();
    }
}
