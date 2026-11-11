//! VR: whether the chain from headset to game is complete, and what holds frames back.

use std::time::Duration;

use aasl_system::{OpenVrLayer, RuntimeKind, VrStatus, VrTuning};
use freya::prelude::*;

use crate::i18n::{fill, tr};
use crate::state::{Section, Ui};
use crate::svc;
use crate::theme::{T, alpha, size, space};
use crate::widgets::{Tone, banner, body, chip, divided, group, mono_box, muted, title};

#[derive(PartialEq)]
pub struct VrSection(pub Ui);

impl Component for VrSection {
    fn render(&self) -> impl IntoElement {
        view(self.0)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum St {
    Ok,
    Todo,
    Wait,
}

struct Step {
    name: String,
    what: String,
    value: Option<String>,
    note: Option<&'static str>,
    st: St,
    chip: &'static str,
    todo: Option<String>,
    cmd: Option<String>,
}

impl Step {
    fn new(name: impl Into<String>, what: impl Into<String>, st: St, chip: &'static str) -> Self {
        Self {
            name: name.into(),
            what: what.into(),
            value: None,
            note: None,
            st,
            chip,
            todo: None,
            cmd: None,
        }
    }
    fn value(mut self, v: impl Into<String>) -> Self {
        self.value = Some(v.into());
        self
    }
    fn todo(mut self, v: impl Into<String>) -> Self {
        self.todo = Some(v.into());
        self
    }
    fn cmd(mut self, v: impl Into<String>) -> Self {
        self.cmd = Some(v.into());
        self
    }
    fn note(mut self, v: &'static str) -> Self {
        self.note = Some(v);
        self
    }
}

fn layer_name(l: OpenVrLayer) -> &'static str {
    match l {
        OpenVrLayer::Xrizer => "xrizer",
        OpenVrLayer::OpenComposite => "OpenComposite",
        OpenVrLayer::VapoR => "VapoR",
        OpenVrLayer::SteamVr => "SteamVR",
        OpenVrLayer::Other => "?",
    }
}

/// The chain, in the order a frame travels the other way: runtime, service, bridge.
fn chain(v: &VrStatus) -> Vec<Step> {
    let kind = v.active.as_ref().map(|r| r.kind);
    let name = v
        .active
        .as_ref()
        .map(|r| r.name.clone().unwrap_or_else(|| r.kind.to_string()));
    let installed = v
        .installed
        .iter()
        .map(|k| k.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let broken = v.active.as_ref().is_some_and(|r| !r.library.exists());

    let what1 = tr(
        "The program that talks to the headset. WiVRn suits standalone headsets (Quest, Pico). Monado, through Envision, suits wired ones.",
        "Le programme qui parle au casque. WiVRn convient aux casques autonomes (Quest, Pico). Monado, via Envision, aux casques filaires.",
    );
    let mut s1 = match (&name, v.installed.is_empty()) {
        (Some(n), _) if broken => Step::new(tr("VR runtime", "Runtime VR"), what1, St::Todo, tr("Broken", "Cassé"))
            .value(n.clone())
            .todo(tr(
                "It points to a file that no longer exists. Restart or reinstall it.",
                "Il pointe vers un fichier qui n'existe plus. Relancez-le ou réinstallez-le.",
            )),
        (Some(n), _) => Step::new(tr("VR runtime", "Runtime VR"), what1, St::Ok, tr("Active", "Actif")).value(n.clone()),
        (None, true) => Step::new(tr("VR runtime", "Runtime VR"), what1, St::Todo, tr("Missing", "Absent")).todo(tr(
            "Install WiVRn for a standalone headset, or Monado through Envision for a wired one.",
            "Installez WiVRn pour un casque autonome, ou Monado via Envision pour un casque filaire.",
        )),
        (None, false) => Step::new(tr("VR runtime", "Runtime VR"), what1, St::Todo, tr("Inactive", "Inactif"))
            .value(installed.clone())
            .todo(fill(
                tr(
                    "Start {}, then connect the headset. A runtime only becomes active once the headset connects.",
                    "Démarrez {}, puis connectez le casque. Un runtime n'est actif qu'une fois le casque connecté.",
                ),
                std::slice::from_ref(&installed),
            )),
    };
    if kind == Some(RuntimeKind::SteamVr) {
        s1 = s1.note(tr(
            "Valve only ships SteamVR as a development release on Linux. WiVRn or Monado are the usual choice.",
            "Valve ne publie SteamVR sous Linux qu'en version de développement. WiVRn ou Monado sont le choix habituel.",
        ));
    }

    let what2 = tr(
        "It has to be running while you play: it sends the images to the headset.",
        "Il doit tourner pendant que vous jouez : c'est lui qui envoie l'image au casque.",
    );
    let s2 = match (&name, v.service_running) {
        (None, _) => Step::new(
            tr("Runtime service", "Service du runtime"),
            what2,
            St::Wait,
            tr("Waiting", "En attente"),
        ),
        (Some(_), true) => Step::new(
            tr("Runtime service", "Service du runtime"),
            what2,
            St::Ok,
            tr("Running", "En cours"),
        ),
        (Some(n), false) => Step::new(
            tr("Runtime service", "Service du runtime"),
            what2,
            St::Todo,
            tr("Stopped", "Arrêté"),
        )
        .todo(fill(
            tr(
                "Start {}, then connect the headset.",
                "Démarrez {}, puis connectez le casque.",
            ),
            std::slice::from_ref(n),
        )),
    };

    let what3 = tr(
        "Somnium speaks OpenVR. This layer translates it for your OpenXR runtime. Without it the game starts without VR.",
        "Somnium parle OpenVR. Cette couche le traduit pour votre runtime OpenXR. Sans elle, le jeu démarre sans VR.",
    );
    let needs = matches!(
        kind,
        Some(RuntimeKind::WiVRn | RuntimeKind::Monado | RuntimeKind::Envision)
    );
    let n3 = tr("OpenVR bridge", "Passerelle OpenVR");
    let s3 = if kind.is_none() {
        Step::new(n3, what3, St::Wait, tr("Waiting", "En attente"))
    } else if !needs {
        Step::new(n3, what3, St::Wait, tr("Not needed", "Inutile")).value(tr(
            "Your runtime speaks OpenVR itself.",
            "Votre runtime parle OpenVR lui-même.",
        ))
    } else {
        match &v.openvr {
            None => Step::new(n3, what3, St::Todo, tr("Missing", "Absente")).todo(tr(
                "Install xrizer (or OpenComposite) for your runtime.",
                "Installez xrizer (ou OpenComposite) pour votre runtime.",
            )),
            Some(o) if !o.exists => Step::new(n3, what3, St::Todo, tr("Broken", "Cassée"))
                .value(layer_name(o.layer))
                .todo(fill(
                    tr(
                        "The registered file {} does not exist. Reinstall it.",
                        "Le fichier déclaré {} n'existe pas. Réinstallez-la.",
                    ),
                    &[o.path.display().to_string()],
                )),
            Some(o) if o.layer == OpenVrLayer::SteamVr => Step::new(n3, what3, St::Todo, tr("Wrong target", "Mal réglée"))
                .value("SteamVR")
                .todo(tr(
                    "OpenVR still points at SteamVR. Point it at xrizer; the WiVRn and Envision settings offer it.",
                    "OpenVR pointe encore vers SteamVR. Pointez-le vers xrizer ; les réglages de WiVRn et d'Envision le proposent.",
                )),
            Some(o) => Step::new(n3, what3, St::Ok, tr("In place", "En place")).value(layer_name(o.layer)),
        }
    };
    vec![s1, s2, s3]
}

/// Settings outside AASL that decide whether frames arrive on time.
fn tuning(t: &VrTuning) -> Vec<Step> {
    let mut out = Vec::new();
    if let Some(c) = &t.compositor {
        let name = tr("Compositor priority", "Priorité du compositeur");
        let what = fill(
            tr(
                "When the CPU is busy, {} has to keep its deadline. If it cannot, the headset rate halves for a while (90 to 45 Hz).",
                "Quand le processeur est chargé, {} doit tenir son échéance. Sinon la fréquence du casque est divisée par deux un moment (90 à 45 Hz).",
            ),
            std::slice::from_ref(&c.name),
        );
        out.push(match c.realtime {
            Some(true) => Step::new(name, what, St::Ok, tr("OK", "OK")),
            Some(false) => Step::new(name, what, St::Todo, tr("To fix", "À régler"))
                .todo(tr(
                    "Allow it once, then restart it. A package update removes the permission.",
                    "Autorisez-le une fois, puis relancez-le. Une mise à jour du paquet retire cette permission.",
                ))
                .cmd(format!("sudo setcap CAP_SYS_NICE=+ep {}", c.path.display())),
            None => Step::new(name, what, St::Wait, tr("Unknown", "Inconnu")).todo(tr(
                "getcap is not installed, so AASL cannot tell.",
                "getcap n'est pas installé, AASL ne peut pas le savoir.",
            )),
        });
    }
    for p in &t.amd_power {
        let Some(idx) = p.vr_index else { continue };
        let name = tr("AMD GPU power profile", "Profil d'alimentation du GPU AMD");
        let what = tr(
            "VR load comes in short bursts. The default profile lets the clocks fall between them, so some frames arrive late. SteamVR switches profile itself; WiVRn and Monado do not.",
            "La charge VR arrive par rafales. Le profil par défaut laisse les fréquences retomber entre deux, donc certaines images arrivent en retard. SteamVR change de profil tout seul, pas WiVRn ni Monado.",
        );
        out.push(if p.active == "VR" {
            Step::new(name, what, St::Ok, tr("VR profile", "Profil VR"))
        } else {
            Step::new(name, what, St::Todo, tr("To fix", "À régler"))
                .value(fill(
                    tr("Active profile: {}", "Profil actif : {}"),
                    std::slice::from_ref(&p.active),
                ))
                .todo(tr(
                    "Lasts until reboot. To keep it, set it in LACT or CoreCtrl.",
                    "Dure jusqu'au redémarrage. Pour le garder, réglez-le dans LACT ou CoreCtrl.",
                ))
                .cmd(format!(
                    "echo {idx} | sudo tee /sys/class/drm/{}/device/pp_power_profile_mode",
                    p.card
                ))
        });
    }
    if !t.overlays.is_empty() {
        out.push(
            Step::new(
                tr("XR overlay", "Surcouche XR"),
                tr(
                    "An overlay is a second client of the compositor and takes focus from the game when it shows. If slowdowns line up with it, close it to compare.",
                    "Une surcouche est un second client du compositeur et lui prend le focus quand elle s'affiche. Si les ralentissements coïncident, fermez-la pour comparer.",
                ),
                St::Wait,
                tr("Running", "Active"),
            )
            .value(t.overlays.join(", ")),
        );
    }
    out
}

fn tone(s: St) -> Tone {
    match s {
        St::Ok => Tone::Ok,
        St::Todo => Tone::Warn,
        St::Wait => Tone::Neutral,
    }
}

fn step_row(num: Option<usize>, s: &Step) -> Element {
    let tone = tone(s.st);
    let badge = num.map(|n| {
        rect()
            .width(Size::px(26.))
            .height(Size::px(26.))
            .corner_radius(13.)
            .background(alpha(tone.color(), 0.16))
            .main_align(Alignment::Center)
            .cross_align(Alignment::Center)
            .child(
                label()
                    .text(n.to_string())
                    .font_size(size::SMALL)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(tone.color()),
            )
    });
    rect()
        .width(Size::fill())
        .horizontal()
        .content(Content::Flex)
        .padding((space::M, space::L))
        .spacing(space::M)
        .maybe_child(badge)
        .child(
            rect()
                .width(Size::flex(1.))
                .spacing(space::XS)
                .child(body(s.name.clone()))
                .child(muted(s.what.clone()))
                .maybe_child(s.value.clone().map(|v| {
                    label()
                        .text(v)
                        .font_size(size::SMALL)
                        .font_weight(FontWeight::MEDIUM)
                        .color(T.text)
                }))
                .maybe_child(s.note.map(muted))
                .maybe_child(
                    s.todo
                        .clone()
                        .map(|t| label().text(t).font_size(size::SMALL).color(T.warn)),
                )
                .maybe_child(s.cmd.clone().map(mono_box)),
        )
        .child(chip(s.chip, tone))
        .into_element()
}

fn view(mut ui: Ui) -> impl IntoElement {
    let mut tick = use_state(|| 0u32);
    let mut tun = use_state(|| svc().system().vr_tuning);
    let mut wait = use_state(|| svc().config().wait_for_headset);
    let wait_setter = WritableSetter::new(wait, move |v: bool| {
        let mut cfg = svc().config();
        cfg.wait_for_headset = v;
        match svc().set_config(cfg) {
            Ok(()) => wait.set(v),
            Err(e) => ui.error.set(Some(e.to_string())),
        }
    });
    // The runtime comes and goes while the user starts WiVRn: follow it.
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let n = *tick.peek();
            tick.set(n.wrapping_add(1));
        }
    });
    let _ = *tick.read();

    let v = svc().vr_status();
    let steps = chain(&v);
    let todo = steps.iter().filter(|s| s.st == St::Todo).count();
    let ready = v.ready && todo == 0;
    let name = v
        .active
        .as_ref()
        .map(|r| r.name.clone().unwrap_or_else(|| r.kind.to_string()))
        .unwrap_or_default();

    let recheck = Button::new()
        .flat()
        .on_press(move |_| {
            tun.set(svc().system().vr_tuning);
            let n = *tick.peek();
            tick.set(n.wrapping_add(1));
        })
        .child(tr("Check again", "Revérifier"))
        .into_element();
    let verdict = if ready {
        banner(
            Tone::Ok,
            tr("Ready for VR", "Prêt pour la VR"),
            fill(
                tr(
                    "{} is running and the game will find the headset through it.",
                    "{} tourne et le jeu y trouvera le casque.",
                ),
                &[name],
            ),
            Some(recheck),
        )
    } else {
        let head = if todo == 1 {
            tr(
                "One step to fix before VR",
                "Une étape à régler avant la VR",
            )
            .to_string()
        } else {
            fill(
                tr(
                    "{} steps to fix before VR",
                    "{} étapes à régler avant la VR",
                ),
                &[todo.max(1).to_string()],
            )
        };
        banner(
            Tone::Warn,
            head,
            tr(
                "Until then the game starts on the desktop, or waits for the headset if the option below is on.",
                "D'ici là, le jeu démarre sur le bureau, ou attend le casque si l'option plus bas est activée.",
            ),
            Some(recheck),
        )
    };

    let chain_rows = divided(
        steps
            .iter()
            .enumerate()
            .map(|(i, s)| step_row(Some(i + 1), s))
            .collect(),
    );

    let t = tun.read().clone();
    let mut tune_steps = tuning(&t);
    if tune_steps.is_empty() {
        tune_steps.push(Step::new(
            tr("Nothing to report", "Rien à signaler"),
            tr(
                "No compositor, AMD power profile or overlay that could cost frames was found.",
                "Aucun compositeur, profil d'alimentation AMD ou surcouche susceptible de coûter des images n'a été trouvé.",
            ),
            St::Ok,
            tr("OK", "OK"),
        ));
    }
    let tune_rows = divided(tune_steps.iter().map(|s| step_row(None, s)).collect());

    let mut details = Vec::new();
    if let Some(r) = &v.active {
        details.push(crate::widgets::row(
            tr("Runtime manifest", "Manifeste du runtime"),
            Some(r.manifest.display().to_string()),
            None,
        ));
        details.push(crate::widgets::row(
            tr("Runtime library", "Bibliothèque du runtime"),
            Some(r.library.display().to_string()),
            None,
        ));
    }
    if let Some(o) = &v.openvr {
        details.push(crate::widgets::row(
            tr("OpenVR registration", "Déclaration OpenVR"),
            Some(o.path.display().to_string()),
            None,
        ));
    }

    rect()
        .spacing(space::XL)
        .child(title(tr("VR headset", "Casque VR")))
        .child(muted(tr(
            "Somnium Space is played with a VR headset. This page checks that the path from headset to game is complete. It starts nothing.",
            "Somnium Space se joue avec un casque VR. Cette page vérifie que le chemin du casque au jeu est complet. Elle ne lance rien.",
        )))
        .child(verdict)
        .child(group(
            tr("From the headset to the game", "Du casque au jeu"),
            chain_rows,
        ))
        .child(group(
            tr("At launch", "Au lancement"),
            vec![crate::widgets::row(
                tr("Wait for the headset", "Attendre le casque"),
                Some(
                    tr(
                        "If the runtime is not ready at launch, hold the game until it is instead of starting on the desktop.",
                        "Si le runtime n'est pas prêt au lancement, le jeu patiente au lieu de démarrer sur le bureau.",
                    )
                    .into(),
                ),
                Some(Switch::new(wait_setter).into_element()),
            )],
        ))
        .child(group(
            tr("For smooth frames", "Pour une image fluide"),
            tune_rows,
        ))
        .child(group(
            tr("What happened in game", "Ce qui s'est passé en jeu"),
            vec![crate::widgets::row(
                tr("Frame rate of your sessions", "Fluidité de vos parties"),
                Some(
                    tr(
                        "Each game is measured with MangoHud: curve, slowdowns, freezes and their likely cause.",
                        "Chaque partie est mesurée avec MangoHud : courbe, ralentissements, gels et cause probable.",
                    )
                    .into(),
                ),
                Some(
                    Button::new()
                        .on_press(move |_| ui.section.set(Section::Recipes))
                        .child(tr("Open sessions", "Voir les sessions"))
                        .into_element(),
                ),
            )],
        ))
        .maybe_child((!details.is_empty()).then(|| {
            group(tr("Technical details", "Détails techniques"), divided(details))
        }))
}
