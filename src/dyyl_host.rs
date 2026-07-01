//! dyyl NDJSON host-protocol bridge.
//!
//! mcm acts as the **host**: it spawns `dyyl --host-json <script>` and
//! answers `mcm.*` commands emitted by the interpreter over stdin/stdout.
//!
//! Protocol (one JSON object per line, NDJSON):
//!
//! dyyl → mcm (request):
//! ```json
//! {"type":"mcm_command","id":"1","name":"mcm.game.choose",
//!  "args":["dev","1.20.1"],"source_line":"mcm.game.choose(\"dev\",\"1.20.1\")"}
//! ```
//!
//! mcm → dyyl (response):
//! ```json
//! {"type":"mcm_response","id":"1","ok":true}
//! ```
//! On error:
//! ```json
//! {"type":"mcm_response","id":"1","ok":false,
//!  "error":{"code":"unknown_command","message":"..."}}
//! ```

use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::mcm_package::{new_lock, LockStep, StepPermission};

/// A single mcm command request from dyyl.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct McmCommand {
    #[serde(rename = "type")]
    msg_type: String,
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) args: Vec<McmArg>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source_line: Option<String>,
}

/// The response mcm writes back to dyyl's stdin.
#[derive(Serialize)]
struct McmResponse {
    #[serde(rename = "type")]
    msg_type: &'static str,
    id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<McmArg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McmError>,
}

/// Error payload sent on `ok: false`.
#[derive(Serialize)]
struct McmError {
    code: String,
    message: String,
}

/// A scalar argument — mirrors dyyl's `McmArg` enum (untagged).
///
/// The unit `Null` variant acts as the serde fallback for JSON `null`:
/// when deserialization fails to match Num/Str/Bool, the untagged enum
/// falls through to the unit variant.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum McmArg {
    Num(i64),
    Str(String),
    Bool(bool),
    Null,
}

impl McmArg {
    fn to_json(&self) -> serde_json::Value {
        match self {
            McmArg::Num(n) => serde_json::Value::from(*n),
            McmArg::Str(s) => serde_json::Value::String(s.clone()),
            McmArg::Bool(b) => serde_json::Value::Bool(*b),
            McmArg::Null => serde_json::Value::Null,
        }
    }
}

/// Check whether a `dyyl` binary is available on PATH.
pub(crate) fn dyyl_available() -> bool {
    Command::new("dyyl")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Spawn `dyyl --host-json <script>`, answer every `mcm_command` with an
/// `ok` acknowledgement, and collect the full command stream.
///
/// Non-command stdout (script `io.out` output, sentinel values) is forwarded
/// to mcm's stdout so the user sees script output.
///
/// Returns the ordered list of collected `McmCommand`s on success.
pub(crate) fn run_dyyl_host(script_path: &Path) -> Result<Vec<McmCommand>> {
    let mut child = Command::new("dyyl")
        .arg("--host-json")
        .arg(script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| "spawn `dyyl --host-json` (is dyyl installed and on PATH?)")?;

    let stdout = child
        .stdout
        .take()
        .context("attach dyyl stdout")?;
    let stdin = child
        .stdin
        .take()
        .context("attach dyyl stdin")?;

    let mut reader = BufReader::new(stdout);
    let mut writer = BufWriter::new(stdin);

    let mut commands = Vec::new();
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader
            .read_line(&mut line)
            .context("read line from dyyl stdout")?;
        if n == 0 {
            break; // EOF — dyyl exited
        }

        let trimmed = line.trim_end();

        // Try to parse as an mcm_command request.
        if let Ok(cmd) = serde_json::from_str::<McmCommand>(trimmed) {
            if cmd.msg_type == "mcm_command" {
                let id = cmd.id.clone();
                commands.push(cmd);
                // Acknowledge with ok so dyyl proceeds.
                let resp = McmResponse {
                    msg_type: "mcm_response",
                    id,
                    ok: true,
                    value: None,
                    error: None,
                };
                let resp_json = serde_json::to_string(&resp)
                    .context("serialize mcm_response")?;
                writeln!(writer, "{resp_json}").context("write mcm_response")?;
                writer.flush().context("flush mcm_response")?;
                continue;
            }
        }

        // Not a command — forward as script output.
        println!("{trimmed}");
    }

    let status = child
        .wait()
        .context("wait for dyyl to exit")?;
    if !status.success() {
        bail!("dyyl exited with non-zero status: {status}");
    }

    Ok(commands)
}

