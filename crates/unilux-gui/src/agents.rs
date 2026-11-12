//! Agents: connect a coding agent, see what it may do and what it did.

use std::time::Duration;

use freya::prelude::*;
use unilux_agent::ToolInfo;

use crate::i18n::{fill, tr};
use crate::state::Ui;
use crate::svc;
use crate::theme::{T, size, space};
use crate::widgets::{Tone, banner, body, chip, divided, group, mono_box, muted, row};
use crate::{approvals, sessions};

#[derive(PartialEq)]
pub struct AgentsSection(pub Ui);

impl Component for AgentsSection {
    fn render(&self) -> impl IntoElement {
        view(self.0)
    }
}

/// Tool, category, what it does in English and in French.
const TOOLS: &[(&str, usize, &str, &str)] = &[
    (
        "status",
        0,
        "Install state and what is running",
        "État de l'installation et de ce qui tourne",
    ),
    (
        "system",
        0,
        "Your machine: distribution, GPU, drivers",
        "Votre machine : distribution, GPU, pilotes",
    ),
    (
        "list_processes",
        0,
        "Processes of the game",
        "Processus du jeu",
    ),
    (
        "tail_logs",
        0,
        "Recent log lines",
        "Dernières lignes des journaux",
    ),
    (
        "export_report",
        0,
        "Anonymous bug report",
        "Rapport de bug anonyme",
    ),
    (
        "run_doctor",
        1,
        "Run every check",
        "Lancer toutes les vérifications",
    ),
    (
        "list_fixes",
        1,
        "List the fixes Unilux can apply",
        "Lister les corrections applicables",
    ),
    (
        "apply_fix",
        1,
        "Apply a fix (repair, stop the game...)",
        "Appliquer une correction (réparer, arrêter le jeu...)",
    ),
    ("vr_status", 2, "VR headset readiness", "État du casque VR"),
    (
        "proton_installed",
        3,
        "Installed Proton versions",
        "Versions de Proton installées",
    ),
    (
        "proton_available",
        3,
        "Proton versions you could install",
        "Versions de Proton installables",
    ),
    (
        "proton_use",
        3,
        "Switch to another Proton",
        "Passer à un autre Proton",
    ),
    ("recipe_list", 4, "List recipes", "Lister les recettes"),
    ("recipe_get", 4, "Read a recipe", "Lire une recette"),
    (
        "ingredient_list",
        4,
        "Fixes a recipe can use",
        "Correctifs utilisables dans une recette",
    ),
    (
        "recipe_check",
        4,
        "Validate a recipe without saving it",
        "Valider une recette sans l'enregistrer",
    ),
    (
        "recipe_save",
        4,
        "Create or replace a recipe",
        "Créer ou remplacer une recette",
    ),
    (
        "recipe_build",
        4,
        "Build a recipe",
        "Construire une recette",
    ),
    (
        "recipe_use",
        4,
        "Make the game use a recipe",
        "Faire utiliser une recette au jeu",
    ),
    (
        "session_list",
        5,
        "Your game sessions",
        "Vos sessions de jeu",
    ),
    (
        "recipe_verdict",
        5,
        "How a recipe does on your sessions",
        "Comment une recette se comporte sur vos sessions",
    ),
    (
        "recipe_share",
        5,
        "Prepare a community submission, sends nothing",
        "Préparer un partage communautaire, n'envoie rien",
    ),
    (
        "community_list",
        5,
        "Community recipes",
        "Recettes de la communauté",
    ),
    (
        "community_install",
        5,
        "Add a community recipe to yours",
        "Ajouter une recette de la communauté aux vôtres",
    ),
    (
        "patch_status",
        6,
        "Patches in the current Proton",
        "Correctifs du Proton actuel",
    ),
    (
        "patch_list",
        6,
        "List every patch",
        "Lister tous les correctifs",
    ),
    ("patch_read", 6, "Read one patch", "Lire un correctif"),
    (
        "patch_plan",
        6,
        "Preview a build, runs nothing",
        "Prévoir une compilation, ne lance rien",
    ),
    (
        "patch_build",
        6,
        "Compile the patches (long)",
        "Compiler les correctifs (long)",
    ),
    (
        "patch_apply",
        6,
        "Install a compiled bundle",
        "Installer un paquet compilé",
    ),
    ("launch", 7, "Start the game", "Lancer le jeu"),
    ("stop", 7, "Stop the game", "Arrêter le jeu"),
    (
        "update_check",
        7,
        "Look for a new Unilux",
        "Chercher une nouvelle version d'Unilux",
    ),
    (
        "update_apply",
        7,
        "Install the new Unilux",
        "Installer la nouvelle version d'Unilux",
    ),
];

