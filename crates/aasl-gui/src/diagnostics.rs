//! Diagnostics: what AASL checked, what is wrong, and how to fix it.

use aasl_core::Severity;
use aasl_diag::checks::CHECKS;
use aasl_diag::{Finding, FixId};
use freya::prelude::*;

use crate::i18n::{Lang, current, fill, tr};
use crate::state::{Section, Ui};
use crate::svc;
use crate::theme::space;
use crate::widgets::{
    Tone, banner, body, chip, divided, group, mono_box, muted, row, severity_tone, title,
};

#[derive(PartialEq)]
pub struct DiagnosticsSection(pub Ui);

impl Component for DiagnosticsSection {
    fn render(&self) -> impl IntoElement {
        view(self.0)
    }
}

async fn run_checks(mut ui: Ui, mut running: State<bool>) {
    running.set(true);
    if let Ok(v) = tokio::spawn(async { svc().doctor(true).await }).await {
        ui.findings.set(Some(v));
    }
    running.set(false);
}

/// What each check looks at, in plain words.
fn check_meta(id: &str) -> (&'static str, &'static str) {
    match id {
        "python" => (
            tr("Python", "Python"),
            tr(
                "umu-run, which starts the game, needs Python 3.10 or newer.",
                "umu-run, qui démarre le jeu, a besoin de Python 3.10 ou plus récent.",
            ),
        ),
        "gpu" => (
            tr("Graphics card", "Carte graphique"),
            tr(
                "A Vulkan driver is installed. Proton draws the game with it.",
                "Un pilote Vulkan est installé. Proton dessine le jeu avec.",
            ),
        ),
        "network" => (
            tr("Connection to GitHub", "Connexion à GitHub"),
            tr(
                "Installs and updates are downloaded from GitHub.",
                "Les installations et les mises à jour se téléchargent depuis GitHub.",
            ),
        ),
        "install" => (
            tr("Installation", "Installation"),
            tr(
                "Somnium Space, umu-run and Proton are all there.",
                "Somnium Space, umu-run et Proton sont tous présents.",
            ),
        ),
        "patches" => (
            tr("Wine patches", "Correctifs Wine"),
            tr(
                "The sign-in patches are in Proton, untouched, and fit this version.",
                "Les correctifs de connexion sont dans Proton, intacts, et adaptés à cette version.",
            ),
        ),
        "proton" => (
            tr("Proton version", "Version de Proton"),
            tr(
                "Proton 9 or newer, which VR needs.",
                "Proton 9 ou plus récent, ce dont la VR a besoin.",
            ),
        ),
        "openxr" => (
            tr("VR headset", "Casque VR"),
            tr(
                "A VR runtime is active and linked to the game. Details are on the VR page.",
                "Un runtime VR est actif et relié au jeu. Le détail est dans la page VR.",
            ),
        ),
        "session" => (
            tr("Game running", "Jeu en cours"),
            tr(
                "Whether Somnium Space is already running.",
                "Si Somnium Space tourne déjà.",
            ),
        ),
        "desktop_entry" => (
            tr("Menu shortcut", "Raccourci du menu"),
            tr(
                "The entry in your applications menu exists.",
                "L'entrée du menu des applications existe.",
            ),
        ),
        "sandbox" => (
            tr("Environment", "Environnement"),
            tr(
                "AASL is not in a sandbox that would block Proton.",
                "AASL n'est pas dans un bac à sable qui bloquerait Proton.",
            ),
        ),
        "launch_log" => (
            tr("Last launch", "Dernier lancement"),
            tr(
                "The launch log has none of the known errors (certificates, Vulkan, container).",
                "Le journal de lancement ne contient aucune erreur connue (certificats, Vulkan, conteneur).",
            ),
        ),
        "vr_perf" => (
            tr("VR smoothness", "Fluidité VR"),
            tr(
                "Compositor and GPU settings, plus freezes and slowdowns in your last game.",
                "Réglages du compositeur et du GPU, gels et ralentissements de votre dernière partie.",
            ),
        ),
        _ => ("?", ""),
    }
}

