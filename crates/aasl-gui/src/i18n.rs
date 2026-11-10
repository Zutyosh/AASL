//! UI strings in English and French, picked from LC_ALL/LANG. Every
//! visible string goes through `t`.

use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq)]
pub enum Lang {
    En,
    Fr,
}

pub fn detect() -> Lang {
    let v = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    if v.starts_with("fr") {
        Lang::Fr
    } else {
        Lang::En
    }
}

pub fn t(key: &str) -> &'static str {
    static LANG: OnceLock<Lang> = OnceLock::new();
    let lang = *LANG.get_or_init(detect);
    match (TABLE.iter().find(|(k, _, _)| *k == key), lang) {
        (Some((_, en, _)), Lang::En) => en,
        (Some((_, _, fr)), Lang::Fr) => fr,
        (None, _) => "?",
    }
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
    (
        "wiz.welcome.title",
        "Somnium Space, set up for Linux",
        "Somnium Space, prêt pour Linux",
    ),
    (
        "wiz.welcome.body",
        "AASL checks your system, downloads a patched Proton and runs Somnium's own installer. Nothing outside your home folder changes.",
        "AASL vérifie votre système, télécharge un Proton patché et lance l'installeur officiel de Somnium. Rien ne change en dehors de votre dossier personnel.",
    ),
    ("wiz.start", "Get started", "Commencer"),
    ("wiz.next", "Continue", "Continuer"),
    ("wiz.back", "Back", "Retour"),
    ("wiz.system.title", "Your system", "Votre système"),
    (
        "wiz.system.body",
        "These are read from your machine. Red items block the install; amber ones only matter for VR.",
        "Ces informations viennent de votre machine. En rouge, ce qui bloque l'installation ; en orange, ce qui ne compte que pour la VR.",
    ),
    ("wiz.proton.title", "Choose Proton", "Choisir Proton"),
    (
        "wiz.proton.body",
        "Somnium needs Wine fixes that only exist as AASL patch bundles. Pick a build with verified patches unless you want to test.",
        "Somnium a besoin de correctifs Wine qui n'existent que sous forme de bundles AASL. Choisissez une version aux correctifs vérifiés, sauf pour tester.",
    ),
    ("wiz.proton.recommended", "Recommended", "Recommandé"),
    (
        "wiz.installer.title",
        "Somnium's installer",
        "L'installeur de Somnium",
    ),
    (
        "wiz.installer.body",
        "Download the Windows installer from somniumspace.com and paste its path. AASL never downloads it for you.",
        "Téléchargez l'installeur Windows sur somniumspace.com et collez son chemin. AASL ne le télécharge jamais à votre place.",
    ),
    (
        "wiz.installer.path",
        "Path to SomniumSpaceSetup.exe",
        "Chemin de SomniumSpaceSetup.exe",
    ),
    ("wiz.install", "Install", "Installer"),
    ("wiz.installing.title", "Installing", "Installation"),
    ("wiz.done.title", "Ready", "Prêt"),
    (
        "wiz.done.body",
        "Somnium Space is installed. Connect your headset, start your VR runtime, then press Play.",
        "Somnium Space est installé. Branchez le casque, lancez votre runtime VR, puis appuyez sur Jouer.",
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
    ("prefs.proton", "Proton", "Proton"),
    ("prefs.patches", "Patches", "Correctifs"),
    ("prefs.vr", "VR", "VR"),
    ("prefs.diagnostics", "Diagnostics", "Diagnostic"),
    ("prefs.logs", "Logs", "Journaux"),
    ("prefs.agents", "Agents", "Agents"),
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
    ("prefs.patches.current", "Current bundle", "Bundle actuel"),
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
        "Compiles Wine with the patches in a container (podman or docker). Takes a while; follow it in Logs.",
        "Compile Wine avec les correctifs dans un conteneur (podman ou docker). C'est long ; suivez-le dans Journaux.",
    ),
    (
        "prefs.patches.restore",
        "Restore stock files",
        "Restaurer les fichiers d'origine",
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