fn category(i: usize) -> (&'static str, &'static str) {
    match i {
        0 => (
            tr("State and logs", "État et journaux"),
            tr(
                "Read how Unilux, the game and your machine are doing.",
                "Lire comment vont Unilux, le jeu et votre machine.",
            ),
        ),
        1 => (
            tr("Checks and fixes", "Vérifications et corrections"),
            tr(
                "Run the diagnostics and apply the fixes Unilux offers.",
                "Lancer le diagnostic et appliquer les corrections proposées par Unilux.",
            ),
        ),
        2 => (
            tr("VR headset", "Casque VR"),
            tr(
                "Check that the headset is ready for the game.",
                "Vérifier que le casque est prêt pour le jeu.",
            ),
        ),
        3 => (
            tr("Proton", "Proton"),
            tr(
                "See the Proton versions and switch to another one.",
                "Voir les versions de Proton et passer à une autre.",
            ),
        ),
        4 => (
            tr("Recipes", "Recettes"),
            tr(
                "Read, write, build and use compatibility recipes.",
                "Lire, écrire, construire et utiliser des recettes de compatibilité.",
            ),
        ),
        5 => (
            tr("Sessions and community", "Sessions et communauté"),
            tr(
                "Read your game sessions and the community's recipes.",
                "Lire vos sessions de jeu et les recettes de la communauté.",
            ),
        ),
        6 => (
            tr("Wine patches", "Correctifs Wine"),
            tr(
                "Inspect, compile and install the patches.",
                "Examiner, compiler et installer les correctifs.",
            ),
        ),
        _ => (
            tr("Game and updates", "Jeu et mises à jour"),
            tr(
                "Start or stop the game, update Unilux.",
                "Lancer ou arrêter le jeu, mettre Unilux à jour.",
            ),
        ),
    }
}

fn label_of(name: &str) -> String {
    TOOLS
        .iter()
        .find(|(n, ..)| *n == name)
        .map(|(_, _, en, fr)| tr(en, fr).to_string())
        .unwrap_or_else(|| name.to_string())
}

fn ago(secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let d = now.saturating_sub(secs);
    match d {
        0..=59 => tr("just now", "à l'instant").to_string(),
        60..=3599 => fill(tr("{} min ago", "il y a {} min"), &[(d / 60).to_string()]),
        3600..=86_399 => fill(tr("{} h ago", "il y a {} h"), &[(d / 3600).to_string()]),
        _ => fill(
            tr("{} days ago", "il y a {} j"),
            &[(d / 86_400).to_string()],
        ),
    }
}

struct Call {
    at: u64,
    tool: String,
    ok: bool,
    what: Option<String>,
}

fn calls() -> Vec<Call> {
    let text = std::fs::read_to_string(svc().paths().audit_log()).unwrap_or_default();
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .map(|v| {
            let args = &v["args"];
            let what = ["id", "name", "provider", "path"]
                .iter()
                .find_map(|k| args.get(*k).and_then(|x| x.as_str()))
                .map(|s| {
                    let s = s.rsplit('/').next().unwrap_or(s);
                    if s.chars().count() > 40 {
                        format!("{}...", s.chars().take(40).collect::<String>())
                    } else {
                        s.to_string()
                    }
                });
            Call {
                at: v["at"].as_u64().unwrap_or(0),
                tool: v["tool"].as_str().unwrap_or("?").to_string(),
                ok: v["ok"].as_bool().unwrap_or(false),
                what,
            }
        })
        .collect()
}

fn access_chip(t: &ToolInfo) -> Rect {
    if t.read_only {
        chip(tr("Read only", "Lecture seule"), Tone::Ok)
    } else if t.needs_approval {
        chip(
            tr("Needs your approval", "Demande votre accord"),
            Tone::Warn,
        )
    } else {
        chip(tr("Acts on its own", "Agit seul"), Tone::Accent)
    }
}

fn step(n: usize, head: &str, text: &str, code: Option<&str>) -> Element {
    rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .padding((space::M, space::L))
        .spacing(space::M)
        .child(
            rect()
                .width(Size::px(26.))
                .height(Size::px(26.))
                .corner_radius(13.)
                .background(T.raised)
                .main_align(Alignment::Center)
                .cross_align(Alignment::Center)
                .child(
                    label()
                        .text(n.to_string())
                        .font_size(size::SMALL)
                        .font_weight(FontWeight::MEDIUM)
                        .color(T.muted),
                ),
        )
        .child(
            rect()
                .width(Size::flex(1.))
                .spacing(space::XS)
                .child(body(head.to_string()))
                .child(muted(text.to_string()))
                .maybe_child(code.map(|c| mono_box(c.to_string()))),
        )
        .into_element()
}

