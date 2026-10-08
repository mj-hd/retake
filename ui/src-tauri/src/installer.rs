use serde::Serialize;
use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{Array, DocumentMut, Item, Table, Value as TomlValue};

const SERVER_NAME: &str = "retake";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ClientId {
    #[serde(rename = "claude_code")]
    ClaudeCode,
    #[serde(rename = "claude_desktop")]
    ClaudeDesktop,
    #[serde(rename = "codex")]
    Codex,
    /// Spelled without the extra underscore on the wire; the frontend and the
    /// install/remove commands both use `opencode`.
    #[serde(rename = "opencode")]
    OpenCode,
    #[serde(rename = "gemini")]
    Gemini,
}

impl ClientId {
    pub fn all() -> [Self; 5] {
        [
            Self::ClaudeCode,
            Self::ClaudeDesktop,
            Self::Codex,
            Self::OpenCode,
            Self::Gemini,
        ]
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "claude_code" => Ok(Self::ClaudeCode),
            "claude_desktop" => Ok(Self::ClaudeDesktop),
            "codex" => Ok(Self::Codex),
            "opencode" | "open_code" => Ok(Self::OpenCode),
            "gemini" => Ok(Self::Gemini),
            _ => Err(format!("unknown client: {value}")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallState {
    Unavailable,
    Missing,
    Installed,
    Outdated,
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClientStatus {
    pub id: ClientId,
    pub state: InstallState,
    pub config_path: String,
    /// Version of the Retake build the client is registered against, when it
    /// can be read from the configured command. `None` for source installs.
    pub version: Option<String>,
    pub message: Option<String>,
}

pub struct Installer {
    home: PathBuf,
    executable: PathBuf,
    skill_source: Option<PathBuf>,
    app_directories: Vec<PathBuf>,
    binary_directories: Vec<PathBuf>,
}

impl Installer {
    #[cfg(test)]
    pub fn isolated(
        home: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        skill_source: Option<PathBuf>,
    ) -> Self {
        Self {
            home: home.into(),
            executable: executable.into(),
            skill_source,
            app_directories: Vec::new(),
            binary_directories: Vec::new(),
        }
    }

    pub fn for_current_user() -> Result<Self, String> {
        let home = std::env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or("home directory is unavailable")?;
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        Ok(Self {
            skill_source: bundled_skill(&executable),
            app_directories: vec![PathBuf::from("/Applications"), home.join("Applications")],
            binary_directories: vec![
                home.join(".local/bin"),
                home.join(".local/share/mise/shims"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ],
            home,
            executable,
        })
    }

    pub fn status(&self) -> Vec<ClientStatus> {
        ClientId::all().map(|id| self.status_for(id)).to_vec()
    }

    pub fn install(&self, ids: &[ClientId]) -> Vec<ClientStatus> {
        ids.iter().map(|id| self.install_one(*id)).collect()
    }

    pub fn remove(&self, ids: &[ClientId]) -> Vec<ClientStatus> {
        ids.iter().map(|id| self.remove_one(*id)).collect()
    }

    fn status_for(&self, id: ClientId) -> ClientStatus {
        if !self.available(id) {
            return self.report(id, InstallState::Unavailable, None);
        }
        match self.registration_state(id) {
            Ok(state) if state == InstallState::Installed && !self.skill_current(id) => {
                self.report(id, InstallState::Outdated, None)
            }
            Ok(state) => self.report(id, state, None),
            Err(message) => self.report(id, InstallState::Invalid, Some(message)),
        }
    }

    fn install_one(&self, id: ClientId) -> ClientStatus {
        if !self.available(id) {
            return self.report(id, InstallState::Unavailable, None);
        }
        let message = match id {
            ClientId::ClaudeCode => self.install_stdio(id, true),
            ClientId::ClaudeDesktop | ClientId::Gemini => self.install_stdio(id, false),
            ClientId::Codex => self.install_codex(),
            ClientId::OpenCode => self.install_opencode(),
        };
        if let Err(message) = message {
            return self.report(id, InstallState::Invalid, Some(message));
        }
        if let Err(message) = self.install_skill(id) {
            return self.report(id, InstallState::Outdated, Some(message));
        }
        self.status_for(id)
    }

    fn remove_one(&self, id: ClientId) -> ClientStatus {
        if !self.available(id) {
            return self.report(id, InstallState::Unavailable, None);
        }
        let message = match id {
            ClientId::Codex => self.remove_codex(),
            ClientId::OpenCode => self.remove_opencode(),
            _ => self.remove_stdio(id),
        };
        if let Err(message) = message {
            return self.report(id, InstallState::Invalid, Some(message));
        }
        if let Err(message) = self.remove_skill(id) {
            return self.report(id, InstallState::Invalid, Some(message));
        }
        self.status_for(id)
    }

    fn report(&self, id: ClientId, state: InstallState, message: Option<String>) -> ClientStatus {
        let registered = matches!(state, InstallState::Installed | InstallState::Outdated);
        ClientStatus {
            id,
            state,
            config_path: self.config_path(id).to_string_lossy().to_string(),
            version: if registered {
                self.configured_version(id)
            } else {
                None
            },
            message,
        }
    }

    /// Version of the Retake build a client currently points at. App bundles
    /// carry it in Info.plist; a bare `retake` binary on PATH does not.
    fn configured_version(&self, id: ClientId) -> Option<String> {
        let command = self.configured_command(id)?;
        command
            .ancestors()
            .find(|path| path.extension().is_some_and(|value| value == "app"))
            .and_then(|bundle| bundle_version(&bundle.join("Contents/Info.plist")))
    }

    fn configured_command(&self, id: ClientId) -> Option<PathBuf> {
        let path = self.config_path(id);
        let text = fs::read_to_string(&path).ok()?;
        let command = match id {
            ClientId::Codex => text
                .parse::<DocumentMut>()
                .ok()?
                .get("mcp_servers")
                .and_then(|item| item.get(SERVER_NAME))
                .and_then(|server| server.get("command"))
                .and_then(Item::as_str)
                .map(str::to_string),
            ClientId::OpenCode => {
                let root: Value = serde_json::from_str(&text).ok()?;
                root.pointer("/mcp/servers/retake/command/0")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }
            _ => {
                let root: Value = serde_json::from_str(&text).ok()?;
                root.pointer("/mcpServers/retake/command")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }
        }?;
        (!command.is_empty()).then(|| PathBuf::from(command))
    }

    fn available(&self, id: ClientId) -> bool {
        let marker = match id {
            ClientId::ClaudeCode => {
                self.home.join(".claude").is_dir() || self.home.join(".claude.json").is_file()
            }
            ClientId::ClaudeDesktop => self
                .home
                .join("Library/Application Support/Claude")
                .is_dir(),
            ClientId::Codex => self.home.join(".codex").is_dir(),
            ClientId::OpenCode => self.home.join(".config/opencode").is_dir(),
            ClientId::Gemini => self.home.join(".gemini").is_dir(),
        };
        marker
            || self.app_name(id).is_some_and(|name| {
                self.app_directories
                    .iter()
                    .any(|dir| dir.join(name).is_dir())
            })
            || self.binary_names(id).iter().any(|name| {
                self.binary_directories
                    .iter()
                    .any(|dir| dir.join(name).is_file())
            })
    }

    fn app_name(&self, id: ClientId) -> Option<&'static str> {
        match id {
            ClientId::ClaudeDesktop => Some("Claude.app"),
            ClientId::Codex => Some("Codex.app"),
            _ => None,
        }
    }

    fn binary_names(&self, id: ClientId) -> &'static [&'static str] {
        match id {
            ClientId::ClaudeCode => &["claude"],
            ClientId::Codex => &["codex"],
            ClientId::OpenCode => &["opencode", "opencode2"],
            ClientId::Gemini => &["gemini"],
            ClientId::ClaudeDesktop => &[],
        }
    }

    fn config_path(&self, id: ClientId) -> PathBuf {
        match id {
            ClientId::ClaudeCode => self.home.join(".claude.json"),
            ClientId::ClaudeDesktop => self
                .home
                .join("Library/Application Support/Claude/claude_desktop_config.json"),
            ClientId::Codex => self.home.join(".codex/config.toml"),
            ClientId::OpenCode => self.home.join(".config/opencode/opencode.json"),
            ClientId::Gemini => self.home.join(".gemini/settings.json"),
        }
    }

    fn registration_state(&self, id: ClientId) -> Result<InstallState, String> {
        let path = self.config_path(id);
        if !path.is_file() {
            return Ok(InstallState::Missing);
        }
        match id {
            ClientId::Codex => codex_state(
                &fs::read_to_string(&path).map_err(|e| e.to_string())?,
                &self.executable,
            ),
            ClientId::OpenCode => opencode_state(&read_json(&path)?, &self.executable),
            _ => stdio_state(&read_json(&path)?, &self.executable),
        }
    }

    fn install_stdio(&self, id: ClientId, typed: bool) -> Result<(), String> {
        let path = self.config_path(id);
        let mut root = read_json(&path)?;
        let servers = ensure_object(&mut root, "mcpServers")?;
        let mut entry = json!({
            "command": self.executable_string(),
            "args": ["mcp"],
        });
        if typed {
            entry["type"] = json!("stdio");
        }
        servers.insert(SERVER_NAME.to_string(), entry);
        write_json(&path, &root)
    }

    fn remove_stdio(&self, id: ClientId) -> Result<(), String> {
        let path = self.config_path(id);
        if !path.is_file() {
            return Ok(());
        }
        let mut root = read_json(&path)?;
        if let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
            servers.remove(SERVER_NAME);
        }
        write_json(&path, &root)
    }

    fn install_codex(&self) -> Result<(), String> {
        let path = self.config_path(ClientId::Codex);
        let mut document = read_toml(&path)?;
        let servers = ensure_toml_table(&mut document, "mcp_servers")?;
        let server = servers
            .entry(SERVER_NAME)
            .or_insert(Item::Table(Table::new()));
        let server = server
            .as_table_mut()
            .ok_or("codex retake entry is not a table")?;
        server.insert("command", toml_string(&self.executable_string()));
        let mut args = Array::new();
        args.push("mcp");
        server.insert("args", Item::Value(TomlValue::Array(args)));
        write_atomic(&path, &document.to_string())
    }

    fn remove_codex(&self) -> Result<(), String> {
        let path = self.config_path(ClientId::Codex);
        if !path.is_file() {
            return Ok(());
        }
        let mut document = read_toml(&path)?;
        if let Some(servers) = document.get_mut("mcp_servers").and_then(Item::as_table_mut) {
            servers.remove(SERVER_NAME);
        }
        write_atomic(&path, &document.to_string())
    }

    fn install_opencode(&self) -> Result<(), String> {
        let path = self.config_path(ClientId::OpenCode);
        let mut root = read_json(&path)?;
        let mcp = ensure_object(&mut root, "mcp")?;
        if mcp.contains_key(SERVER_NAME) {
            upsert_opencode_entry(
                mcp.get_mut(SERVER_NAME).expect("retake entry exists"),
                &self.executable_string(),
                true,
            )?;
        }
        let servers = ensure_object(&mut root, "mcp")?;
        let servers = ensure_nested_object(servers, "servers")?;
        let entry = servers
            .entry(SERVER_NAME.to_string())
            .or_insert_with(|| json!({}));
        upsert_opencode_entry(entry, &self.executable_string(), false)?;
        write_json(&path, &root)
    }

    fn remove_opencode(&self) -> Result<(), String> {
        let path = self.config_path(ClientId::OpenCode);
        if !path.is_file() {
            return Ok(());
        }
        let mut root = read_json(&path)?;
        if let Some(mcp) = root.get_mut("mcp").and_then(Value::as_object_mut) {
            mcp.remove(SERVER_NAME);
            if let Some(servers) = mcp.get_mut("servers").and_then(Value::as_object_mut) {
                servers.remove(SERVER_NAME);
            }
        }
        write_json(&path, &root)
    }

    fn skill_destination(&self, id: ClientId) -> Option<PathBuf> {
        let relative = match id {
            ClientId::ClaudeCode => ".claude/skills/retake/SKILL.md",
            ClientId::Codex => ".codex/skills/retake/SKILL.md",
            ClientId::OpenCode => ".config/opencode/skills/retake/SKILL.md",
            ClientId::Gemini => ".gemini/skills/retake/SKILL.md",
            ClientId::ClaudeDesktop => return None,
        };
        Some(self.home.join(relative))
    }

    fn skill_current(&self, id: ClientId) -> bool {
        let Some(source) = &self.skill_source else {
            return true;
        };
        let Some(destination) = self.skill_destination(id) else {
            return true;
        };
        fs::read(source).ok() == fs::read(&destination).ok() && is_retake_skill(&destination)
    }

    fn install_skill(&self, id: ClientId) -> Result<(), String> {
        let (Some(source), Some(destination)) = (&self.skill_source, self.skill_destination(id))
        else {
            return Ok(());
        };
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::copy(source, &destination).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn remove_skill(&self, id: ClientId) -> Result<(), String> {
        let Some(destination) = self.skill_destination(id) else {
            return Ok(());
        };
        if destination.is_file() && is_retake_skill(&destination) {
            fs::remove_file(&destination).map_err(|error| error.to_string())?;
        }
        if let Some(directory) = destination.parent() {
            let _ = fs::remove_dir(directory);
        }
        Ok(())
    }

    fn executable_string(&self) -> String {
        self.executable.to_string_lossy().to_string()
    }
}

fn bundled_skill(executable: &Path) -> Option<PathBuf> {
    let bundled = executable
        .parent()?
        .parent()?
        .join("Resources/skills/retake/SKILL.md");
    if bundled.is_file() {
        return Some(bundled);
    }
    let development =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.agents/skills/retake/SKILL.md");
    development.is_file().then_some(development)
}

fn read_json(path: &Path) -> Result<Value, String> {
    if !path.is_file() {
        return Ok(json!({}));
    }
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(&text).map_err(|_| "invalid_config".to_string())
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    text.push('\n');
    write_atomic(path, &text)
}

fn read_toml(path: &Path) -> Result<DocumentMut, String> {
    if !path.is_file() {
        return Ok(DocumentMut::new());
    }
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    if text.trim().is_empty() {
        return Ok(DocumentMut::new());
    }
    text.parse::<DocumentMut>()
        .map_err(|_| "invalid_config".to_string())
}

fn write_atomic(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("config");
    let temp = path.with_file_name(format!(".{name}.{}.retake-tmp", std::process::id()));
    fs::write(&temp, contents).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o777)
            .unwrap_or(0o600);
        if let Err(error) = fs::set_permissions(&temp, fs::Permissions::from_mode(mode)) {
            let _ = fs::remove_file(&temp);
            return Err(error.to_string());
        }
    }
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        error.to_string()
    })
}

