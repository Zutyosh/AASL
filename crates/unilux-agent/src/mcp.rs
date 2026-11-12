use std::path::PathBuf;
use std::sync::Arc;

use unilux_core::exec::BoxFuture;
use unilux_diag::FixId;
use unilux_service::{LaunchOpts, Service};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Asks the person at the GUI to approve a destructive action. Returns
/// true to proceed.
pub type ConfirmHook = Arc<dyn Fn(String) -> BoxFuture<'static, bool> + Send + Sync>;

#[derive(Clone)]
pub struct Server {
    svc: Service,
    confirm: Option<ConfirmHook>,
    audit: Option<PathBuf>,
}

struct ToolDef {
    name: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    read_only: bool,
    destructive: bool,
}

fn no_args() -> Value {
    json!({"type": "object", "properties": {}, "additionalProperties": false})
}

const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "status",
        description: "Install state, prefix, Proton version, running session, active OpenXR runtime.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "system",
        description: "Host probe: distribution, Python, GPUs, Vulkan drivers, OpenXR runtimes, Flatpak sandbox.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "list_processes",
        description: "Processes running inside the Somnium Wine prefix, with pid, parent and command line.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "tail_logs",
        description: "Recent log lines from Unilux, Proton and Somnium. Pass the last seq you saw as 'since' to poll for new lines.",
        schema: || json!({"type": "object", "properties": {"since": {"type": "integer", "minimum": 0}, "limit": {"type": "integer", "minimum": 1, "maximum": 5000, "default": 200}}}),
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "run_doctor",
        description: "Run every diagnostic. Each finding has severity, evidence, advice and an optional fix id for apply_fix.",
        schema: || json!({"type": "object", "properties": {"network": {"type": "boolean", "default": true}}}),
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "list_fixes",
        description: "Every fix Unilux can apply, with what it does and whether it can lose user state.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "apply_fix",
        description: "Apply a fix by id. dry_run defaults to true; pass dry_run=false to change anything. Destructive fixes need the user's approval.",
        schema: || json!({"type": "object", "properties": {"id": {"type": "string", "enum": ["repair", "stop_session", "clear_openxr_override", "clear_downloads"]}, "dry_run": {"type": "boolean", "default": true}, "confirm": {"type": "boolean", "description": "Required for destructive fixes when no GUI is attached."}}, "required": ["id"]}),
        read_only: false,
        destructive: true,
    },
    ToolDef {
        name: "launch",
        description: "Start Somnium Space. Waits for an OpenXR runtime unless flat=true.",
        schema: || json!({"type": "object", "properties": {"flat": {"type": "boolean", "default": false}}}),
        read_only: false,
        destructive: false,
    },
    ToolDef {
        name: "stop",
        description: "Stop Somnium Space and every process in its prefix. Needs the user's approval.",
        schema: || json!({"type": "object", "properties": {"confirm": {"type": "boolean"}}}),
        read_only: false,
        destructive: true,
    },
    ToolDef {
        name: "export_report",
        description: "Anonymised JSON bug report: system, findings, config, install state, log tail.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "vr_status",
        description: "VR readiness: active OpenXR runtime, installed runtimes (SteamVR, WiVRn, Monado, Envision), OpenVR layer (xrizer, OpenComposite), blockers in fix order.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "proton_installed",
        description: "Proton builds installed by Unilux, which one Somnium uses, and whether each one's patch bundle is intact.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "proton_available",
        description: "Upstream releases for a provider (ge, cachyos) with patch-bundle status per release.",
        schema: || json!({"type": "object", "properties": {"provider": {"type": "string", "enum": ["ge", "cachyos"]}, "refresh": {"type": "boolean"}}, "required": ["provider"]}),
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "proton_use",
        description: "Switch Somnium to another Proton build (provider:tag); downloads and patches it. Needs the user's approval.",
        schema: || json!({"type": "object", "properties": {"id": {"type": "string"}, "force": {"type": "boolean"}, "confirm": {"type": "boolean"}}, "required": ["id"]}),
        read_only: false,
        destructive: true,
    },
    ToolDef {
        name: "patch_status",
        description: "Patch bundle in the current Proton and any files changed since it was applied.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "recipe_list",
        description: "Compatibility recipes: built-in and the user's, which one is in use, their Proton and ingredients, whether they carry custom code or compile anything.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "recipe_get",
        description: "One recipe as TOML (the format recipe_save takes). export=true inlines every patch, for sharing.",
        schema: || json!({"type": "object", "properties": {"id": {"type": "string"}, "export": {"type": "boolean"}}, "required": ["id"]}),
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "ingredient_list",
        description: "The ingredient library recipes pick from: id, kind (patch or env), base Wine tree, patches, DLLs, superseded_if conditions, env.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "recipe_check",
        description: "Validate a recipe TOML without saving it: errors, ingredients, custom code, whether it compiles anything.",
        schema: || json!({"type": "object", "properties": {"toml": {"type": "string"}}, "required": ["toml"]}),
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "recipe_save",
        description: "Save a user recipe from TOML (create or replace). Recipes are declarative: format, id, title, description, proton (provider:tag, provider:latest or omitted), ingredients (ids in order), env, and [[ingredient]] tables with inline patches for new fixes. Needs the user's approval.",
        schema: || json!({"type": "object", "properties": {"toml": {"type": "string"}, "confirm": {"type": "boolean"}}, "required": ["toml"]}),
        read_only: false,
        destructive: true,
    },
    ToolDef {
        name: "recipe_build",
        description: "Build a recipe's bundle (default: the recipe in use, on its Proton). Cached by recipe, Proton and builder. Long; follow with tail_logs. Needs the user's approval.",
        schema: || json!({"type": "object", "properties": {"id": {"type": "string"}, "proton": {"type": "string"}, "force": {"type": "boolean"}, "confirm": {"type": "boolean"}}}),
        read_only: false,
        destructive: true,
    },
    ToolDef {
        name: "recipe_use",
        description: "Make Somnium use a recipe: switch to its Proton if it pins one, build when nothing fits, then repair. Needs the user's approval.",
        schema: || json!({"type": "object", "properties": {"id": {"type": "string"}, "confirm": {"type": "boolean"}}, "required": ["id"]}),
        read_only: false,
        destructive: true,
    },
    ToolDef {
        name: "session_list",
        description: "Saved session reports, newest first: recipe and its key, hardware, duration, worlds entered, video opens/failures, crash (in_game or on_exit), MangoHud frame pacing (avg fps, 1 % low, p99 frametime, stutters).",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "recipe_verdict",
        description: "How a recipe stands on this machine's session reports, with the same rules CI applies to community submissions (status, hours, configurations, crashes, video success, pacing, what is missing).",
        schema: || json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "recipe_share",
        description: "Prepare a community submission: the GitHub form URL (prefilled when it fits) plus the recipe and reports to paste. Nothing is sent; the user opens the link.",
        schema: || json!({"type": "object", "properties": {"id": {"type": "string"}, "reports": {"type": "array", "items": {"type": "string"}}}, "required": ["id"]}),
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "community_list",
        description: "Community recipes from Unilux's repository with their verdicts.",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "community_install",
        description: "Save a community recipe as one of the user's (does not switch to it). Needs the user's approval.",
        schema: || json!({"type": "object", "properties": {"id": {"type": "string"}, "confirm": {"type": "boolean"}}, "required": ["id"]}),
        read_only: false,
        destructive: true,
    },
    ToolDef {
        name: "patch_list",
        description: "Every patch of the ingredient library (ingredient, file, subject, sha256).",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "patch_read",
        description: "Full text of one patch file, by file name from patch_list.",
        schema: || json!({"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}),
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "patch_plan",
        description: "What building the recipe in use for a Proton would do: Wine repo and commit, Wine trees, bundle name, engine. Nothing runs.",
        schema: || json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "patch_build",
        description: "Rebuild the recipe in use for a Proton in a container. Takes a long time; follow it with tail_logs. Needs the user's approval.",
        schema: || json!({"type": "object", "properties": {"id": {"type": "string"}, "confirm": {"type": "boolean"}}, "required": ["id"]}),
        read_only: false,
        destructive: true,
    },
    ToolDef {
        name: "patch_apply",
        description: "Install a bundle file (e.g. from patch_build) into the current Proton. Needs the user's approval.",
        schema: || json!({"type": "object", "properties": {"path": {"type": "string"}, "confirm": {"type": "boolean"}}, "required": ["path"]}),
        read_only: false,
        destructive: true,
    },
    ToolDef {
        name: "update_check",
        description: "Whether a newer Unilux release exists on GitHub, the current version, and how this copy was installed (AppImage, portable, package).",
        schema: no_args,
        read_only: true,
        destructive: false,
    },
    ToolDef {
        name: "update_apply",
        description: "Download, verify and install the newer Unilux over this copy. Takes effect after a restart. Needs the user's approval.",
        schema: || json!({"type": "object", "properties": {"confirm": {"type": "boolean"}}}),
        read_only: false,
        destructive: true,
    },
];

/// What an agent can call, for the GUI to list.
pub struct ToolInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub read_only: bool,
    /// Asks the person at the GUI first.
    pub needs_approval: bool,
}

