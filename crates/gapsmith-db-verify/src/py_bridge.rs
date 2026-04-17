//! Python subprocess bridge.
//!
//! Every Python-backed verifier goes through this module. Protocol:
//!
//! 1. Rust spawns `uv run --project python python -m gapsmith_bridge.verify
//!    --action <name>` (or a bare `python -m …` if `use_uv = false`).
//! 2. Rust writes a JSON payload to stdin and closes it.
//! 3. Python writes a single JSON response to stdout and exits 0 on
//!    success. Non-zero exit codes or non-JSON output surface as a
//!    [`VerifyError::PyBridge`].

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::VerifyError;

/// Options for locating the Python bridge.
#[derive(Debug, Clone)]
pub struct PyBridge {
    /// Directory containing `pyproject.toml` (the uv-managed project).
    pub project_dir: PathBuf,
    /// If true, invoke through `uv run --project …`; otherwise plain `python`.
    pub use_uv: bool,
}

impl PyBridge {
    #[must_use]
    pub fn new(project_dir: PathBuf) -> Self {
        Self {
            project_dir,
            use_uv: true,
        }
    }

    /// Call the Python bridge with `action` and JSON payload `req`; expect
    /// a JSON response deserialisable as `Resp`.
    pub fn call<Req, Resp>(&self, action: &str, req: &Req) -> crate::Result<Resp>
    where
        Req: Serialize,
        Resp: for<'de> Deserialize<'de>,
    {
        let req_bytes = serde_json::to_vec(req)?;
        let mut cmd = if self.use_uv {
            let mut c = Command::new("uv");
            c.arg("run")
                .arg("--project")
                .arg(&self.project_dir)
                .arg("python");
            c
        } else {
            Command::new("python")
        };
        cmd.arg("-m")
            .arg("gapsmith_bridge.verify")
            .arg("--action")
            .arg(action)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!(action, "spawning python bridge");
        let mut child = cmd
            .spawn()
            .map_err(|e| VerifyError::PyBridge(format!("spawn: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&req_bytes)
                .map_err(|e| VerifyError::PyBridge(format!("stdin: {e}")))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|e| VerifyError::PyBridge(format!("wait: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VerifyError::PyBridge(format!(
                "python exited {:?}: {stderr}",
                output.status.code()
            )));
        }
        let resp: Resp = serde_json::from_slice(&output.stdout)
            .map_err(|e| VerifyError::PyBridge(format!("decode: {e}")))?;
        Ok(resp)
    }

    /// Best-effort probe: is the bridge reachable? `true` if `uv run …
    /// python -m gapsmith_bridge.verify --action ping` exits 0.
    #[must_use]
    pub fn ping(&self) -> bool {
        #[derive(Serialize)]
        struct Empty {}
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct PingResp {
            ok: bool,
        }
        self.call::<Empty, PingResp>("ping", &Empty {}).is_ok()
    }
}