/// French text for the engine's English findings: (title, title in French, advice in French).
const FR: &[(&str, &str, &str)] = &[
    (
        "python3 is not installed",
        "python3 n'est pas installé",
        "umu-run a besoin de Python 3.10 ou plus récent. Installez-le avec votre gestionnaire de paquets.",
    ),
    (
        "No Vulkan driver found",
        "Aucun pilote Vulkan trouvé",
        "Proton dessine le jeu avec Vulkan. Installez le pilote Vulkan de votre carte graphique (mesa-vulkan-drivers, vulkan-radeon, nvidia-utils...).",
    ),
    (
        "No GPU found under /sys/class/drm",
        "Aucune carte graphique détectée",
        "",
    ),
    (
        "GitHub is not reachable",
        "GitHub est injoignable",
        "Les installations et les réparations se téléchargent depuis GitHub. Vérifiez votre connexion, votre proxy ou votre pare-feu.",
    ),
    (
        "Somnium Space is not installed in the prefix",
        "Somnium Space n'est pas installé",
        "Lancez l'installation avec l'installeur Windows de Somnium, sur somniumspace.com.",
    ),
    (
        "umu-run is missing",
        "umu-run est absent",
        "Une réparation le télécharge à nouveau.",
    ),
    (
        "Proton is missing",
        "Proton est absent",
        "Une réparation le télécharge à nouveau.",
    ),
    (
        "No Proton version recorded",
        "Aucune version de Proton enregistrée",
        "Une réparation en choisit une.",
    ),
    (
        "No Somnium patch bundle is installed in this Proton",
        "Les correctifs Somnium ne sont pas dans ce Proton",
        "Sans eux, le launcher ne peut pas se connecter : Wine d'origine ne gère pas les certificats clients TLS dont Somnium a besoin.",
    ),
    (
        "OpenXR detection is replaced by manual settings",
        "La détection VR est remplacée par des réglages manuels",
        "Ces réglages remplacent la détection automatique. Si vous en avez besoin, signalez votre runtime pour que la détection le gère.",
    ),
    (
        "AASL is running inside a Flatpak sandbox",
        "AASL tourne dans un bac à sable Flatpak",
        "Le conteneur de Proton ne peut pas démarrer depuis Flatpak sans accès à l'hôte. Utilisez l'AppImage ou le paquet natif.",
    ),
    (
        "The start-menu entry is missing",
        "Le raccourci du menu est absent",
        "Une réparation le recrée.",
    ),
    (
        "TLS errors in the launch log",
        "Erreurs TLS dans le journal de lancement",
        "Les bibliothèques secur32 et crypt32 corrigées ne sont probablement pas utilisées. Lancez une réparation.",
    ),
    (
        "Certificate errors in the launch log",
        "Erreurs de certificat dans le journal de lancement",
        "Les bibliothèques corrigées ne sont probablement pas utilisées. Lancez une réparation.",
    ),
    (
        "Vulkan errors in the launch log",
        "Erreurs Vulkan dans le journal de lancement",
        "Vérifiez le pilote de la carte graphique et que Vulkan fonctionne (vulkaninfo).",
    ),
    (
        "The game could not reach the OpenXR runtime",
        "Le jeu n'a pas trouvé le runtime VR",
        "Branchez le casque avant de lancer, puis vérifiez que le runtime le voit.",
    ),
    (
        "Proton's container failed to start",
        "Le conteneur de Proton n'a pas démarré",
        "La ligne en détail nomme souvent un chemin absent ou une permission manquante.",
    ),
    (
        "Freezes in the last session",
        "Gels pendant la dernière partie",
        "Des gels irréguliers coïncident souvent avec des chargements : mondes, arrivée d'avatars, vidéos.",
    ),
    (
        "The GPU idles during the slowdowns",
        "Le GPU est au repos pendant les ralentissements",
        "Les images sont en retard alors que le GPU ne travaille pas : le thread principal du jeu, le compositeur ou un chargement (avatars, mondes, vidéo) les retient. Beaucoup d'avatars sur un processeur à 6 cœurs, c'est la limite habituelle.",
    ),
    (
        "The GPU clock drops during the slowdowns",
        "La fréquence du GPU chute pendant les ralentissements",
        "La gestion d'énergie baisse la fréquence au moment où il faut des images. Utilisez le profil d'alimentation VR (page VR) ou un niveau de performance fixe.",
    ),
    (
        "The GPU is saturated during the slowdowns",
        "Le GPU est saturé pendant les ralentissements",
        "Baissez la résolution dans WiVRn ou les graphismes du jeu, ou masquez les avatars lointains.",
    ),
    (
        "Slowdowns in the last session",
        "Ralentissements pendant la dernière partie",
        "MangoHud n'a pas mesuré la charge du GPU : la cause reste inconnue.",
    ),
];

