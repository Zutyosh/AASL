//! UI strings in English and French, picked from LC_ALL/LANG. Every
//! visible string goes through `t`, or through `tr` when it is used in a
//! single place and reads better next to its code.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq)]
pub enum Lang {
    En,
    Fr,
}

pub fn detect() -> Lang {
    let chosen = crate::try_svc().and_then(|s| s.config().locale);
    let v = chosen.unwrap_or_else(|| {
        std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default()
    });
    if v.starts_with("fr") {
        Lang::Fr
    } else {
        Lang::En
    }
}

static CUR: AtomicU8 = AtomicU8::new(0);

/// The language in use; resolved once, then changed only by `set`.
pub fn current() -> Lang {
    match CUR.load(Ordering::Relaxed) {
        1 => Lang::En,
        2 => Lang::Fr,
        _ => {
            let l = detect();
            set(l);
            l
        }
    }
}

pub fn set(l: Lang) {
    CUR.store(if l == Lang::Fr { 2 } else { 1 }, Ordering::Relaxed);
}

pub fn t(key: &str) -> &'static str {
    let lang = current();
    match (TABLE.iter().find(|(k, _, _)| *k == key), lang) {
        (Some((_, en, _)), Lang::En) => en,
        (Some((_, _, fr)), Lang::Fr) => fr,
        (None, _) => "?",
    }
}

/// A string used in one place: English and French side by side.
pub fn tr(en: &'static str, fr: &'static str) -> &'static str {
    match current() {
        Lang::En => en,
        Lang::Fr => fr,
    }
}

/// "{}" slots filled in order.
pub fn fill(template: &str, args: &[String]) -> String {
    let mut out = template.to_string();
    for a in args {
        out = out.replacen("{}", a, 1);
    }
    out
}

/// A string that may be absent: the caller falls back to something else.
pub fn lookup(key: &str) -> Option<&'static str> {
    let lang = current();
    TABLE
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, en, fr)| match lang {
            Lang::En => *en,
            Lang::Fr => *fr,
        })
}

/// A fix's name in the user's language; unknown (custom) fixes keep theirs.
pub fn fix_title(id: &str, fallback: &str) -> String {
    lookup(&format!("fix.{id}.title"))
        .unwrap_or(fallback)
        .to_string()
}

/// What a fix does, in plain words.
pub fn fix_summary(id: &str, fallback: &str) -> String {
    lookup(&format!("fix.{id}.sum"))
        .unwrap_or(fallback)
        .to_string()
}

/// A shipped recipe's name and description, translated.
pub fn recipe_title(id: &str, fallback: &str) -> String {
    lookup(&format!("recipe.{id}.title"))
        .unwrap_or(fallback)
        .to_string()
}

pub fn recipe_desc(id: &str, fallback: &str) -> String {
    lookup(&format!("recipe.{id}.desc"))
        .unwrap_or(fallback)
        .to_string()
}

/// Step label for an install step id, falling back to the id.
pub fn step(id: &str) -> String {
    let s = t(&format!("step.{id}"));
    if s == "?" {
        id.replace('_', " ")
    } else {
        s.to_string()
    }
}