fn ensure_object<'a>(root: &'a mut Value, key: &str) -> Result<&'a mut Map<String, Value>, String> {
    if !root.is_object() {
        return Err("invalid_config".into());
    }
    let entry = root
        .as_object_mut()
        .expect("root is an object")
        .entry(key.to_string())
        .or_insert_with(|| json!({}));
    entry
        .as_object_mut()
        .ok_or_else(|| format!("{key} is not an object"))
}

fn ensure_nested_object<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>, String> {
    let entry = parent.entry(key.to_string()).or_insert_with(|| json!({}));
    entry
        .as_object_mut()
        .ok_or_else(|| format!("{key} is not an object"))
}

fn ensure_toml_table<'a>(
    document: &'a mut DocumentMut,
    key: &str,
) -> Result<&'a mut Table, String> {
    if document.get(key).is_none() {
        let mut table = Table::new();
        table.set_implicit(true);
        document.insert(key, Item::Table(table));
    }
    document
        .get_mut(key)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| format!("{key} is not a table"))
}

fn toml_string(value: &str) -> Item {
    Item::Value(TomlValue::from(value))
}

fn stdio_state(root: &Value, executable: &Path) -> Result<InstallState, String> {
    let Some(server) = root.pointer("/mcpServers/retake") else {
        return Ok(InstallState::Missing);
    };
    Ok(if stdio_matches(server, executable) {
        InstallState::Installed
    } else {
        InstallState::Outdated
    })
}