/// Titles with a value inside: (prefix, suffix, French title with {}, French advice with {}).
const FR_DYN: &[(&str, &str, &str, &str)] = &[
    (
        "Patched files from ",
        " were replaced",
        "Les fichiers corrigés de {} ont été remplacés",
        "Une mise à jour de Proton ou une copie manuelle les a écrasés. Une réparation les remet.",
    ),
    (
        "Patch bundle ",
        " installed and intact",
        "Correctifs {} installés et intacts",
        "",
    ),
    (
        "The catalog has no patch bundle for ",
        "",
        "Le catalogue n'a pas de correctifs pour {}",
        "Passez à un Proton vérifié, ou construisez les correctifs vous-même (Recettes) et testez-les.",
    ),
    (
        "The patch bundle for ",
        " is untested",
        "Les correctifs pour {} ne sont pas testés",
        "Ils ont été construits pour un Proton voisin. Si la connexion ou la vidéo échoue, passez à un Proton vérifié et signalez le résultat.",
    ),
    (
        "python3 is ",
        ", older than 3.10",
        "python3 est en version {}, antérieure à 3.10",
        "umu-run a besoin de la 3.10 ou plus comme python3 dans le PATH. Sur une LTS ancienne, cela passe souvent par une mise à jour de la distribution.",
    ),
    (
        "Somnium Space is running (",
        " processes)",
        "Somnium Space est en cours d'exécution ({} processus)",
        "",
    ),
    (
        "The game freezes every ",
        " s",
        "Le jeu se fige toutes les {} s",
        "Des gels à intervalle fixe viennent d'un minuteur dans le jeu (nettoyage périodique, sondage, tâche réseau ou voix), pas du GPU ni du compositeur. Notez ce que vous faisiez et comparez avec le micro et le chat vocal coupés.",
    ),
];

/// Title and advice in the interface language; the engine's English when no translation exists.
fn localized(f: &Finding) -> (String, Option<String>) {
    if current() == Lang::En {
        return (f.title.clone(), f.advice.clone());
    }
    let keep = |fr: &str| {
        (!fr.is_empty())
            .then(|| fr.to_string())
            .or_else(|| f.advice.clone())
    };
    if let Some((_, t, a)) = FR.iter().find(|(en, _, _)| *en == f.title) {
        return (t.to_string(), keep(a));
    }
    for (pre, suf, t, a) in FR_DYN {
        if let Some(mid) = f.title.strip_prefix(pre).and_then(|r| r.strip_suffix(suf)) {
            return (t.replacen("{}", mid, 1), keep(&a.replacen("{}", mid, 1)));
        }
    }
    if let Some(p) = f.title.strip_suffix(" is older than Proton 9") {
        return (
            format!("{p} est plus ancien que Proton 9"),
            Some("Proton 8 et antérieurs ne savent pas initialiser la VR. Choisissez Proton 9 ou plus récent.".into()),
        );
    }
    if let Some(d) = f.title.strip_suffix(" is an immutable distribution") {
        return (
            format!("{d} est une distribution immuable"),
            Some("AASL n'écrit que dans votre dossier personnel, cela devrait fonctionner. Les retours sont bienvenus.".into()),
        );
    }
    (f.title.clone(), f.advice.clone())
}

fn fix_label(fix: FixId) -> &'static str {
    match fix {
        FixId::Repair => tr("Repair", "Réparer"),
        FixId::StopSession => tr("Stop the game", "Arrêter le jeu"),
        FixId::ClearOpenxrOverride => tr("Use automatic detection", "Détection automatique"),
        FixId::ClearDownloads => tr("Clear downloads", "Vider les téléchargements"),
    }
}

fn sev_label(s: Severity) -> &'static str {
    match s {
        Severity::Error => tr("Problem", "Problème"),
        Severity::Warn => tr("Worth checking", "À surveiller"),
        Severity::Info => tr("Info", "Info"),
    }
}

