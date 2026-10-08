use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Paths to the optional Node-based capture runtime shipped with Retake.app.
/// Environment overrides are intentionally shared by the server and renderers
/// so source builds and packaged builds use the same launch behavior.
#[derive(Clone, Debug, Default)]
pub struct RuntimePaths {
    root: Option<PathBuf>,
    node_override: Option<PathBuf>,
    worker_override: Option<PathBuf>,
    chromium_override: Option<PathBuf>,
}

impl RuntimePaths {
    pub fn discover() -> Self {
        let root_override = std::env::var_os("RETAKE_RUNTIME_DIR").map(PathBuf::from);
        let root = root_override
            .filter(|path| path.is_dir())
            .or_else(|| discover_root(std::env::current_exe().ok().as_deref()))
            .or_else(|| discover_root(std::env::current_dir().ok().as_deref()));

        Self {
            root,
            node_override: std::env::var_os("RETAKE_NODE_BIN").map(PathBuf::from),
            worker_override: std::env::var_os("RETAKE_WEB_WORKER").map(PathBuf::from),
            chromium_override: std::env::var_os("RETAKE_CHROMIUM_BIN").map(PathBuf::from),
        }
    }

    /// Construct paths rooted at a known resource directory (also useful to
    /// validate a staged app layout without changing process environment).
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
            ..Self::default()
        }
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn node_executable(&self) -> PathBuf {
        if let Some(override_path) = &self.node_override {
            return override_path.clone();
        }
        self.root
            .as_ref()
            .map(|root| root.join("runtime/node"))
            .filter(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from("node"))
    }

    pub fn web_worker(&self) -> Option<PathBuf> {
        self.worker_override
            .clone()
            .or_else(|| {
                self.root
                    .as_ref()
                    .map(|root| root.join("workers/web-capture/index.js"))
            })
            .filter(|path| path.is_file())
    }

    pub fn chromium_executable(&self) -> Option<PathBuf> {
        self.chromium_override
            .clone()
            .or_else(|| {
                self.root
                    .as_ref()
                    .map(|root| root.join("chromium/chrome-headless-shell"))
            })
            .filter(|path| path.is_file())
    }

    /// Apply packaged browser settings to any Node worker command. An explicit
    /// process-level override always wins over the bundled Chromium binary.
    pub fn configure_worker_command(&self, command: &mut Command) {
        if std::env::var_os("RETAKE_CHROMIUM_BIN").is_none() {
            if let Some(chromium) = self.chromium_executable() {
                command.env("RETAKE_CHROMIUM_BIN", chromium);
            }
        }
    }

    /// In an app bundle, the MCP process is the desktop executable itself and
    /// can relaunch that executable in its preserved review-window mode.
    pub fn desktop_executable(&self) -> Option<PathBuf> {
        if self
            .root
            .as_ref()
            .is_some_and(|root| root.join("runtime/node").is_file())
        {
            std::env::current_exe().ok()
        } else {
            std::env::var_os("RETAKE_DESKTOP_BIN").map(PathBuf::from)
        }
    }
}

fn discover_root(executable: Option<&Path>) -> Option<PathBuf> {
    let executable = executable?;
    let start = if executable.is_file() {
        executable.parent()?
    } else {
        executable
    };
    start
        .ancestors()
        .flat_map(|directory| [directory.to_path_buf(), directory.join("Resources")])
        .find(|candidate| candidate.join("workers/web-capture/index.js").is_file())
}

#[cfg(test)]
mod tests {
    use super::RuntimePaths;
    use std::fs;

    #[test]
    fn resolves_packaged_node_worker_and_chromium_paths() {
        let root = std::env::temp_dir().join(format!("retake-runtime-{}", uuid::Uuid::new_v4()));
        let worker = root.join("workers/web-capture/index.js");
        let node = root.join("runtime/node");
        let chromium = root.join("chromium/chrome-headless-shell");
        fs::create_dir_all(worker.parent().unwrap()).unwrap();
        fs::create_dir_all(node.parent().unwrap()).unwrap();
        fs::create_dir_all(chromium.parent().unwrap()).unwrap();
        fs::write(&worker, "").unwrap();
        fs::write(&node, "").unwrap();
        fs::write(&chromium, "").unwrap();

        let paths = RuntimePaths::from_root(&root);
        assert_eq!(paths.node_executable(), node);
        assert_eq!(paths.web_worker(), Some(worker));
        assert_eq!(paths.chromium_executable(), Some(chromium));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn falls_back_to_system_node_when_not_packaged() {
        assert_eq!(
            RuntimePaths::default().node_executable(),
            std::path::PathBuf::from("node")
        );
    }
}