fn stdio_matches(server: &Value, executable: &Path) -> bool {
    server
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| same_executable(command, executable))
        && args_are_mcp(server.get("args"))
}

fn opencode_state(root: &Value, executable: &Path) -> Result<InstallState, String> {
    let current = root.pointer("/mcp/servers/retake");
    let legacy = root.pointer("/mcp/retake");
    if current.is_none() && legacy.is_none() {
        return Ok(InstallState::Missing);
    }
    let current_ok = current.is_some_and(|entry| opencode_matches(entry, executable));
    let legacy_ok = legacy.is_none_or(|entry| opencode_matches(entry, executable));
    Ok(if current_ok && legacy_ok {
        InstallState::Installed
    } else {
        InstallState::Outdated
    })
}

fn opencode_matches(entry: &Value, executable: &Path) -> bool {
    let Some(command) = entry.get("command").and_then(Value::as_array) else {
        return false;
    };
    let path_ok = command
        .first()
        .and_then(Value::as_str)
        .is_some_and(|path| same_executable(path, executable));
    let args_ok = command.len() == 2 && command.get(1).and_then(Value::as_str) == Some("mcp");
    let environment_ok = entry
        .get("environment")
        .and_then(Value::as_object)
        .is_none_or(|environment| environment.keys().all(|key| !key.starts_with("RETAKE_")));
    let enabled = entry.get("disabled").and_then(Value::as_bool) != Some(true);
    path_ok && args_ok && environment_ok && enabled
}