/// The VR page owns these: the headset chain and the compositor settings.
fn vr_owned(f: &Finding) -> bool {
    (f.check == "openxr" && f.fix.is_none())
        || (f.check == "vr_perf"
            && (f.title.contains("compositor at realtime")
                || f.title.contains("VR power profile")
                || f.title.starts_with("XR overlay")))
}

fn finding_row(ui: Ui, running: State<bool>, f: &Finding) -> Element {
    let (head, advice) = localized(f);
    let fix = f.fix;
    let side = rect()
        .spacing(space::S)
        .cross_align(Alignment::End)
        .child(chip(sev_label(f.severity), severity_tone(f.severity)))
        .maybe_child(fix.map(|fix| {
            Button::new()
                .on_press(move |_| {
                    ui.run(async move { svc().apply_fix(fix, false).await }, move |o| {
                        ui.notice(Severity::Info, o.message);
                        spawn_in_window(run_checks(ui, running));
                    })
                })
                .child(fix_label(fix))
        }));
    rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .padding((space::M, space::L))
        .spacing(space::M)
        .child(
            rect()
                .width(Size::flex(1.))
                .spacing(space::XS)
                .child(body(head))
                .maybe_child(advice.map(muted))
                .maybe_child((!f.evidence.is_empty()).then(|| {
                    mono_box(
                        f.evidence
                            .iter()
                            .take(4)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join("\n"),
                    )
                })),
        )
        .child(side)
        .into_element()
}