const TABLE: &[(&str, &str, &str)] = &[
    ("update.available", "Update", "Mise à jour"),
    ("update.install", "Install", "Installer"),
    ("update.restart", "Restart AASL", "Redémarrer AASL"),
    (
        "update.package",
        "Update through your package manager",
        "Mettez à jour via votre gestionnaire de paquets",
    ),
    ("prefs.updates", "Updates", "Mises à jour"),
    ("prefs.updates.version", "Version", "Version"),
    ("prefs.updates.check", "Check now", "Vérifier"),
    ("prefs.updates.uptodate", "Up to date", "À jour"),
    (
        "prefs.updates.auto",
        "Check for updates at start-up",
        "Vérifier les mises à jour au démarrage",
    ),
    (
        "prefs.updates.beta",
        "Include beta versions",
        "Inclure les versions bêta",
    ),
    (
        "app.tagline",
        "Somnium Space on Linux",
        "Somnium Space sous Linux",
    ),
    ("nav.settings", "Settings", "Réglages"),
    ("nav.back", "Back", "Retour"),
    ("play", "Play", "Jouer"),
    ("play.flat", "Desktop mode", "Mode bureau"),
    ("play.stop", "Stop", "Arrêter"),
    ("play.running", "Running", "En cours"),
    ("play.waiting", "Waiting for VR", "En attente de la VR"),
    ("play.install", "Install", "Installer"),
    ("play.repair", "Repair", "Réparer"),
    ("stat.vr", "VR", "VR"),
    ("stat.proton", "Proton", "Proton"),
    ("stat.patches", "Patches", "Correctifs"),
    ("stat.ready", "ready", "prête"),
    ("stat.not_ready", "not ready", "pas prête"),
    ("stat.none", "none", "aucun"),
    ("stat.intact", "intact", "intacts"),
    ("stat.changed", "changed", "modifiés"),
    ("busy", "Working", "En cours"),
    ("cancel", "Cancel", "Annuler"),
    ("dismiss", "Dismiss", "Fermer"),
    (
        "wiz.welcome.title",
        "Somnium Space on Linux",
        "Somnium Space sous Linux",
    ),
    (
        "wiz.welcome.body",
        "Two choices, then AASL installs everything in the background: Proton, the Wine fixes Somnium needs, and Somnium itself. Nothing outside your home folder changes.",
        "Deux choix, puis AASL installe tout en arrière-plan : Proton, les correctifs Wine dont Somnium a besoin, et Somnium lui-même. Rien ne change en dehors de votre dossier personnel.",
    ),
    (
        "wiz.sys.python_old",
        "umu-run needs Python 3.10 or newer",
        "umu-run demande Python 3.10 ou plus récent",
    ),
    (
        "wiz.sys.python_missing",
        "Python 3 is missing. Install python3 with your package manager.",
        "Python 3 est absent. Installez python3 avec votre gestionnaire de paquets.",
    ),
    (
        "wiz.sys.vulkan",
        "No Vulkan driver found. Install your GPU's Vulkan driver first.",
        "Aucun pilote Vulkan trouvé. Installez d'abord le pilote Vulkan de votre carte graphique.",
    ),
    (
        "wiz.proton.more",
        "Every other build (GE, CachyOS, EM, Sarek, RTSP) is in Settings > Proton once Somnium is installed.",
        "Les autres versions (GE, CachyOS, EM, Sarek, RTSP) sont dans Réglages > Proton une fois Somnium installé.",
    ),
    (
        "wiz.source.title",
        "Somnium Space's installer",
        "L'installeur de Somnium Space",
    ),
    (
        "wiz.source.body",
        "AASL downloads the official installer from somniumspace.com and runs it silently, in",
        "AASL télécharge l'installeur officiel sur somniumspace.com et le lance en silence, en",
    ),
    (
        "wiz.source.download",
        "Download and install",
        "Télécharger et installer",
    ),
    (
        "wiz.source.have",
        "Already have the installer?",
        "Vous avez déjà l'installeur ?",
    ),
    (
        "wiz.source.drop",
        "Drop the .exe anywhere on this window.",
        "Glissez le .exe n'importe où sur cette fenêtre.",
    ),
    (
        "wiz.source.drop_now",
        "Drop the installer",
        "Déposez l'installeur",
    ),
    (
        "wiz.source.drop_hint",
        "AASL installs Somnium with this file instead of downloading it.",
        "AASL installe Somnium avec ce fichier au lieu de le télécharger.",
    ),
    ("wiz.source.browse", "Choose the file", "Choisir le fichier"),
    (
        "wiz.source.not_exe",
        "That is not a Windows installer. Pick the .exe from somniumspace.com.",
        "Ce n'est pas un installeur Windows. Choisissez le .exe de somniumspace.com.",
    ),
    ("wiz.installing.step", "Step", "Étape"),
    (
        "wiz.installing.hint",
        "You can leave this window open; AASL moves on by itself.",
        "Laissez cette fenêtre ouverte, AASL continue tout seul.",
    ),
    (
        "wiz.failed.title",
        "The install stopped",
        "L'installation s'est arrêtée",
    ),
    (
        "wiz.failed.hint",
        "Steps already done are kept, so trying again resumes where it stopped.",
        "Les étapes terminées sont gardées : réessayer reprend là où ça s'est arrêté.",
    ),
    ("wiz.failed.retry", "Try again", "Réessayer"),
    ("load.0", "Waking up Wine", "Réveil de Wine"),
    (
        "load.1",
        "Teaching Proton the way to Somnium",
        "Proton apprend le chemin de Somnium",
    ),
    (
        "load.2",
        "Unpacking a few thousand files",
        "Déballage de quelques milliers de fichiers",
    ),
    (
        "load.3",
        "Fitting the TLS fixes so sign-in works",
        "Pose des correctifs TLS pour la connexion",
    ),
    (
        "load.4",
        "Building a small Windows in your home folder",
        "Construction d'un petit Windows dans votre dossier",
    ),
    (
        "load.5",
        "Checking every download against its checksum",
        "Vérification de chaque téléchargement",
    ),
    (
        "load.6",
        "Placing the last voxels",
        "Pose des derniers voxels",
    ),
    (
        "load.7",
        "Tuning video playback for VR worlds",
        "Réglage de la vidéo pour les mondes VR",
    ),
    (
        "load.8",
        "The first start of a Wine prefix takes a minute",
        "Le premier démarrage d'un préfixe Wine prend une minute",
    ),
    (
        "load.9",
        "Somnium's installer is running in the background",
        "L'installeur de Somnium tourne en arrière-plan",
    ),
    ("wiz.start", "Get started", "Commencer"),
    ("wiz.next", "Continue", "Continuer"),
    ("wiz.back", "Back", "Retour"),
    ("wiz.proton.title", "Choose Proton", "Choisir Proton"),
    (
        "wiz.proton.body",
        "These builds are tested with Somnium and its Wine fixes. The recommended one suits most setups.",
        "Ces versions sont testées avec Somnium et ses correctifs Wine. La version recommandée convient à la plupart des configurations.",
    ),
    ("wiz.proton.recommended", "Recommended", "Recommandé"),
    ("wiz.installing.title", "Installing", "Installation"),
    ("wiz.done.title", "Ready", "Prêt"),
    (
        "wiz.done.body",
        "Somnium Space is installed. Start your VR runtime, connect the headset, then press Play.",
        "Somnium Space est installé. Lancez votre runtime VR, branchez le casque, puis appuyez sur Jouer.",
    ),
    ("wiz.done.go", "Open the launcher", "Ouvrir le lanceur"),
    (
        "step.check_deps",
        "Checking dependencies",
        "Vérification des dépendances",
    ),
    (
        "step.stop_running",
        "Checking for a running session",
        "Recherche d'une session en cours",
    ),
    (
        "step.catalog",
        "Reading the catalog",
        "Lecture du catalogue",
    ),
    ("step.umu", "umu-run", "umu-run"),
    ("step.proton", "Proton", "Proton"),
    ("step.patches", "Wine patches", "Correctifs Wine"),
    ("step.prefix", "Wine prefix", "Préfixe Wine"),
    (
        "step.somnium_installer",
        "Somnium's installer",
        "Installeur de Somnium",
    ),
    ("step.desktop_entry", "Menu entry", "Entrée de menu"),
    ("prefs.general", "General", "Général"),
    (
        "prefs.recipes",
        "Compatibility recipes",
        "Recettes de compatibilité",
    ),
    (
        "recipes.lead",
        "Somnium needs a Proton and a few fixes on top of it. The setup walks you through both.",
        "Somnium a besoin d'un Proton et de quelques correctifs par-dessus. L'assistant vous guide pour les deux.",
    ),
    ("recipes.tab.setup", "Setup", "Configuration"),
    ("recipes.tab.mine", "My recipes", "Mes recettes"),
    ("setup.current", "Current setup", "Configuration actuelle"),
    (
        "setup.change",
        "Change the setup",
        "Modifier la configuration",
    ),
    ("setup.configure", "Set up", "Configurer"),
    ("setup.repair", "Repair", "Réparation"),
    ("setup.mine", "My recipe", "Ma recette"),
    ("setup.step.proton", "Proton", "Proton"),
    ("setup.step.fixes", "Fixes", "Correctifs"),
    ("setup.step.build", "Install", "Installation"),
    ("setup.choose", "Choose", "Choisir"),
    ("setup.chosen", "chosen", "choisi"),
    ("setup.recommended", "recommended", "recommandé"),
    ("setup.downloaded", "downloaded", "téléchargé"),
    ("setup.picked", "Chosen:", "Choisi :"),
    ("setup.next", "Continue", "Continuer"),
    ("setup.back", "Back", "Retour"),
    ("setup.on", "on", "sur"),
    (
        "setup.proton.title",
        "Pick a Proton",
        "Choisissez un Proton",
    ),
    (
        "setup.proton.lead",
        "Proton runs Somnium on Linux. The recommended build suits most setups.",
        "Proton fait tourner Somnium sous Linux. La version recommandée convient à la plupart des configurations.",
    ),
    (
        "setup.fixes.title",
        "Pick the fixes",
        "Choisissez les correctifs",
    ),
    (
        "setup.fixes.lead",
        "Sign-in and video need them. Start from a recipe, then tick or untick.",
        "La connexion et les vidéos en ont besoin. Partez d'une recette, puis cochez ou décochez.",
    ),
    ("setup.start_from", "Start from", "Partir de"),
    ("setup.name", "Name your recipe", "Nommez votre recette"),
    (
        "setup.name.sub",
        "Your choice differs from the recipe, so it is saved as a new one.",
        "Votre choix diffère de la recette : il est enregistré comme une nouvelle recette.",
    ),
    (
        "setup.build.title",
        "Build and install",
        "Construire et installer",
    ),
    (
        "setup.build.lead",
        "AASL downloads the Proton, builds the fixes if needed and installs them.",
        "AASL télécharge le Proton, construit les correctifs si besoin et les installe.",
    ),
    ("setup.sum", "Summary", "Récapitulatif"),
    ("setup.sum.proton", "Proton", "Proton"),
    ("setup.sum.recipe", "Recipe", "Recette"),
    ("setup.sum.fixes", "Fixes", "Correctifs"),
    ("setup.sum.patches", "Wine patches", "Correctifs Wine"),
    ("setup.apply", "Install", "Installer"),
    (
        "setup.apply.build",
        "Build and install",
        "Construire et installer",
    ),
    ("setup.running", "Setting things up", "Mise en place"),
    ("setup.running.sub", "Starting…", "Démarrage…"),
    ("setup.done", "ready", "prêt"),
    (
        "setup.done.sub",
        "Somnium now runs on this Proton with its fixes.",
        "Somnium tourne maintenant sur ce Proton avec ses correctifs.",
    ),
    ("setup.finish", "Done", "Terminer"),
    ("setup.failed", "It did not work", "Échec"),
    (
        "setup.failed.lead",
        "Nothing was changed. Fix the cause below, then try again.",
        "Rien n'a été modifié. Corrigez la cause ci-dessous, puis réessayez.",
    ),
    ("setup.retry", "Try again", "Réessayer"),
    ("recipes.tab.sessions", "Sessions", "Sessions"),
    ("recipes.tab.community", "Community", "Communauté"),
    ("recipes.copy", "copy", "copie"),
    (
        "recipes.copied",
        "Copied to the clipboard.",
        "Copié dans le presse-papiers.",
    ),
    ("recipes.standing.community", "community", "communauté"),
    ("recipes.standing.proposed", "proposed", "proposée"),
    ("recipes.standing.untested", "untested", "non testée"),
    ("recipes.sessions", "sessions", "sessions"),
    ("ses.sum.count", "Sessions", "Parties"),
    ("ses.sum.time", "Play time", "Temps de jeu"),
    ("ses.sum.fps", "Average frame rate", "Fluidité moyenne"),
    ("ses.sum.crash", "Crashes in game", "Plantages en jeu"),
    ("ses.back", "Sessions", "Sessions"),
    ("ses.world.one", "1 world", "1 monde"),
    (
        "ses.avg_line",
        "{} fps on average, {} fps at its worst",
        "{} fps en moyenne, {} fps dans les pires moments",
    ),
    (
        "ses.no_perf",
        "No frame data: MangoHud was not recording",
        "Pas de mesure de fluidité : MangoHud n'enregistrait pas",
    ),
    ("ses.t.duration", "Duration", "Durée"),
    ("ses.t.avg", "Frame rate", "Images par seconde"),
    ("ses.t.low", "Worst moments", "Pires moments"),
    ("ses.t.crash", "Crash", "Plantage"),
    ("ses.crash.none", "None", "Aucun"),
    (
        "ses.chart.fps",
        "Frame rate over time",
        "Fluidité au fil du temps",
    ),
    (
        "ses.chart.fps.sub",
        "Higher is smoother. Move the pointer over the curve for the exact values.",
        "Plus c'est haut, plus c'est fluide. Passez la souris sur la courbe pour les valeurs exactes.",
    ),
    (
        "ses.chart.load",
        "GPU and processor load",
        "Charge du GPU et du processeur",
    ),
    (
        "ses.chart.load.sub",
        "A GPU near 100 % is asked too much. A low GPU while the game slows down points at the processor or at loading.",
        "Un GPU proche de 100 % en demande trop. Un GPU bas pendant un ralentissement pointe le processeur ou un chargement.",
    ),
    ("ses.lg.avg", "Average", "Moyenne"),
    ("ses.lg.min", "Slowest frame", "Image la plus lente"),
    ("ses.lg.freeze", "Freeze", "Gel"),
    ("ses.lg.gpu", "GPU", "GPU"),
    ("ses.lg.cpu", "Processor", "Processeur"),
    (
        "ses.read.fps",
        "At {}: {} fps on average, {} fps at the slowest",
        "À {} : {} fps en moyenne, {} fps au plus bas",
    ),
    (
        "ses.read.load",
        "At {}: GPU {} %, processor {} %",
        "À {} : GPU {} %, processeur {} %",
    ),
    (
        "ses.no_chart",
        "No curve for this session",
        "Pas de courbe pour cette session",
    ),
    (
        "ses.no_chart.sub",
        "It was recorded before AASL kept the detail, or MangoHud was not there.",
        "Elle date d'avant que AASL garde le détail, ou MangoHud n'était pas là.",
    ),
    (
        "ses.partial",
        "Only part of the session was measured",
        "Une partie seulement de la session a été mesurée",
    ),
    (
        "ses.partial.sub",
        "MangoHud saw {} of {}: it only measures what the game draws in its window, not in the headset. The figures cover that part.",
        "MangoHud a vu {} sur {} : il ne mesure que ce que le jeu affiche dans sa fenêtre, pas dans le casque. Les chiffres portent sur cette partie.",
    ),
    (
        "ses.slow",
        "What slowed the game down",
        "Ce qui a ralenti le jeu",
    ),
    ("ses.dips", "Slowdowns", "Ralentissements"),
    (
        "ses.dips.sub",
        "{} episodes, {} s in total, around {} fps",
        "{} épisodes, {} s au total, vers {} fps",
    ),
    ("ses.cause", "Likely cause", "Cause probable"),
    (
        "ses.cause.gpu_bound",
        "The GPU is saturated: the game draws more than it can handle (resolution, avatars, world).",
        "Le GPU est saturé : le jeu dessine plus qu'il ne peut (résolution, avatars, monde).",
    ),
    (
        "ses.cause.gpu_clock",
        "The GPU clock drops during slowdowns: power management.",
        "La fréquence du GPU chute pendant les ralentissements : gestion d'énergie.",
    ),
    (
        "ses.cause.gpu_waiting",
        "The GPU waits: the bottleneck is the processor, the VR compositor or loading.",
        "Le GPU attend : le goulot est le processeur, le compositeur VR ou un chargement.",
    ),
    (
        "ses.cause.unknown",
        "The cause could not be told from the log.",
        "La cause n'a pas pu être déterminée avec le journal.",
    ),
    (
        "ses.dips.gpu",
        "GPU load: {} % normally, {} % during slowdowns",
        "Charge GPU : {} % en temps normal, {} % pendant les ralentissements",
    ),
    ("ses.hitch", "Freezes", "Gels"),
    (
        "ses.hitch.sub",
        "{} frames of {} ms or more, the longest {} ms",
        "{} images de {} ms ou plus, la plus longue {} ms",
    ),
    (
        "ses.hitch.period",
        "They come back every {} s: something in the game runs on a timer, it is not a load problem.",
        "Ils reviennent toutes les {} s : une tâche du jeu tourne sur minuterie, ce n'est pas un problème de charge.",
    ),
    ("ses.stutter", "Slow frames", "Images lentes"),
    (
        "ses.stutter.sub",
        "{} frames took more than twice the usual time.",
        "{} images ont pris plus de deux fois le temps habituel.",
    ),
    ("ses.p99", "Time per frame", "Temps par image"),
    (
        "ses.p99.sub",
        "99 % of frames took under {} ms.",
        "99 % des images ont pris moins de {} ms.",
    ),
    ("ses.g.title", "In the game", "Dans le jeu"),
    ("ses.g.worlds", "Worlds entered", "Mondes visités"),
    ("ses.g.joins", "Sessions joined", "Salons rejoints"),
    ("ses.g.videos", "Videos played", "Vidéos lues"),
    (
        "ses.g.videos.sub",
        "Opened without error, out of all opened.",
        "Ouvertes sans erreur, sur le total ouvert.",
    ),
    (
        "ses.g.errors",
        "Errors logged by the game",
        "Erreurs notées par le jeu",
    ),
    (
        "ses.g.errors.sub",
        "Somnium logs a few on every platform.",
        "Somnium en note quelques-unes sur toutes les plateformes.",
    ),
    ("ses.hw", "This machine", "Cette machine"),
    ("ses.hw.gpu", "Graphics card", "Carte graphique"),
    ("ses.hw.driver", "Driver", "Pilote"),
    ("ses.hw.cpu", "Processor", "Processeur"),
    ("ses.hw.ram", "Memory", "Mémoire"),
    ("ses.hw.ram.v", "{} GB", "{} Go"),
    ("ses.hw.os", "System", "Système"),
    ("ses.hw.kernel", "Kernel", "Noyau"),
    ("ses.hw.vr", "VR runtime", "Runtime VR"),
    (
        "ses.delete",
        "Delete this session",
        "Supprimer cette session",
    ),
    (
        "ses.delete.confirm",
        "Delete for good",
        "Supprimer définitivement",
    ),
    (
        "ses.delete.sub",
        "Its report and curve are erased. Recipes lose this evidence.",
        "Son rapport et sa courbe sont effacés. Les recettes perdent cette preuve.",
    ),
    ("recipes.stat.one", "1 session", "1 partie"),
    ("recipes.stat.many", "sessions", "parties"),
    ("recipes.stat.nocrash", "no crash", "aucun crash"),
    ("recipes.stat.crash.one", "1 crash", "1 crash"),
    ("recipes.stat.crash.many", "crashes", "crashs"),
    ("recipes.stat.videos", "videos played", "vidéos lues"),
    ("recipes.stat.fixes.one", "1 fix", "1 correctif"),
    ("recipes.stat.fixes.many", "fixes", "correctifs"),
    ("recipes.stat.pinned", "Proton", "Proton"),
    ("recipes.stat.tested", "tested on", "testée sur"),
    ("recipes.crashes", "crashes", "crashs"),
    ("recipes.videos", "videos", "vidéos"),
    (
        "fix.tls-client-certs.title",
        "Account sign-in",
        "Connexion au compte",
    ),
    (
        "fix.tls-client-certs.sum",
        "Without it, signing in to Somnium never completes.",
        "Sans lui, la connexion à Somnium n'aboutit jamais.",
    ),
    (
        "fix.media-by-url.title",
        "Videos from links",
        "Vidéos par lien",
    ),
    (
        "fix.media-by-url.sum",
        "Plays the live and web video streams that worlds use.",
        "Lit les flux vidéo (direct, web) utilisés dans les mondes.",
    ),
    (
        "fix.ytdlp-hook.title",
        "YouTube downloader",
        "Téléchargeur YouTube",
    ),
    (
        "fix.ytdlp-hook.sum",
        "Lets Somnium fetch YouTube videos through yt-dlp.",
        "Permet à Somnium de récupérer les vidéos YouTube avec yt-dlp.",
    ),
    (
        "fix.ytdlp-android-client.title",
        "YouTube with sound (obsolete)",
        "YouTube avec le son (obsolète)",
    ),
    (
        "fix.ytdlp-android-client.sum",
        "Picks a 360p stream that carries picture and sound together. Needs the YouTube downloader.",
        "Choisit un flux 360p qui contient l'image et le son. Demande le téléchargeur YouTube.",
    ),
    ("recipe.recommended.title", "Recommended", "Recommandée"),
    (
        "recipe.recommended.desc",
        "Sign-in, videos and YouTube with sound on the Proton you pick.",
        "Connexion, vidéos et YouTube avec le son sur le Proton choisi.",
    ),
    ("recipes.worlds", "worlds", "mondes"),
    (
        "recipes.build",
        "Build the patches",
        "Construire les correctifs",
    ),
    (
        "recipes.build.sub",
        "Compiles the recipe's Wine patches for this Proton in podman, then installs them. Cached: the same recipe and Proton are built once.",
        "Compile les correctifs Wine de la recette pour ce Proton dans podman, puis les installe. Mis en cache : une même recette sur un même Proton n'est construite qu'une fois.",
    ),
    ("recipes.build.now", "Build", "Construire"),
    ("recipes.restore", "Restore", "Restaurer"),
    (
        "recipes.restore.sub",
        "Puts Proton's own files back. Sign-in and some videos stop working until the next repair.",
        "Remet les fichiers d'origine de Proton. La connexion et certaines vidéos ne marchent plus jusqu'à la prochaine réparation.",
    ),
    ("recipes.in_use", "In use", "Utilisée"),
    ("recipes.active", "in use", "utilisée"),
    ("recipes.builtin", "built-in", "intégrée"),
    ("recipes.yours", "Your recipes", "Vos recettes"),
    (
        "recipes.official",
        "Official recipes",
        "Recettes officielles",
    ),
    ("recipes.fork", "Fork", "Forker"),
    ("recipes.fork.suffix", "fork", "fork"),
    ("recipes.edit", "Edit", "Modifier"),
    (
        "recipes.yours.none",
        "No recipe of your own yet",
        "Pas encore de recette perso",
    ),
    (
        "recipes.yours.none.sub",
        "Fork an official one to start from it, or import a shared recipe.",
        "Forkez une recette officielle pour partir de celle-ci, ou importez une recette partagée.",
    ),
    (
        "recipes.proton.picked",
        "Proton picked in AASL",
        "Proton choisi dans AASL",
    ),
    ("recipes.proton.latest", "latest", "dernière"),
    ("recipes.custom", "custom patches", "correctifs perso"),
    (
        "recipes.custom.warn",
        "This recipe brings Wine patches that are not in AASL's library. They run inside Somnium: only use recipes from people you trust.",
        "Cette recette apporte des correctifs Wine absents de la bibliothèque d'AASL. Ils tournent dans Somnium : n'utilisez que des recettes de personnes de confiance.",
    ),
    ("recipes.duplicate", "Modify", "Modifier"),
    ("recipes.export", "Copy", "Copier"),
    ("recipes.share", "Share", "Partager"),
    (
        "recipes.share.prefilled",
        "The GitHub form opened, filled in",
        "Le formulaire GitHub s'est ouvert, déjà rempli",
    ),
    (
        "recipes.share.paste",
        "The GitHub form opened: paste the two parts below",
        "Le formulaire GitHub s'est ouvert : collez-y les deux parties ci-dessous",
    ),
    (
        "recipes.share.sub",
        "Submit it; a bot checks the recipe, judges the reports and opens the pull request. No git needed.",
        "Envoyez-le : un bot vérifie la recette, juge les rapports et ouvre la pull request. Pas besoin de git.",
    ),
    (
        "recipes.share.open",
        "Open the form",
        "Ouvrir le formulaire",
    ),
    ("recipes.share.recipe", "Recipe", "Recette"),
    (
        "recipes.share.reports",
        "Session reports",
        "Rapports de session",
    ),
    ("recipes.copy.button", "Copy", "Copier"),
    ("recipes.new", "New recipe", "Nouvelle recette"),
    (
        "recipes.import",
        "Import from the clipboard",
        "Importer depuis le presse-papiers",
    ),
    (
        "recipes.import.sub",
        "Copy a recipe (TOML) shared by someone, then import it here.",
        "Copiez une recette (TOML) partagée par quelqu'un, puis importez-la ici.",
    ),
    ("recipes.imported", "Recipe saved:", "Recette enregistrée :"),
    ("recipes.saved", "Saved", "Enregistrée"),
    (
        "recipes.create.lead",
        "Pick a Proton and the fixes to put on it. Fixes the Proton already carries are skipped when building.",
        "Choisissez un Proton et les correctifs à y ajouter. Ceux que le Proton contient déjà sont ignorés à la construction.",
    ),
    ("recipes.field.title", "Name", "Nom"),
    ("recipes.field.title.ph", "My recipe", "Ma recette"),
    (
        "recipes.field.id",
        "Identifier (lowercase, digits, - and _)",
        "Identifiant (minuscules, chiffres, - et _)",
    ),
    ("recipes.field.description", "Description", "Description"),
    ("recipes.field.proton", "Proton", "Proton"),
    (
        "recipes.field.proton.sub",
        "\"latest\" follows new releases; a fixed version never changes.",
        "« dernière » suit les nouvelles versions ; une version fixe ne change jamais.",
    ),
    ("recipes.ingredients", "Fixes", "Correctifs"),
    (
        "recipes.order",
        "Applied in this order",
        "Appliqués dans cet ordre",
    ),
    ("recipes.kind.patch", "compiled", "à compiler"),
    ("recipes.kind.env", "setting", "réglage"),
    (
        "recipes.check.build",
        "Valid · builds patches",
        "Valide · construit des correctifs",
    ),
    (
        "recipes.check.env",
        "Valid · nothing to build",
        "Valide · rien à construire",
    ),
    ("recipes.cancel", "Cancel", "Annuler"),
    ("recipes.save", "Save", "Enregistrer"),
    (
        "recipes.save_use",
        "Save and use",
        "Enregistrer et utiliser",
    ),
    ("recipes.crash", "crash", "crash"),
    (
        "recipes.crash.exit",
        "crash on exit",
        "crash à la fermeture",
    ),
    ("recipes.desktop", "desktop", "bureau"),
    ("recipes.sessions.title", "Recording", "Enregistrement"),
    (
        "recipes.sessions.record",
        "Record sessions",
        "Enregistrer les sessions",
    ),
    (
        "recipes.sessions.record.sub",
        "After each game: crashes, worlds, videos, and frame pacing with MangoHud. Anonymous and kept here; nothing is sent unless you share it.",
        "Après chaque partie : crashs, mondes, vidéos et régularité d'image avec MangoHud. Anonyme et gardé ici ; rien n'est envoyé sans partage.",
    ),
    (
        "recipes.sessions.last",
        "Record the last session",
        "Enregistrer la dernière session",
    ),
    (
        "recipes.sessions.last.sub",
        "Only needed when Somnium was started with aasl launch --detach.",
        "Utile seulement si Somnium a été lancé avec aasl launch --detach.",
    ),
    ("recipes.sessions.last.button", "Record", "Enregistrer"),
    (
        "recipes.sessions.saved",
        "Saved sessions",
        "Sessions enregistrées",
    ),
    (
        "recipes.sessions.none",
        "No session yet",
        "Aucune session pour l'instant",
    ),
    (
        "recipes.sessions.none.sub",
        "Play in VR through AASL; the report appears when the game closes.",
        "Jouez en VR via AASL ; le rapport apparaît à la fermeture du jeu.",
    ),
    (
        "recipes.community.lead",
        "Recipes shared by players, judged on their session reports by the same rules AASL uses.",
        "Les recettes partagées par les joueurs, jugées sur leurs rapports de session avec les mêmes règles qu'AASL.",
    ),
    (
        "recipes.community.offline",
        "Could not reach the community index",
        "Index communautaire inaccessible",
    ),
    (
        "recipes.community.empty",
        "No community recipe yet",
        "Aucune recette communautaire pour l'instant",
    ),
    (
        "recipes.community.empty.sub",
        "Be the first: make a recipe, play with it, then Share.",
        "Soyez le premier : créez une recette, jouez avec, puis Partager.",
    ),
    ("recipes.community.add", "Add", "Ajouter"),
    (
        "build.title",
        "Building the patches",
        "Construction des correctifs",
    ),
    (
        "build.sub",
        "Wine compiles in a container. The first build takes a while; later ones reuse its work.",
        "Wine se compile dans un conteneur. Le premier build est long ; les suivants réutilisent son travail.",
    ),
    (
        "build.stage.image",
        "Build environment",
        "Environnement de compilation",
    ),
    ("build.stage.sources", "Wine sources", "Sources de Wine"),
    (
        "build.stage.prep",
        "Proton's own patches",
        "Correctifs propres au Proton",
    ),
    (
        "build.stage.patch",
        "Applying the recipe",
        "Application de la recette",
    ),
    ("build.stage.compile", "Compiling", "Compilation"),
    ("build.stage.package", "Packaging", "Empaquetage"),
    (
        "build.hide",
        "Continue in the background",
        "Continuer en arrière-plan",
    ),
    ("prefs.proton", "Proton", "Proton"),
    ("prefs.patches", "Wine patches", "Correctifs Wine"),
    ("prefs.vr", "VR", "VR"),
    ("prefs.diagnostics", "Diagnostics", "Diagnostic"),
    ("prefs.logs", "Logs", "Journaux"),
    ("prefs.agents", "Agents", "Agents"),
    ("prefs.lang", "Language", "Langue"),
    ("prefs.lang.auto", "Auto", "Auto"),
    (
        "prefs.lang.sub",
        "Auto follows the language of your system",
        "Auto suit la langue du système",
    ),
    ("prefs.wait", "Wait for the headset", "Attendre le casque"),
    (
        "prefs.wait.sub",
        "Hold the launch until the VR runtime is ready instead of starting in desktop mode.",
        "Retarder le lancement jusqu'à ce que la VR soit prête plutôt que de démarrer en mode bureau.",
    ),
    ("prefs.prefix", "Wine prefix", "Préfixe Wine"),
    (
        "prefs.repair",
        "Repair installation",
        "Réparer l'installation",
    ),
    (
        "prefs.repair.sub",
        "Re-apply umu-run, Proton and the patches. Your prefix and settings stay.",
        "Réapplique umu-run, Proton et les correctifs. Le préfixe et les réglages restent.",
    ),
    ("prefs.proton.installed", "Installed", "Installés"),
    ("prefs.proton.available", "Available", "Disponibles"),
    ("prefs.proton.use", "Use", "Utiliser"),
    ("prefs.proton.in_use", "In use", "Utilisé"),
    ("prefs.proton.download", "Download", "Télécharger"),
    ("prefs.proton.remove", "Remove", "Supprimer"),
    ("prefs.proton.refresh", "Refresh", "Actualiser"),
    (
        "prefs.patch.verified",
        "patches verified",
        "correctifs vérifiés",
    ),
    (
        "prefs.patch.untested",
        "patches untested",
        "correctifs non testés",
    ),
    ("prefs.patch.none", "no patches", "sans correctifs"),
    (
        "prefs.patch.built",
        "patches built here",
        "correctifs construits ici",
    ),
    (
        "prefs.patch.on_use",
        "patches built on use",
        "correctifs construits à l'usage",
    ),
    (
        "prefs.patch.not_needed",
        "nothing to build",
        "rien à construire",
    ),
    (
        "prefs.patch.need_builder",
        "needs podman or docker",
        "nécessite podman ou docker",
    ),
    ("prefs.proton.pre", "pre-release", "préversion"),
    (
        "prefs.proton.more",
        "Show older releases",
        "Afficher les versions plus anciennes",
    ),
    (
        "prefs.proton.lead",
        "Proton runs Somnium (a Windows game) on Linux. Pick the build Somnium uses: AASL downloads it, then adds the Wine patches Somnium needs (see Wine patches). \"patches verified\" builds work right away; the others need podman or docker to build those patches first, which takes a while.",
        "Proton fait tourner Somnium (un jeu Windows) sous Linux. Choisissez la version utilisée : AASL la télécharge, puis y ajoute les correctifs Wine dont Somnium a besoin (voir Correctifs Wine). Les versions « correctifs vérifiés » marchent tout de suite ; les autres demandent podman ou docker pour construire ces correctifs d'abord, ce qui est long.",
    ),
    (
        "prefs.patches.lead",
        "Stock Proton lacks a few fixes Somnium needs: TLS client certificates for sign-in, and video playback. AASL ships them as a bundle of Wine DLLs copied over the Proton in use. Nothing to do here normally; the Proton page handles it. Use this page to check the files, rebuild them or undo them.",
        "Proton d'origine n'a pas certains correctifs dont Somnium a besoin : certificats TLS pour la connexion et lecture vidéo. AASL les fournit sous forme d'un bundle de DLL Wine copiées par-dessus le Proton utilisé. Normalement rien à faire ici, la page Proton s'en charge. Cette page sert à vérifier ces fichiers, les reconstruire ou les retirer.",
    ),
    (
        "prefs.patches.current",
        "Bundle on the current Proton",
        "Bundle sur le Proton actuel",
    ),
    (
        "prefs.patches.build.missing",
        "Needs podman or docker, neither is installed. Install podman, then reopen this page.",
        "Nécessite podman ou docker, aucun n'est installé. Installez podman puis revenez sur cette page.",
    ),
    (
        "prefs.patches.build.optional",
        "Not needed: a verified bundle already fits this Proton. Rebuild only if it misbehaves; Wine compiles in podman (long), then the result replaces it.",
        "Inutile ici : un bundle vérifié convient déjà à ce Proton. Ne reconstruisez que s'il pose problème ; Wine se compile dans podman (long), puis le résultat le remplace.",
    ),
    (
        "prefs.patches.build.missing.cmd",
        "Needs podman or docker, neither is installed. Install podman, then reopen this page:",
        "Nécessite podman ou docker, aucun n'est installé. Installez podman puis revenez sur cette page :",
    ),
    (
        "prefs.patches.files",
        "Patch sources",
        "Sources des correctifs",
    ),
    (
        "prefs.patches.build",
        "Build a bundle for the current Proton",
        "Construire un bundle pour le Proton actuel",
    ),
    (
        "prefs.patches.build.sub",
        "No published bundle fits this Proton: compiles Wine with the patches in podman or docker, then applies them. Takes a while; follow it in Logs.",
        "Aucun bundle publié pour ce Proton : compile Wine avec les correctifs dans podman ou docker, puis les applique. C'est long ; suivez-le dans Journaux.",
    ),
    (
        "prefs.patches.restore",
        "Remove the patches (restore stock files)",
        "Retirer les correctifs (fichiers d'origine)",
    ),
    ("prefs.vr.runtime", "Active runtime", "Runtime actif"),
    (
        "prefs.vr.installed",
        "Installed runtimes",
        "Runtimes installés",
    ),
    ("prefs.vr.openvr", "OpenVR layer", "Couche OpenVR"),
    ("prefs.vr.blockers", "What is missing", "Ce qui manque"),
    (
        "prefs.vr.ready",
        "Everything is in place.",
        "Tout est en place.",
    ),
    ("diag.run", "Run checks", "Lancer les vérifications"),
    ("diag.none", "No problems found.", "Aucun problème trouvé."),
    ("diag.apply", "Apply", "Appliquer"),
    (
        "diag.report",
        "Save a bug report",
        "Enregistrer un rapport de bug",
    ),
    (
        "agents.lead",
        "Coding agents connect over MCP to read state and logs, propose fixes and build patch bundles. Add `aasl mcp` to your agent's MCP servers; it attaches to this window.",
        "Les agents de code se connectent en MCP pour lire l'état et les journaux, proposer des correctifs et construire des bundles. Ajoutez `aasl mcp` aux serveurs MCP de votre agent ; il se branche sur cette fenêtre.",
    ),
    ("agents.socket", "Socket", "Socket"),
    ("agents.approve", "Allow", "Autoriser"),
    ("agents.deny", "Deny", "Refuser"),
    (
        "agents.pending",
        "An agent asks for approval",
        "Un agent demande une autorisation",
    ),
    (
        "agents.audit",
        "Recent agent calls",
        "Appels récents des agents",
    ),
];

#[cfg(test)]
mod tests {
    #[test]
    fn keys_unique_and_filled() {
        let mut seen = std::collections::HashSet::new();
        for (k, en, fr) in super::TABLE {
            assert!(seen.insert(k), "duplicate {k}");
            assert!(!en.is_empty() && !fr.is_empty(), "{k}");
        }
    }
}