fn upsert_opencode_entry(entry: &mut Value, executable: &str, legacy: bool) -> Result<(), String> {
    if !entry.is_object() {
        *entry = json!({});
    }
    let object = entry.as_object_mut().expect("entry is an object");
    object.insert("type".into(), json!("local"));
    object.insert("command".into(), json!([executable, "mcp"]));
    object.remove("url");
    object.remove("disabled");
    if legacy {
        object.insert("enabled".into(), json!(true));
    } else {
        object.remove("enabled");
    }
    if let Some(environment) = object.get_mut("environment").and_then(Value::as_object_mut) {
        environment.retain(|key, _| !key.starts_with("RETAKE_"));
        if environment.is_empty() {
            object.remove("environment");
        }
    }
    Ok(())
}

fn codex_state(text: &str, executable: &Path) -> Result<InstallState, String> {
    let document = text
        .parse::<DocumentMut>()
        .map_err(|_| "invalid_config".to_string())?;
    let Some(server) = document
        .get("mcp_servers")
        .and_then(|item| item.get(SERVER_NAME))
    else {
        return Ok(InstallState::Missing);
    };
    let command_ok = server
        .get("command")
        .and_then(Item::as_str)
        .is_some_and(|command| same_executable(command, executable));
    let args_ok = server.get("args").is_some_and(toml_args_are_mcp);
    Ok(if command_ok && args_ok {
        InstallState::Installed
    } else {
        InstallState::Outdated
    })
}