fn view(mut ui: Ui) -> impl IntoElement {
    let running = use_state(|| false);
    use_future(move || async move { run_checks(ui, running).await });
    let busy = *running.read();
    let findings = ui.findings.read().clone();
    let list: Vec<Finding> = findings.clone().unwrap_or_default();
    let shown: Vec<&Finding> = list.iter().filter(|f| !vr_owned(f)).collect();
    let vr_attention = list
        .iter()
        .any(|f| vr_owned(f) && f.severity != Severity::Info);
    let errors = shown
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warns = shown
        .iter()
        .filter(|f| f.severity == Severity::Warn)
        .count()
        + usize::from(vr_attention);

    let rerun = Button::new()
        .filled()
        .enabled(!busy)
        .on_press(move |_| {
            spawn_in_window(run_checks(ui, running));
        })
        .child(if busy {
            tr("Checking...", "Vérification...")
        } else {
            tr("Check again", "Relancer")
        })
        .into_element();
    let plural = |n: usize, one: &'static str, many: &'static str| {
        if n == 1 {
            one.to_string()
        } else {
            fill(many, &[n.to_string()])
        }
    };
    let verdict = if findings.is_none() {
        banner(
            Tone::Neutral,
            tr("Checking...", "Vérification en cours..."),
            tr("This takes a few seconds.", "Cela prend quelques secondes."),
            Some(rerun),
        )
    } else if errors > 0 {
        banner(
            Tone::Error,
            plural(
                errors,
                tr("One problem to fix", "Un problème à corriger"),
                tr("{} problems to fix", "{} problèmes à corriger"),
            ),
            tr(
                "Start with these. The game probably cannot start or sign in while they remain.",
                "Commencez par ceux-là. Le jeu ne démarre ou ne se connecte probablement pas tant qu'ils restent.",
            ),
            Some(rerun),
        )
    } else if warns > 0 {
        banner(
            Tone::Warn,
            plural(
                warns,
                tr("One thing worth checking", "Un point à surveiller"),
                tr("{} things worth checking", "{} points à surveiller"),
            ),
            tr(
                "Nothing is broken, but these can explain slowdowns or odd behaviour.",
                "Rien n'est cassé, mais ces points peuvent expliquer des ralentissements ou un comportement étrange.",
            ),
            Some(rerun),
        )
    } else {
        banner(
            Tone::Ok,
            tr("All good", "Tout est en ordre"),
            fill(
                tr(
                    "{} checks passed: install, system, VR and your last game.",
                    "{} vérifications réussies : installation, système, VR et dernière partie.",
                ),
                &[CHECKS.len().to_string()],
            ),
            Some(rerun),
        )
    };

    let mut to_fix: Vec<Element> = shown
        .iter()
        .filter(|f| f.severity != Severity::Info)
        .map(|f| finding_row(ui, running, f))
        .collect();
    if vr_attention {
        to_fix.push(row(
            tr("Your VR headset needs attention", "Le casque VR demande de l'attention"),
            Some(
                tr(
                    "A step is missing between the headset and the game, or a setting costs frames.",
                    "Il manque une étape entre le casque et le jeu, ou un réglage coûte des images.",
                )
                .into(),
            ),
            Some(
                Button::new()
                    .on_press(move |_| ui.section.set(Section::Vr))
                    .child(tr("Open the VR page", "Ouvrir la page VR"))
                    .into_element(),
            ),
        ));
    }
    let infos: Vec<Element> = shown
        .iter()
        .filter(|f| f.severity == Severity::Info)
        .map(|f| finding_row(ui, running, f))
        .collect();

    let checked: Vec<Element> = CHECKS
        .iter()
        .map(|c| {
            let (name, what) = check_meta(c.id);
            let chip_el = if findings.is_none() {
                chip("...", Tone::Neutral)
            } else {
                match list
                    .iter()
                    .filter(|f| f.check == c.id)
                    .map(|f| f.severity)
                    .max()
                {
                    Some(Severity::Error) => chip(tr("Problem", "Problème"), Tone::Error),
                    Some(Severity::Warn) => chip(tr("Worth checking", "À surveiller"), Tone::Warn),
                    _ => chip(tr("OK", "OK"), Tone::Ok),
                }
            };
            row(name, Some(what.to_string()), Some(chip_el.into_element()))
        })
        .collect();

    let help = vec![
        row(
            tr("Anonymous bug report", "Rapport de bug anonyme"),
            Some(
                tr(
                    "A JSON file with these results and your setup, without your user name or personal paths. Attach it when you ask for help.",
                    "Un fichier JSON avec ces résultats et votre configuration, sans nom d'utilisateur ni chemin personnel. À joindre quand vous demandez de l'aide.",
                )
                .into(),
            ),
            Some(
                Button::new()
                    .on_press(move |_| {
                        spawn_in_window(async move {
                            if let Ok(r) = tokio::spawn(async { svc().report().await }).await {
                                let p = svc().paths().logs_dir().join("bug-report.json");
                                let _ = std::fs::create_dir_all(svc().paths().logs_dir());
                                match std::fs::write(&p, r) {
                                    Ok(()) => ui.notice(
                                        Severity::Info,
                                        fill(
                                            tr("Report saved: {}", "Rapport enregistré : {}"),
                                            &[p.display().to_string()],
                                        ),
                                    ),
                                    Err(e) => ui.error.set(Some(e.to_string())),
                                }
                            }
                        });
                    })
                    .child(tr("Save the report", "Enregistrer le rapport"))
                    .into_element(),
            ),
        ),
        row(
            tr("Launch logs", "Journaux de lancement"),
            Some(
                tr(
                    "What Proton and the game printed, line by line.",
                    "Ce que Proton et le jeu ont écrit, ligne par ligne.",
                )
                .into(),
            ),
            Some(
                Button::new()
                    .flat()
                    .on_press(move |_| ui.section.set(Section::Logs))
                    .child(tr("Open the logs", "Ouvrir les journaux"))
                    .into_element(),
            ),
        ),
    ];

    rect()
        .spacing(space::XL)
        .child(title(tr("Diagnostics", "Diagnostic")))
        .child(muted(tr(
            "When something does not work, start here. AASL checks the install, the system and your last launch, and fixes what it can.",
            "Quand quelque chose ne marche pas, commencez ici. AASL vérifie l'installation, le système et le dernier lancement, et corrige ce qu'il peut.",
        )))
        .child(verdict)
        .maybe_child((!to_fix.is_empty()).then(|| group(tr("To fix", "À corriger"), divided(to_fix))))
        .maybe_child((!infos.is_empty()).then(|| group(tr("For your information", "Pour information"), divided(infos))))
        .child(group(tr("What was checked", "Ce qui a été vérifié"), divided(checked)))
        .child(group(tr("Ask for help", "Demander de l'aide"), divided(help)))
}
