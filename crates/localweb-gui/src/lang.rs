//! Interface languages shown in the rail language select.
//!
//! Interface copy lives here; product names and technical details keep their original spelling.
//! Language codes persist in a plain text file so no new dependencies are needed. Persian and
//! Urdu wait until the text engine supports RTL/bidi layout.

use localweb_core::{Cache, Database, Profile};
use std::{fs, path::Path};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// English.
    English,
    /// Italiano (default).
    #[default]
    Italiano,
    /// Shqip.
    Shqip,
    /// Español.
    Spanish,
    /// Français.
    French,
    /// Português.
    Portuguese,
    /// Русский.
    Russian,
    /// Polski.
    Polish,
    /// Українська.
    Ukrainian,
    /// Türkçe.
    Turkish,
    /// Ελληνικά.
    Greek,
    /// हिन्दी.
    Hindi,
    /// 简体中文.
    Chinese,
    /// Svenska.
    Swedish,
}

impl Lang {
    /// Every language in display order (alphabetical by selector initials).
    #[rustfmt::skip]
    pub const ALL: [Self; 14] = [
        Self::Shqip, Self::English, Self::Spanish, Self::French, Self::Greek,
        Self::Hindi, Self::Italiano, Self::Polish, Self::Portuguese, Self::Russian,
        Self::Swedish, Self::Turkish, Self::Ukrainian, Self::Chinese,
    ];

    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Italiano => "it",
            Self::Shqip => "sq",
            Self::Spanish => "es",
            Self::French => "fr",
            Self::Portuguese => "pt",
            Self::Russian => "ru",
            Self::Polish => "pl",
            Self::Ukrainian => "uk",
            Self::Turkish => "tr",
            Self::Greek => "el",
            Self::Hindi => "hi",
            Self::Chinese => "zh",
            Self::Swedish => "sv",
        }
    }

    /// Two-letter badge shown in the language selector.
    #[must_use]
    pub fn initials(self) -> &'static str {
        match self {
            Self::English => "EN",
            Self::Italiano => "IT",
            Self::Shqip => "AL",
            Self::Spanish => "ES",
            Self::French => "FR",
            Self::Portuguese => "PT",
            Self::Russian => "RU",
            Self::Polish => "PL",
            Self::Ukrainian => "UK",
            Self::Turkish => "TR",
            Self::Greek => "GR",
            Self::Hindi => "HI",
            Self::Chinese => "ZH",
            Self::Swedish => "SV",
        }
    }

    #[must_use]
    pub fn from_code(code: &str) -> Self {
        match code.trim() {
            "en" => Self::English,
            "sq" => Self::Shqip,
            "es" => Self::Spanish,
            "fr" => Self::French,
            "pt" => Self::Portuguese,
            "ru" => Self::Russian,
            "pl" => Self::Polish,
            "uk" => Self::Ukrainian,
            "tr" => Self::Turkish,
            "el" => Self::Greek,
            "hi" => Self::Hindi,
            "zh" => Self::Chinese,
            "sv" => Self::Swedish,
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
            Self::Spanish => "Resumen",
            Self::French => "Aperçu",
            Self::Portuguese => "Visão geral",
            Self::Russian => "Обзор",
            Self::Polish => "Przegląd",
            Self::Ukrainian => "Огляд",
            Self::Turkish => "Genel bakış",
            Self::Greek => "Επισκόπηση",
            Self::Hindi => "अवलोकन",
            Self::Chinese => "概览",
            Self::Swedish => "Översikt",
        }
    }

    pub fn nav_projects(self) -> &'static str {
        match self {
            Self::English => "Projects",
            Self::Italiano => "Progetti",
            Self::Shqip => "Projektet",
            Self::Spanish => "Proyectos",
            Self::French => "Projets",
            Self::Portuguese => "Projetos",
            Self::Russian => "Проекты",
            Self::Polish => "Projekty",
            Self::Ukrainian => "Проєкти",
            Self::Turkish => "Projeler",
            Self::Greek => "Έργα",
            Self::Hindi => "प्रोजेक्ट",
            Self::Chinese => "项目",
            Self::Swedish => "Projekt",
        }
    }

    pub fn nav_settings(self) -> &'static str {
        match self {
            Self::English => "Settings",
            Self::Italiano => "Impostazioni",
            Self::Shqip => "Cilësimet",
            Self::Spanish => "Ajustes",
            Self::French => "Paramètres",
            Self::Portuguese => "Definições",
            Self::Russian => "Настройки",
            Self::Polish => "Ustawienia",
            Self::Ukrainian => "Налаштування",
            Self::Turkish => "Ayarlar",
            Self::Greek => "Ρυθμίσεις",
            Self::Hindi => "सेटिंग्स",
            Self::Chinese => "设置",
            Self::Swedish => "Inställningar",
        }
    }

    pub fn nav_add(self) -> &'static str {
        match self {
            Self::English => "Add project",
            Self::Italiano => "Aggiungi progetto",
            Self::Shqip => "Shto projekt",
            Self::Spanish => "Añadir proyecto",
            Self::French => "Ajouter un projet",
            Self::Portuguese => "Adicionar projeto",
            Self::Russian => "Добавить проект",
            Self::Polish => "Dodaj projekt",
            Self::Ukrainian => "Додати проєкт",
            Self::Turkish => "Proje ekle",
            Self::Greek => "Προσθήκη έργου",
            Self::Hindi => "प्रोजेक्ट जोड़ें",
            Self::Chinese => "添加项目",
            Self::Swedish => "Lägg till projekt",
        }
    }

    pub fn nav_close_note(self) -> &'static str {
        match self {
            Self::English => "Closing this window keeps sites running.",
            Self::Italiano => "Chiudendo questa finestra i siti restano attivi.",
            Self::Shqip => "Mbyllja e kësaj dritareje i lë faqet aktive.",
            Self::Spanish => "Cerrar esta ventana mantiene los sitios activos.",
            Self::French => "Fermer cette fenêtre laisse les sites en marche.",
            Self::Portuguese => "Fechar esta janela mantém os sites a funcionar.",
            Self::Russian => "Закрытие окна не останавливает сайты.",
            Self::Polish => "Zamknięcie okna nie zatrzymuje stron.",
            Self::Ukrainian => "Закриття вікна не зупиняє сайти.",
            Self::Turkish => "Bu pencereyi kapatmak siteleri çalışır durumda bırakır.",
            Self::Greek => "Το κλείσιμο του παραθύρου αφήνει τους ιστότοπους σε λειτουργία.",
            Self::Hindi => "यह विंडो बंद करने पर साइटें चलती रहती हैं।",
            Self::Chinese => "关闭此窗口不会停止站点。",
            Self::Swedish => "Att stänga fönstret låter webbplatserna köra vidare.",
        }
    }

    pub fn nav_running(self) -> &'static str {
        match self {
            Self::English => "Daemon connected",
            Self::Italiano => "Daemon connesso",
            Self::Shqip => "Daemon-i i lidhur",
            Self::Spanish => "Daemon conectado",
            Self::French => "Daemon connecté",
            Self::Portuguese => "Daemon ligado",
            Self::Russian => "Daemon подключён",
            Self::Polish => "Daemon połączony",
            Self::Ukrainian => "Daemon під'єднано",
            Self::Turkish => "Daemon bağlı",
            Self::Greek => "Daemon συνδεδεμένος",
            Self::Hindi => "Daemon जुड़ा है",
            Self::Chinese => "Daemon 已连接",
            Self::Swedish => "Daemon ansluten",
        }
    }

    pub fn nav_stopped(self) -> &'static str {
        match self {
            Self::English => "LocalWeb stopped",
            Self::Italiano => "LocalWeb fermo",
            Self::Shqip => "LocalWeb i ndaluar",
            Self::Spanish => "LocalWeb detenido",
            Self::French => "LocalWeb arrêté",
            Self::Portuguese => "LocalWeb parado",
            Self::Russian => "LocalWeb остановлен",
            Self::Polish => "LocalWeb zatrzymany",
            Self::Ukrainian => "LocalWeb зупинено",
            Self::Turkish => "LocalWeb durduruldu",
            Self::Greek => "LocalWeb σταματημένο",
            Self::Hindi => "LocalWeb बंद है",
            Self::Chinese => "LocalWeb 已停止",
            Self::Swedish => "LocalWeb stoppat",
        }
    }

    // Global actions.

    pub fn stop_localweb(self) -> &'static str {
        match self {
            Self::English => "Stop LocalWeb",
            Self::Italiano => "Ferma LocalWeb",
            Self::Shqip => "Ndalo LocalWeb",
            Self::Spanish => "Detener LocalWeb",
            Self::French => "Arrêter LocalWeb",
            Self::Portuguese => "Parar o LocalWeb",
            Self::Russian => "Остановить LocalWeb",
            Self::Polish => "Zatrzymaj LocalWeb",
            Self::Ukrainian => "Зупинити LocalWeb",
            Self::Turkish => "LocalWeb'i durdur",
            Self::Greek => "Διακοπή LocalWeb",
            Self::Hindi => "LocalWeb बंद करें",
            Self::Chinese => "停止 LocalWeb",
            Self::Swedish => "Stoppa LocalWeb",
        }
    }

    pub fn op_stopping(self) -> &'static str {
        match self {
            Self::English => "Stopping…",
            Self::Italiano => "Arresto in corso",
            Self::Shqip => "Duke u ndaluar…",
            Self::Spanish => "Deteniendo…",
            Self::French => "Arrêt en cours…",
            Self::Portuguese => "A parar…",
            Self::Russian => "Остановка…",
            Self::Polish => "Zatrzymywanie…",
            Self::Ukrainian => "Зупинення…",
            Self::Turkish => "Durduruluyor…",
            Self::Greek => "Διακοπή…",
            Self::Hindi => "बंद हो रहा है…",
            Self::Chinese => "正在停止…",
            Self::Swedish => "Stoppar…",
        }
    }

    pub fn op_setup(self) -> &'static str {
        match self {
            Self::English => "Setting up…",
            Self::Italiano => "Configurazione in corso",
            Self::Shqip => "Duke u konfiguruar…",
            Self::Spanish => "Configurando…",
            Self::French => "Configuration…",
            Self::Portuguese => "A configurar…",
            Self::Russian => "Настройка…",
            Self::Polish => "Konfigurowanie…",
            Self::Ukrainian => "Налаштування…",
            Self::Turkish => "Ayarlanıyor…",
            Self::Greek => "Ρύθμιση…",
            Self::Hindi => "सेट अप हो रहा है…",
            Self::Chinese => "正在设置…",
            Self::Swedish => "Ställer in…",
        }
    }

    pub fn op_starting(self) -> &'static str {
        match self {
            Self::English => "Starting…",
            Self::Italiano => "Avvio in corso",
            Self::Shqip => "Duke u nisur…",
            Self::Spanish => "Iniciando…",
            Self::French => "Démarrage…",
            Self::Portuguese => "A iniciar…",
            Self::Russian | Self::Ukrainian => "Запуск…",
            Self::Polish => "Uruchamianie…",
            Self::Turkish => "Başlatılıyor…",
            Self::Greek => "Εκκίνηση…",
            Self::Hindi => "शुरू हो रहा है…",
            Self::Chinese => "正在启动…",
            Self::Swedish => "Startar…",
        }
    }

    pub fn start_localweb(self) -> &'static str {
        match self {
            Self::English => "Start LocalWeb",
            Self::Italiano => "Avvia LocalWeb",
            Self::Shqip => "Nis LocalWeb",
            Self::Spanish => "Iniciar LocalWeb",
            Self::French => "Démarrer LocalWeb",
            Self::Portuguese => "Iniciar o LocalWeb",
            Self::Russian => "Запустить LocalWeb",
            Self::Polish => "Uruchom LocalWeb",
            Self::Ukrainian => "Запустити LocalWeb",
            Self::Turkish => "LocalWeb'i başlat",
            Self::Greek => "Εκκίνηση LocalWeb",
            Self::Hindi => "LocalWeb शुरू करें",
            Self::Chinese => "启动 LocalWeb",
            Self::Swedish => "Starta LocalWeb",
        }
    }

    pub fn configure_localweb(self) -> &'static str {
        match self {
            Self::English => "Set up LocalWeb",
            Self::Italiano => "Configura LocalWeb",
            Self::Shqip => "Konfiguro LocalWeb",
            Self::Spanish => "Configurar LocalWeb",
            Self::French => "Configurer LocalWeb",
            Self::Portuguese => "Configurar o LocalWeb",
            Self::Russian => "Настроить LocalWeb",
            Self::Polish => "Skonfiguruj LocalWeb",
            Self::Ukrainian => "Налаштувати LocalWeb",
            Self::Turkish => "LocalWeb'i ayarla",
            Self::Greek => "Ρύθμιση LocalWeb",
            Self::Hindi => "LocalWeb सेट अप करें",
            Self::Chinese => "设置 LocalWeb",
            Self::Swedish => "Ställ in LocalWeb",
        }
    }

    // Dashboard.

    pub fn dash_title(self) -> &'static str {
        self.nav_dashboard()
    }

    pub fn ca_callout_title(self) -> &'static str {
        match self {
            Self::English => "HTTPS needs one more step",
            Self::Italiano => "HTTPS richiede ancora un passaggio",
            Self::Shqip => "HTTPS kërkon ende një hap",
            Self::Spanish => "HTTPS requiere un paso más",
            Self::French => "HTTPS nécessite une étape de plus",
            Self::Portuguese => "O HTTPS precisa de mais um passo",
            Self::Russian => "Для HTTPS нужен ещё один шаг",
            Self::Polish => "HTTPS wymaga jeszcze jednego kroku",
            Self::Ukrainian => "Для HTTPS потрібен ще один крок",
            Self::Turkish => "HTTPS için bir adım daha gerekli",
            Self::Greek => "Το HTTPS χρειάζεται ένα ακόμη βήμα",
            Self::Hindi => "HTTPS के लिए एक और चरण चाहिए",
            Self::Chinese => "HTTPS 还差一步",
            Self::Swedish => "HTTPS behöver ett steg till",
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
            Self::Spanish => {
                "Confía en el certificado de desarrollo de LocalWeb. Tu sistema puede pedir autorización."
            }
            Self::French => {
                "Faites confiance au certificat de développement LocalWeb. Votre système peut demander une autorisation."
            }
            Self::Portuguese => {
                "Confie no certificado de desenvolvimento do LocalWeb. O sistema pode pedir autorização."
            }
            Self::Russian => {
                "Доверьтесь сертификату разработки LocalWeb. Система может запросить авторизацию."
            }
            Self::Polish => {
                "Zaufaj certyfikatowi deweloperskiemu LocalWeb. System może poprosić o autoryzację."
            }
            Self::Ukrainian => {
                "Довіртеся сертифікату розробки LocalWeb. Система може попросити авторизацію."
            }
            Self::Turkish => {
                "LocalWeb geliştirme sertifikasına güvenin. Sistem yetkilendirme isteyebilir."
            }
            Self::Greek => {
                "Εμπιστευτείτε το πιστοποιητικό ανάπτυξης του LocalWeb. Το σύστημα μπορεί να ζητήσει εξουσιοδότηση."
            }
            Self::Hindi => "LocalWeb विकास प्रमाणपत्र पर भरोसा करें। सिस्टम अनुमति माँग सकता है।",
            Self::Chinese => "信任 LocalWeb 开发证书。系统可能会请求授权。",
            Self::Swedish => {
                "Lita på LocalWebs utvecklingscertifikat. Systemet kan begära behörighet."
            }
        }
    }

    pub fn install_ca(self) -> &'static str {
        match self {
            Self::English => "Install CA",
            Self::Italiano => "Installa CA",
            Self::Shqip => "Instalo CA",
            Self::Spanish => "Instalar CA",
            Self::French => "Installer l'AC",
            Self::Portuguese => "Instalar a CA",
            Self::Russian => "Установить CA",
            Self::Polish => "Zainstaluj CA",
            Self::Ukrainian => "Встановити CA",
            Self::Turkish => "CA'yı yükle",
            Self::Greek => "Εγκατάσταση CA",
            Self::Hindi => "CA इंस्टॉल करें",
            Self::Chinese => "安装 CA",
            Self::Swedish => "Installera CA",
        }
    }

    // Environment overview.

    pub fn env_on(self) -> &'static str {
        match self {
            Self::English => "Environment ready",
            Self::Italiano => "Ambiente operativo",
            Self::Shqip => "Mjedisi gati",
            Self::Spanish => "Entorno listo",
            Self::French => "Environnement prêt",
            Self::Portuguese => "Ambiente pronto",
            Self::Russian => "Среда готова",
            Self::Polish => "Środowisko gotowe",
            Self::Ukrainian => "Середовище готове",
            Self::Turkish => "Ortam hazır",
            Self::Greek => "Το περιβάλλον είναι έτοιμο",
            Self::Hindi => "परिवेश तैयार है",
            Self::Chinese => "环境已就绪",
            Self::Swedish => "Miljön är redo",
        }
    }

    pub fn env_off(self) -> &'static str {
        match self {
            Self::English => "Environment stopped",
            Self::Italiano => "Ambiente fermo",
            Self::Shqip => "Mjedisi i ndaluar",
            Self::Spanish => "Entorno detenido",
            Self::French => "Environnement arrêté",
            Self::Portuguese => "Ambiente parado",
            Self::Russian => "Среда остановлена",
            Self::Polish => "Środowisko zatrzymane",
            Self::Ukrainian => "Середовище зупинено",
            Self::Turkish => "Ortam durduruldu",
            Self::Greek => "Το περιβάλλον είναι σταματημένο",
            Self::Hindi => "परिवेश बंद है",
            Self::Chinese => "环境已停止",
            Self::Swedish => "Miljön är stoppad",
        }
    }

    pub fn env_on_body(self) -> &'static str {
        match self {
            Self::English => {
                "Daemon connected; direct HTTP ports are ready. HTTPS is available when the gateway holds port 443."
            }
            Self::Italiano => {
                "Daemon connesso; le porte HTTP dirette sono pronte. HTTPS è disponibile quando il gateway occupa la porta 443."
            }
            Self::Shqip => {
                "Daemon-i i lidhur; portat e drejtpërdrejta HTTP janë gati. HTTPS është i disponueshëm kur gateway zë portën 443."
            }
            Self::Spanish => {
                "Daemon conectado; los puertos HTTP directos están listos. HTTPS está disponible cuando el gateway ocupa el puerto 443."
            }
            Self::French => {
                "Daemon connecté ; les ports HTTP directs sont prêts. HTTPS est disponible lorsque le gateway occupe le port 443."
            }
            Self::Portuguese => {
                "Daemon ligado; as portas HTTP diretas estão prontas. HTTPS está disponível quando o gateway ocupa a porta 443."
            }
            Self::Russian => {
                "Daemon подключён; прямые порты HTTP готовы. HTTPS доступен, когда gateway занимает порт 443."
            }
            Self::Polish => {
                "Daemon połączony; bezpośrednie porty HTTP są gotowe. HTTPS działa, gdy gateway zajmuje port 443."
            }
            Self::Ukrainian => {
                "Daemon під'єднано; прямі порти HTTP готові. HTTPS доступний, коли gateway займає порт 443."
            }
            Self::Turkish => {
                "Daemon bağlı; doğrudan HTTP portları hazır. Gateway 443 portunu kullandığında HTTPS kullanılabilir."
            }
            Self::Greek => {
                "Daemon συνδεδεμένο· οι απευθείας θύρες HTTP είναι έτοιμες. Το HTTPS είναι διαθέσιμο όταν το gateway καταλαμβάνει τη θύρα 443."
            }
            Self::Hindi => {
                "Daemon जुड़ा है; सीधे HTTP पोर्ट तैयार हैं। जब gateway पोर्ट 443 पर होता है तब HTTPS उपलब्ध होता है।"
            }
            Self::Chinese => {
                "Daemon 已连接；直接 HTTP 端口已就绪。当 gateway 占用 443 端口时可用 HTTPS。"
            }
            Self::Swedish => {
                "Daemon ansluten; direkta HTTP-portar är redo. HTTPS är tillgängligt när gatewayen använder port 443."
            }
        }
    }

    pub fn env_off_body(self) -> &'static str {
        match self {
            Self::English => "Start LocalWeb to make projects available.",
            Self::Italiano => "Avvia LocalWeb per rendere disponibili i progetti.",
            Self::Shqip => "Nis LocalWeb për t'i bërë projektet të disponueshme.",
            Self::Spanish => "Inicia LocalWeb para que los proyectos estén disponibles.",
            Self::French => "Démarrez LocalWeb pour rendre les projets disponibles.",
            Self::Portuguese => "Inicie o LocalWeb para disponibilizar os projetos.",
            Self::Russian => "Запустите LocalWeb, чтобы проекты стали доступны.",
            Self::Polish => "Uruchom LocalWeb, aby projekty były dostępne.",
            Self::Ukrainian => "Запустіть LocalWeb, щоб проєкти стали доступними.",
            Self::Turkish => "Projelerin kullanılabilir olması için LocalWeb'i başlatın.",
            Self::Greek => "Εκκινήστε το LocalWeb για να γίνουν διαθέσιμα τα έργα.",
            Self::Hindi => "प्रोजेक्ट उपलब्ध कराने के लिए LocalWeb शुरू करें।",
            Self::Chinese => "启动 LocalWeb 即可提供项目访问。",
            Self::Swedish => "Starta LocalWeb för att göra projekten tillgängliga.",
        }
    }

    pub fn projects_summary(self, active: usize, total: usize) -> String {
        match self {
            Self::English => format!("{active} of {total} active"),
            Self::Italiano => format!("{active} attivi su {total}"),
            Self::Shqip => format!("{active} aktive nga {total}"),
            Self::Spanish => format!("{active} de {total} activos"),
            Self::French => format!("{active} sur {total} actifs"),
            Self::Portuguese => format!("{active} de {total} ativos"),
            Self::Russian => format!("{active} из {total} активны"),
            Self::Polish => format!("{active} z {total} aktywnych"),
            Self::Ukrainian => format!("{active} з {total} активні"),
            Self::Turkish => format!("{total} projeden {active} etkin"),
            Self::Greek => format!("{active} από {total} ενεργά"),
            Self::Hindi => format!("{total} में से {active} सक्रिय"),
            Self::Chinese => format!("{total} 个中 {active} 个活跃"),
            Self::Swedish => format!("{active} av {total} aktiva"),
        }
    }

    // Projects view.

    pub fn projects_title(self) -> &'static str {
        self.nav_projects()
    }

    pub fn stopped_title(self) -> &'static str {
        match self {
            Self::English => "LocalWeb is stopped",
            Self::Italiano => "LocalWeb è fermo",
            Self::Shqip => "LocalWeb është i ndaluar",
            Self::Spanish => "LocalWeb está detenido",
            Self::French => "LocalWeb est arrêté",
            Self::Portuguese => "O LocalWeb está parado",
            Self::Russian => "LocalWeb остановлен",
            Self::Polish => "LocalWeb jest zatrzymany",
            Self::Ukrainian => "LocalWeb зупинено",
            Self::Turkish => "LocalWeb durduruldu",
            Self::Greek => "Το LocalWeb είναι σταματημένο",
            Self::Hindi => "LocalWeb बंद है",
            Self::Chinese => "LocalWeb 已停止",
            Self::Swedish => "LocalWeb är stoppat",
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
            Self::Spanish => {
                "Conéctate a LocalWeb para gestionar los proyectos. Inícialo o actualiza el estado."
            }
            Self::French => {
                "Connectez-vous à LocalWeb pour gérer les projets. Démarrez-le ou actualisez l'état."
            }
            Self::Portuguese => {
                "Ligue-se ao LocalWeb para gerir os projetos. Inicie-o ou atualize o estado."
            }
            Self::Russian => {
                "Подключитесь к LocalWeb для управления проектами. Запустите его или обновите состояние."
            }
            Self::Polish => {
                "Połącz się z LocalWeb, aby zarządzać projektami. Uruchom go lub odśwież stan."
            }
            Self::Ukrainian => {
                "Під'єднайтеся до LocalWeb, щоб керувати проєктами. Запустіть його або оновіть стан."
            }
            Self::Turkish => {
                "Projeleri yönetmek için LocalWeb'e bağlanın. Başlatın veya durumu yenileyin."
            }
            Self::Greek => {
                "Συνδεθείτε στο LocalWeb για να διαχειριστείτε τα έργα. Εκκινήστε το ή ανανεώστε την κατάσταση."
            }
            Self::Hindi => "प्रोजेक्ट प्रबंधित करने के लिए LocalWeb से जुड़ें। इसे शुरू करें या स्थिति रीफ़्रेश करें।",
            Self::Chinese => "连接 LocalWeb 以管理项目。启动它或刷新状态。",
            Self::Swedish => {
                "Anslut till LocalWeb för att hantera projekt. Starta det eller uppdatera statusen."
            }
        }
    }

    pub fn stopped_add_body(self) -> &'static str {
        match self {
            Self::English => "Start the daemon before adding a project.",
            Self::Italiano => "Avvia il daemon prima di aggiungere un progetto.",
            Self::Shqip => "Nis daemon-in para se të shtosh projektin.",
            Self::Spanish => "Inicia el daemon antes de añadir un proyecto.",
            Self::French => "Démarrez le daemon avant d'ajouter un projet.",
            Self::Portuguese => "Inicie o daemon antes de adicionar um projeto.",
            Self::Russian => "Запустите daemon, прежде чем добавлять проект.",
            Self::Polish => "Uruchom daemon przed dodaniem projektu.",
            Self::Ukrainian => "Запустіть daemon, перш ніж додавати проєкт.",
            Self::Turkish => "Proje eklemeden önce daemon'ı başlatın.",
            Self::Greek => "Εκκινήστε το daemon πριν προσθέσετε έργο.",
            Self::Hindi => "प्रोजेक्ट जोड़ने से पहले daemon शुरू करें।",
            Self::Chinese => "请先启动 daemon，再添加项目。",
            Self::Swedish => "Starta daemon innan du lägger till ett projekt.",
        }
    }

    pub fn search_hint(self) -> &'static str {
        match self {
            Self::English => "Search by name, domain, or path",
            Self::Italiano => "Cerca per nome, dominio o percorso",
            Self::Shqip => "Kërko sipas emrit, domain-it ose shtegut",
            Self::Spanish => "Buscar por nombre, dominio o ruta",
            Self::French => "Rechercher par nom, domaine ou chemin",
            Self::Portuguese => "Pesquisar por nome, domínio ou caminho",
            Self::Russian => "Поиск по имени, домену или пути",
            Self::Polish => "Szukaj po nazwie, domenie lub ścieżce",
            Self::Ukrainian => "Пошук за назвою, доменом або шляхом",
            Self::Turkish => "Ada, alan adı veya yola göre ara",
            Self::Greek => "Αναζήτηση με όνομα, τομέα ή διαδρομή",
            Self::Hindi => "नाम, डोमेन या पथ से खोजें",
            Self::Chinese => "按名称、域名或路径搜索",
            Self::Swedish => "Sök på namn, domän eller sökväg",
        }
    }

    pub fn col_project(self) -> &'static str {
        match self {
            Self::English => "Project",
            Self::Italiano => "Progetto",
            Self::Shqip => "Projekti",
            Self::Spanish => "Proyecto",
            Self::French => "Projet",
            Self::Portuguese => "Projeto",
            Self::Russian => "Проект",
            Self::Polish | Self::Swedish => "Projekt",
            Self::Ukrainian => "Проєкт",
            Self::Turkish => "Proje",
            Self::Greek => "Έργο",
            Self::Hindi => "प्रोजेक्ट",
            Self::Chinese => "项目",
        }
    }

    pub fn col_domain(self) -> &'static str {
        match self {
            Self::English => "Domain and path",
            Self::Italiano => "Dominio e percorso",
            Self::Shqip => "Domain-i dhe shtegu",
            Self::Spanish => "Dominio y ruta",
            Self::French => "Domaine et chemin",
            Self::Portuguese => "Domínio e caminho",
            Self::Russian => "Домен и путь",
            Self::Polish => "Domena i ścieżka",
            Self::Ukrainian => "Домен і шлях",
            Self::Turkish => "Alan adı ve yol",
            Self::Greek => "Τομέας και διαδρομή",
            Self::Hindi => "डोमेन और पथ",
            Self::Chinese => "域名和路径",
            Self::Swedish => "Domän och sökväg",
        }
    }

    pub fn col_services(self) -> &'static str {
        match self {
            Self::English | Self::French => "Services",
            Self::Italiano => "Servizi",
            Self::Shqip => "Shërbimet",
            Self::Spanish => "Servicios",
            Self::Portuguese => "Serviços",
            Self::Russian => "Сервисы",
            Self::Polish => "Usługi",
            Self::Ukrainian => "Сервіси",
            Self::Turkish => "Servisler",
            Self::Greek => "Υπηρεσίες",
            Self::Hindi => "सेवाएँ",
            Self::Chinese => "服务",
            Self::Swedish => "Tjänster",
        }
    }

    pub fn col_state(self) -> &'static str {
        match self {
            Self::English | Self::Swedish => "Status",
            Self::Italiano => "Stato",
            Self::Shqip => "Statusi",
            Self::Spanish | Self::Portuguese => "Estado",
            Self::French => "État",
            Self::Russian => "Состояние",
            Self::Polish => "Stan",
            Self::Ukrainian => "Стан",
            Self::Turkish => "Durum",
            Self::Greek => "Κατάσταση",
            Self::Hindi => "स्थिति",
            Self::Chinese => "状态",
        }
    }

    pub fn col_actions(self) -> &'static str {
        match self {
            Self::English | Self::French => "Actions",
            Self::Italiano => "Azioni",
            Self::Shqip => "Veprimet",
            Self::Spanish => "Acciones",
            Self::Portuguese => "Ações",
            Self::Russian => "Действия",
            Self::Polish => "Akcje",
            Self::Ukrainian => "Дії",
            Self::Turkish => "Eylemler",
            Self::Greek => "Ενέργειες",
            Self::Hindi => "क्रियाएँ",
            Self::Chinese => "操作",
            Self::Swedish => "Åtgärder",
        }
    }

    pub fn empty_title(self) -> &'static str {
        match self {
            Self::English => "No projects yet",
            Self::Italiano => "Nessun progetto configurato",
            Self::Shqip => "Ende asnjë projekt",
            Self::Spanish => "Aún no hay proyectos",
            Self::French => "Aucun projet pour l'instant",
            Self::Portuguese => "Ainda não há projetos",
            Self::Russian => "Проектов пока нет",
            Self::Polish => "Brak projektów",
            Self::Ukrainian => "Проєктів ще немає",
            Self::Turkish => "Henüz proje yok",
            Self::Greek => "Δεν υπάρχουν έργα ακόμη",
            Self::Hindi => "अभी कोई प्रोजेक्ट नहीं",
            Self::Chinese => "还没有项目",
            Self::Swedish => "Inga projekt ännu",
        }
    }

    pub fn empty_online(self) -> &'static str {
        match self {
            Self::English => "Add a source folder to get started.",
            Self::Italiano => "Aggiungi una cartella di codice sorgente per iniziare.",
            Self::Shqip => "Shto një dosje burimore për të filluar.",
            Self::Spanish => "Añade una carpeta de código fuente para empezar.",
            Self::French => "Ajoutez un dossier de code source pour commencer.",
            Self::Portuguese => "Adicione uma pasta de código-fonte para começar.",
            Self::Russian => "Добавьте папку с исходным кодом, чтобы начать.",
            Self::Polish => "Dodaj folder z kodem źródłowym, aby zacząć.",
            Self::Ukrainian => "Додайте теку з вихідним кодом, щоб почати.",
            Self::Turkish => "Başlamak için bir kaynak kod klasörü ekleyin.",
            Self::Greek => "Προσθέστε έναν φάκελο πηγαίου κώδικα για να ξεκινήσετε.",
            Self::Hindi => "शुरू करने के लिए स्रोत कोड फ़ोल्डर जोड़ें।",
            Self::Chinese => "添加源代码文件夹即可开始。",
            Self::Swedish => "Lägg till en källkodsmapp för att komma igång.",
        }
    }

    pub fn empty_offline(self) -> &'static str {
        match self {
            Self::English => "Start LocalWeb, then add the first project.",
            Self::Italiano => "Avvia LocalWeb, quindi aggiungi il primo progetto.",
            Self::Shqip => "Nis LocalWeb, pastaj shto projektin e parë.",
            Self::Spanish => "Inicia LocalWeb y añade el primer proyecto.",
            Self::French => "Démarrez LocalWeb, puis ajoutez le premier projet.",
            Self::Portuguese => "Inicie o LocalWeb e adicione o primeiro projeto.",
            Self::Russian => "Запустите LocalWeb и добавьте первый проект.",
            Self::Polish => "Uruchom LocalWeb, a następnie dodaj pierwszy projekt.",
            Self::Ukrainian => "Запустіть LocalWeb, а потім додайте перший проєкт.",
            Self::Turkish => "LocalWeb'i başlatın, ardından ilk projeyi ekleyin.",
            Self::Greek => "Εκκινήστε το LocalWeb και προσθέστε το πρώτο έργο.",
            Self::Hindi => "LocalWeb शुरू करें, फिर पहला प्रोजेक्ट जोड़ें।",
            Self::Chinese => "启动 LocalWeb，然后添加第一个项目。",
            Self::Swedish => "Starta LocalWeb och lägg sedan till det första projektet.",
        }
    }

    // Project ledger.

    pub fn transition_starting(self) -> &'static str {
        match self {
            Self::English => "Starting",
            Self::Italiano => "Avvio",
            Self::Shqip => "Duke u nisur",
            Self::Spanish => "Iniciando",
            Self::French => "Démarrage",
            Self::Portuguese => "A iniciar",
            Self::Russian | Self::Ukrainian => "Запуск",
            Self::Polish => "Uruchamianie",
            Self::Turkish => "Başlatılıyor",
            Self::Greek => "Εκκίνηση",
            Self::Hindi => "शुरू हो रहा है",
            Self::Chinese => "启动中",
            Self::Swedish => "Startar",
        }
    }

    pub fn transition_stopping(self) -> &'static str {
        match self {
            Self::English => "Stopping",
            Self::Italiano => "Arresto",
            Self::Shqip => "Duke u ndaluar",
            Self::Spanish => "Deteniendo",
            Self::French => "Arrêt",
            Self::Portuguese => "A parar",
            Self::Russian => "Остановка",
            Self::Polish => "Zatrzymywanie",
            Self::Ukrainian => "Зупинення",
            Self::Turkish => "Durduruluyor",
            Self::Greek => "Διακοπή",
            Self::Hindi => "बंद हो रहा है",
            Self::Chinese => "停止中",
            Self::Swedish => "Stoppar",
        }
    }

    pub fn transition_restarting(self) -> &'static str {
        match self {
            Self::English => "Restarting",
            Self::Italiano => "Riavvio",
            Self::Shqip => "Duke u rinisur",
            Self::Spanish => "Reiniciando",
            Self::French => "Redémarrage",
            Self::Portuguese => "A reiniciar",
            Self::Russian | Self::Ukrainian => "Перезапуск",
            Self::Polish => "Restartowanie",
            Self::Turkish => "Yeniden başlatılıyor",
            Self::Greek => "Επανεκκίνηση",
            Self::Hindi => "पुनः शुरू हो रहा है",
            Self::Chinese => "重启中",
            Self::Swedish => "Startar om",
        }
    }

    pub fn badge_active(self) -> &'static str {
        match self {
            Self::English => "Active",
            Self::Italiano => "Attivo",
            Self::Shqip | Self::Swedish => "Aktiv",
            Self::Spanish => "Activo",
            Self::French => "Actif",
            Self::Portuguese => "Ativo",
            Self::Russian => "Активен",
            Self::Polish => "Aktywny",
            Self::Ukrainian => "Активний",
            Self::Turkish => "Etkin",
            Self::Greek => "Ενεργό",
            Self::Hindi => "सक्रिय",
            Self::Chinese => "活跃",
        }
    }

    pub fn badge_stopped(self) -> &'static str {
        match self {
            Self::English => "Stopped",
            Self::Italiano => "Fermo",
            Self::Shqip => "I ndaluar",
            Self::Spanish => "Detenido",
            Self::French => "Arrêté",
            Self::Portuguese => "Parado",
            Self::Russian => "Остановлен",
            Self::Polish => "Zatrzymany",
            Self::Ukrainian => "Зупинено",
            Self::Turkish => "Durduruldu",
            Self::Greek => "Σταματημένο",
            Self::Hindi => "बंद",
            Self::Chinese => "已停止",
            Self::Swedish => "Stoppad",
        }
    }

    pub fn action_stop(self) -> &'static str {
        match self {
            Self::English => "Stop",
            Self::Italiano => "Ferma",
            Self::Shqip => "Ndalo",
            Self::Spanish => "Detener",
            Self::French => "Arrêter",
            Self::Portuguese => "Parar",
            Self::Russian => "Остановить",
            Self::Polish => "Zatrzymaj",
            Self::Ukrainian => "Зупинити",
            Self::Turkish => "Durdur",
            Self::Greek => "Διακοπή",
            Self::Hindi => "बंद करें",
            Self::Chinese => "停止",
            Self::Swedish => "Stoppa",
        }
    }

    pub fn action_restart(self) -> &'static str {
        match self {
            Self::English => "Restart",
            Self::Italiano => "Riavvia",
            Self::Shqip => "Rinise",
            Self::Spanish | Self::Portuguese => "Reiniciar",
            Self::French => "Redémarrer",
            Self::Russian => "Перезапустить",
            Self::Polish => "Restartuj",
            Self::Ukrainian => "Перезапустити",
            Self::Turkish => "Yeniden başlat",
            Self::Greek => "Επανεκκίνηση",
            Self::Hindi => "पुनः शुरू करें",
            Self::Chinese => "重启",
            Self::Swedish => "Starta om",
        }
    }

    pub fn action_open(self) -> &'static str {
        match self {
            Self::English => "Open",
            Self::Italiano => "Apri",
            Self::Shqip => "Hap",
            Self::Spanish | Self::Portuguese => "Abrir",
            Self::French => "Ouvrir",
            Self::Russian => "Открыть",
            Self::Polish => "Otwórz",
            Self::Ukrainian => "Відкрити",
            Self::Turkish => "Aç",
            Self::Greek => "Άνοιγμα",
            Self::Hindi => "खोलें",
            Self::Chinese => "打开",
            Self::Swedish => "Öppna",
        }
    }

    pub fn action_remove(self) -> &'static str {
        match self {
            Self::English => "Remove",
            Self::Italiano => "Rimuovi",
            Self::Shqip => "Hiq",
            Self::Spanish => "Eliminar",
            Self::French => "Supprimer",
            Self::Portuguese => "Remover",
            Self::Russian => "Удалить",
            Self::Polish => "Usuń",
            Self::Ukrainian => "Видалити",
            Self::Turkish => "Kaldır",
            Self::Greek => "Αφαίρεση",
            Self::Hindi => "हटाएँ",
            Self::Chinese => "移除",
            Self::Swedish => "Ta bort",
        }
    }

    pub fn action_start(self) -> &'static str {
        match self {
            Self::English => "Start",
            Self::Italiano => "Avvia",
            Self::Shqip => "Nis",
            Self::Spanish | Self::Portuguese => "Iniciar",
            Self::French => "Démarrer",
            Self::Russian => "Запустить",
            Self::Polish => "Uruchom",
            Self::Ukrainian => "Запустити",
            Self::Turkish => "Başlat",
            Self::Greek => "Εκκίνηση",
            Self::Hindi => "शुरू करें",
            Self::Chinese => "启动",
            Self::Swedish => "Starta",
        }
    }

    pub fn supervisor_tag() -> &'static str {
        "Supervisor"
    }

    // Add-project view.

    pub fn add_title(self) -> &'static str {
        self.nav_add()
    }

    pub fn cancel(self) -> &'static str {
        match self {
            Self::English => "Cancel",
            Self::Italiano => "Annulla",
            Self::Shqip => "Anulo",
            Self::Spanish | Self::Portuguese => "Cancelar",
            Self::French => "Annuler",
            Self::Russian => "Отмена",
            Self::Polish => "Anuluj",
            Self::Ukrainian => "Скасувати",
            Self::Turkish => "İptal",
            Self::Greek => "Άκυρο",
            Self::Hindi => "रद्द करें",
            Self::Chinese => "取消",
            Self::Swedish => "Avbryt",
        }
    }

    pub fn create_project(self) -> &'static str {
        match self {
            Self::English => "Add project",
            Self::Italiano => "Aggiungi progetto",
            Self::Shqip => "Shto projekt",
            Self::Spanish => "Añadir proyecto",
            Self::French => "Ajouter un projet",
            Self::Portuguese => "Adicionar projeto",
            Self::Russian => "Добавить проект",
            Self::Polish => "Dodaj projekt",
            Self::Ukrainian => "Додати проєкт",
            Self::Turkish => "Proje ekle",
            Self::Greek => "Προσθήκη έργου",
            Self::Hindi => "प्रोजेक्ट जोड़ें",
            Self::Chinese => "添加项目",
            Self::Swedish => "Lägg till projekt",
        }
    }

    pub fn operation_busy(self) -> &'static str {
        match self {
            Self::English => "Working…",
            Self::Italiano => "Operazione in corso",
            Self::Shqip => "Duke punuar…",
            Self::Spanish => "Procesando…",
            Self::French => "En cours…",
            Self::Portuguese => "Em curso…",
            Self::Russian => "Выполняется…",
            Self::Polish => "Przetwarzanie…",
            Self::Ukrainian => "Виконується…",
            Self::Turkish => "İşleniyor…",
            Self::Greek => "Σε εξέλιξη…",
            Self::Hindi => "काम चल रहा है…",
            Self::Chinese => "处理中…",
            Self::Swedish => "Arbetar…",
        }
    }

    pub fn source_title(self) -> &'static str {
        match self {
            Self::English => "Source code",
            Self::Italiano => "Codice sorgente",
            Self::Shqip => "Kodi burimor",
            Self::Spanish => "Código fuente",
            Self::French => "Code source",
            Self::Portuguese => "Código-fonte",
            Self::Russian => "Исходный код",
            Self::Polish => "Kod źródłowy",
            Self::Ukrainian => "Вихідний код",
            Self::Turkish => "Kaynak kodu",
            Self::Greek => "Πηγαίος κώδικας",
            Self::Hindi => "स्रोत कोड",
            Self::Chinese => "源代码",
            Self::Swedish => "Källkod",
        }
    }

    pub fn source_body(self) -> &'static str {
        match self {
            Self::English => "The folder stays yours; LocalWeb never deletes it.",
            Self::Italiano => {
                "La cartella resta di tua proprietà e non verrà mai eliminata da LocalWeb."
            }
            Self::Shqip => "Dosja mbetet e jotja; LocalWeb nuk e fshin kurrë.",
            Self::Spanish => "La carpeta sigue siendo tuya; LocalWeb nunca la elimina.",
            Self::French => "Le dossier reste le vôtre ; LocalWeb ne le supprime jamais.",
            Self::Portuguese => "A pasta continua a ser sua; o LocalWeb nunca a elimina.",
            Self::Russian => "Папка остаётся вашей; LocalWeb никогда её не удаляет.",
            Self::Polish => "Folder pozostaje twój; LocalWeb nigdy go nie usuwa.",
            Self::Ukrainian => "Тека залишається вашою; LocalWeb ніколи її не видаляє.",
            Self::Turkish => "Klasör sizin kalır; LocalWeb onu asla silmez.",
            Self::Greek => "Ο φάκελος παραμένει δικός σας· το LocalWeb δεν τον διαγράφει ποτέ.",
            Self::Hindi => "फ़ोल्डर आपका ही रहता है; LocalWeb उसे कभी नहीं हटाता।",
            Self::Chinese => "文件夹始终归你所有；LocalWeb 绝不会删除它。",
            Self::Swedish => "Mappen förblir din; LocalWeb raderar den aldrig.",
        }
    }

    pub fn path_label(self) -> &'static str {
        match self {
            Self::English => "Source folder",
            Self::Italiano => "Cartella sorgente",
            Self::Shqip => "Dosja burimore",
            Self::Spanish => "Carpeta de origen",
            Self::French => "Dossier source",
            Self::Portuguese => "Pasta de origem",
            Self::Russian => "Папка с исходниками",
            Self::Polish => "Folder źródłowy",
            Self::Ukrainian => "Тека з вихідним кодом",
            Self::Turkish => "Kaynak klasörü",
            Self::Greek => "Φάκελος πηγαίου κώδικα",
            Self::Hindi => "स्रोत फ़ोल्डर",
            Self::Chinese => "源文件夹",
            Self::Swedish => "Källkodsmapp",
        }
    }

    pub fn browse(self) -> &'static str {
        match self {
            Self::English => "Browse",
            Self::Italiano => "Sfoglia",
            Self::Shqip => "Shfleto",
            Self::Spanish => "Examinar",
            Self::French => "Parcourir",
            Self::Portuguese => "Procurar",
            Self::Russian => "Обзор",
            Self::Polish => "Przeglądaj",
            Self::Ukrainian => "Огляд",
            Self::Turkish => "Gözat",
            Self::Greek => "Περιήγηση",
            Self::Hindi => "ब्राउज़ करें",
            Self::Chinese => "浏览",
            Self::Swedish => "Bläddra",
        }
    }

    pub fn name_label(self) -> &'static str {
        match self {
            Self::English => "Name",
            Self::Italiano | Self::Portuguese => "Nome",
            Self::Shqip => "Emri",
            Self::Spanish => "Nombre",
            Self::French => "Nom",
            Self::Russian => "Имя",
            Self::Polish => "Nazwa",
            Self::Ukrainian => "Назва",
            Self::Turkish => "Ad",
            Self::Greek => "Όνομα",
            Self::Hindi => "नाम",
            Self::Chinese => "名称",
            Self::Swedish => "Namn",
        }
    }

    pub fn host_label(self) -> &'static str {
        match self {
            Self::English => "Local domain",
            Self::Italiano => "Dominio locale",
            Self::Shqip => "Domain-i lokal",
            Self::Spanish => "Dominio local",
            Self::French => "Domaine local",
            Self::Portuguese => "Domínio local",
            Self::Russian => "Локальный домен",
            Self::Polish => "Domena lokalna",
            Self::Ukrainian => "Локальний домен",
            Self::Turkish => "Yerel alan adı",
            Self::Greek => "Τοπικός τομέας",
            Self::Hindi => "स्थानीय डोमेन",
            Self::Chinese => "本地域名",
            Self::Swedish => "Lokal domän",
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
            Self::Spanish => "Preajuste de aplicación",
            Self::French => "Préréglage d'application",
            Self::Portuguese => "Predefinição de aplicação",
            Self::Russian => "Профиль приложения",
            Self::Polish => "Profil aplikacji",
            Self::Ukrainian => "Профіль застосунку",
            Self::Turkish => "Uygulama hazır ayarı",
            Self::Greek => "Προεπιλογή εφαρμογής",
            Self::Hindi => "एप्लिकेशन प्रीसेट",
            Self::Chinese => "应用预设",
            Self::Swedish => "Applikationsförval",
        }
    }

    pub fn services_title(self) -> &'static str {
        match self {
            Self::English => "Runtime and services",
            Self::Italiano => "Runtime e servizi",
            Self::Shqip => "Runtime dhe shërbimet",
            Self::Spanish => "Runtime y servicios",
            Self::French => "Runtime et services",
            Self::Portuguese => "Runtime e serviços",
            Self::Russian => "Runtime и сервисы",
            Self::Polish => "Runtime i usługi",
            Self::Ukrainian => "Runtime та сервіси",
            Self::Turkish => "Runtime ve servisler",
            Self::Greek => "Runtime και υπηρεσίες",
            Self::Hindi => "Runtime और सेवाएँ",
            Self::Chinese => "Runtime 和服务",
            Self::Swedish => "Runtime och tjänster",
        }
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
            Self::Spanish => {
                "Elige el runtime y los servicios opcionales. Servidores web, PHP y bases de datos son compartidos; Redis, Mailpit y los workers son por proyecto."
            }
            Self::French => {
                "Choisissez le runtime et les services optionnels. Les serveurs web, PHP et les bases de données sont partagés ; Redis, Mailpit et les workers sont propres à chaque projet."
            }
            Self::Portuguese => {
                "Escolha o runtime e os serviços opcionais. Servidores web, PHP e bases de dados são partilhados; Redis, Mailpit e workers são por projeto."
            }
            Self::Russian => {
                "Выберите runtime и дополнительные сервисы. Веб-серверы, PHP и базы данных общие; Redis, Mailpit и worker — для каждого проекта."
            }
            Self::Polish => {
                "Wybierz runtime i usługi opcjonalne. Serwery WWW, PHP i bazy danych są współdzielone; Redis, Mailpit i workers są per projekt."
            }
            Self::Ukrainian => {
                "Виберіть runtime та додаткові сервіси. Вебсервери, PHP і бази даних спільні; Redis, Mailpit і workers — для кожного проєкту."
            }
            Self::Turkish => {
                "Runtime ve isteğe bağlı servisleri seçin. Web sunucuları, PHP ve veritabanları paylaşılır; Redis, Mailpit ve worker'lar proje başındır."
            }
            Self::Greek => {
                "Επιλέξτε το runtime και τις προαιρετικές υπηρεσίες. Οι διακομιστές ιστού, η PHP και οι βάσεις δεδομένων είναι κοινόχρηστα· τα Redis, Mailpit και workers είναι ανά έργο."
            }
            Self::Hindi => {
                "Runtime और वैकल्पिक सेवाएँ चुनें। वेब सर्वर, PHP और डेटाबेस साझा हैं; Redis, Mailpit और worker प्रति प्रोजेक्ट हैं।"
            }
            Self::Chinese => {
                "选择 runtime 和可选服务。Web 服务器、PHP 和数据库为共享；Redis、Mailpit 和 worker 按项目配置。"
            }
            Self::Swedish => {
                "Välj runtime och valfria tjänster. Webbservrar, PHP och databaser delas; Redis, Mailpit och workers är per projekt."
            }
        }
    }

    pub fn autostart_label(self) -> &'static str {
        match self {
            Self::English | Self::Polish | Self::Swedish => "Autostart",
            Self::Italiano => "Avvio automatico",
            Self::Shqip => "Nisja automatike",
            Self::Spanish => "Inicio automático",
            Self::French => "Démarrage automatique",
            Self::Portuguese => "Início automático",
            Self::Russian | Self::Ukrainian => "Автозапуск",
            Self::Turkish => "Otomatik başlatma",
            Self::Greek => "Αυτόματη εκκίνηση",
            Self::Hindi => "स्वतः शुरू",
            Self::Chinese => "自动启动",
        }
    }

    pub fn autostart_check(self) -> &'static str {
        match self {
            Self::English => "Start on the next LocalWeb launch",
            Self::Italiano => "Avvia al prossimo avvio di LocalWeb",
            Self::Shqip => "Nise në nisjen tjetër të LocalWeb",
            Self::Spanish => "Iniciar en el próximo arranque de LocalWeb",
            Self::French => "Démarrer au prochain lancement de LocalWeb",
            Self::Portuguese => "Iniciar no próximo arranque do LocalWeb",
            Self::Russian => "Запускать при следующем старте LocalWeb",
            Self::Polish => "Uruchom przy następnym starcie LocalWeb",
            Self::Ukrainian => "Запускати при наступному старті LocalWeb",
            Self::Turkish => "Sonraki LocalWeb başlatmasında başlat",
            Self::Greek => "Εκκίνηση στην επόμενη εκκίνηση του LocalWeb",
            Self::Hindi => "अगली LocalWeb शुरुआत पर चालू करें",
            Self::Chinese => "下次启动 LocalWeb 时自动运行",
            Self::Swedish => "Starta vid nästa LocalWeb-start",
        }
    }

    pub fn db_none(self) -> &'static str {
        match self {
            Self::English => "None",
            Self::Italiano => "Nessuno",
            Self::Shqip => "Asnjë",
            Self::Spanish => "Ninguna",
            Self::French => "Aucune",
            Self::Portuguese => "Nenhuma",
            Self::Russian => "Нет",
            Self::Polish => "Brak",
            Self::Ukrainian => "Немає",
            Self::Turkish => "Yok",
            Self::Greek => "Καμία",
            Self::Hindi => "कोई नहीं",
            Self::Chinese => "无",
            Self::Swedish => "Ingen",
        }
    }

    pub fn cache_none(self) -> &'static str {
        match self {
            Self::English => "None",
            Self::Italiano => "Nessuna",
            Self::Shqip => "Asnjë",
            Self::Spanish => "Ninguna",
            Self::French => "Aucun",
            Self::Portuguese => "Nenhuma",
            Self::Russian => "Нет",
            Self::Polish => "Brak",
            Self::Ukrainian => "Немає",
            Self::Turkish => "Yok",
            Self::Greek => "Καμία",
            Self::Hindi => "कोई नहीं",
            Self::Chinese => "无",
            Self::Swedish => "Ingen",
        }
    }

    pub fn supervisor_title(self) -> &'static str {
        match self {
            Self::English => "Background worker",
            Self::Italiano => "Worker in background",
            Self::Shqip => "Worker në sfond",
            Self::Spanish => "Worker en segundo plano",
            Self::French => "Worker en arrière-plan",
            Self::Portuguese => "Worker em segundo plano",
            Self::Russian => "Фоновый воркер",
            Self::Polish => "Worker w tle",
            Self::Ukrainian => "Фоновий воркер",
            Self::Turkish => "Arka plan worker",
            Self::Greek => "Worker παρασκηνίου",
            Self::Hindi => "पृष्ठभूमि वर्कर",
            Self::Chinese => "后台 worker",
            Self::Swedish => "Bakgrundsworker",
        }
    }

    pub fn supervisor_check(self) -> &'static str {
        match self {
            Self::English => "Enable Supervisor for this project",
            Self::Italiano => "Attiva Supervisor per questo progetto",
            Self::Shqip => "Aktivizo Supervisor për këtë projekt",
            Self::Spanish => "Activar Supervisor para este proyecto",
            Self::French => "Activer Supervisor pour ce projet",
            Self::Portuguese => "Ativar o Supervisor para este projeto",
            Self::Russian => "Включить Supervisor для этого проекта",
            Self::Polish => "Włącz Supervisor dla tego projektu",
            Self::Ukrainian => "Увімкнути Supervisor для цього проєкту",
            Self::Turkish => "Bu proje için Supervisor'ı etkinleştir",
            Self::Greek => "Ενεργοποίηση Supervisor για αυτό το έργο",
            Self::Hindi => "इस प्रोजेक्ट के लिए Supervisor सक्षम करें",
            Self::Chinese => "为此项目启用 Supervisor",
            Self::Swedish => "Aktivera Supervisor för detta projekt",
        }
    }

    pub fn supervisor_preset_label(self) -> &'static str {
        match self {
            Self::English => "Preset",
            Self::Italiano => "Configurazione",
            Self::Shqip => "Paracaktimi",
            Self::Spanish => "Preajuste",
            Self::French => "Préréglage",
            Self::Portuguese => "Predefinição",
            Self::Russian => "Профиль",
            Self::Polish => "Profil",
            Self::Ukrainian => "Профіль",
            Self::Turkish => "Hazır ayar",
            Self::Greek => "Προεπιλογή",
            Self::Hindi => "प्रीसेट",
            Self::Chinese => "预设",
            Self::Swedish => "Förval",
        }
    }

    pub fn supervisor_custom(self) -> &'static str {
        match self {
            Self::English => "Use a custom command",
            Self::Italiano => "Usa un comando personalizzato",
            Self::Shqip => "Përdor komandë të personalizuar",
            Self::Spanish => "Usar un comando personalizado",
            Self::French => "Utiliser une commande personnalisée",
            Self::Portuguese => "Usar um comando personalizado",
            Self::Russian => "Использовать свою команду",
            Self::Polish => "Użyj własnej komendy",
            Self::Ukrainian => "Використати власну команду",
            Self::Turkish => "Özel komut kullan",
            Self::Greek => "Χρήση προσαρμοσμένης εντολής",
            Self::Hindi => "कस्टम कमांड उपयोग करें",
            Self::Chinese => "使用自定义命令",
            Self::Swedish => "Använd ett anpassat kommando",
        }
    }

    pub fn supervisor_executable(self) -> &'static str {
        match self {
            Self::English => "Worker executable",
            Self::Italiano => "Eseguibile worker",
            Self::Shqip => "Ekzekutuesi i worker-it",
            Self::Spanish => "Ejecutable del worker",
            Self::French => "Exécutable du worker",
            Self::Portuguese => "Executável do worker",
            Self::Russian => "Исполняемый файл воркера",
            Self::Polish => "Plik wykonywalny workera",
            Self::Ukrainian => "Виконуваний файл воркера",
            Self::Turkish => "Worker yürütülebilir dosyası",
            Self::Greek => "Εκτελέσιμο του worker",
            Self::Hindi => "वर्कर निष्पादन योग्य फ़ाइल",
            Self::Chinese => "worker 可执行文件",
            Self::Swedish => "Workerns körbara fil",
        }
    }

    pub fn supervisor_executable_hint(self) -> &'static str {
        match self {
            Self::English => "@php or executable path",
            Self::Italiano => "@php oppure percorso eseguibile",
            Self::Shqip => "@php ose shtegu i ekzekutuesit",
            Self::Spanish => "@php o ruta del ejecutable",
            Self::French => "@php ou chemin de l'exécutable",
            Self::Portuguese => "@php ou caminho do executável",
            Self::Russian => "@php или путь к исполняемому файлу",
            Self::Polish => "@php lub ścieżka do pliku wykonywalnego",
            Self::Ukrainian => "@php або шлях до виконуваного файлу",
            Self::Turkish => "@php veya yürütülebilir dosya yolu",
            Self::Greek => "@php ή διαδρομή εκτελέσιμου",
            Self::Hindi => "@php या निष्पादन योग्य फ़ाइल का पथ",
            Self::Chinese => "@php 或可执行文件路径",
            Self::Swedish => "@php eller sökväg till körbar fil",
        }
    }

    pub fn supervisor_args(self) -> &'static str {
        match self {
            Self::English => "Worker arguments",
            Self::Italiano => "Argomenti worker",
            Self::Shqip => "Argumentet e worker-it",
            Self::Spanish => "Argumentos del worker",
            Self::French => "Arguments du worker",
            Self::Portuguese => "Argumentos do worker",
            Self::Russian => "Аргументы воркера",
            Self::Polish => "Argumenty workera",
            Self::Ukrainian => "Аргументи воркера",
            Self::Turkish => "Worker argümanları",
            Self::Greek => "Ορίσματα του worker",
            Self::Hindi => "वर्कर आर्ग्युमेंट",
            Self::Chinese => "worker 参数",
            Self::Swedish => "Workerns argument",
        }
    }

    pub fn supervisor_args_hint(self) -> &'static str {
        match self {
            Self::English => "One argument per line",
            Self::Italiano => "Un argomento per riga",
            Self::Shqip => "Një argument për rresht",
            Self::Spanish => "Un argumento por línea",
            Self::French => "Un argument par ligne",
            Self::Portuguese => "Um argumento por linha",
            Self::Russian => "Один аргумент на строку",
            Self::Polish => "Jeden argument na wiersz",
            Self::Ukrainian => "Один аргумент на рядок",
            Self::Turkish => "Her satıra bir argüman",
            Self::Greek => "Ένα όρισμα ανά γραμμή",
            Self::Hindi => "प्रति पंक्ति एक आर्ग्युमेंट",
            Self::Chinese => "每行一个参数",
            Self::Swedish => "Ett argument per rad",
        }
    }

    pub fn supervisor_args_note(self) -> &'static str {
        match self {
            Self::English => "Each line is a separate argument. No shell is used.",
            Self::Italiano => "Ogni riga è un argomento separato. Non viene usata una shell.",
            Self::Shqip => "Secila rresht është argument më vete. Nuk përdoret shell.",
            Self::Spanish => "Cada línea es un argumento separado. No se usa shell.",
            Self::French => "Chaque ligne est un argument distinct. Aucun shell n'est utilisé.",
            Self::Portuguese => "Cada linha é um argumento separado. Não é usada nenhuma shell.",
            Self::Russian => "Каждая строка — отдельный аргумент. Shell не используется.",
            Self::Polish => "Każdy wiersz to osobny argument. Shell nie jest używany.",
            Self::Ukrainian => "Кожен рядок — окремий аргумент. Shell не використовується.",
            Self::Turkish => "Her satır ayrı bir argümandır. Shell kullanılmaz.",
            Self::Greek => "Κάθε γραμμή είναι ξεχωριστό όρισμα. Δεν χρησιμοποιείται shell.",
            Self::Hindi => "हर पंक्ति अलग आर्ग्युमेंट है। कोई shell उपयोग नहीं होता।",
            Self::Chinese => "每行是独立参数。不使用 shell。",
            Self::Swedish => "Varje rad är ett separat argument. Ingen shell används.",
        }
    }

    pub fn cron_title(self) -> &'static str {
        match self {
            Self::English => "Scheduled tasks",
            Self::Italiano => "Attività pianificate",
            Self::Shqip => "Detyra të planifikuara",
            Self::Spanish => "Tareas programadas",
            Self::French => "Tâches planifiées",
            Self::Portuguese => "Tarefas agendadas",
            Self::Russian => "Запланированные задачи",
            Self::Polish => "Zadania zaplanowane",
            Self::Ukrainian => "Заплановані завдання",
            Self::Turkish => "Zamanlanmış görevler",
            Self::Greek => "Προγραμματισμένες εργασίες",
            Self::Hindi => "निर्धारित कार्य",
            Self::Chinese => "计划任务",
            Self::Swedish => "Schemalagda uppgifter",
        }
    }

    pub fn cron_check(self) -> &'static str {
        match self {
            Self::English => "Run scheduled tasks with the project",
            Self::Italiano => "Esegui attività pianificate con il progetto",
            Self::Shqip => "Ekzekuto detyrat e planifikuara me projektin",
            Self::Spanish => "Ejecutar tareas programadas con el proyecto",
            Self::French => "Exécuter les tâches planifiées avec le projet",
            Self::Portuguese => "Executar tarefas agendadas com o projeto",
            Self::Russian => "Запускать запланированные задачи вместе с проектом",
            Self::Polish => "Uruchamiaj zadania zaplanowane z projektem",
            Self::Ukrainian => "Запускати заплановані завдання разом із проєктом",
            Self::Turkish => "Zamanlanmış görevleri projeyle çalıştır",
            Self::Greek => "Εκτέλεση προγραμματισμένων εργασιών με το έργο",
            Self::Hindi => "प्रोजेक्ट के साथ निर्धारित कार्य चलाएँ",
            Self::Chinese => "随项目运行计划任务",
            Self::Swedish => "Kör schemalagda uppgifter med projektet",
        }
    }

    pub fn cron_lines(self) -> &'static str {
        match self {
            Self::English => "Schedules",
            Self::Italiano => "Pianificazioni",
            Self::Shqip => "Planifikimet",
            Self::Spanish => "Programaciones",
            Self::French => "Planifications",
            Self::Portuguese => "Agendamentos",
            Self::Russian => "Расписания",
            Self::Polish => "Harmonogramy",
            Self::Ukrainian => "Розклади",
            Self::Turkish => "Zamanlamalar",
            Self::Greek => "Χρονοδιαγράμματα",
            Self::Hindi => "शेड्यूल",
            Self::Chinese => "调度",
            Self::Swedish => "Scheman",
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
            Self::Spanish => {
                "Una por línea: minutos horas día mes día-semana + comando. @php es el PHP del proyecto. Vacío usa el valor del perfil cuando existe."
            }
            Self::French => {
                "Une par ligne : minutes heures jour mois jour-semaine + commande. @php correspond au PHP du projet. Vide utilise la valeur du profil lorsqu'elle existe."
            }
            Self::Portuguese => {
                "Uma por linha: minutos horas dia mês dia-da-semana + comando. @php é o PHP do projeto. Vazio usa o valor do perfil quando existe."
            }
            Self::Russian => {
                "По одному в строке: минуты часы день месяц день-недели + команда. @php — PHP проекта. Пусто — значение профиля, где оно есть."
            }
            Self::Polish => {
                "Jedna na wiersz: minuty godziny dzień miesiąc dzień-tygodnia + komenda. @php to PHP projektu. Puste używa wartości profilu, jeśli istnieje."
            }
            Self::Ukrainian => {
                "Один на рядок: хвилини години день місяць день-тижня + команда. @php — PHP проєкту. Порожньо — значення профілю, де воно є."
            }
            Self::Turkish => {
                "Her satıra bir tane: dakika saat gün ay haftanın-günü + komut. @php projenin PHP'sidir. Boş bırakılırsa profil varsayılanı kullanılır."
            }
            Self::Greek => {
                "Ένα ανά γραμμή: λεπτά ώρες ημέρα μήνας ημέρα-εβδομάδας + εντολή. Το @php είναι η PHP του έργου. Κενό χρησιμοποιεί την προεπιλογή του προφίλ όπου υπάρχει."
            }
            Self::Hindi => {
                "प्रति पंक्ति एक: मिनट घंटे दिन महीना सप्ताह-का-दिन + कमांड। @php प्रोजेक्ट की PHP है। खाली छोड़ने पर प्रोफ़ाइल डिफ़ॉल्ट उपयोग होता है जहाँ मौजूद हो।"
            }
            Self::Chinese => {
                "每行一条：分 时 日 月 星期 + 命令。@php 表示项目的 PHP。留空则使用预设默认值（如有）。"
            }
            Self::Swedish => {
                "En per rad: minuter timmar dag månad veckodag + kommando. @php är projektets PHP. Tomt använder profilens standard där en sådan finns."
            }
        }
    }

    pub fn mail_title(self) -> &'static str {
        match self {
            Self::English => "Dev email",
            Self::Italiano => "Email di sviluppo",
            Self::Shqip => "Email zhvillimi",
            Self::Spanish => "Email de desarrollo",
            Self::French => "Email de développement",
            Self::Portuguese => "Email de desenvolvimento",
            Self::Russian => "Почта для разработки",
            Self::Polish => "Poczta deweloperska",
            Self::Ukrainian => "Пошта для розробки",
            Self::Turkish => "Geliştirme e-postası",
            Self::Greek => "Email ανάπτυξης",
            Self::Hindi => "डेव ईमेल",
            Self::Chinese => "开发邮件",
            Self::Swedish => "Utvecklingsmejl",
        }
    }

    pub fn mail_check(self) -> &'static str {
        match self {
            Self::English => "Enable Mailpit for this project",
            Self::Italiano => "Attiva Mailpit per questo progetto",
            Self::Shqip => "Aktivizo Mailpit për këtë projekt",
            Self::Spanish => "Activar Mailpit para este proyecto",
            Self::French => "Activer Mailpit pour ce projet",
            Self::Portuguese => "Ativar o Mailpit para este projeto",
            Self::Russian => "Включить Mailpit для этого проекта",
            Self::Polish => "Włącz Mailpit dla tego projektu",
            Self::Ukrainian => "Увімкнути Mailpit для цього проєкту",
            Self::Turkish => "Bu proje için Mailpit'i etkinleştir",
            Self::Greek => "Ενεργοποίηση Mailpit για αυτό το έργο",
            Self::Hindi => "इस प्रोजेक्ट के लिए Mailpit सक्षम करें",
            Self::Chinese => "为此项目启用 Mailpit",
            Self::Swedish => "Aktivera Mailpit för detta projekt",
        }
    }

    // Validation errors.

    pub fn error_no_path(self) -> &'static str {
        match self {
            Self::English => "Select the source folder",
            Self::Italiano => "Seleziona la cartella del codice sorgente",
            Self::Shqip => "Zgjidh dosjen burimore",
            Self::Spanish => "Selecciona la carpeta del código fuente",
            Self::French => "Sélectionnez le dossier du code source",
            Self::Portuguese => "Selecione a pasta do código-fonte",
            Self::Russian => "Выберите папку с исходным кодом",
            Self::Polish => "Wybierz folder z kodem źródłowym",
            Self::Ukrainian => "Виберіть теку з вихідним кодом",
            Self::Turkish => "Kaynak kod klasörünü seçin",
            Self::Greek => "Επιλέξτε τον φάκελο πηγαίου κώδικα",
            Self::Hindi => "स्रोत कोड फ़ोल्डर चुनें",
            Self::Chinese => "选择源代码文件夹",
            Self::Swedish => "Välj källkodsmappen",
        }
    }

    pub fn error_no_executable(self) -> &'static str {
        match self {
            Self::English => "Provide the supervisor worker executable",
            Self::Italiano => "Indica l'eseguibile del worker supervisor",
            Self::Shqip => "Jep ekzekutuesin e worker-it supervisor",
            Self::Spanish => "Indica el ejecutable del worker supervisor",
            Self::French => "Indiquez l'exécutable du worker supervisor",
            Self::Portuguese => "Indique o executável do worker supervisor",
            Self::Russian => "Укажите исполняемый файл воркера supervisor",
            Self::Polish => "Podaj plik wykonywalny workera supervisor",
            Self::Ukrainian => "Укажіть виконуваний файл воркера supervisor",
            Self::Turkish => "Supervisor worker yürütülebilir dosyasını belirtin",
            Self::Greek => "Δώστε το εκτελέσιμο του worker supervisor",
            Self::Hindi => "supervisor वर्कर की निष्पादन योग्य फ़ाइल दें",
            Self::Chinese => "提供 supervisor worker 可执行文件",
            Self::Swedish => "Ange workerns körbara fil för supervisor",
        }
    }

    // Folder picker.

    pub fn picker_title(self) -> &'static str {
        match self {
            Self::English => "Select the source folder",
            Self::Italiano => "Seleziona la cartella sorgente",
            Self::Shqip => "Zgjidh dosjen burimore",
            Self::Spanish => "Selecciona la carpeta de origen",
            Self::French => "Sélectionnez le dossier source",
            Self::Portuguese => "Selecione a pasta de origem",
            Self::Russian => "Выберите папку с исходным кодом",
            Self::Polish => "Wybierz folder źródłowy",
            Self::Ukrainian => "Виберіть теку з вихідним кодом",
            Self::Turkish => "Kaynak klasörünü seçin",
            Self::Greek => "Επιλέξτε τον φάκελο πηγαίου κώδικα",
            Self::Hindi => "स्रोत फ़ोल्डर चुनें",
            Self::Chinese => "选择源文件夹",
            Self::Swedish => "Välj källkodsmappen",
        }
    }

    pub fn picker_parent(self) -> &'static str {
        match self {
            Self::English => "Parent folder",
            Self::Italiano => "Cartella superiore",
            Self::Shqip => "Dosja prind",
            Self::Spanish => "Carpeta superior",
            Self::French => "Dossier parent",
            Self::Portuguese => "Pasta superior",
            Self::Russian => "Родительская папка",
            Self::Polish => "Folder nadrzędny",
            Self::Ukrainian => "Батьківська тека",
            Self::Turkish => "Üst klasör",
            Self::Greek => "Γονικός φάκελος",
            Self::Hindi => "मूल फ़ोल्डर",
            Self::Chinese => "上级文件夹",
            Self::Swedish => "Överordnad mapp",
        }
    }

    pub fn picker_home(self) -> &'static str {
        match self {
            Self::English
            | Self::Italiano
            | Self::Shqip
            | Self::Spanish
            | Self::French
            | Self::Portuguese
            | Self::Russian
            | Self::Polish
            | Self::Ukrainian
            | Self::Turkish
            | Self::Greek
            | Self::Hindi
            | Self::Chinese
            | Self::Swedish => "Home",
        }
    }

    pub fn picker_empty(self) -> &'static str {
        match self {
            Self::English => "No subfolders. You can select this folder.",
            Self::Italiano => "Nessuna sottocartella. Puoi selezionare questa cartella.",
            Self::Shqip => "Asnjë nëndosje. Mund të zgjedhësh këtë dosje.",
            Self::Spanish => "No hay subcarpetas. Puedes seleccionar esta carpeta.",
            Self::French => "Aucun sous-dossier. Vous pouvez sélectionner ce dossier.",
            Self::Portuguese => "Não há subpastas. Pode selecionar esta pasta.",
            Self::Russian => "Подпапок нет. Можно выбрать эту папку.",
            Self::Polish => "Brak podfolderów. Możesz wybrać ten folder.",
            Self::Ukrainian => "Підтек немає. Можна вибрати цю теку.",
            Self::Turkish => "Alt klasör yok. Bu klasörü seçebilirsiniz.",
            Self::Greek => "Δεν υπάρχουν υποφάκελοι. Μπορείτε να επιλέξετε αυτόν τον φάκελο.",
            Self::Hindi => "कोई उप-फ़ोल्डर नहीं। आप यह फ़ोल्डर चुन सकते हैं।",
            Self::Chinese => "没有子文件夹。可以选择此文件夹。",
            Self::Swedish => "Inga undermappar. Du kan välja den här mappen.",
        }
    }

    pub fn picker_open(self, name: &str) -> String {
        match self {
            Self::English => format!("Open folder: {name}"),
            Self::Italiano => format!("Apri cartella: {name}"),
            Self::Shqip => format!("Hap dosjen: {name}"),
            Self::Spanish => format!("Abrir carpeta: {name}"),
            Self::French => format!("Ouvrir le dossier : {name}"),
            Self::Portuguese => format!("Abrir pasta: {name}"),
            Self::Russian => format!("Открыть папку: {name}"),
            Self::Polish => format!("Otwórz folder: {name}"),
            Self::Ukrainian => format!("Відкрити теку: {name}"),
            Self::Turkish => format!("Klasörü aç: {name}"),
            Self::Greek => format!("Άνοιγμα φακέλου: {name}"),
            Self::Hindi => format!("फ़ोल्डर खोलें: {name}"),
            Self::Chinese => format!("打开文件夹：{name}"),
            Self::Swedish => format!("Öppna mapp: {name}"),
        }
    }

    pub fn picker_use(self) -> &'static str {
        match self {
            Self::English => "Use this folder",
            Self::Italiano => "Usa questa cartella",
            Self::Shqip => "Përdor këtë dosje",
            Self::Spanish => "Usar esta carpeta",
            Self::French => "Utiliser ce dossier",
            Self::Portuguese => "Usar esta pasta",
            Self::Russian => "Использовать эту папку",
            Self::Polish => "Użyj tego folderu",
            Self::Ukrainian => "Використати цю теку",
            Self::Turkish => "Bu klasörü kullan",
            Self::Greek => "Χρήση αυτού του φακέλου",
            Self::Hindi => "यह फ़ोल्डर उपयोग करें",
            Self::Chinese => "使用此文件夹",
            Self::Swedish => "Använd den här mappen",
        }
    }

    pub fn picker_back(self) -> &'static str {
        match self {
            Self::English => "Back",
            Self::Italiano => "Indietro",
            Self::Shqip => "Prapa",
            Self::Spanish => "Atrás",
            Self::French => "Précédent",
            Self::Portuguese => "Voltar",
            Self::Russian | Self::Ukrainian => "Назад",
            Self::Polish => "Wstecz",
            Self::Turkish => "Geri",
            Self::Greek => "Πίσω",
            Self::Hindi => "वापस",
            Self::Chinese => "后退",
            Self::Swedish => "Tillbaka",
        }
    }

    pub fn picker_forward(self) -> &'static str {
        match self {
            Self::English => "Forward",
            Self::Italiano => "Avanti",
            Self::Shqip => "Përpara",
            Self::Spanish => "Adelante",
            Self::French => "Suivant",
            Self::Portuguese => "Avançar",
            Self::Russian => "Вперёд",
            Self::Polish => "Dalej",
            Self::Ukrainian => "Уперед",
            Self::Turkish => "İleri",
            Self::Greek => "Μπροστά",
            Self::Hindi => "आगे",
            Self::Chinese => "前进",
            Self::Swedish => "Framåt",
        }
    }

    pub fn picker_desktop(self) -> &'static str {
        match self {
            Self::English | Self::Shqip => "Desktop",
            Self::Italiano => "Scrivania",
            Self::Spanish => "Escritorio",
            Self::French => "Bureau",
            Self::Portuguese => "Ambiente de trabalho",
            Self::Russian => "Рабочий стол",
            Self::Polish => "Pulpit",
            Self::Ukrainian => "Робочий стіл",
            Self::Turkish => "Masaüstü",
            Self::Greek => "Επιφάνεια εργασίας",
            Self::Hindi => "डेस्कटॉप",
            Self::Chinese => "桌面",
            Self::Swedish => "Skrivbord",
        }
    }

    pub fn picker_documents(self) -> &'static str {
        match self {
            Self::English | Self::French => "Documents",
            Self::Italiano => "Documenti",
            Self::Shqip => "Dokumentet",
            Self::Spanish | Self::Portuguese => "Documentos",
            Self::Russian => "Документы",
            Self::Polish => "Dokumenty",
            Self::Ukrainian => "Документи",
            Self::Turkish => "Belgeler",
            Self::Greek => "Έγγραφα",
            Self::Hindi => "दस्तावेज़",
            Self::Chinese => "文档",
            Self::Swedish => "Dokument",
        }
    }

    pub fn picker_downloads(self) -> &'static str {
        match self {
            Self::English => "Downloads",
            Self::Italiano => "Download",
            Self::Shqip => "Shkarkimet",
            Self::Spanish => "Descargas",
            Self::French => "Téléchargements",
            Self::Portuguese => "Transferências",
            Self::Russian => "Загрузки",
            Self::Polish => "Pobrane",
            Self::Ukrainian => "Завантаження",
            Self::Turkish => "İndirilenler",
            Self::Greek => "Λήψεις",
            Self::Hindi => "डाउनलोड",
            Self::Chinese => "下载",
            Self::Swedish => "Hämtningar",
        }
    }

    // Remove dialog.

    pub fn remove_title(self) -> &'static str {
        match self {
            Self::English => "Remove project",
            Self::Italiano => "Rimuovi progetto",
            Self::Shqip => "Hiq projektin",
            Self::Spanish => "Eliminar proyecto",
            Self::French => "Supprimer le projet",
            Self::Portuguese => "Remover projeto",
            Self::Russian => "Удалить проект",
            Self::Polish => "Usuń projekt",
            Self::Ukrainian => "Видалити проєкт",
            Self::Turkish => "Projeyi kaldır",
            Self::Greek => "Αφαίρεση έργου",
            Self::Hindi => "प्रोजेक्ट हटाएँ",
            Self::Chinese => "移除项目",
            Self::Swedish => "Ta bort projekt",
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
            Self::Spanish => {
                format!(
                    "¿Eliminar {name} de LocalWeb? La carpeta de origen y los datos de la base de datos se conservan. El dominio local y la configuración del proyecto se eliminan."
                )
            }
            Self::French => {
                format!(
                    "Supprimer {name} de LocalWeb ? Le dossier source et les données de la base sont conservés. Le domaine local et la configuration du projet sont supprimés."
                )
            }
            Self::Portuguese => {
                format!(
                    "Remover {name} do LocalWeb? A pasta de origem e os dados da base de dados são mantidos. O domínio local e a configuração do projeto são removidos."
                )
            }
            Self::Russian => {
                format!(
                    "Удалить {name} из LocalWeb? Папка с исходниками и данные базы сохранятся. Локальный домен и конфигурация проекта будут удалены."
                )
            }
            Self::Polish => {
                format!(
                    "Usunąć {name} z LocalWeb? Folder źródłowy i dane bazy danych zostaną zachowane. Domena lokalna i konfiguracja projektu zostaną usunięte."
                )
            }
            Self::Ukrainian => {
                format!(
                    "Видалити {name} з LocalWeb? Тека з вихідним кодом і дані бази залишаться. Локальний домен і конфігурацію проєкту буде видалено."
                )
            }
            Self::Turkish => {
                format!(
                    "{name} projesi LocalWeb'den kaldırılsın mı? Kaynak klasör ve veritabanı verileri korunur. Yerel alan adı ve proje yapılandırması kaldırılır."
                )
            }
            Self::Greek => {
                format!(
                    "Να αφαιρεθεί το {name} από το LocalWeb; Ο φάκελος πηγαίου κώδικα και τα δεδομένα της βάσης διατηρούνται. Ο τοπικός τομέας και η ρύθμιση του έργου αφαιρούνται."
                )
            }
            Self::Hindi => {
                format!(
                    "{name} को LocalWeb से हटाएँ? स्रोत फ़ोल्डर और डेटाबेस डेटा सुरक्षित रहते हैं। स्थानीय डोमेन और प्रोजेक्ट कॉन्फ़िगरेशन हट जाते हैं।"
                )
            }
            Self::Chinese => {
                format!(
                    "将 {name} 从 LocalWeb 中移除？源文件夹和数据库数据会保留，本地域名和项目配置将被删除。"
                )
            }
            Self::Swedish => {
                format!(
                    "Ta bort {name} från LocalWeb? Källkodsmappen och databasens data behålls. Den lokala domänen och projektkonfigurationen tas bort."
                )
            }
        }
    }

    pub fn remove_confirm(self) -> &'static str {
        match self {
            Self::English => "Remove configuration",
            Self::Italiano => "Rimuovi configurazione",
            Self::Shqip => "Hiq konfigurimin",
            Self::Spanish => "Eliminar configuración",
            Self::French => "Supprimer la configuration",
            Self::Portuguese => "Remover configuração",
            Self::Russian => "Удалить конфигурацию",
            Self::Polish => "Usuń konfigurację",
            Self::Ukrainian => "Видалити конфігурацію",
            Self::Turkish => "Yapılandırmayı kaldır",
            Self::Greek => "Αφαίρεση ρύθμισης",
            Self::Hindi => "कॉन्फ़िगरेशन हटाएँ",
            Self::Chinese => "移除配置",
            Self::Swedish => "Ta bort konfiguration",
        }
    }

    // Notices.

    pub fn notice_setup(self) -> &'static str {
        match self {
            Self::English => "Initial setup complete",
            Self::Italiano => "Configurazione iniziale completata",
            Self::Shqip => "Konfigurimi fillestar u përfundua",
            Self::Spanish => "Configuración inicial completada",
            Self::French => "Configuration initiale terminée",
            Self::Portuguese => "Configuração inicial concluída",
            Self::Russian => "Первичная настройка завершена",
            Self::Polish => "Konfiguracja wstępna zakończona",
            Self::Ukrainian => "Початкове налаштування завершено",
            Self::Turkish => "İlk kurulum tamamlandı",
            Self::Greek => "Η αρχική ρύθμιση ολοκληρώθηκε",
            Self::Hindi => "प्रारंभिक सेटअप पूरा हुआ",
            Self::Chinese => "初始设置完成",
            Self::Swedish => "Den inledande konfigurationen är klar",
        }
    }

    pub fn notice_ca(self) -> &'static str {
        match self {
            Self::English => "HTTPS CA installed on the system",
            Self::Italiano => "CA HTTPS installata nel sistema",
            Self::Shqip => "CA HTTPS u instalua në sistem",
            Self::Spanish => "CA HTTPS instalada en el sistema",
            Self::French => "AC HTTPS installée sur le système",
            Self::Portuguese => "CA HTTPS instalada no sistema",
            Self::Russian => "CA HTTPS установлен в системе",
            Self::Polish => "CA HTTPS zainstalowany w systemie",
            Self::Ukrainian => "CA HTTPS встановлено в системі",
            Self::Turkish => "HTTPS CA sisteme yüklendi",
            Self::Greek => "Το CA HTTPS εγκαταστάθηκε στο σύστημα",
            Self::Hindi => "HTTPS CA सिस्टम पर इंस्टॉल हो गया",
            Self::Chinese => "HTTPS CA 已安装到系统",
            Self::Swedish => "HTTPS-CA installerat på systemet",
        }
    }

    pub fn notice_started(self) -> &'static str {
        match self {
            Self::English => "LocalWeb started",
            Self::Italiano => "LocalWeb è stato avviato",
            Self::Shqip => "LocalWeb u nis",
            Self::Spanish | Self::Portuguese => "LocalWeb iniciado",
            Self::French => "LocalWeb démarré",
            Self::Russian => "LocalWeb запущен",
            Self::Polish => "LocalWeb uruchomiony",
            Self::Ukrainian => "LocalWeb запущено",
            Self::Turkish => "LocalWeb başlatıldı",
            Self::Greek => "Το LocalWeb ξεκίνησε",
            Self::Hindi => "LocalWeb शुरू हो गया",
            Self::Chinese => "LocalWeb 已启动",
            Self::Swedish => "LocalWeb startat",
        }
    }

    pub fn notice_hosts(self) -> &'static str {
        match self {
            Self::English => "Local domains synced",
            Self::Italiano => "Domini locali sincronizzati",
            Self::Shqip => "Domain-et lokale u sinkronizuan",
            Self::Spanish => "Dominios locales sincronizados",
            Self::French => "Domaines locaux synchronisés",
            Self::Portuguese => "Domínios locais sincronizados",
            Self::Russian => "Локальные домены синхронизированы",
            Self::Polish => "Domeny lokalne zsynchronizowane",
            Self::Ukrainian => "Локальні домени синхронізовано",
            Self::Turkish => "Yerel alan adları eşitlendi",
            Self::Greek => "Οι τοπικοί τομείς συγχρονίστηκαν",
            Self::Hindi => "स्थानीय डोमेन सिंक हो गए",
            Self::Chinese => "本地域名已同步",
            Self::Swedish => "Lokala domäner synkroniserade",
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
            Self::Spanish => {
                format!("Proyecto {name} añadido. Sincronizando el dominio local…")
            }
            Self::French => format!("Projet {name} ajouté. Synchronisation du domaine local…"),
            Self::Portuguese => {
                format!("Projeto {name} adicionado. A sincronizar o domínio local…")
            }
            Self::Russian => {
                format!("Проект {name} добавлен. Синхронизация локального домена…")
            }
            Self::Polish => {
                format!("Projekt {name} dodany. Synchronizowanie domeny lokalnej…")
            }
            Self::Ukrainian => {
                format!("Проєкт {name} додано. Синхронізація локального домену…")
            }
            Self::Turkish => format!("{name} projesi eklendi. Yerel alan adı eşitleniyor…"),
            Self::Greek => {
                format!("Το έργο {name} προστέθηκε. Συγχρονισμός του τοπικού τομέα…")
            }
            Self::Hindi => {
                format!("प्रोजेक्ट {name} जोड़ा गया। स्थानीय डोमेन सिंक हो रहा है…")
            }
            Self::Chinese => format!("项目 {name} 已添加。正在同步本地域名…"),
            Self::Swedish => {
                format!("Projektet {name} har lagts till. Synkroniserar den lokala domänen…")
            }
        }
    }

    pub fn notice_unexpected(self, detail: &str) -> String {
        match self {
            Self::English => format!("Unexpected daemon response: {detail}"),
            Self::Italiano => format!("Risposta inattesa dal daemon: {detail}"),
            Self::Shqip => format!("Përgjigje e papritur nga daemon-i: {detail}"),
            Self::Spanish => format!("Respuesta inesperada del daemon: {detail}"),
            Self::French => format!("Réponse inattendue du daemon : {detail}"),
            Self::Portuguese => format!("Resposta inesperada do daemon: {detail}"),
            Self::Russian => format!("Неожиданный ответ daemon: {detail}"),
            Self::Polish => format!("Nieoczekiwana odpowiedź daemon: {detail}"),
            Self::Ukrainian => format!("Неочікувана відповідь daemon: {detail}"),
            Self::Turkish => format!("Daemon'dan beklenmeyen yanıt: {detail}"),
            Self::Greek => format!("Απροσδόκητη απάντηση από το daemon: {detail}"),
            Self::Hindi => format!("daemon से अप्रत्याशित प्रतिक्रिया: {detail}"),
            Self::Chinese => format!("daemon 返回了意外响应：{detail}"),
            Self::Swedish => format!("Oväntat svar från daemon: {detail}"),
        }
    }

    pub fn notice_browser(self, error: &str) -> String {
        match self {
            Self::English => format!("Cannot open the browser: {error}"),
            Self::Italiano => format!("Impossibile aprire il browser: {error}"),
            Self::Shqip => format!("Nuk hapet dot shfletuesi: {error}"),
            Self::Spanish => format!("No se puede abrir el navegador: {error}"),
            Self::French => format!("Impossible d'ouvrir le navigateur : {error}"),
            Self::Portuguese => format!("Não é possível abrir o navegador: {error}"),
            Self::Russian => format!("Не удалось открыть браузер: {error}"),
            Self::Polish => format!("Nie można otworzyć przeglądarki: {error}"),
            Self::Ukrainian => format!("Не вдалося відкрити браузер: {error}"),
            Self::Turkish => format!("Tarayıcı açılamıyor: {error}"),
            Self::Greek => {
                format!("Δεν είναι δυνατό το άνοιγμα του προγράμματος περιήγησης: {error}")
            }
            Self::Hindi => format!("ब्राउज़र नहीं खोल सकते: {error}"),
            Self::Chinese => format!("无法打开浏览器：{error}"),
            Self::Swedish => format!("Kan inte öppna webbläsaren: {error}"),
        }
    }

    // Value labels.

    pub fn profile_name(self, profile: Profile) -> &'static str {
        match profile {
            Profile::GenericPhp => match self {
                Self::English => "Generic PHP",
                Self::Italiano => "PHP generico",
                Self::Shqip => "PHP e përgjithshme",
                Self::Spanish | Self::Portuguese => "PHP genérico",
                Self::French => "PHP générique",
                Self::Russian => "Обычный PHP",
                Self::Polish => "Ogólny PHP",
                Self::Ukrainian => "Звичайний PHP",
                Self::Turkish => "Genel PHP",
                Self::Greek => "Γενική PHP",
                Self::Hindi => "सामान्य PHP",
                Self::Chinese => "通用 PHP",
                Self::Swedish => "Generisk PHP",
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
                Self::Spanish => "Sin base de datos",
                Self::French => "Aucune base de données",
                Self::Portuguese => "Sem base de dados",
                Self::Russian => "Без базы данных",
                Self::Polish => "Bez bazy danych",
                Self::Ukrainian => "Без бази даних",
                Self::Turkish => "Veritabanı yok",
                Self::Greek => "Χωρίς βάση δεδομένων",
                Self::Hindi => "कोई डेटाबेस नहीं",
                Self::Chinese => "无数据库",
                Self::Swedish => "Ingen databas",
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
                Self::Spanish => "Sin caché",
                Self::French => "Aucun cache",
                Self::Portuguese => "Sem cache",
                Self::Russian => "Без кэша",
                Self::Polish => "Bez cache",
                Self::Ukrainian => "Без кешу",
                Self::Turkish => "Önbellek yok",
                Self::Greek => "Χωρίς cache",
                Self::Hindi => "कोई कैश नहीं",
                Self::Chinese => "无缓存",
                Self::Swedish => "Ingen cache",
            },
            Cache::Redis => "Redis",
            Cache::Memcached => match self {
                Self::English => "Memcached (unavailable)",
                Self::Italiano => "Memcached (non disponibile)",
                Self::Shqip => "Memcached (i padisponueshëm)",
                Self::Spanish => "Memcached (no disponible)",
                Self::French => "Memcached (indisponible)",
                Self::Portuguese => "Memcached (indisponível)",
                Self::Russian => "Memcached (недоступен)",
                Self::Polish => "Memcached (niedostępny)",
                Self::Ukrainian => "Memcached (недоступний)",
                Self::Turkish => "Memcached (kullanılamıyor)",
                Self::Greek => "Memcached (μη διαθέσιμο)",
                Self::Hindi => "Memcached (अनुपलब्ध)",
                Self::Chinese => "Memcached（不可用）",
                Self::Swedish => "Memcached (otillgänglig)",
            },
        }
    }

    pub fn checking_status(self) -> &'static str {
        match self {
            Self::English => "Checking status…",
            Self::Italiano => "Verifica stato…",
            Self::Shqip => "Duke kontrolluar statusin…",
            Self::Spanish => "Comprobando estado…",
            Self::French => "Vérification de l'état…",
            Self::Portuguese => "A verificar o estado…",
            Self::Russian => "Проверка состояния…",
            Self::Polish => "Sprawdzanie stanu…",
            Self::Ukrainian => "Перевірка стану…",
            Self::Turkish => "Durum denetleniyor…",
            Self::Greek => "Έλεγχος κατάστασης…",
            Self::Hindi => "स्थिति जाँची जा रही है…",
            Self::Chinese => "正在检查状态…",
            Self::Swedish => "Kontrollerar status…",
        }
    }
    pub fn daemon_unavailable(self) -> &'static str {
        match self {
            Self::English => "Daemon unavailable",
            Self::Italiano => "Daemon non raggiungibile",
            Self::Shqip => "Daemon-i i padisponueshëm",
            Self::Spanish => "Daemon no disponible",
            Self::French => "Daemon indisponible",
            Self::Portuguese => "Daemon indisponível",
            Self::Russian => "Daemon недоступен",
            Self::Polish => "Daemon niedostępny",
            Self::Ukrainian => "Daemon недоступний",
            Self::Turkish => "Daemon kullanılamıyor",
            Self::Greek => "Το Daemon δεν είναι διαθέσιμο",
            Self::Hindi => "Daemon उपलब्ध नहीं",
            Self::Chinese => "Daemon 不可用",
            Self::Swedish => "Daemon inte tillgänglig",
        }
    }
    pub fn loading_projects(self) -> &'static str {
        match self {
            Self::English => "Loading projects…",
            Self::Italiano => "Caricamento progetti…",
            Self::Shqip => "Duke ngarkuar projektet…",
            Self::Spanish => "Cargando proyectos…",
            Self::French => "Chargement des projets…",
            Self::Portuguese => "A carregar projetos…",
            Self::Russian => "Загрузка проектов…",
            Self::Polish => "Wczytywanie projektów…",
            Self::Ukrainian => "Завантаження проєктів…",
            Self::Turkish => "Projeler yükleniyor…",
            Self::Greek => "Φόρτωση έργων…",
            Self::Hindi => "प्रोजेक्ट लोड हो रहे हैं…",
            Self::Chinese => "正在加载项目…",
            Self::Swedish => "Laddar projekt…",
        }
    }
    pub fn last_known_state(self) -> &'static str {
        match self {
            Self::English => "Last known state",
            Self::Italiano => "Ultimo stato noto",
            Self::Shqip => "Statusi i fundit i njohur",
            Self::Spanish => "Último estado conocido",
            Self::French => "Dernier état connu",
            Self::Portuguese => "Último estado conhecido",
            Self::Russian => "Последнее известное состояние",
            Self::Polish => "Ostatni znany stan",
            Self::Ukrainian => "Останній відомий стан",
            Self::Turkish => "Son bilinen durum",
            Self::Greek => "Τελευταία γνωστή κατάσταση",
            Self::Hindi => "अंतिम ज्ञात स्थिति",
            Self::Chinese => "最后已知状态",
            Self::Swedish => "Senast kända status",
        }
    }
    pub fn refresh(self) -> &'static str {
        match self {
            Self::English => "Refresh status",
            Self::Italiano => "Aggiorna stato",
            Self::Shqip => "Rifresko statusin",
            Self::Spanish => "Actualizar estado",
            Self::French => "Actualiser l'état",
            Self::Portuguese => "Atualizar estado",
            Self::Russian => "Обновить состояние",
            Self::Polish => "Odśwież stan",
            Self::Ukrainian => "Оновити стан",
            Self::Turkish => "Durumu yenile",
            Self::Greek => "Ανανέωση κατάστασης",
            Self::Hindi => "स्थिति रीफ़्रेश करें",
            Self::Chinese => "刷新状态",
            Self::Swedish => "Uppdatera status",
        }
    }
    pub fn sync_domains(self) -> &'static str {
        match self {
            Self::English => "Sync local domains",
            Self::Italiano => "Sincronizza domini locali",
            Self::Shqip => "Sinkronizo domain-et lokale",
            Self::Spanish => "Sincronizar dominios locales",
            Self::French => "Synchroniser les domaines locaux",
            Self::Portuguese => "Sincronizar domínios locais",
            Self::Russian => "Синхронизировать локальные домены",
            Self::Polish => "Synchronizuj domeny lokalne",
            Self::Ukrainian => "Синхронізувати локальні домени",
            Self::Turkish => "Yerel alan adlarını eşitle",
            Self::Greek => "Συγχρονισμός τοπικών τομέων",
            Self::Hindi => "स्थानीय डोमेन सिंक करें",
            Self::Chinese => "同步本地域名",
            Self::Swedish => "Synkronisera lokala domäner",
        }
    }
    pub fn repair_setup(self) -> &'static str {
        match self {
            Self::English => "Repair setup",
            Self::Italiano => "Ripristina configurazione",
            Self::Shqip => "Riparo konfigurimin",
            Self::Spanish => "Reparar configuración",
            Self::French => "Réparer la configuration",
            Self::Portuguese => "Reparar configuração",
            Self::Russian => "Восстановить настройку",
            Self::Polish => "Napraw konfigurację",
            Self::Ukrainian => "Відновити налаштування",
            Self::Turkish => "Kurulumu onar",
            Self::Greek => "Επιδιόρθωση ρύθμισης",
            Self::Hindi => "सेटअप ठीक करें",
            Self::Chinese => "修复设置",
            Self::Swedish => "Reparera konfigurationen",
        }
    }
    pub fn technical_details(self) -> &'static str {
        match self {
            Self::English => "Technical details",
            Self::Italiano => "Dettagli tecnici",
            Self::Shqip => "Hollësi teknike",
            Self::Spanish => "Detalles técnicos",
            Self::French => "Détails techniques",
            Self::Portuguese => "Detalhes técnicos",
            Self::Russian => "Технические подробности",
            Self::Polish => "Szczegóły techniczne",
            Self::Ukrainian => "Технічні подробиці",
            Self::Turkish => "Teknik ayrıntılar",
            Self::Greek => "Τεχνικές λεπτομέρειες",
            Self::Hindi => "तकनीकी विवरण",
            Self::Chinese => "技术细节",
            Self::Swedish => "Tekniska detaljer",
        }
    }
    pub fn advanced(self) -> &'static str {
        match self {
            Self::English => "Advanced",
            Self::Italiano => "Avanzate",
            Self::Shqip => "Të avancuara",
            Self::Spanish => "Avanzado",
            Self::French => "Avancé",
            Self::Portuguese => "Avançado",
            Self::Russian => "Дополнительно",
            Self::Polish => "Zaawansowane",
            Self::Ukrainian => "Додатково",
            Self::Turkish => "Gelişmiş",
            Self::Greek => "Για προχωρημένους",
            Self::Hindi => "उन्नत",
            Self::Chinese => "高级",
            Self::Swedish => "Avancerat",
        }
    }

    pub fn settings_mcp_title(self) -> &'static str {
        match self {
            Self::English => "MCP for AI assistants",
            Self::Italiano => "MCP per assistenti AI",
            Self::Shqip => "MCP për asistentët AI",
            Self::Spanish => "MCP para asistentes de IA",
            Self::French => "MCP pour les assistants IA",
            Self::Portuguese => "MCP para assistentes de IA",
            Self::Russian => "MCP для ИИ-ассистентов",
            Self::Polish => "MCP dla asystentów AI",
            Self::Ukrainian => "MCP для ШІ-асистентів",
            Self::Turkish => "Yapay zekâ asistanları için MCP",
            Self::Greek => "MCP για βοηθούς AI",
            Self::Hindi => "AI सहायकों के लिए MCP",
            Self::Chinese => "面向 AI 助手的 MCP",
            Self::Swedish => "MCP för AI-assistenter",
        }
    }

    pub fn settings_mcp_body(self) -> &'static str {
        match self {
            Self::English => {
                "Connect Claude Desktop, Cursor, or another MCP client to manage projects from a conversation."
            }
            Self::Italiano => {
                "Collega Claude Desktop, Cursor o un altro client MCP per gestire i progetti da una conversazione."
            }
            Self::Shqip => {
                "Lidh Claude Desktop, Cursor o një klient tjetër MCP për të menaxhuar projektet nga një bisedë."
            }
            Self::Spanish => {
                "Conecta Claude Desktop, Cursor u otro cliente MCP para gestionar proyectos desde una conversación."
            }
            Self::French => {
                "Connectez Claude Desktop, Cursor ou un autre client MCP pour gérer les projets depuis une conversation."
            }
            Self::Portuguese => {
                "Ligue o Claude Desktop, o Cursor ou outro cliente MCP para gerir projetos a partir de uma conversa."
            }
            Self::Russian => {
                "Подключите Claude Desktop, Cursor или другой MCP-клиент, чтобы управлять проектами из диалога."
            }
            Self::Polish => {
                "Połącz Claude Desktop, Cursor lub innego klienta MCP, aby zarządzać projektami z poziomu rozmowy."
            }
            Self::Ukrainian => {
                "Підключте Claude Desktop, Cursor або інший MCP-клієнт, щоб керувати проєктами з розмови."
            }
            Self::Turkish => {
                "Projeleri bir sohbetten yönetmek için Claude Desktop, Cursor veya başka bir MCP istemcisini bağlayın."
            }
            Self::Greek => {
                "Συνδέστε το Claude Desktop, το Cursor ή άλλο πρόγραμμα-πελάτη MCP για διαχείριση έργων από μια συνομιλία."
            }
            Self::Hindi => {
                "बातचीत से प्रोजेक्ट प्रबंधित करने के लिए Claude Desktop, Cursor या किसी अन्य MCP क्लाइंट को जोड़ें।"
            }
            Self::Chinese => {
                "连接 Claude Desktop、Cursor 或其他 MCP 客户端，即可通过对话管理项目。"
            }
            Self::Swedish => {
                "Anslut Claude Desktop, Cursor eller en annan MCP-klient för att hantera projekt från en konversation."
            }
        }
    }

    pub fn settings_mcp_missing(self) -> &'static str {
        match self {
            Self::English => {
                "The localweb-mcp binary was not found. Install the LocalWeb package or add it to PATH."
            }
            Self::Italiano => {
                "Il binario localweb-mcp non è stato trovato. Installa il pacchetto LocalWeb o aggiungilo al PATH."
            }
            Self::Shqip => {
                "Binari localweb-mcp nuk u gjet. Instalo paketën LocalWeb ose shtoje në PATH."
            }
            Self::Spanish => {
                "No se encontró el binario localweb-mcp. Instala el paquete LocalWeb o añádelo al PATH."
            }
            Self::French => {
                "Le binaire localweb-mcp est introuvable. Installez le paquet LocalWeb ou ajoutez-le au PATH."
            }
            Self::Portuguese => {
                "O binário localweb-mcp não foi encontrado. Instale o pacote LocalWeb ou adicione-o ao PATH."
            }
            Self::Russian => {
                "Двоичный файл localweb-mcp не найден. Установите пакет LocalWeb или добавьте его в PATH."
            }
            Self::Polish => {
                "Nie znaleziono pliku localweb-mcp. Zainstaluj pakiet LocalWeb lub dodaj go do PATH."
            }
            Self::Ukrainian => {
                "Двійковий файл localweb-mcp не знайдено. Встановіть пакет LocalWeb або додайте його до PATH."
            }
            Self::Turkish => {
                "localweb-mcp ikili dosyası bulunamadı. LocalWeb paketini kurun veya PATH'e ekleyin."
            }
            Self::Greek => {
                "Το εκτελέσιμο localweb-mcp δεν βρέθηκε. Εγκαταστήστε το πακέτο LocalWeb ή προσθέστε το στο PATH."
            }
            Self::Hindi => "localweb-mcp बाइनरी नहीं मिली। LocalWeb पैकेज इंस्टॉल करें या इसे PATH में जोड़ें।",
            Self::Chinese => {
                "未找到 localweb-mcp 可执行文件。请安装 LocalWeb 软件包或将其加入 PATH。"
            }
            Self::Swedish => {
                "Binärfilen localweb-mcp hittades inte. Installera LocalWeb-paketet eller lägg till den i PATH."
            }
        }
    }

    pub fn settings_copy_mcp(self) -> &'static str {
        match self {
            Self::English => "Copy configuration",
            Self::Italiano => "Copia configurazione",
            Self::Shqip => "Kopjo konfigurimin",
            Self::Spanish => "Copiar configuración",
            Self::French => "Copier la configuration",
            Self::Portuguese => "Copiar configuração",
            Self::Russian => "Скопировать конфигурацию",
            Self::Polish => "Kopiuj konfigurację",
            Self::Ukrainian => "Копіювати конфігурацію",
            Self::Turkish => "Yapılandırmayı kopyala",
            Self::Greek => "Αντιγραφή ρύθμισης",
            Self::Hindi => "कॉन्फ़िगरेशन कॉपी करें",
            Self::Chinese => "复制配置",
            Self::Swedish => "Kopiera konfiguration",
        }
    }

    pub fn settings_copied(self) -> &'static str {
        match self {
            Self::English => "MCP configuration copied.",
            Self::Italiano => "Configurazione MCP copiata.",
            Self::Shqip => "Konfigurimi MCP u kopjua.",
            Self::Spanish => "Configuración MCP copiada.",
            Self::French => "Configuration MCP copiée.",
            Self::Portuguese => "Configuração MCP copiada.",
            Self::Russian => "Конфигурация MCP скопирована.",
            Self::Polish => "Skopiowano konfigurację MCP.",
            Self::Ukrainian => "Конфігурацію MCP скопійовано.",
            Self::Turkish => "MCP yapılandırması kopyalandı.",
            Self::Greek => "Η ρύθμιση MCP αντιγράφηκε.",
            Self::Hindi => "MCP कॉन्फ़िगरेशन कॉपी हो गया।",
            Self::Chinese => "MCP 配置已复制。",
            Self::Swedish => "MCP-konfigurationen kopierades.",
        }
    }

    pub fn settings_environment_title(self) -> &'static str {
        match self {
            Self::English => "Environment",
            Self::Italiano | Self::Portuguese => "Ambiente",
            Self::Shqip => "Mjedisi",
            Self::Spanish => "Entorno",
            Self::French => "Environnement",
            Self::Russian => "Окружение",
            Self::Polish => "Środowisko",
            Self::Ukrainian => "Середовище",
            Self::Turkish => "Ortam",
            Self::Greek => "Περιβάλλον",
            Self::Hindi => "वातावरण",
            Self::Chinese => "环境",
            Self::Swedish => "Miljö",
        }
    }

    pub fn settings_domain_suffix(self) -> &'static str {
        match self {
            Self::English => "Domain suffix",
            Self::Italiano => "Suffisso dominio",
            Self::Shqip => "Prapashtesa e domenit",
            Self::Spanish => "Sufijo de dominio",
            Self::French => "Suffixe de domaine",
            Self::Portuguese => "Sufixo de domínio",
            Self::Russian => "Суффикс домена",
            Self::Polish => "Przyrostek domeny",
            Self::Ukrainian => "Суфікс домену",
            Self::Turkish => "Alan adı son eki",
            Self::Greek => "Επίθημα τομέα",
            Self::Hindi => "डोमेन सफ़िक्स",
            Self::Chinese => "域名后缀",
            Self::Swedish => "Domänsuffix",
        }
    }

    pub fn settings_example_url(self) -> &'static str {
        match self {
            Self::English => "Example URL",
            Self::Italiano => "URL di esempio",
            Self::Shqip => "URL shembull",
            Self::Spanish => "URL de ejemplo",
            Self::French => "URL d'exemple",
            Self::Portuguese => "URL de exemplo",
            Self::Russian => "Пример URL",
            Self::Polish => "Przykładowy URL",
            Self::Ukrainian => "Приклад URL",
            Self::Turkish => "Örnek URL",
            Self::Greek => "Δείγμα URL",
            Self::Hindi => "उदाहरण URL",
            Self::Chinese => "示例 URL",
            Self::Swedish => "Exempel-URL",
        }
    }

    pub fn settings_defaults(self) -> &'static str {
        match self {
            Self::English => "Project defaults",
            Self::Italiano => "Valori predefiniti progetto",
            Self::Shqip => "Parazgjedhjet e projektit",
            Self::Spanish => "Valores predeterminados",
            Self::French => "Valeurs par défaut",
            Self::Portuguese => "Predefinições do projeto",
            Self::Russian => "Параметры проекта по умолчанию",
            Self::Polish => "Domyślne ustawienia projektu",
            Self::Ukrainian => "Типові налаштування проєкту",
            Self::Turkish => "Proje varsayılanları",
            Self::Greek => "Προεπιλογές έργου",
            Self::Hindi => "प्रोजेक्ट डिफ़ॉल्ट",
            Self::Chinese => "项目默认值",
            Self::Swedish => "Projektstandard",
        }
    }

    pub fn settings_config_folder(self) -> &'static str {
        match self {
            Self::English => "Configuration folder",
            Self::Italiano => "Cartella di configurazione",
            Self::Shqip => "Folderi i konfigurimit",
            Self::Spanish => "Carpeta de configuración",
            Self::French => "Dossier de configuration",
            Self::Portuguese => "Pasta de configuração",
            Self::Russian => "Папка конфигурации",
            Self::Polish => "Folder konfiguracji",
            Self::Ukrainian => "Тека конфігурації",
            Self::Turkish => "Yapılandırma klasörü",
            Self::Greek => "Φάκελος ρύθμισης",
            Self::Hindi => "कॉन्फ़िगरेशन फ़ोल्डर",
            Self::Chinese => "配置文件夹",
            Self::Swedish => "Konfigurationsmapp",
        }
    }

    pub fn settings_read_only(self) -> &'static str {
        match self {
            Self::English => {
                "Read from config.toml. Edit the file and restart LocalWeb to apply changes."
            }
            Self::Italiano => {
                "Letto da config.toml. Modifica il file e riavvia LocalWeb per applicare le modifiche."
            }
            Self::Shqip => {
                "Lexohet nga config.toml. Ndrysho skedarin dhe rinis LocalWeb për t'i aplikuar ndryshimet."
            }
            Self::Spanish => {
                "Se lee de config.toml. Edita el archivo y reinicia LocalWeb para aplicar los cambios."
            }
            Self::French => {
                "Lu depuis config.toml. Modifiez le fichier et redémarrez LocalWeb pour appliquer les changements."
            }
            Self::Portuguese => {
                "Lido de config.toml. Edite o ficheiro e reinicie o LocalWeb para aplicar as alterações."
            }
            Self::Russian => {
                "Читается из config.toml. Измените файл и перезапустите LocalWeb, чтобы применить изменения."
            }
            Self::Polish => {
                "Odczytywane z config.toml. Edytuj plik i uruchom ponownie LocalWeb, aby zastosować zmiany."
            }
            Self::Ukrainian => {
                "Читається з config.toml. Змініть файл і перезапустіть LocalWeb, щоб застосувати зміни."
            }
            Self::Turkish => {
                "config.toml dosyasından okunur. Değişiklikleri uygulamak için dosyayı düzenleyip LocalWeb'i yeniden başlatın."
            }
            Self::Greek => {
                "Διαβάζεται από το config.toml. Επεξεργαστείτε το αρχείο και επανεκκινήστε το LocalWeb για να εφαρμοστούν οι αλλαγές."
            }
            Self::Hindi => {
                "config.toml से पढ़ा जाता है। बदलाव लागू करने के लिए फ़ाइल संपादित करें और LocalWeb को पुनः आरंभ करें।"
            }
            Self::Chinese => "读取自 config.toml。编辑该文件并重启 LocalWeb 以应用更改。",
            Self::Swedish => {
                "Läses från config.toml. Redigera filen och starta om LocalWeb för att tillämpa ändringarna."
            }
        }
    }

    pub fn check_form(self) -> &'static str {
        match self {
            Self::English => "Check the project settings",
            Self::Italiano => "Controlla i dati del progetto",
            Self::Shqip => "Kontrollo cilësimet e projektit",
            Self::Spanish => "Revisa los datos del proyecto",
            Self::French => "Vérifiez les paramètres du projet",
            Self::Portuguese => "Verifique os dados do projeto",
            Self::Russian => "Проверьте параметры проекта",
            Self::Polish => "Sprawdź ustawienia projektu",
            Self::Ukrainian => "Перевірте параметри проєкту",
            Self::Turkish => "Proje ayarlarını denetleyin",
            Self::Greek => "Ελέγξτε τις ρυθμίσεις του έργου",
            Self::Hindi => "प्रोजेक्ट सेटिंग्स जाँचें",
            Self::Chinese => "检查项目设置",
            Self::Swedish => "Kontrollera projektinställningarna",
        }
    }

    pub fn operation_failed(self) -> &'static str {
        match self {
            Self::English => "Operation failed. Review the details and retry.",
            Self::Italiano => "Operazione non riuscita. Consulta i dettagli e riprova.",
            Self::Shqip => "Veprimi dështoi. Kontrollo hollësitë dhe provo përsëri.",
            Self::Spanish => "La operación falló. Revisa los detalles y vuelve a intentarlo.",
            Self::French => "L'opération a échoué. Consultez les détails et réessayez.",
            Self::Portuguese => "A operação falhou. Reveja os detalhes e tente novamente.",
            Self::Russian => "Операция не удалась. Проверьте подробности и повторите попытку.",
            Self::Polish => "Operacja nie powiodła się. Sprawdź szczegóły i spróbuj ponownie.",
            Self::Ukrainian => "Операція не вдалася. Перегляньте подробиці та спробуйте ще раз.",
            Self::Turkish => "İşlem başarısız oldu. Ayrıntıları inceleyip yeniden deneyin.",
            Self::Greek => "Η λειτουργία απέτυχε. Ελέγξτε τις λεπτομέρειες και δοκιμάστε ξανά.",
            Self::Hindi => "कार्रवाई विफल रही। विवरण देखें और फिर से प्रयास करें।",
            Self::Chinese => "操作失败。请查看详情后重试。",
            Self::Swedish => "Åtgärden misslyckades. Granska detaljerna och försök igen.",
        }
    }
    pub fn ca_load_failed(self) -> &'static str {
        match self {
            Self::English => "Certificate status unavailable. Refresh to retry.",
            Self::Italiano => "Stato del certificato non disponibile. Aggiorna per riprovare.",
            Self::Shqip => {
                "Statusi i certifikatës i padisponueshëm. Rifresko për të provuar përsëri."
            }
            Self::Spanish => "Estado del certificado no disponible. Actualiza para reintentar.",
            Self::French => "État du certificat indisponible. Actualisez pour réessayer.",
            Self::Portuguese => {
                "Estado do certificado indisponível. Atualize para tentar novamente."
            }
            Self::Russian => "Состояние сертификата недоступно. Обновите, чтобы повторить.",
            Self::Polish => "Stan certyfikatu niedostępny. Odśwież, aby spróbować ponownie.",
            Self::Ukrainian => "Стан сертифіката недоступний. Оновіть, щоб повторити.",
            Self::Turkish => "Sertifika durumu kullanılamıyor. Yenileyip tekrar deneyin.",
            Self::Greek => {
                "Η κατάσταση του πιστοποιητικού δεν είναι διαθέσιμη. Ανανεώστε για νέα προσπάθεια."
            }
            Self::Hindi => "प्रमाणपत्र स्थिति उपलब्ध नहीं। दोबारा प्रयास के लिए रीफ़्रेश करें।",
            Self::Chinese => "证书状态不可用。刷新以重试。",
            Self::Swedish => "Certifikatstatusen är otillgänglig. Uppdatera för att försöka igen.",
        }
    }
    pub fn projects_load_failed(self) -> &'static str {
        match self {
            Self::English => "Project list could not be refreshed. Showing the last loaded list.",
            Self::Italiano => {
                "Impossibile aggiornare i progetti. È mostrato l'ultimo elenco caricato."
            }
            Self::Shqip => "Lista nuk u rifreskua. Shfaqet lista e fundit e ngarkuar.",
            Self::Spanish => {
                "No se pudo actualizar la lista de proyectos. Se muestra la última lista cargada."
            }
            Self::French => {
                "La liste des projets n'a pas pu être actualisée. La dernière liste chargée est affichée."
            }
            Self::Portuguese => {
                "Não foi possível atualizar a lista de projetos. Mostra-se a última lista carregada."
            }
            Self::Russian => {
                "Не удалось обновить список проектов. Показан последний загруженный список."
            }
            Self::Polish => {
                "Nie udało się odświeżyć listy projektów. Wyświetlana jest ostatnia wczytana lista."
            }
            Self::Ukrainian => {
                "Не вдалося оновити список проєктів. Показано останній завантажений список."
            }
            Self::Turkish => "Proje listesi yenilenemedi. Son yüklenen liste gösteriliyor.",
            Self::Greek => {
                "Η λίστα έργων δεν ενημερώθηκε. Εμφανίζεται η τελευταία φορτωμένη λίστα."
            }
            Self::Hindi => "प्रोजेक्ट सूची रीफ़्रेश नहीं हो सकी। अंतिम लोड की गई सूची दिखाई जा रही है।",
            Self::Chinese => "无法刷新项目列表。正在显示上次加载的列表。",
            Self::Swedish => {
                "Projektlistan kunde inte uppdateras. Visar den senast laddade listan."
            }
        }
    }
    pub fn stop_hint(self) -> &'static str {
        match self {
            Self::English => {
                "Stops LocalWeb and all active projects. Source files and database data are kept."
            }
            Self::Italiano => {
                "Ferma LocalWeb e tutti i progetti attivi. Sorgenti e dati dei database restano."
            }
            Self::Shqip => "Ndalon LocalWeb dhe projektet aktive. Skedarët dhe të dhënat ruhen.",
            Self::Spanish => {
                "Detiene LocalWeb y todos los proyectos activos. Los archivos de origen y los datos de las bases de datos se conservan."
            }
            Self::French => {
                "Arrête LocalWeb et tous les projets actifs. Les fichiers source et les données des bases sont conservés."
            }
            Self::Portuguese => {
                "Para o LocalWeb e todos os projetos ativos. Os ficheiros de origem e os dados das bases de dados são mantidos."
            }
            Self::Russian => {
                "Останавливает LocalWeb и все активные проекты. Исходные файлы и данные баз сохраняются."
            }
            Self::Polish => {
                "Zatrzymuje LocalWeb i wszystkie aktywne projekty. Pliki źródłowe i dane baz danych zostają zachowane."
            }
            Self::Ukrainian => {
                "Зупиняє LocalWeb і всі активні проєкти. Вихідні файли та дані баз зберігаються."
            }
            Self::Turkish => {
                "LocalWeb'i ve tüm etkin projeleri durdurur. Kaynak dosyalar ve veritabanı verileri korunur."
            }
            Self::Greek => {
                "Σταματά το LocalWeb και όλα τα ενεργά έργα. Τα αρχεία πηγαίου κώδικα και τα δεδομένα των βάσεων διατηρούνται."
            }
            Self::Hindi => {
                "LocalWeb और सभी सक्रिय प्रोजेक्ट बंद करता है। स्रोत फ़ाइलें और डेटाबेस डेटा सुरक्षित रहते हैं।"
            }
            Self::Chinese => "停止 LocalWeb 和所有活跃项目。源文件和数据库数据会保留。",
            Self::Swedish => {
                "Stoppar LocalWeb och alla aktiva projekt. Källfiler och databasdata behålls."
            }
        }
    }
    pub fn view_all_projects(self) -> &'static str {
        match self {
            Self::English => "View all projects",
            Self::Italiano => "Vedi tutti i progetti",
            Self::Shqip => "Shiko të gjitha projektet",
            Self::Spanish => "Ver todos los proyectos",
            Self::French => "Voir tous les projets",
            Self::Portuguese => "Ver todos os projetos",
            Self::Russian => "Показать все проекты",
            Self::Polish => "Zobacz wszystkie projekty",
            Self::Ukrainian => "Переглянути всі проєкти",
            Self::Turkish => "Tüm projeleri gör",
            Self::Greek => "Προβολή όλων των έργων",
            Self::Hindi => "सभी प्रोजेक्ट देखें",
            Self::Chinese => "查看所有项目",
            Self::Swedish => "Visa alla projekt",
        }
    }
    pub fn search_projects(self) -> &'static str {
        match self {
            Self::English => "Search projects",
            Self::Italiano => "Cerca progetti",
            Self::Shqip => "Kërko projekte",
            Self::Spanish => "Buscar proyectos",
            Self::French => "Rechercher des projets",
            Self::Portuguese => "Pesquisar projetos",
            Self::Russian => "Поиск проектов",
            Self::Polish => "Szukaj projektów",
            Self::Ukrainian => "Пошук проєктів",
            Self::Turkish => "Projelerde ara",
            Self::Greek => "Αναζήτηση έργων",
            Self::Hindi => "प्रोजेक्ट खोजें",
            Self::Chinese => "搜索项目",
            Self::Swedish => "Sök projekt",
        }
    }
    pub fn clear_search(self) -> &'static str {
        match self {
            Self::English => "Clear",
            Self::Italiano => "Azzera",
            Self::Shqip => "Pastro",
            Self::Spanish => "Borrar",
            Self::French => "Effacer",
            Self::Portuguese => "Limpar",
            Self::Russian => "Очистить",
            Self::Polish => "Wyczyść",
            Self::Ukrainian => "Очистити",
            Self::Turkish => "Temizle",
            Self::Greek => "Καθαρισμός",
            Self::Hindi => "साफ़ करें",
            Self::Chinese => "清除",
            Self::Swedish => "Rensa",
        }
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
            Self::Spanish => format!(
                "Ningún proyecto coincide con «{query}». Borra la búsqueda para ver todos los proyectos."
            ),
            Self::French => format!(
                "Aucun projet ne correspond à « {query} ». Effacez la recherche pour voir tous les projets."
            ),
            Self::Portuguese => format!(
                "Nenhum projeto corresponde a «{query}». Limpe a pesquisa para ver todos os projetos."
            ),
            Self::Russian => format!(
                "Нет проектов по запросу «{query}». Очистите поиск, чтобы увидеть все проекты."
            ),
            Self::Polish => format!(
                "Żaden projekt nie pasuje do „{query}”. Wyczyść wyszukiwanie, aby zobaczyć wszystkie projekty."
            ),
            Self::Ukrainian => format!(
                "Жоден проєкт не відповідає «{query}». Очистіть пошук, щоб побачити всі проєкти."
            ),
            Self::Turkish => format!(
                "“{query}” ile eşleşen proje yok. Tüm projeleri görmek için aramayı temizleyin."
            ),
            Self::Greek => format!(
                "Κανένα έργο δεν ταιριάζει με «{query}». Καθαρίστε την αναζήτηση για να δείτε όλα τα έργα."
            ),
            Self::Hindi => {
                format!("“{query}” से कोई प्रोजेक्ट मेल नहीं खाता। सभी प्रोजेक्ट देखने के लिए खोज साफ़ करें।")
            }
            Self::Chinese => {
                format!("没有匹配“{query}”的项目。清除搜索以查看所有项目。")
            }
            Self::Swedish => {
                format!("Inga projekt matchar ”{query}”. Rensa sökningen för att se alla projekt.")
            }
        }
    }
    pub fn project_details(self) -> &'static str {
        match self {
            Self::English => "Details",
            Self::Italiano => "Dettagli",
            Self::Shqip => "Hollësi",
            Self::Spanish => "Detalles",
            Self::French => "Détails",
            Self::Portuguese => "Detalhes",
            Self::Russian => "Подробности",
            Self::Polish => "Szczegóły",
            Self::Ukrainian => "Подробиці",
            Self::Turkish => "Ayrıntılar",
            Self::Greek => "Λεπτομέρειες",
            Self::Hindi => "विवरण",
            Self::Chinese => "详情",
            Self::Swedish => "Detaljer",
        }
    }
    pub fn reveal_environment(self) -> &'static str {
        match self {
            Self::English => "Show connection settings",
            Self::Italiano => "Mostra dati di connessione",
            Self::Shqip => "Shfaq cilësimet e lidhjes",
            Self::Spanish => "Mostrar datos de conexión",
            Self::French => "Afficher les paramètres de connexion",
            Self::Portuguese => "Mostrar dados de ligação",
            Self::Russian => "Показать параметры подключения",
            Self::Polish => "Pokaż ustawienia połączenia",
            Self::Ukrainian => "Показати параметри підключення",
            Self::Turkish => "Bağlantı ayarlarını göster",
            Self::Greek => "Εμφάνιση ρυθμίσεων σύνδεσης",
            Self::Hindi => "कनेक्शन सेटिंग्स दिखाएँ",
            Self::Chinese => "显示连接设置",
            Self::Swedish => "Visa anslutningsinställningar",
        }
    }
    pub fn environment_help(self) -> &'static str {
        match self {
            Self::English => {
                "Use these values in your app's configuration. LocalWeb does not edit your .env file."
            }
            Self::Italiano => {
                "Usa questi valori nella configurazione dell'app. LocalWeb non modifica il file .env."
            }
            Self::Shqip => {
                "Përdor këto vlera në konfigurimin e aplikacionit. LocalWeb nuk ndryshon skedarin .env."
            }
            Self::Spanish => {
                "Usa estos valores en la configuración de tu app. LocalWeb no modifica tu archivo .env."
            }
            Self::French => {
                "Utilisez ces valeurs dans la configuration de votre app. LocalWeb ne modifie pas votre fichier .env."
            }
            Self::Portuguese => {
                "Use estes valores na configuração da sua app. O LocalWeb não altera o seu ficheiro .env."
            }
            Self::Russian => {
                "Используйте эти значения в конфигурации приложения. LocalWeb не изменяет ваш файл .env."
            }
            Self::Polish => {
                "Użyj tych wartości w konfiguracji aplikacji. LocalWeb nie modyfikuje pliku .env."
            }
            Self::Ukrainian => {
                "Використовуйте ці значення в конфігурації застосунку. LocalWeb не змінює ваш файл .env."
            }
            Self::Turkish => {
                "Bu değerleri uygulamanızın yapılandırmasında kullanın. LocalWeb .env dosyanızı değiştirmez."
            }
            Self::Greek => {
                "Χρησιμοποιήστε αυτές τις τιμές στη ρύθμιση της εφαρμογής σας. Το LocalWeb δεν τροποποιεί το αρχείο .env."
            }
            Self::Hindi => {
                "इन मानों को अपने ऐप की कॉन्फ़िगरेशन में उपयोग करें। LocalWeb आपकी .env फ़ाइल नहीं बदलता।"
            }
            Self::Chinese => "在应用配置中使用这些值。LocalWeb 不会修改你的 .env 文件。",
            Self::Swedish => {
                "Använd dessa värden i appens konfiguration. LocalWeb redigerar inte din .env-fil."
            }
        }
    }
    pub fn copy_environment(self) -> &'static str {
        match self {
            Self::English => "Copy as .env",
            Self::Italiano => "Copia come .env",
            Self::Shqip => "Kopjo si .env",
            Self::Spanish | Self::Portuguese => "Copiar como .env",
            Self::French => "Copier en .env",
            Self::Russian => "Скопировать как .env",
            Self::Polish => "Kopiuj jako .env",
            Self::Ukrainian => "Копіювати як .env",
            Self::Turkish => ".env olarak kopyala",
            Self::Greek => "Αντιγραφή ως .env",
            Self::Hindi => ".env के रूप में कॉपी करें",
            Self::Chinese => "复制为 .env",
            Self::Swedish => "Kopiera som .env",
        }
    }
    pub fn environment_failed(self) -> &'static str {
        match self {
            Self::English => "Connection settings could not be loaded. Retry after refreshing.",
            Self::Italiano => {
                "Impossibile caricare i dati di connessione. Riprova dopo aver aggiornato."
            }
            Self::Shqip => "Cilësimet nuk u ngarkuan. Provo përsëri pas rifreskimit.",
            Self::Spanish => {
                "No se pudieron cargar los datos de conexión. Reintenta tras actualizar."
            }
            Self::French => {
                "Les paramètres de connexion n'ont pas pu être chargés. Réessayez après actualisation."
            }
            Self::Portuguese => {
                "Não foi possível carregar os dados de ligação. Tente novamente após atualizar."
            }
            Self::Russian => {
                "Не удалось загрузить параметры подключения. Повторите после обновления."
            }
            Self::Polish => {
                "Nie udało się wczytać ustawień połączenia. Spróbuj ponownie po odświeżeniu."
            }
            Self::Ukrainian => {
                "Не вдалося завантажити параметри підключення. Спробуйте після оновлення."
            }
            Self::Turkish => "Bağlantı ayarları yüklenemedi. Yeniledikten sonra tekrar deneyin.",
            Self::Greek => {
                "Οι ρυθμίσεις σύνδεσης δεν φορτώθηκαν. Δοκιμάστε ξανά μετά την ανανέωση."
            }
            Self::Hindi => "कनेक्शन सेटिंग्स लोड नहीं हो सकीं। रीफ़्रेश के बाद फिर से प्रयास करें।",
            Self::Chinese => "无法加载连接设置。刷新后重试。",
            Self::Swedish => {
                "Anslutningsinställningarna kunde inte läsas in. Försök igen efter uppdatering."
            }
        }
    }
    pub fn web_server_label(self) -> &'static str {
        match self {
            Self::English
            | Self::Italiano
            | Self::Shqip
            | Self::Spanish
            | Self::French
            | Self::Portuguese
            | Self::Russian
            | Self::Polish
            | Self::Ukrainian
            | Self::Turkish
            | Self::Greek
            | Self::Hindi
            | Self::Chinese
            | Self::Swedish => "Web server",
        }
    }
    pub fn runtime_availability(self, bundled: bool) -> &'static str {
        if bundled {
            match self {
                Self::English => "included",
                Self::Italiano => "incluso",
                Self::Shqip => "i përfshirë",
                Self::Spanish => "incluido",
                Self::French => "inclus",
                Self::Portuguese => "incluído",
                Self::Russian | Self::Ukrainian => "включено",
                Self::Polish => "w zestawie",
                Self::Turkish => "dahil",
                Self::Greek => "περιλαμβάνεται",
                Self::Hindi => "शामिल",
                Self::Chinese => "已包含",
                Self::Swedish => "ingår",
            }
        } else {
            match self {
                Self::English => "needs CLI/FPM",
                Self::Italiano => "richiede CLI/FPM",
                Self::Shqip => "kërkon CLI/FPM",
                Self::Spanish => "requiere CLI/FPM",
                Self::French => "nécessite CLI/FPM",
                Self::Portuguese => "requer CLI/FPM",
                Self::Russian => "требует CLI/FPM",
                Self::Polish => "wymaga CLI/FPM",
                Self::Ukrainian => "потребує CLI/FPM",
                Self::Turkish => "CLI/FPM gerekir",
                Self::Greek => "απαιτεί CLI/FPM",
                Self::Hindi => "CLI/FPM चाहिए",
                Self::Chinese => "需要 CLI/FPM",
                Self::Swedish => "kräver CLI/FPM",
            }
        }
    }
    pub fn php_version_availability(self, version: &str, bundled: bool) -> &'static str {
        if matches!(version, "8.0" | "8.1") {
            match self {
                Self::Shqip => "EOL · për përputhshmëri",
                Self::English
                | Self::Italiano
                | Self::Spanish
                | Self::French
                | Self::Portuguese
                | Self::Russian
                | Self::Polish
                | Self::Ukrainian
                | Self::Turkish
                | Self::Greek
                | Self::Hindi
                | Self::Chinese
                | Self::Swedish => "EOL · legacy",
            }
        } else {
            self.runtime_availability(bundled)
        }
    }
    pub fn notice_stopped(self) -> &'static str {
        match self {
            Self::English => "LocalWeb stopped",
            Self::Italiano => "LocalWeb fermato",
            Self::Shqip => "LocalWeb u ndalua",
            Self::Spanish => "LocalWeb detenido",
            Self::French => "LocalWeb arrêté",
            Self::Portuguese => "LocalWeb parado",
            Self::Russian => "LocalWeb остановлен",
            Self::Polish => "LocalWeb zatrzymany",
            Self::Ukrainian => "LocalWeb зупинено",
            Self::Turkish => "LocalWeb durduruldu",
            Self::Greek => "Το LocalWeb σταμάτησε",
            Self::Hindi => "LocalWeb बंद हो गया",
            Self::Chinese => "LocalWeb 已停止",
            Self::Swedish => "LocalWeb stoppat",
        }
    }
    pub fn notice_project_started(self) -> &'static str {
        match self {
            Self::English => "Project started",
            Self::Italiano => "Progetto avviato",
            Self::Shqip => "Projekti u nis",
            Self::Spanish => "Proyecto iniciado",
            Self::French => "Projet démarré",
            Self::Portuguese => "Projeto iniciado",
            Self::Russian => "Проект запущен",
            Self::Polish => "Projekt uruchomiony",
            Self::Ukrainian => "Проєкт запущено",
            Self::Turkish => "Proje başlatıldı",
            Self::Greek => "Το έργο ξεκίνησε",
            Self::Hindi => "प्रोजेक्ट शुरू हुआ",
            Self::Chinese => "项目已启动",
            Self::Swedish => "Projektet startat",
        }
    }
    pub fn notice_project_stopped(self) -> &'static str {
        match self {
            Self::English => "Project stopped",
            Self::Italiano => "Progetto fermato",
            Self::Shqip => "Projekti u ndalua",
            Self::Spanish => "Proyecto detenido",
            Self::French => "Projet arrêté",
            Self::Portuguese => "Projeto parado",
            Self::Russian => "Проект остановлен",
            Self::Polish => "Projekt zatrzymany",
            Self::Ukrainian => "Проєкт зупинено",
            Self::Turkish => "Proje durduruldu",
            Self::Greek => "Το έργο σταμάτησε",
            Self::Hindi => "प्रोजेक्ट बंद हुआ",
            Self::Chinese => "项目已停止",
            Self::Swedish => "Projektet stoppat",
        }
    }
    pub fn notice_project_restarted(self) -> &'static str {
        match self {
            Self::English => "Project restarted",
            Self::Italiano => "Progetto riavviato",
            Self::Shqip => "Projekti u rinis",
            Self::Spanish => "Proyecto reiniciado",
            Self::French => "Projet redémarré",
            Self::Portuguese => "Projeto reiniciado",
            Self::Russian => "Проект перезапущен",
            Self::Polish => "Projekt zrestartowany",
            Self::Ukrainian => "Проєкт перезапущено",
            Self::Turkish => "Proje yeniden başlatıldı",
            Self::Greek => "Το έργο επανεκκινήθηκε",
            Self::Hindi => "प्रोजेक्ट पुनः शुरू हुआ",
            Self::Chinese => "项目已重启",
            Self::Swedish => "Projektet omstartat",
        }
    }
    pub fn notice_project_removed(self) -> &'static str {
        match self {
            Self::English => "Project configuration removed",
            Self::Italiano => "Configurazione del progetto rimossa",
            Self::Shqip => "Konfigurimi i projektit u hoq",
            Self::Spanish => "Configuración del proyecto eliminada",
            Self::French => "Configuration du projet supprimée",
            Self::Portuguese => "Configuração do projeto removida",
            Self::Russian => "Конфигурация проекта удалена",
            Self::Polish => "Konfiguracja projektu usunięta",
            Self::Ukrainian => "Конфігурацію проєкту видалено",
            Self::Turkish => "Proje yapılandırması kaldırıldı",
            Self::Greek => "Η ρύθμιση του έργου αφαιρέθηκε",
            Self::Hindi => "प्रोजेक्ट कॉन्फ़िगरेशन हटाया गया",
            Self::Chinese => "项目配置已移除",
            Self::Swedish => "Projektkonfigurationen borttagen",
        }
    }
    pub fn project_options(self, worker: bool, cron: usize, mail: bool, autostart: bool) -> String {
        let yes = match self {
            Self::English => "on",
            Self::Italiano => "attivo",
            Self::Shqip => "aktiv",
            Self::Spanish => "activo",
            Self::French => "actif",
            Self::Portuguese => "ativo",
            Self::Russian => "вкл",
            Self::Polish => "wł",
            Self::Ukrainian => "увімк",
            Self::Turkish => "açık",
            Self::Greek => "ενεργό",
            Self::Hindi => "चालू",
            Self::Chinese => "开",
            Self::Swedish => "på",
        };
        let no = match self {
            Self::English => "off",
            Self::Italiano => "disattivo",
            Self::Shqip => "joaktiv",
            Self::Spanish => "inactivo",
            Self::French => "inactif",
            Self::Portuguese => "inativo",
            Self::Russian => "выкл",
            Self::Polish => "wył",
            Self::Ukrainian => "вимк",
            Self::Turkish => "kapalı",
            Self::Greek => "ανενεργό",
            Self::Hindi => "बंद",
            Self::Chinese => "关",
            Self::Swedish => "av",
        };
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