/// Convert collected `McmCommand`s into a `.mcm` v2 lock.
///
/// Each command becomes a `LockStep`. The `name` (e.g. `mcm.game.choose`)
/// is classified into a step `op` and `StepPermission`. The positional
/// `args` array is mapped to a JSON object keyed by index (matching the
/// legacy `parse_dyyl_args` shape so `execute_step` works unchanged).
/// The `source_line` is preserved on the step.
pub(crate) fn commands_to_lock(commands: &[McmCommand]) -> Result<crate::mcm_package::McmLock> {
    let mut lock = new_lock("dyyl-build", "1.0.0");
    let mut steps: Vec<LockStep> = Vec::new();

    for cmd in commands {
        // Strip the `mcm.` prefix to get the dyyl op (e.g. "game.choose").
        let op = cmd
            .name
            .strip_prefix("mcm.")
            .unwrap_or(&cmd.name);
        let (permission, step_op) = classify_mcm_op(op);
        let args = args_to_object(&cmd.args);
        let step = LockStep {
            op: step_op,
            permission,
            args,
            source_line: cmd.source_line.clone(),
        };
        steps.push(step);
    }

    lock.steps = steps;
    Ok(lock)
}

/// Classify a dyyl mcm command into step op and permission.
///
/// Mirrors `pkg_cmd::classify_mcm_op` so host-protocol and text-parser
/// paths produce identical locks.
fn classify_mcm_op(dyyl_op: &str) -> (StepPermission, String) {
    match dyyl_op {
        "game.choose" | "game.install" | "mod.install" | "pkg.install" | "file.copy"
        | "file.write" | "net.download" | "config.set" => {
            (StepPermission::Install, dyyl_op.to_owned())
        }
        "shell.run" | "do" => (StepPermission::Do, dyyl_op.to_owned()),
        "root.system" => (StepPermission::Full, dyyl_op.to_owned()),
        _ => (StepPermission::Install, format!("mcm.{dyyl_op}")),
    }
}

/// Map a positional args array to the JSON-object shape `execute_step`
/// expects: `{"0": arg0, "1": arg1, ...}`.
fn args_to_object(args: &[McmArg]) -> serde_json::Value {
    if args.is_empty() {
        return serde_json::Value::Null;
    }
    let mut map = serde_json::Map::new();
    for (i, arg) in args.iter().enumerate() {
        map.insert(i.to_string(), arg.to_json());
    }
    serde_json::Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_to_object_maps_positional() {
        let args = vec![
            McmArg::Str("dev".into()),
            McmArg::Str("1.20.1".into()),
        ];
        let obj = args_to_object(&args);
        let map = obj.as_object().unwrap();
        assert_eq!(map.get("0").unwrap().as_str().unwrap(), "dev");
        assert_eq!(map.get("1").unwrap().as_str().unwrap(), "1.20.1");
    }

    #[test]
    fn args_to_object_empty_is_null() {
        let obj = args_to_object(&[]);
        assert!(obj.is_null());
    }

    #[test]
    fn classify_install_ops() {
        let (perm, op) = classify_mcm_op("game.choose");
        assert_eq!(op, "game.choose");
        assert!(matches!(perm, StepPermission::Install));
    }

    #[test]
    fn classify_do_ops() {
        let (perm, op) = classify_mcm_op("shell.run");
        assert_eq!(op, "shell.run");
        assert!(matches!(perm, StepPermission::Do));
    }

    #[test]
    fn classify_full_ops() {
        let (perm, op) = classify_mcm_op("root.system");
        assert_eq!(op, "root.system");
        assert!(matches!(perm, StepPermission::Full));
    }

    #[test]
    fn commands_to_lock_preserves_source_line() {
        let cmds = vec![McmCommand {
            msg_type: "mcm_command".into(),
            id: "1".into(),
            name: "mcm.game.choose".into(),
            args: vec![McmArg::Str("1.21.1".into())],
            source_line: Some("mcm.game.choose(\"1.21.1\")".into()),
        }];
        let lock = commands_to_lock(&cmds).unwrap();
        assert_eq!(lock.steps.len(), 1);
        assert_eq!(lock.steps[0].op, "game.choose");
        assert_eq!(
            lock.steps[0].source_line.as_deref(),
            Some("mcm.game.choose(\"1.21.1\")")
        );
    }

    #[test]
    fn mcm_command_deserializes_from_protocol_sample() {
        let line = r#"{"type":"mcm_command","id":"1","name":"mcm.game.choose","args":["1.21.1"],"source_line":"mcm.game.choose(1.21.1)"}"#;
        let cmd: McmCommand = serde_json::from_str(line).unwrap();
        assert_eq!(cmd.msg_type, "mcm_command");
        assert_eq!(cmd.id, "1");
        assert_eq!(cmd.name, "mcm.game.choose");
        assert_eq!(cmd.args.len(), 1);
        assert_eq!(cmd.source_line.as_deref(), Some("mcm.game.choose(1.21.1)"));
    }

    #[test]
    fn response_serializes_compact_one_line() {
        let resp = McmResponse {
            msg_type: "mcm_response",
            id: "1".into(),
            ok: true,
            value: None,
            error: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains('\n'));
        assert!(json.contains("\"type\":\"mcm_response\""));
        assert!(json.contains("\"ok\":true"));
        // value/error omitted, not null.
        assert!(!json.contains("null"));
    }
}