pub fn tools() -> Vec<ToolInfo> {
    TOOLS
        .iter()
        .map(|t| ToolInfo {
            name: t.name,
            description: t.description,
            read_only: t.read_only,
            needs_approval: t.destructive,
        })
        .collect()
}

const RESOURCES: &[(&str, &str, &str)] = &[
    ("unilux://config", "config.toml", "Unilux configuration"),
    (
        "unilux://state",
        "install-state.json",
        "What the last install or repair did",
    ),
    ("unilux://logs/launch", "launch.log", "Full launch log"),
];

const NOT_APPROVED: &str = "not approved by the user. Ask them, then retry with confirm=true.";

fn proton_arg(args: &Value) -> Result<unilux_components::ProtonId, String> {
    unilux_components::ProtonId::parse(args.get("id").and_then(Value::as_str).unwrap_or_default())
        .map_err(|e| e.to_string())
}

fn str_arg(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("missing {key}"))
}

fn rpc_ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_err(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

fn tool_text(v: &Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(v).unwrap_or_default();
    json!({"content": [{"type": "text", "text": text}], "structuredContent": v, "isError": is_error})
}

impl Server {
    pub fn new(svc: Service) -> Self {
        let audit = Some(svc.paths().audit_log());
        Self {
            svc,
            confirm: None,
            audit,
        }
    }

    /// Route destructive actions through the GUI for approval.
    pub fn with_confirm(mut self, hook: ConfirmHook) -> Self {
        self.confirm = Some(hook);
        self
    }

    pub fn without_audit(mut self) -> Self {
        self.audit = None;
        self
    }

    /// Handle one JSON-RPC message. None for notifications.
    pub async fn handle(&self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let id = id?;
        Some(match method.as_str() {
            "initialize" => rpc_ok(
                &id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {}, "resources": {}},
                    "serverInfo": {"name": "unilux", "title": "Unilux (Somnium Space on Linux)", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "Start with run_doctor, status and vr_status. Read tail_logs for evidence. Compatibility is described by recipes (recipe_list, recipe_get, ingredient_list): a Proton plus ingredients. To change it, write a recipe (recipe_check, then recipe_save), recipe_build, then recipe_use. Each play session leaves a report (session_list); recipe_verdict judges a recipe on them, and recipe_share prepares a community submission once it holds up. patch_status, patch_list/patch_read and patch_plan show the details. Use apply_fix with dry_run first; actions that change the setup need the user's approval."
                }),
            ),
            "ping" => rpc_ok(&id, json!({})),
            "tools/list" => rpc_ok(
                &id,
                json!({"tools": TOOLS.iter().map(|t| json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": (t.schema)(),
                    "annotations": {"readOnlyHint": t.read_only, "destructiveHint": t.destructive}
                })).collect::<Vec<_>>()}),
            ),
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !TOOLS.iter().any(|t| t.name == name) {
                    return Some(rpc_err(&id, -32602, format!("unknown tool: {name}")));
                }
                let r = self.call(&name, &args).await;
                self.audit(&name, &args, r.is_ok()).await;
                match r {
                    Ok(v) => rpc_ok(&id, tool_text(&v, false)),
                    Err(e) => rpc_ok(&id, tool_text(&json!({"error": e}), true)),
                }
            }
            "resources/list" => rpc_ok(
                &id,
                json!({"resources": RESOURCES.iter().map(|(uri, name, d)| json!({"uri": uri, "name": name, "description": d})).collect::<Vec<_>>()}),
            ),
            "resources/read" => {
                let uri = params
                    .get("uri")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let p = self.svc.paths();
                let (path, mime) = match uri {
                    "unilux://config" => (p.config_file(), "application/toml"),
                    "unilux://state" => (p.state_file(), "application/json"),
                    "unilux://logs/launch" => (p.launch_log(), "text/plain"),
                    _ => return Some(rpc_err(&id, -32002, format!("resource not found: {uri}"))),
                };
                let text = tokio::fs::read_to_string(&path).await.unwrap_or_default();
                rpc_ok(
                    &id,
                    json!({"contents": [{"uri": uri, "mimeType": mime, "text": text}]}),
                )
            }
            _ => rpc_err(&id, -32601, format!("method not found: {method}")),
        })
    }

    async fn approved(&self, what: String, args: &Value) -> bool {
        match &self.confirm {
            Some(hook) => hook(what).await,
            None => args
                .get("confirm")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    async fn call(&self, name: &str, args: &Value) -> Result<Value, String> {
        let s = &self.svc;
        let to = |v: Result<Value, serde_json::Error>| v.map_err(|e| e.to_string());
        match name {
            "status" => to(serde_json::to_value(s.status())),
            "system" => to(serde_json::to_value(s.system())),
            "list_processes" => to(serde_json::to_value(s.processes())),
            "tail_logs" => {
                let since = args.get("since").and_then(Value::as_u64);
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(200).clamp(1, 5000) as usize;
                to(serde_json::to_value(s.tail_logs(since, limit)))
            }
            "run_doctor" => {
                let net = args.get("network").and_then(Value::as_bool).unwrap_or(true);
                to(serde_json::to_value(s.doctor(net).await))
            }
            "list_fixes" => Ok(Value::Array(
                FixId::ALL.iter().map(|f| json!({"id": f, "description": f.describe(), "destructive": f.destructive()})).collect(),
            )),
            "apply_fix" => {
                let id = args.get("id").and_then(Value::as_str).unwrap_or_default();
                let fix = FixId::parse(id).ok_or_else(|| format!("unknown fix '{id}'"))?;
                let dry = args.get("dry_run").and_then(Value::as_bool).unwrap_or(true);
                if !dry && fix.destructive() && !self.approved(format!("An agent wants to apply: {}", fix.describe()), args).await {
                    return Err(NOT_APPROVED.into());
                }
                s.apply_fix(fix, dry).await.map_err(|e| e.to_string()).and_then(|o| to(serde_json::to_value(o)))
            }
            "launch" => {
                let flat = args.get("flat").and_then(Value::as_bool).unwrap_or(false);
                s.launch(LaunchOpts { allow_flat: flat }).await.map(|pid| json!({"pid": pid})).map_err(|e| e.to_string())
            }
            "stop" => {
                if !self.approved("An agent wants to stop Somnium Space.".into(), args).await {
                    return Err(NOT_APPROVED.into());
                }
                s.stop().await.map(|n| json!({"stopped": n})).map_err(|e| e.to_string())
            }
            "export_report" => serde_json::from_str(&s.report().await).map_err(|e| e.to_string()),
            "update_check" => s.update_check().await.map_err(|e| e.to_string()).and_then(|i| to(serde_json::to_value(i))),
            "update_apply" => {
                if !self.approved("An agent wants to update Unilux itself.".into(), args).await {
                    return Err(NOT_APPROVED.into());
                }
                s.update_apply().await.map_err(|e| e.to_string()).and_then(|a| to(serde_json::to_value(a)))
            }
            "vr_status" => to(serde_json::to_value(s.vr_status())),
            "proton_installed" => to(serde_json::to_value(s.proton_installed())),
            "proton_available" => {
                let provider = args.get("provider").and_then(Value::as_str).unwrap_or("ge");
                let refresh = args.get("refresh").and_then(Value::as_bool).unwrap_or(false);
                s.proton_releases(provider, refresh).await.map_err(|e| e.to_string()).and_then(|v| to(serde_json::to_value(v)))
            }
            "proton_use" => {
                let id = proton_arg(args)?;
                let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
                if !self.approved(format!("An agent wants Somnium to use {id}."), args).await {
                    return Err(NOT_APPROVED.into());
                }
                s.proton_use(id.clone(), force).await.map(|_| json!({"proton": id.to_string()})).map_err(|e| e.to_string())
            }
            "patch_status" => to(serde_json::to_value(s.status().patches)),
            "recipe_list" => s.recipes().map_err(|e| e.to_string()).and_then(|v| to(serde_json::to_value(v))),
            "recipe_get" => {
                let id = str_arg(args, "id")?;
                let r = if args.get("export").and_then(Value::as_bool).unwrap_or(false) {
                    s.recipe_export(&id)
                } else {
                    s.recipe(&id).and_then(|r| r.to_toml())
                };
                r.map(|t| json!({"id": id, "toml": t})).map_err(|e| e.to_string())
            }
            "ingredient_list" => s.library().map_err(|e| e.to_string()).and_then(|l| to(serde_json::to_value(l.ingredients))),
            "recipe_check" => s.recipe_check(&str_arg(args, "toml")?).map_err(|e| e.to_string()).and_then(|i| to(serde_json::to_value(i))),
            "recipe_save" => {
                let text = str_arg(args, "toml")?;
                let info = s.recipe_check(&text).map_err(|e| e.to_string())?;
                let what = if info.custom_code { " It brings its own Wine patches." } else { "" };
                if !self.approved(format!("An agent wants to save the recipe {}.{what}", info.id), args).await {
                    return Err(NOT_APPROVED.into());
                }
                s.recipe_import(&text).map_err(|e| e.to_string()).and_then(|i| to(serde_json::to_value(i)))
            }
            "recipe_build" => {
                let id = args.get("id").and_then(Value::as_str).map(str::to_string);
                let proton = match args.get("proton").and_then(Value::as_str) {
                    Some(p) => Some(unilux_components::ProtonId::parse(p).map_err(|e| e.to_string())?),
                    None => None,
                };
                let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);
                let which = id.clone().unwrap_or_else(|| "in use".into());
                if !self.approved(format!("An agent wants to build the recipe {which} (long, uses the CPU)."), args).await {
                    return Err(NOT_APPROVED.into());
                }
                s.recipe_build(id.as_deref(), proton, force).await.map(|p| json!({"bundle": p})).map_err(|e| e.to_string())
            }
            "recipe_use" => {
                let id = str_arg(args, "id")?;
                if !self.approved(format!("An agent wants Somnium to use the recipe {id}."), args).await {
                    return Err(NOT_APPROVED.into());
                }
                s.recipe_use(&id).await.map(|_| json!({"recipe": id})).map_err(|e| e.to_string())
            }
            "session_list" => to(serde_json::to_value(s.sessions())),
            "recipe_verdict" => s.recipe_verdict(&str_arg(args, "id")?).map_err(|e| e.to_string()).and_then(|v| to(serde_json::to_value(v))),
            "recipe_share" => {
                let reports: Vec<String> = args
                    .get("reports")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                s.recipe_share(&str_arg(args, "id")?, &reports).map_err(|e| e.to_string()).and_then(|v| to(serde_json::to_value(v)))
            }
            "community_list" => s.community().await.map_err(|e| e.to_string()).and_then(|v| to(serde_json::to_value(v))),
            "community_install" => {
                let id = str_arg(args, "id")?;
                if !self.approved(format!("An agent wants to save the community recipe {id}."), args).await {
                    return Err(NOT_APPROVED.into());
                }
                s.community_install(&id).await.map_err(|e| e.to_string()).and_then(|v| to(serde_json::to_value(v)))
            }
            "patch_list" => s.patch_list().map_err(|e| e.to_string()).and_then(|v| to(serde_json::to_value(v))),
            "patch_read" => {
                let name = str_arg(args, "name")?;
                s.patch_read(&name).map(|text| json!({"name": name, "text": text})).map_err(|e| e.to_string())
            }
            "patch_plan" => {
                let id = proton_arg(args)?;
                s.patch_plan(&id).await.map_err(|e| e.to_string()).and_then(|p| to(serde_json::to_value(p)))
            }
            "patch_build" => {
                let id = proton_arg(args)?;
                if !self.approved(format!("An agent wants to build a patch bundle for {id} (long, uses the CPU)."), args).await {
                    return Err(NOT_APPROVED.into());
                }
                s.patch_build(id).await.map(|p| json!({"bundle": p})).map_err(|e| e.to_string())
            }
            "patch_apply" => {
                let path = args.get("path").and_then(Value::as_str).unwrap_or_default().to_string();
                if !self.approved(format!("An agent wants to install the patch bundle {path}."), args).await {
                    return Err(NOT_APPROVED.into());
                }
                s.patch_apply(path.into()).await.map(|_| json!({"applied": true})).map_err(|e| e.to_string())
            }
            _ => Err(format!("unknown tool {name}")),
        }
    }

    async fn audit(&self, tool: &str, args: &Value, ok: bool) {
        let Some(path) = &self.audit else { return };
        if let Some(d) = path.parent() {
            let _ = tokio::fs::create_dir_all(d).await;
        }
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let line = format!(
            "{}\n",
            json!({"at": at, "tool": tool, "args": args, "ok": ok})
        );
        if let Ok(mut f) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
        {
            let _ = f.write_all(line.as_bytes()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(home: &std::path::Path) -> Server {
        let h = home.display().to_string();
        let paths = unilux_core::Paths::resolve(|k| (k == "HOME").then(|| h.clone())).unwrap();
        let svc = Service::builder(paths)
            .host(Arc::new(unilux_system::FakeHost::new()))
            .log_file(false)
            .build()
            .unwrap();
        Server::new(svc)
    }

    async fn call(s: &Server, method: &str, params: Value) -> Value {
        s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn handshake_and_listing() {
        let d = tempfile::tempdir().unwrap();
        let s = server(d.path());
        let r = call(&s, "initialize", json!({"protocolVersion": PROTOCOL_VERSION, "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}})).await;
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert!(
            s.handle(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
                .await
                .is_none()
        );
        let r = call(&s, "tools/list", json!({})).await;
        assert_eq!(r["result"]["tools"].as_array().unwrap().len(), TOOLS.len());
        assert_eq!(call(&s, "nope", json!({})).await["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn tools_and_audit() {
        let d = tempfile::tempdir().unwrap();
        let s = server(d.path());
        let r = call(&s, "tools/call", json!({"name": "status", "arguments": {}})).await;
        assert_eq!(r["result"]["isError"], false);
        assert_eq!(r["result"]["structuredContent"]["installed"], false);
        let r = call(
            &s,
            "tools/call",
            json!({"name": "run_doctor", "arguments": {"network": false}}),
        )
        .await;
        assert!(
            r["result"]["structuredContent"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["check"] == "install")
        );
        let audit = std::fs::read_to_string(s.svc.paths().audit_log()).unwrap();
        assert_eq!(audit.lines().count(), 2);
    }

    #[tokio::test]
    async fn destructive_needs_approval() {
        let d = tempfile::tempdir().unwrap();
        let s = server(d.path()).without_audit();
        let r = call(&s, "tools/call", json!({"name": "stop", "arguments": {}})).await;
        assert_eq!(r["result"]["isError"], true);
        let r = call(
            &s,
            "tools/call",
            json!({"name": "apply_fix", "arguments": {"id": "stop_session"}}),
        )
        .await;
        assert_eq!(
            r["result"]["structuredContent"]["applied"], false,
            "dry run by default"
        );
        let s = s.with_confirm(Arc::new(|_| Box::pin(async { false })));
        let r = call(
            &s,
            "tools/call",
            json!({"name": "stop", "arguments": {"confirm": true}}),
        )
        .await;
        assert_eq!(
            r["result"]["isError"], true,
            "GUI refusal wins over confirm=true"
        );
        let s = s.with_confirm(Arc::new(|_| Box::pin(async { true })));
        let r = call(&s, "tools/call", json!({"name": "stop", "arguments": {}})).await;
        assert_eq!(r["result"]["structuredContent"]["stopped"], 0);
    }

    #[tokio::test]
    async fn self_update_needs_approval() {
        let d = tempfile::tempdir().unwrap();
        let s = server(d.path()).without_audit();
        let list = call(&s, "tools/list", json!({})).await;
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"update_check") && names.contains(&"update_apply"));
        let r = call(
            &s,
            "tools/call",
            json!({"name": "update_apply", "arguments": {}}),
        )
        .await;
        assert_eq!(r["result"]["isError"], true);
        assert!(
            r["result"]["structuredContent"]["error"]
                .as_str()
                .unwrap()
                .contains("not approved")
        );
    }

    #[tokio::test]
    async fn resources() {
        let d = tempfile::tempdir().unwrap();
        let s = server(d.path());
        let r = call(&s, "resources/list", json!({})).await;
        assert_eq!(r["result"]["resources"].as_array().unwrap().len(), 3);
        let r = call(&s, "resources/read", json!({"uri": "unilux://nope"})).await;
        assert!(r["error"].is_object());
    }
}