fn view(ui: Ui) -> impl IntoElement {
    let mut tick = use_state(|| 0u32);
    let mut open = use_state(|| false);
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let n = *tick.peek();
            tick.set(n.wrapping_add(1));
        }
    });
    let _ = *tick.read();

    let pending = ui.approvals.read().clone();
    let log = calls();
    let total = log.len();
    let failed = log.iter().filter(|c| !c.ok).count();
    let last = log.last().map(|c| c.at);
    let tools = unilux_agent::tools();
    let n_read = tools.iter().filter(|t| t.read_only).count();
    let n_ask = tools.iter().filter(|t| t.needs_approval).count();

    let verdict = if !pending.is_empty() {
        banner(
            Tone::Warn,
            if pending.len() == 1 {
                tr(
                    "An agent is waiting for your answer",
                    "Un agent attend votre réponse",
                )
                .to_string()
            } else {
                fill(
                    tr(
                        "{} requests are waiting for your answer",
                        "{} demandes attendent votre réponse",
                    ),
                    &[pending.len().to_string()],
                )
            },
            tr(
                "It does nothing until you decide. A request left alone for 2 minutes is refused.",
                "Il ne fait rien tant que vous n'avez pas décidé. Une demande sans réponse pendant 2 minutes est refusée.",
            ),
            None,
        )
    } else {
        banner(
            Tone::Neutral,
            tr("Ready for an agent", "Prêt pour un agent"),
            match last {
                Some(at) => fill(
                    tr(
                        "Last call {}. Nothing that changes your setup runs without your approval here.",
                        "Dernier appel {}. Rien qui modifie votre installation ne s'exécute sans votre accord ici.",
                    ),
                    &[ago(at)],
                ),
                None => tr(
                    "No agent has connected yet. Nothing that changes your setup runs without your approval here.",
                    "Aucun agent ne s'est encore connecté. Rien qui modifie votre installation ne s'exécute sans votre accord ici.",
                )
                .to_string(),
            },
            None,
        )
    };

    let asks: Vec<Element> = divided(
        pending
            .iter()
            .map(|(id, msg)| {
                let id = *id;
                row(
                    msg.clone(),
                    Some(tr("Requested by an agent", "Demandé par un agent").to_string()),
                    Some(
                        rect()
                            .horizontal()
                            .spacing(space::S)
                            .child(
                                Button::new()
                                    .filled()
                                    .on_press(move |_| approvals::answer(id, true))
                                    .child(tr("Allow", "Autoriser")),
                            )
                            .child(
                                Button::new()
                                    .on_press(move |_| approvals::answer(id, false))
                                    .child(tr("Refuse", "Refuser")),
                            )
                            .into_element(),
                    ),
                )
            })
            .collect(),
    );

    let connect = divided(vec![
        step(
            1,
            tr("Claude Code", "Claude Code"),
            tr(
                "Run this once in a terminal.",
                "À lancer une fois dans un terminal.",
            ),
            Some("claude mcp add unilux -- unilux mcp"),
        ),
        step(
            2,
            tr(
                "Another agent (Cursor, Zed, and so on)",
                "Un autre agent (Cursor, Zed, etc.)",
            ),
            tr(
                "Add this server to its MCP settings.",
                "Ajoutez ce serveur dans ses réglages MCP.",
            ),
            Some(
                "{ \"mcpServers\": { \"unilux\": { \"command\": \"unilux\", \"args\": [\"mcp\"] } } }",
            ),
        ),
        step(
            3,
            tr("Keep Unilux open", "Gardez Unilux ouvert"),
            tr(
                "The agent then shares this window: it sees the running game, and its requests appear above for you to allow or refuse. With Unilux closed, an agent can confirm its own actions.",
                "L'agent partage alors cette fenêtre : il voit le jeu en cours, et ses demandes apparaissent plus haut pour que vous les autorisiez ou refusiez. Avec Unilux fermé, un agent peut confirmer lui-même ses actions.",
            ),
            None,
        ),
    ]);

    let mut cats: Vec<Element> = Vec::new();
    for c in 0..8 {
        let (name, what) = category(c);
        let mine: Vec<&ToolInfo> = tools
            .iter()
            .filter(|t| {
                TOOLS
                    .iter()
                    .find(|(n, ..)| *n == t.name)
                    .map_or(c == 7, |(_, k, ..)| *k == c)
            })
            .collect();
        if mine.is_empty() {
            continue;
        }
        let reads = mine.iter().filter(|t| t.read_only).count();
        let asks_n = mine.iter().filter(|t| t.needs_approval).count();
        let mut chips = rect().horizontal().spacing(space::S);
        if reads > 0 {
            chips = chips.child(chip(
                fill(tr("{} read only", "{} en lecture"), &[reads.to_string()]),
                Tone::Ok,
            ));
        }
        if asks_n > 0 {
            chips = chips.child(chip(
                fill(
                    tr("{} with approval", "{} avec accord"),
                    &[asks_n.to_string()],
                ),
                Tone::Warn,
            ));
        }
        let own = mine.len() - reads - asks_n;
        if own > 0 {
            chips = chips.child(chip(
                fill(tr("{} on its own", "{} en autonomie"), &[own.to_string()]),
                Tone::Accent,
            ));
        }
        cats.push(row(
            name,
            Some(what.to_string()),
            Some(chips.into_element()),
        ));
        if *open.read() {
            for t in mine {
                cats.push(
                    rect()
                        .key(t.name)
                        .width(Size::fill())
                        .horizontal()
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .padding((space::S, space::L, space::S, space::XXL))
                        .spacing(space::M)
                        .child(
                            rect()
                                .width(Size::flex(1.))
                                .child(body(label_of(t.name)))
                                .child(
                                    label()
                                        .text(t.name)
                                        .font_size(size::SMALL)
                                        .font_family("monospace")
                                        .color(T.muted),
                                ),
                        )
                        .child(access_chip(t))
                        .into_element(),
                );
            }
        }
    }
    let toggle = Button::new()
        .flat()
        .on_press(move |_| {
            let v = *open.peek();
            open.set(!v);
        })
        .child(if *open.read() {
            tr("Hide the details", "Masquer le détail").to_string()
        } else {
            fill(
                tr("Show the {} tools", "Voir les {} outils"),
                &[tools.len().to_string()],
            )
        })
        .into_element();

    let recent: Vec<Element> = log
        .iter()
        .rev()
        .take(15)
        .map(|c| {
            row(
                label_of(&c.tool),
                Some(
                    [Some(sessions::when(c.at)), c.what.clone()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                Some(
                    if c.ok {
                        chip(tr("Done", "Réussi"), Tone::Ok)
                    } else {
                        chip(tr("Failed or refused", "Échec ou refusé"), Tone::Warn)
                    }
                    .into_element(),
                ),
            )
        })
        .collect();
    let recent = if recent.is_empty() {
        vec![row(
            tr("Nothing yet", "Rien pour l'instant"),
            Some(
                tr(
                    "Every call an agent makes is listed here, with its result.",
                    "Chaque appel d'un agent est listé ici, avec son résultat.",
                )
                .to_string(),
            ),
            None,
        )]
    } else {
        divided(recent)
    };

    rect()
        .spacing(space::XL)
        .child(crate::widgets::page_header(tr("Agents", "Agents"), tr(
            "A coding agent like Claude Code can read Unilux's logs, run checks and test fixes; changes to your setup wait for your approval.",
            "Un agent de code comme Claude Code peut lire les journaux d'Unilux, lancer les vérifications et tester des corrections ; toute modification de votre installation attend votre accord.",
        )))
        .child(verdict)
        .maybe_child((!asks.is_empty()).then(|| group(tr("Waiting for you", "En attente de vous"), asks)))
        .child(group(tr("Connect an agent", "Connecter un agent"), connect))
        .child(
            rect()
                .width(Size::fill())
                .spacing(space::S)
                .child(group(
                    fill(
                        tr(
                            "What an agent can do: {} tools, {} read only, {} ask you first",
                            "Ce qu'un agent peut faire : {} outils, {} en lecture seule, {} demandent votre accord",
                        ),
                        &[tools.len().to_string(), n_read.to_string(), n_ask.to_string()],
                    )
                    .as_str(),
                    cats,
                ))
                .child(toggle),
        )
        .child(sessions::tiles(vec![
            sessions::tile(tr("Calls", "Appels"), total.to_string(), None),
            sessions::tile(
                tr("Last call", "Dernier appel"),
                last.map_or("-".to_string(), ago),
                None,
            ),
            sessions::tile(
                tr("Failed or refused", "Échecs ou refus"),
                failed.to_string(),
                (failed > 0).then_some(Tone::Warn),
            ),
        ]))
        .child(group(tr("Recent activity", "Activité récente"), recent))
        .child(group(
            tr("Technical details", "Détails techniques"),
            divided(vec![
                row(
                    tr("Socket", "Socket"),
                    Some(svc().paths().socket().display().to_string()),
                    None,
                ),
                row(
                    tr("Activity log", "Journal d'activité"),
                    Some(svc().paths().audit_log().display().to_string()),
                    None,
                ),
            ]),
        ))
}