fn toml_args_are_mcp(item: &Item) -> bool {
    item.as_array().is_some_and(|args| {
        args.len() == 1 && args.get(0).and_then(TomlValue::as_str) == Some("mcp")
    })
}

fn args_are_mcp(args: Option<&Value>) -> bool {
    args.and_then(Value::as_array)
        .is_some_and(|args| args.len() == 1 && args.first().and_then(Value::as_str) == Some("mcp"))
}

fn same_executable(configured: &str, executable: &Path) -> bool {
    let configured_path = Path::new(configured);
    if configured_path == executable {
        return true;
    }
    match (configured_path.canonicalize(), executable.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn bundle_version(info_plist: &Path) -> Option<String> {
    let text = fs::read_to_string(info_plist).ok()?;
    let after_key = text.split("<key>CFBundleShortVersionString</key>").nth(1)?;
    let value = after_key
        .split("<string>")
        .nth(1)?
        .split("</string>")
        .next()?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn is_retake_skill(path: &Path) -> bool {
    fs::read_to_string(path).ok().is_some_and(|text| {
        text.lines()
            .take(8)
            .any(|line| line.trim() == "name: retake")
    })
}

pub fn parse_client_ids(ids: &[String]) -> Result<Vec<ClientId>, String> {
    if ids.is_empty() {
        return Ok(ClientId::all().to_vec());
    }
    ids.iter().map(|id| ClientId::parse(id)).collect()
}

#[tauri::command]
pub fn installer_status() -> Result<Vec<ClientStatus>, String> {
    Ok(Installer::for_current_user()?.status())
}

#[tauri::command]
pub fn install_clients(ids: Vec<String>) -> Result<Vec<ClientStatus>, String> {
    let installer = Installer::for_current_user()?;
    Ok(installer.install(&parse_client_ids(&ids)?))
}

#[tauri::command]
pub fn remove_clients(ids: Vec<String>) -> Result<Vec<ClientStatus>, String> {
    let installer = Installer::for_current_user()?;
    Ok(installer.remove(&parse_client_ids(&ids)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> (tempfile::TempDir, Installer) {
        let home = tempfile::tempdir().unwrap();
        let executable = home.path().join("Retake.app/Contents/MacOS/retake-desktop");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, "").unwrap();
        let skill = home.path().join("skill/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, "---\nname: retake\n---\n# retake\n").unwrap();
        let installer = Installer::isolated(home.path(), &executable, Some(skill));
        (home, installer)
    }

    fn mark_available(home: &Path) {
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::create_dir_all(home.join("Library/Application Support/Claude")).unwrap();
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::create_dir_all(home.join(".config/opencode")).unwrap();
        fs::create_dir_all(home.join(".gemini")).unwrap();
    }

    #[test]
    fn reports_the_registered_app_version_and_none_for_source_installs() {
        let (home, installer) = fixture();
        mark_available(home.path());
        let claude = home.path().join(".claude.json");
        fs::write(
            &claude,
            serde_json::json!({
                "mcpServers": {"retake": {"command": installer.executable_string(), "args": ["mcp"], "type": "stdio"}}
            })
            .to_string(),
        )
        .unwrap();
        let bundle = home
            .path()
            .join("Applications/Retake.app/Contents/MacOS/retake-desktop");
        fs::create_dir_all(bundle.parent().unwrap()).unwrap();
        fs::write(
            bundle.parent().unwrap().parent().unwrap().join("Info.plist"),
            "<plist><dict><key>CFBundleShortVersionString</key><string>1.4.2</string></dict></plist>",
        )
        .unwrap();
        fs::write(
            &claude,
            serde_json::json!({
                "mcpServers": {"retake": {"command": bundle.to_string_lossy(), "args": ["mcp"], "type": "stdio"}}
            })
            .to_string(),
        )
        .unwrap();
        let status = installer.status_for(ClientId::ClaudeCode);
        assert_eq!(status.version.as_deref(), Some("1.4.2"));

        fs::write(
            &claude,
            serde_json::json!({
                "mcpServers": {"retake": {"command": "/usr/local/bin/retake", "args": ["mcp"], "type": "stdio"}}
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(installer.status_for(ClientId::ClaudeCode).version, None);
        assert_eq!(installer.status_for(ClientId::Gemini).version, None);
    }

    #[test]
    fn does_not_create_config_for_a_missing_client() {
        let (home, installer) = fixture();
        let status = installer.install(&[ClientId::ClaudeCode]);
        assert_eq!(status[0].state, InstallState::Unavailable);
        assert!(!home.path().join(".claude.json").exists());
    }

    #[test]
    fn installs_preserves_unrelated_settings_and_is_idempotent() {
        let (home, installer) = fixture();
        mark_available(home.path());
        let claude = home.path().join(".claude.json");
        fs::write(
            &claude,
            serde_json::to_string_pretty(&json!({
                "numStartups": 4,
                "mcpServers": {
                    "pencil": {"command": "/Applications/Pen.app/server", "args": ["--app"], "type": "stdio"}
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let desktop = home
            .path()
            .join("Library/Application Support/Claude/claude_desktop_config.json");
        fs::write(
            &desktop,
            "{\"preferences\":{\"coworkWebSearchEnabled\":true}}\n",
        )
        .unwrap();
        fs::write(
            home.path().join(".gemini/settings.json"),
            "{\"theme\":\"dark\",\"mcpServers\":{\"pencil\":{\"command\":\"pen\"}}}\n",
        )
        .unwrap();

        let installed = installer.install(&ClientId::all());
        assert!(
            installed
                .iter()
                .all(|status| status.state == InstallState::Installed)
        );
        let again = installer.install(&ClientId::all());
        assert_eq!(installed, again);

        let claude_json: Value =
            serde_json::from_str(&fs::read_to_string(claude).unwrap()).unwrap();
        assert_eq!(claude_json["numStartups"], 4);
        assert_eq!(
            claude_json["mcpServers"]["pencil"]["command"],
            "/Applications/Pen.app/server"
        );
        assert_eq!(
            claude_json["mcpServers"]["retake"]["command"],
            installer.executable_string()
        );
        assert_eq!(claude_json["mcpServers"]["retake"]["type"], "stdio");

        let desktop_json: Value =
            serde_json::from_str(&fs::read_to_string(desktop).unwrap()).unwrap();
        assert_eq!(desktop_json["preferences"]["coworkWebSearchEnabled"], true);
        assert!(desktop_json["mcpServers"]["retake"].get("type").is_none());
    }

    #[test]
    fn updates_codex_without_removing_tool_policy_or_other_servers() {
        let (home, installer) = fixture();
        mark_available(home.path());
        let path = home.path().join(".codex/config.toml");
        fs::write(
            &path,
            "\
model = \"gpt\"

[mcp_servers.playwright]
command = \"npx\"
args = [ \"@playwright/mcp@latest\" ]

[mcp_servers.retake]
command = \"/old/retake with spaces\"
args = [ \"mcp\" ]

[mcp_servers.retake.tools.wait_review]
approval_mode = \"approve\"

[projects.\"/tmp/demo\"]
trust_level = \"trusted\"
",
        )
        .unwrap();

        let status = installer.install(&[ClientId::Codex]);
        assert_eq!(status[0].state, InstallState::Installed);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("command = \"npx\""));
        assert!(text.contains(&format!("command = \"{}\"", installer.executable_string())));
        assert!(text.contains("approval_mode = \"approve\""));
        assert!(text.contains("trust_level = \"trusted\""));
        assert!(!text.contains("/old/retake"));

        installer.remove(&[ClientId::Codex]);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("[mcp_servers.playwright]"));
        assert!(!text.contains("mcp_servers.retake"));
        assert!(text.contains("trust_level = \"trusted\""));
    }

    #[test]
    fn updates_opencode_v2_and_strips_packaged_runtime_overrides() {
        let (home, installer) = fixture();
        mark_available(home.path());
        let path = home.path().join(".config/opencode/opencode.json");
        fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "$schema": "https://opencode.ai/config.json",
                "mcp": {
                    "pencil": {"type": "local", "command": ["pen"], "enabled": true},
                    "retake": {"type": "local", "command": ["/old/retake", "mcp"], "enabled": true},
                    "servers": {
                        "retake": {
                            "type": "local",
                            "command": ["/old/retake", "mcp"],
                            "environment": {"RETAKE_DESKTOP_BIN": "/old", "LOG_LEVEL": "info"}
                        }
                    }
                },
                "model": "kept"
            }))
            .unwrap(),
        )
        .unwrap();

        assert_eq!(
            installer.install(&[ClientId::OpenCode])[0].state,
            InstallState::Installed
        );
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["model"], "kept");
        assert_eq!(value["mcp"]["pencil"]["command"][0], "pen");
        assert_eq!(
            value["mcp"]["servers"]["retake"]["command"][0],
            installer.executable_string()
        );
        assert_eq!(
            value["mcp"]["servers"]["retake"]["environment"]["LOG_LEVEL"],
            "info"
        );
        assert!(
            value["mcp"]["servers"]["retake"]["environment"]
                .get("RETAKE_DESKTOP_BIN")
                .is_none()
        );
        assert_eq!(
            value["mcp"]["retake"]["command"][0],
            installer.executable_string()
        );

        installer.remove(&[ClientId::OpenCode]);
        let value: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert!(value["mcp"].get("retake").is_none());
        assert!(value["mcp"]["servers"].get("retake").is_none());
        assert!(value["mcp"].get("pencil").is_some());
    }

    #[test]
    fn refuses_to_overwrite_invalid_config() {
        let (home, installer) = fixture();
        mark_available(home.path());
        let path = home.path().join(".claude.json");
        fs::write(&path, "{not json").unwrap();
        let status = installer.install(&[ClientId::ClaudeCode]);
        assert_eq!(status[0].state, InstallState::Invalid);
        assert_eq!(fs::read_to_string(path).unwrap(), "{not json");
    }

    #[test]
    fn copies_and_removes_only_the_retake_skill() {
        let (home, installer) = fixture();
        mark_available(home.path());
        installer.install(&[ClientId::ClaudeCode]);
        let skill = home.path().join(".claude/skills/retake/SKILL.md");
        assert!(is_retake_skill(&skill));
        installer.remove(&[ClientId::ClaudeCode]);
        assert!(!skill.exists());
    }
}
