//! Interface languages: English, Italiano, Shqip.
//!
//! Interface copy lives here; product names and technical details keep their original spelling. Language codes
//! `en`, `it`, and `sq` persist in a plain text file so no new
//! dependencies are needed.

use localweb_core::{Cache, Database, Profile};
use std::{fs, path::Path};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    English,
    #[default]
    Italiano,
    Shqip,
}

impl Lang {
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Italiano => "it",
            Self::Shqip => "sq",
        }
    }

    #[must_use]
    pub fn from_code(code: &str) -> Self {
        match code.trim() {
            "en" => Self::English,
            "sq" => Self::Shqip,
            _ => Self::Italiano,
        }
    }

    fn settings_file(config: &Path) -> std::path::PathBuf {
        config.join("gui-language")
    }

    /// Loads the saved language, defaulting to Italian.
    #[must_use]
    pub fn load(config: &Path) -> Self {
        fs::read_to_string(Self::settings_file(config))
            .map(|code| Self::from_code(&code))
            .unwrap_or_default()
    }

    /// Persists the language choice; failures are ignored silently.
    pub fn save(self, config: &Path) {
        if let Some(parent) = Self::settings_file(config).parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(Self::settings_file(config), self.code());
    }

    // Navigation rail.

    pub fn nav_dashboard(self) -> &'static str {
        match self {
            Self::English => "Overview",
            Self::Italiano => "Panoramica",
            Self::Shqip => "Përmbledhje",
        }
    }

    pub fn nav_projects(self) -> &'static str {
        match self {
            Self::English => "Projects",
            Self::Italiano => "Progetti",
            Self::Shqip => "Projektet",
        }
    }

    pub fn nav_add(self) -> &'static str {
        match self {
            Self::English => "Add project",
            Self::Italiano => "Aggiungi progetto",
            Self::Shqip => "Shto projekt",
        }
    }

    pub fn nav_tagline(self) -> &'static str {
        match self {
            Self::English => "Local development, under control.",
            Self::Italiano => "Sviluppo locale, sotto controllo.",
            Self::Shqip => "Zhvillim lokal, nën kontroll.",
        }
    }

    pub fn nav_close_note(self) -> &'static str {
        match self {
            Self::English => "Closing this window keeps sites running.",
            Self::Italiano => "Chiudendo questa finestra i siti restano attivi.",
            Self::Shqip => "Mbyllja e kësaj dritareje i lë faqet aktive.",
        }
    }

    pub fn nav_running(self) -> &'static str {
        match self {
            Self::English => "Daemon connected",
            Self::Italiano => "Daemon connesso",
            Self::Shqip => "Daemon-i i lidhur",
        }
    }

    pub fn nav_stopped(self) -> &'static str {
        match self {
            Self::English => "LocalWeb stopped",
            Self::Italiano => "LocalWeb fermo",
            Self::Shqip => "LocalWeb i ndaluar",
        }
    }

    // Global actions.

    pub fn stop_localweb(self) -> &'static str {
        match self {
            Self::English => "Stop LocalWeb",
            Self::Italiano => "Ferma LocalWeb",
            Self::Shqip => "Ndalo LocalWeb",
        }
    }

    pub fn op_stopping(self) -> &'static str {
        match self {
            Self::English => "Stopping…",
            Self::Italiano => "Arresto in corso",
            Self::Shqip => "Duke u ndaluar…",
        }
    }

    pub fn op_setup(self) -> &'static str {
        match self {
            Self::English => "Setting up…",
            Self::Italiano => "Configurazione in corso",
            Self::Shqip => "Duke u konfiguruar…",
        }
    }

    pub fn op_starting(self) -> &'static str {
        match self {
            Self::English => "Starting…",
            Self::Italiano => "Avvio in corso",
            Self::Shqip => "Duke u nisur…",
        }
    }

    pub fn start_localweb(self) -> &'static str {
        match self {
            Self::English => "Start LocalWeb",
            Self::Italiano => "Avvia LocalWeb",
            Self::Shqip => "Nis LocalWeb",
        }
    }

    pub fn configure_localweb(self) -> &'static str {
        match self {
            Self::English => "Set up LocalWeb",
            Self::Italiano => "Configura LocalWeb",
            Self::Shqip => "Konfiguro LocalWeb",
        }
    }

    // Dashboard.

    pub fn dash_title(self) -> &'static str {
        self.nav_dashboard()
    }

    pub fn dash_sub(self) -> &'static str {
        match self {
            Self::English => "LocalWeb status, services, and projects in one place.",
            Self::Italiano => "Stato di LocalWeb, servizi e progetti in un solo posto.",
            Self::Shqip => "Statusi i LocalWeb, shërbimet dhe projektet në një vend.",
        }
    }

    pub fn ca_callout_title(self) -> &'static str {
        match self {
            Self::English => "HTTPS needs one more step",
            Self::Italiano => "HTTPS richiede ancora un passaggio",
            Self::Shqip => "HTTPS kërkon ende një hap",
        }
    }

    pub fn ca_callout_body(self) -> &'static str {
        match self {
            Self::English => {
                "Trust the LocalWeb development certificate. Your system may ask for authorization."
            }
            Self::Italiano => {
                "Rendi attendibile il certificato di sviluppo LocalWeb. Il sistema può chiedere l'autorizzazione."
            }
            Self::Shqip => {
                "Beso certifikatën e zhvillimit LocalWeb. Sistemi mund të kërkojë autorizim."
            }
        }
    }

    pub fn install_ca(self) -> &'static str {
        match self {
            Self::English => "Install CA",
            Self::Italiano => "Installa CA",
            Self::Shqip => "Instalo CA",
        }
    }

    // Environment overview.

    pub fn env_on(self) -> &'static str {
        match self {
            Self::English => "Environment ready",
            Self::Italiano => "Ambiente operativo",
            Self::Shqip => "Mjedisi gati",
        }
    }

    pub fn env_off(self) -> &'static str {
        match self {
            Self::English => "Environment stopped",
            Self::Italiano => "Ambiente fermo",
            Self::Shqip => "Mjedisi i ndaluar",
        }
    }

    pub fn env_on_body(self) -> &'static str {
        match self {
            Self::English => "Daemon connected; HTTPS gateway running.",
            Self::Italiano => "Daemon connesso; gateway HTTPS in esecuzione.",
            Self::Shqip => "Daemon-i i lidhur; gateway HTTPS në ekzekutim.",
        }
    }

    pub fn env_off_body(self) -> &'static str {
        match self {
            Self::English => "Start LocalWeb to make projects available.",
            Self::Italiano => "Avvia LocalWeb per rendere disponibili i progetti.",
            Self::Shqip => "Nis LocalWeb për t'i bërë projektet të disponueshme.",
        }
    }

    pub fn active_projects(self) -> &'static str {
        match self {
            Self::English => "Active projects",
            Self::Italiano => "Progetti attivi",
            Self::Shqip => "Projekte aktive",
        }
    }

    pub fn svc_daemon() -> &'static str {
        "Daemon"
    }

    pub fn svc_gateway() -> &'static str {
        "Gateway"
    }

    pub fn svc_ca() -> &'static str {
        "CA HTTPS"
    }

    pub fn svc_projects(self) -> &'static str {
        match self {
            Self::English => "Projects",
            Self::Italiano => "Progetti",
            Self::Shqip => "Projektet",
        }
    }

    pub fn val_running(self) -> &'static str {
        match self {
            Self::English => "Running",
            Self::Italiano => "In esecuzione",
            Self::Shqip => "Në ekzekutim",
        }
    }

    pub fn val_stopped(self) -> &'static str {
        match self {
            Self::English => "Stopped",
            Self::Italiano => "Fermo",
            Self::Shqip => "I ndaluar",
        }
    }

    pub fn gateway_on(self) -> &'static str {
        match self {
            Self::English | Self::Italiano | Self::Shqip => "HTTP + HTTPS",
        }
    }

    pub fn projects_summary(self, active: usize, total: usize) -> String {
        match self {
            Self::English => format!("{active} of {total} active"),
            Self::Italiano => format!("{active} attivi su {total}"),
            Self::Shqip => format!("{active} aktive nga {total}"),
        }
    }

    pub fn supervised_summary(self, count: usize) -> String {
        match self {
            Self::English => format!("{count} configured"),
            Self::Italiano => format!("{count} configurati"),
            Self::Shqip => format!("{count} të konfiguruar"),
        }
    }

    pub fn ca_trusted(self) -> &'static str {
        match self {
            Self::English => "Trusted",
            Self::Italiano => "Attendibile",
            Self::Shqip => "E besuar",
        }
    }

    pub fn ca_pending(self) -> &'static str {
        match self {
            Self::English => "To install",
            Self::Italiano => "Da installare",
            Self::Shqip => "Për t'u instaluar",
        }
    }

    pub fn ca_missing(self) -> &'static str {
        match self {
            Self::English => "To create",
            Self::Italiano => "Da creare",
            Self::Shqip => "Për t'u krijuar",
        }
    }

    // Projects view.

    pub fn projects_title(self) -> &'static str {
        self.nav_projects()
    }

    pub fn projects_sub(self) -> &'static str {
        match self {
            Self::English => "Start and stop each site independently.",
            Self::Italiano => "Avvia e ferma ogni sito in modo indipendente.",
            Self::Shqip => "Nis dhe ndalo secilën faqe në mënyrë të pavarur.",
        }
    }

    pub fn stopped_title(self) -> &'static str {
        match self {
            Self::English => "LocalWeb is stopped",
            Self::Italiano => "LocalWeb è fermo",
            Self::Shqip => "LocalWeb është i ndaluar",
        }
    }

    pub fn stopped_body(self) -> &'static str {
        match self {
            Self::English => {
                "Connect to LocalWeb to manage projects. Start it or refresh the status."
            }
            Self::Italiano => {
                "Connettiti a LocalWeb per gestire i progetti. Avvialo o aggiorna lo stato."
            }
            Self::Shqip => {
                "Lidhu me LocalWeb për të menaxhuar projektet. Nise ose rifresko statusin."
            }
        }
    }

    pub fn stopped_add_body(self) -> &'static str {
        match self {
            Self::English => "Start the daemon before adding a project.",
            Self::Italiano => "Avvia il daemon prima di aggiungere un progetto.",
            Self::Shqip => "Nis daemon-in para se të shtosh projektin.",
        }
    }

    pub fn search_hint(self) -> &'static str {
        match self {
            Self::English => "Search by name, domain, or path",
            Self::Italiano => "Cerca per nome, dominio o percorso",
            Self::Shqip => "Kërko sipas emrit, domain-it ose shtegut",
        }
    }

    pub fn configured_count(self, total: usize) -> String {
        match self {
            Self::English => format!("{total} configured"),
            Self::Italiano => format!("{total} configurati"),
            Self::Shqip => format!("{total} të konfiguruar"),
        }
    }

    pub fn col_project(self) -> &'static str {
        match self {
            Self::English => "Project",
            Self::Italiano => "Progetto",
            Self::Shqip => "Projekti",
        }
    }

    pub fn col_domain(self) -> &'static str {
        match self {
            Self::English => "Domain and path",
            Self::Italiano => "Dominio e percorso",
            Self::Shqip => "Domain-i dhe shtegu",
        }
    }

    pub fn col_services(self) -> &'static str {
        match self {
            Self::English => "Services",
            Self::Italiano => "Servizi",
            Self::Shqip => "Shërbimet",
        }
    }

    pub fn col_state(self) -> &'static str {
        match self {
            Self::English => "Status",
            Self::Italiano => "Stato",
            Self::Shqip => "Statusi",
        }
    }

    pub fn col_actions(self) -> &'static str {
        match self {
            Self::English => "Actions",
            Self::Italiano => "Azioni",
            Self::Shqip => "Veprimet",
        }
    }

    pub fn empty_title(self) -> &'static str {
        match self {
            Self::English => "No projects yet",
            Self::Italiano => "Nessun progetto configurato",
            Self::Shqip => "Ende asnjë projekt",
        }
    }

    pub fn empty_online(self) -> &'static str {
        match self {
            Self::English => "Add a source folder to get started.",
            Self::Italiano => "Aggiungi una cartella di codice sorgente per iniziare.",
            Self::Shqip => "Shto një dosje burimore për të filluar.",
        }
    }

    pub fn empty_offline(self) -> &'static str {
        match self {
            Self::English => "Start LocalWeb, then add the first project.",
            Self::Italiano => "Avvia LocalWeb, quindi aggiungi il primo progetto.",
            Self::Shqip => "Nis LocalWeb, pastaj shto projektin e parë.",
        }
    }

    // Project ledger.

    pub fn transition_starting(self) -> &'static str {
        match self {
            Self::English => "Starting",
            Self::Italiano => "Avvio",
            Self::Shqip => "Duke u nisur",
        }
    }

    pub fn transition_stopping(self) -> &'static str {
        match self {
            Self::English => "Stopping",
            Self::Italiano => "Arresto",
            Self::Shqip => "Duke u ndaluar",
        }
    }

    pub fn transition_restarting(self) -> &'static str {
        match self {
            Self::English => "Restarting",
            Self::Italiano => "Riavvio",
            Self::Shqip => "Duke u rinisur",
        }
    }

    pub fn badge_active(self) -> &'static str {
        match self {
            Self::English => "Active",
            Self::Italiano => "Attivo",
            Self::Shqip => "Aktiv",
        }
    }

    pub fn badge_stopped(self) -> &'static str {
        match self {
            Self::English => "Stopped",
            Self::Italiano => "Fermo",
            Self::Shqip => "I ndaluar",
        }
    }

    pub fn action_stop(self) -> &'static str {
        match self {
            Self::English => "Stop",
            Self::Italiano => "Ferma",
            Self::Shqip => "Ndalo",
        }
    }

    pub fn action_restart(self) -> &'static str {
        match self {
            Self::English => "Restart",
            Self::Italiano => "Riavvia",
            Self::Shqip => "Rinise",
        }
    }

    pub fn action_open(self) -> &'static str {
        match self {
            Self::English => "Open",
            Self::Italiano => "Apri",
            Self::Shqip => "Hap",
        }
    }

    pub fn action_remove(self) -> &'static str {
        match self {
            Self::English => "Remove",
            Self::Italiano => "Rimuovi",
            Self::Shqip => "Hiq",
        }
    }

    pub fn action_start(self) -> &'static str {
        match self {
            Self::English => "Start",
            Self::Italiano => "Avvia",
            Self::Shqip => "Nis",
        }
    }

    pub fn supervisor_tag() -> &'static str {
        "Supervisor"
    }

    // Add-project view.

    pub fn add_title(self) -> &'static str {
        self.nav_add()
    }

    pub fn add_sub(self) -> &'static str {
        match self {
            Self::English => "Pick the source code and compose the local environment.",
            Self::Italiano => "Scegli il codice sorgente e componi l'ambiente locale.",
            Self::Shqip => "Zgjidh kodin burimor dhe ndërto mjedisin lokal.",
        }
    }

    pub fn cancel(self) -> &'static str {
        match self {
            Self::English => "Cancel",
            Self::Italiano => "Annulla",
            Self::Shqip => "Anulo",
        }
    }

    pub fn create_project(self) -> &'static str {
        match self {
            Self::English => "Add to LocalWeb",
            Self::Italiano => "Aggiungi a LocalWeb",
            Self::Shqip => "Shto në LocalWeb",
        }
    }

    pub fn operation_busy(self) -> &'static str {
        match self {
            Self::English => "Working…",
            Self::Italiano => "Operazione in corso",
            Self::Shqip => "Duke punuar…",
        }
    }

    pub fn source_title(self) -> &'static str {
        match self {
            Self::English => "Source code",
            Self::Italiano => "Codice sorgente",
            Self::Shqip => "Kodi burimor",
        }
    }

    pub fn source_body(self) -> &'static str {
        match self {
            Self::English => "The folder stays yours; LocalWeb never deletes it.",
            Self::Italiano => {
                "La cartella resta di tua proprietà e non verrà mai eliminata da LocalWeb."
            }
            Self::Shqip => "Dosja mbetet e jotja; LocalWeb nuk e fshin kurrë.",
        }
    }

    pub fn path_label(self) -> &'static str {
        match self {
            Self::English => "Source folder",
            Self::Italiano => "Cartella sorgente",
            Self::Shqip => "Dosja burimore",
        }
    }

    pub fn browse(self) -> &'static str {
        match self {
            Self::English => "Browse",
            Self::Italiano => "Sfoglia",
            Self::Shqip => "Shfleto",
        }
    }

    pub fn name_label(self) -> &'static str {
        match self {
            Self::English => "Name",
            Self::Italiano => "Nome",
            Self::Shqip => "Emri",
        }
    }

    pub fn host_label(self) -> &'static str {
        match self {
            Self::English => "Local domain",
            Self::Italiano => "Dominio locale",
            Self::Shqip => "Domain-i lokal",
        }
    }

    pub fn docroot_label() -> &'static str {
        "Document root"
    }

    pub fn profile_label(self) -> &'static str {
        match self {
            Self::English => "Application preset",
            Self::Italiano => "Preset applicazione",
            Self::Shqip => "Paracaktimi i aplikacionit",
        }
    }

    pub fn services_title(self) -> &'static str {
        self.text(
            "Runtime and services",
            "Runtime e servizi",
            "Runtime dhe shërbimet",
        )
    }

    pub fn services_body(self) -> &'static str {
        match self {
            Self::English => {
                "Choose the runtime and optional services. Web servers, PHP and databases are shared; Redis, Mailpit and workers are per project."
            }
            Self::Italiano => {
                "Scegli runtime e servizi opzionali. Web server, PHP e database sono condivisi; Redis, Mailpit e worker sono per progetto."
            }
            Self::Shqip => {
                "Zgjidh runtime dhe shërbimet opsionale. Web serverët, PHP dhe bazat e të dhënave ndahen; Redis, Mailpit dhe worker-at janë për projekt."
            }
        }
    }

    pub fn autostart_label(self) -> &'static str {
        match self {
            Self::English => "Autostart",
            Self::Italiano => "Avvio automatico",
            Self::Shqip => "Nisja automatike",
        }
    }

    pub fn autostart_check(self) -> &'static str {
        match self {
            Self::English => "Start on the next LocalWeb launch",
            Self::Italiano => "Avvia al prossimo avvio di LocalWeb",
            Self::Shqip => "Nise në nisjen tjetër të LocalWeb",
        }
    }

    pub fn db_none(self) -> &'static str {
        match self {
            Self::English => "None",
            Self::Italiano => "Nessuno",
            Self::Shqip => "Asnjë",
        }
    }

    pub fn cache_none(self) -> &'static str {
        match self {
            Self::English => "None",
            Self::Italiano => "Nessuna",
            Self::Shqip => "Asnjë",
        }
    }

    pub fn composer_value(self) -> &'static str {
        match self {
            Self::English => "Composer 2 included",
            Self::Italiano => "Composer 2 incluso",
            Self::Shqip => "Composer 2 i përfshirë",
        }
    }

    pub fn node_value(self) -> &'static str {
        match self {
            Self::English => {
                "24 includes npm/npx. 22 and 20 require matching system Node.js and npm; Node.js 20 is EOL."
            }
            Self::Italiano => {
                "24 include npm/npx. 22 e 20 richiedono Node.js e npm di sistema corrispondenti; Node.js 20 è EOL."
            }
            Self::Shqip => {
                "24 përfshin npm/npx. 22 dhe 20 kërkojnë Node.js dhe npm përkatës në sistem; Node.js 20 është EOL."
            }
        }
    }

    pub fn supervisor_title(self) -> &'static str {
        match self {
            Self::English => "Background worker",
            Self::Italiano => "Worker in background",
            Self::Shqip => "Worker në sfond",
        }
    }

    pub fn supervisor_check(self) -> &'static str {
        match self {
            Self::English => "Enable Supervisor for this project",
            Self::Italiano => "Attiva Supervisor per questo progetto",
            Self::Shqip => "Aktivizo Supervisor për këtë projekt",
        }
    }

    pub fn supervisor_preset_label(self) -> &'static str {
        match self {
            Self::English => "Preset",
            Self::Italiano => "Configurazione",
            Self::Shqip => "Paracaktimi",
        }
    }

    pub fn supervisor_custom(self) -> &'static str {
        match self {
            Self::English => "Use a custom command",
            Self::Italiano => "Usa un comando personalizzato",
            Self::Shqip => "Përdor komandë të personalizuar",
        }
    }

    pub fn supervisor_restart(self) -> &'static str {
        match self {
            Self::English => "Automatic restart",
            Self::Italiano => "Riavvio automatico",
            Self::Shqip => "Rinisje automatike",
        }
    }

    pub fn supervisor_restart_body(self) -> &'static str {
        match self {
            Self::English => "Restarts the worker after it exits",
            Self::Italiano => "Riavvia il worker quando termina",
            Self::Shqip => "Rinis worker-in pasi përfundon",
        }
    }

    pub fn supervisor_executable(self) -> &'static str {
        match self {
            Self::English => "Worker executable",
            Self::Italiano => "Eseguibile worker",
            Self::Shqip => "Ekzekutuesi i worker-it",
        }
    }

    pub fn supervisor_executable_hint(self) -> &'static str {
        match self {
            Self::English => "@php or executable path",
            Self::Italiano => "@php oppure percorso eseguibile",
            Self::Shqip => "@php ose shtegu i ekzekutuesit",
        }
    }

    pub fn supervisor_args(self) -> &'static str {
        match self {
            Self::English => "Worker arguments",
            Self::Italiano => "Argomenti worker",
            Self::Shqip => "Argumentet e worker-it",
        }
    }

    pub fn supervisor_args_hint(self) -> &'static str {
        match self {
            Self::English => "One argument per line",
            Self::Italiano => "Un argomento per riga",
            Self::Shqip => "Një argument për rresht",
        }
    }

    pub fn supervisor_args_note(self) -> &'static str {
        match self {
            Self::English => "Each line is a separate argument. No shell is used.",
            Self::Italiano => "Ogni riga è un argomento separato. Non viene usata una shell.",
            Self::Shqip => "Secila rresht është argument më vete. Nuk përdoret shell.",
        }
    }

    pub fn cron_title(self) -> &'static str {
        match self {
            Self::English => "Scheduled tasks",
            Self::Italiano => "Attività pianificate",
            Self::Shqip => "Detyra të planifikuara",
        }
    }

    pub fn cron_check(self) -> &'static str {
        match self {
            Self::English => "Run scheduled tasks with the project",
            Self::Italiano => "Esegui attività pianificate con il progetto",
            Self::Shqip => "Ekzekuto detyrat e planifikuara me projektin",
        }
    }

    pub fn cron_lines(self) -> &'static str {
        match self {
            Self::English => "Schedules",
            Self::Italiano => "Pianificazioni",
            Self::Shqip => "Planifikimet",
        }
    }

    pub fn cron_note(self) -> &'static str {
        match self {
            Self::English => {
                "One per line: minutes hours day month weekday + command. @php is the project PHP. Empty uses the profile default where one exists."
            }
            Self::Italiano => {
                "Una per riga: minuti ore giorno mese settimana + comando. @php indica il PHP del progetto. Se vuota, vale la pianificazione del profilo dove prevista."
            }
            Self::Shqip => {
                "Një për rresht: minuta orë ditë muaj ditë-jave + komanda. @php është PHP e projektit. Bosh përdor paracaktimin e profilit ku ekziston."
            }
        }
    }

    pub fn mail_title(self) -> &'static str {
        match self {
            Self::English => "Dev email",
            Self::Italiano => "Email di sviluppo",
            Self::Shqip => "Email zhvillimi",
        }
    }

    pub fn mail_check(self) -> &'static str {
        match self {
            Self::English => "Enable Mailpit for this project",
            Self::Italiano => "Attiva Mailpit per questo progetto",
            Self::Shqip => "Aktivizo Mailpit për këtë projekt",
        }
    }

    // Validation errors.

    pub fn error_no_path(self) -> &'static str {
        match self {
            Self::English => "Select the source folder",
            Self::Italiano => "Seleziona la cartella del codice sorgente",
            Self::Shqip => "Zgjidh dosjen burimore",
        }
    }

    pub fn error_no_executable(self) -> &'static str {
        match self {
            Self::English => "Provide the supervisor worker executable",
            Self::Italiano => "Indica l'eseguibile del worker supervisor",
            Self::Shqip => "Jep ekzekutuesin e worker-it supervisor",
        }
    }

    // Folder picker.

    pub fn picker_title(self) -> &'static str {
        match self {
            Self::English => "Select the source folder",
            Self::Italiano => "Seleziona la cartella sorgente",
            Self::Shqip => "Zgjidh dosjen burimore",
        }
    }

    pub fn picker_parent(self) -> &'static str {
        match self {
            Self::English => "Parent folder",
            Self::Italiano => "Cartella superiore",
            Self::Shqip => "Dosja prind",
        }
    }

    pub fn picker_home(self) -> &'static str {
        self.text("Home", "Home", "Home")
    }
    pub fn picker_empty(self) -> &'static str {
        self.text(
            "No subfolders. You can select this folder.",
            "Nessuna sottocartella. Puoi selezionare questa cartella.",
            "Asnjë nëndosje. Mund të zgjedhësh këtë dosje.",
        )
    }

    pub fn picker_open(self, name: &str) -> String {
        match self {
            Self::English => format!("Open folder: {name}"),
            Self::Italiano => format!("Apri cartella: {name}"),
            Self::Shqip => format!("Hap dosjen: {name}"),
        }
    }

    pub fn picker_use(self) -> &'static str {
        match self {
            Self::English => "Use this folder",
            Self::Italiano => "Usa questa cartella",
            Self::Shqip => "Përdor këtë dosje",
        }
    }

    // Remove dialog.

    pub fn remove_title(self) -> &'static str {
        match self {
            Self::English => "Remove project",
            Self::Italiano => "Rimuovi progetto",
            Self::Shqip => "Hiq projektin",
        }
    }

    pub fn remove_body(self, name: &str) -> String {
        match self {
            Self::English => {
                format!(
                    "Remove {name} from LocalWeb? Its source folder and database data are kept. The local domain and project configuration are removed."
                )
            }
            Self::Italiano => {
                format!(
                    "Rimuovere {name} da LocalWeb? Cartella sorgente e dati del database restano. Il dominio locale e la configurazione del progetto vengono rimossi."
                )
            }
            Self::Shqip => {
                format!(
                    "Të hiqet {name} nga LocalWeb? Dosja burimore dhe të dhënat ruhen. Domain-i lokal dhe konfigurimi hiqen."
                )
            }
        }
    }

    pub fn remove_confirm(self) -> &'static str {
        match self {
            Self::English => "Remove configuration",
            Self::Italiano => "Rimuovi configurazione",
            Self::Shqip => "Hiq konfigurimin",
        }
    }

    // Notices.

    pub fn notice_setup(self) -> &'static str {
        match self {
            Self::English => "Initial setup complete",
            Self::Italiano => "Configurazione iniziale completata",
            Self::Shqip => "Konfigurimi fillestar u përfundua",
        }
    }

    pub fn notice_ca(self) -> &'static str {
        match self {
            Self::English => "HTTPS CA installed on the system",
            Self::Italiano => "CA HTTPS installata nel sistema",
            Self::Shqip => "CA HTTPS u instalua në sistem",
        }
    }

    pub fn notice_started(self) -> &'static str {
        match self {
            Self::English => "LocalWeb started",
            Self::Italiano => "LocalWeb è stato avviato",
            Self::Shqip => "LocalWeb u nis",
        }
    }

    pub fn notice_hosts(self) -> &'static str {
        match self {
            Self::English => "Local domains synced",
            Self::Italiano => "Domini locali sincronizzati",
            Self::Shqip => "Domain-et lokale u sinkronizuan",
        }
    }

    pub fn notice_project_added(self, name: &str) -> String {
        match self {
            Self::English => format!("Project {name} added. Syncing the local domain…"),
            Self::Italiano => {
                format!("Progetto {name} aggiunto. Sincronizzo il dominio locale…")
            }
            Self::Shqip => {
                format!("Projekti {name} u shtua. Duke sinkronizuar domain-in lokal…")
            }
        }
    }

    pub fn notice_unexpected(self, detail: &str) -> String {
        match self {
            Self::English => format!("Unexpected daemon response: {detail}"),
            Self::Italiano => format!("Risposta inattesa dal daemon: {detail}"),
            Self::Shqip => format!("Përgjigje e papritur nga daemon-i: {detail}"),
        }
    }

    pub fn notice_browser(self, error: &str) -> String {
        match self {
            Self::English => format!("Cannot open the browser: {error}"),
            Self::Italiano => format!("Impossibile aprire il browser: {error}"),
            Self::Shqip => format!("Nuk hapet dot shfletuesi: {error}"),
        }
    }

    // Value labels.

    pub fn profile_name(self, profile: Profile) -> &'static str {
        match profile {
            Profile::GenericPhp => match self {
                Self::English => "Generic PHP",
                Self::Italiano => "PHP generico",
                Self::Shqip => "PHP e përgjithshme",
            },
            Profile::Laravel => "Laravel",
            Profile::Symfony => "Symfony",
            Profile::Drupal => "Drupal",
            Profile::Wordpress => "WordPress",
            Profile::ProcessWire => "ProcessWire",
        }
    }

    pub fn database_name(self, database: Database) -> &'static str {
        match database {
            Database::None => match self {
                Self::English => "No database",
                Self::Italiano => "Nessun database",
                Self::Shqip => "Asnjë bazë të dhënash",
            },
            Database::Mariadb => "MariaDB",
            Database::Mysql => "MySQL",
            Database::Postgres => "PostgreSQL",
            Database::Sqlite => "SQLite",
        }
    }

    pub fn cache_name(self, cache: Cache) -> &'static str {
        match cache {
            Cache::None => match self {
                Self::English => "No cache",
                Self::Italiano => "Nessuna cache",
                Self::Shqip => "Asnjë cache",
            },
            Cache::Redis => "Redis",
            Cache::Memcached => self.text(
                "Memcached (unavailable)",
                "Memcached (non disponibile)",
                "Memcached (i padisponueshëm)",
            ),
        }
    }

    fn text(self, en: &'static str, it: &'static str, sq: &'static str) -> &'static str {
        match self {
            Self::English => en,
            Self::Italiano => it,
            Self::Shqip => sq,
        }
    }

    pub fn checking_status(self) -> &'static str {
        self.text(
            "Checking status…",
            "Verifica stato…",
            "Duke kontrolluar statusin…",
        )
    }
    pub fn daemon_unavailable(self) -> &'static str {
        self.text(
            "Daemon unavailable",
            "Daemon non raggiungibile",
            "Daemon-i i padisponueshëm",
        )
    }
    pub fn loading_projects(self) -> &'static str {
        self.text(
            "Loading projects…",
            "Caricamento progetti…",
            "Duke ngarkuar projektet…",
        )
    }
    pub fn last_known_state(self) -> &'static str {
        self.text(
            "Last known state",
            "Ultimo stato noto",
            "Statusi i fundit i njohur",
        )
    }
    pub fn refresh(self) -> &'static str {
        self.text("Refresh status", "Aggiorna stato", "Rifresko statusin")
    }
    pub fn sync_domains(self) -> &'static str {
        self.text(
            "Sync local domains",
            "Sincronizza domini locali",
            "Sinkronizo domain-et lokale",
        )
    }
    pub fn repair_setup(self) -> &'static str {
        self.text(
            "Repair setup",
            "Ripristina configurazione",
            "Riparo konfigurimin",
        )
    }
    pub fn technical_details(self) -> &'static str {
        self.text("Technical details", "Dettagli tecnici", "Hollësi teknike")
    }
    pub fn operation_failed(self) -> &'static str {
        self.text(
            "Operation failed. Review the details and retry.",
            "Operazione non riuscita. Consulta i dettagli e riprova.",
            "Veprimi dështoi. Kontrollo hollësitë dhe provo përsëri.",
        )
    }
    pub fn ca_load_failed(self) -> &'static str {
        self.text(
            "Certificate status unavailable. Refresh to retry.",
            "Stato del certificato non disponibile. Aggiorna per riprovare.",
            "Statusi i certifikatës i padisponueshëm. Rifresko për të provuar përsëri.",
        )
    }
    pub fn projects_load_failed(self) -> &'static str {
        self.text(
            "Project list could not be refreshed. Showing the last loaded list.",
            "Impossibile aggiornare i progetti. È mostrato l'ultimo elenco caricato.",
            "Lista nuk u rifreskua. Shfaqet lista e fundit e ngarkuar.",
        )
    }
    pub fn configured_workers(self) -> &'static str {
        self.text(
            "Configured workers",
            "Worker configurati",
            "Worker-a të konfiguruar",
        )
    }
    pub fn gateway_stopped(self) -> &'static str {
        self.text("Gateway stopped", "Gateway fermo", "Gateway i ndaluar")
    }
    pub fn gateway_stopped_body(self) -> &'static str {
        self.text(
            "Start a project to start the HTTPS gateway.",
            "Avvia un progetto per avviare il gateway HTTPS.",
            "Nis një projekt për të nisur gateway HTTPS.",
        )
    }
    pub fn stop_hint(self) -> &'static str {
        self.text(
            "Stops LocalWeb and all active projects. Source files and database data are kept.",
            "Ferma LocalWeb e tutti i progetti attivi. Sorgenti e dati dei database restano.",
            "Ndalon LocalWeb dhe projektet aktive. Skedarët dhe të dhënat ruhen.",
        )
    }
    pub fn view_all_projects(self) -> &'static str {
        self.text(
            "View all projects",
            "Vedi tutti i progetti",
            "Shiko të gjitha projektet",
        )
    }
    pub fn search_projects(self) -> &'static str {
        self.text("Search projects", "Cerca progetti", "Kërko projekte")
    }
    pub fn clear_search(self) -> &'static str {
        self.text("Clear", "Azzera", "Pastro")
    }
    pub fn no_search_results(self, query: &str) -> String {
        match self {
            Self::English => {
                format!("No projects match “{query}”. Clear the search to see all projects.")
            }
            Self::Italiano => format!(
                "Nessun progetto corrisponde a “{query}”. Azzera la ricerca per vedere tutti i progetti."
            ),
            Self::Shqip => format!(
                "Asnjë projekt për “{query}”. Pastro kërkimin për të parë të gjitha projektet."
            ),
        }
    }
    pub fn project_details(self) -> &'static str {
        self.text("Details", "Dettagli", "Hollësi")
    }
    pub fn reveal_environment(self) -> &'static str {
        self.text(
            "Show connection settings",
            "Mostra dati di connessione",
            "Shfaq cilësimet e lidhjes",
        )
    }
    pub fn environment_help(self) -> &'static str {
        self.text("Use these values in your app's configuration. LocalWeb does not edit your .env file.", "Usa questi valori nella configurazione dell'app. LocalWeb non modifica il file .env.", "Përdor këto vlera në konfigurimin e aplikacionit. LocalWeb nuk ndryshon skedarin .env.")
    }
    pub fn copy_environment(self) -> &'static str {
        self.text("Copy as .env", "Copia come .env", "Kopjo si .env")
    }
    pub fn environment_failed(self) -> &'static str {
        self.text(
            "Connection settings could not be loaded. Retry after refreshing.",
            "Impossibile caricare i dati di connessione. Riprova dopo aver aggiornato.",
            "Cilësimet nuk u ngarkuan. Provo përsëri pas rifreskimit.",
        )
    }
    pub fn check_form(self) -> &'static str {
        self.text(
            "Check the project settings",
            "Controlla i dati del progetto",
            "Kontrollo cilësimet e projektit",
        )
    }
    pub fn web_server_label(self) -> &'static str {
        self.text("Web server", "Web server", "Web server")
    }
    pub fn runtime_note_label(self) -> &'static str {
        self.text("Availability", "Disponibilità", "Disponueshmëria")
    }
    pub fn runtime_availability(self, bundled: bool) -> &'static str {
        if bundled {
            self.text("included", "incluso", "i përfshirë")
        } else {
            self.text("needs CLI/FPM", "richiede CLI/FPM", "kërkon CLI/FPM")
        }
    }
    pub fn php_version_availability(self, version: &str, bundled: bool) -> &'static str {
        if matches!(version, "8.0" | "8.1") {
            self.text("EOL · legacy", "EOL · legacy", "EOL · për përputhshmëri")
        } else {
            self.runtime_availability(bundled)
        }
    }
    pub fn php_versions_note(self) -> &'static str {
        self.text("PHP 8.0–8.1 are EOL and receive no security fixes. PHP 8.2 receives security fixes only until 31 Dec 2026. Use a supported branch for current work.", "PHP 8.0–8.1 sono EOL e non ricevono correzioni di sicurezza. PHP 8.2 riceve solo correzioni di sicurezza fino al 31 dic 2026. Per i progetti attuali, usa una versione supportata.", "PHP 8.0–8.1 janë EOL dhe nuk marrin përditësime sigurie. PHP 8.2 merr vetëm përditësime sigurie deri më 31 dhjetor 2026. Përdor një version të mbështetur për projekte aktive.")
    }
    pub fn notice_stopped(self) -> &'static str {
        self.text("LocalWeb stopped", "LocalWeb fermato", "LocalWeb u ndalua")
    }
    pub fn notice_project_started(self) -> &'static str {
        self.text("Project started", "Progetto avviato", "Projekti u nis")
    }
    pub fn notice_project_stopped(self) -> &'static str {
        self.text("Project stopped", "Progetto fermato", "Projekti u ndalua")
    }
    pub fn notice_project_restarted(self) -> &'static str {
        self.text(
            "Project restarted",
            "Progetto riavviato",
            "Projekti u rinis",
        )
    }
    pub fn notice_project_removed(self) -> &'static str {
        self.text(
            "Project configuration removed",
            "Configurazione del progetto rimossa",
            "Konfigurimi i projektit u hoq",
        )
    }
    pub fn project_options(self, worker: bool, cron: usize, mail: bool, autostart: bool) -> String {
        let yes = self.text("on", "attivo", "aktiv");
        let no = self.text("off", "disattivo", "joaktiv");
        format!(
            "{}: {} · Cron: {cron} · Mailpit: {} · {}: {}",
            self.supervisor_title(),
            if worker { yes } else { no },
            if mail { yes } else { no },
            self.autostart_label(),
            if autostart { yes } else { no }
        )
    }
}
