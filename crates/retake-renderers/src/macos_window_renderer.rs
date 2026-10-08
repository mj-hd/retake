use image::{GenericImageView, ImageFormat, ImageReader};
use retake_core::{
    RenderError, Renderer, ResolvedReference, Selection, SnapshotDraft, StoredSnapshot, Target,
    TargetKind,
};
use std::io::Cursor;

pub struct MacosWindowRenderer {
    screencapture: String,
}

impl MacosWindowRenderer {
    pub fn new() -> Self {
        Self {
            screencapture: std::env::var("RETAKE_SCREENCAPTURE")
                .unwrap_or_else(|_| "/usr/sbin/screencapture".into()),
        }
    }
}

impl Default for MacosWindowRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WindowInfo {
    id: u32,
    owner: String,
    title: String,
    layer: i32,
}

#[derive(Debug)]
struct WindowSelector {
    id: Option<u32>,
    owner: Option<String>,
    title: Option<String>,
}

#[async_trait::async_trait]
impl Renderer for MacosWindowRenderer {
    fn id(&self) -> &'static str {
        "macos_window"
    }

    async fn capture(&self, target: &Target) -> Result<SnapshotDraft, RenderError> {
        if target.kind != TargetKind::MacosWindow {
            return Err(RenderError::Unsupported);
        }
        capture_window(&self.screencapture, target).await
    }

    fn resolve(
        &self,
        snapshot: &StoredSnapshot,
        selection: &Selection,
    ) -> Result<ResolvedReference, RenderError> {
        validate_selection(snapshot, selection)?;
        let mapping: serde_json::Value = serde_json::from_str(&snapshot.mapping_json)
            .map_err(|error| RenderError::Resolve(error.to_string()))?;
        let label = &snapshot.source_label;
        let description = match selection {
            Selection::Point { x, y } => {
                format!("{label}: macOS window position ({x:.0},{y:.0})")
            }
            Selection::Rect {
                x,
                y,
                width,
                height,
            } => format!(
                "{label}: macOS window rectangle ({x:.0},{y:.0})–({:.0},{:.0})",
                x + width,
                y + height
            ),
        };
        Ok(ResolvedReference {
            description,
            anchor: serde_json::json!({
                "kind": "macos_window",
                "window_id_at_capture": mapping["window_id"],
                "owner": mapping["owner"],
                "title": mapping["title"],
                "selection": selection,
            }),
        })
    }
}

fn selector_from_target(target: &Target) -> Result<WindowSelector, RenderError> {
    let metadata = target.metadata.as_ref();
    let id = metadata
        .and_then(|value| value.get("window_id"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    let title = metadata
        .and_then(|value| value.get("window_title"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned);
    let owner = target
        .path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(application_name)
        .filter(|value| !value.is_empty());
    if id.is_none() && owner.is_none() {
        return Err(RenderError::Capture(
            "macos_window requires an application name or .app path in path, or metadata.window_id"
                .into(),
        ));
    }
    Ok(WindowSelector { id, owner, title })
}

fn application_name(value: &str) -> String {
    let path = std::path::Path::new(value);
    let name = path
        .file_name()
        .and_then(|part| part.to_str())
        .unwrap_or(value);
    name.strip_suffix(".app").unwrap_or(name).to_owned()
}

fn choose_window<'a>(
    windows: &'a [WindowInfo],
    selector: &WindowSelector,
) -> Option<&'a WindowInfo> {
    windows.iter().find(|window| {
        if window.layer != 0 {
            return false;
        }
        if let Some(id) = selector.id {
            return window.id == id;
        }
        let owner_matches = selector
            .owner
            .as_deref()
            .is_some_and(|owner| names_match(&window.owner, owner));
        let title_matches = match selector.title.as_deref() {
            Some(title) => window.title.to_lowercase().contains(&title.to_lowercase()),
            None => true,
        };
        owner_matches && title_matches
    })
}

fn names_match(actual: &str, requested: &str) -> bool {
    let normalize = |value: &str| {
        value
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let actual = normalize(actual);
    let requested = normalize(requested);
    actual == requested || actual.contains(&requested) || requested.contains(&actual)
}

fn validate_selection(snapshot: &StoredSnapshot, selection: &Selection) -> Result<(), RenderError> {
    let within = |x: f64, y: f64| {
        x >= 0.0 && y >= 0.0 && x <= snapshot.width as f64 && y <= snapshot.height as f64
    };
    match selection {
        Selection::Point { x, y } if within(*x, *y) => Ok(()),
        Selection::Rect {
            x,
            y,
            width,
            height,
        } if *width > 0.0 && *height > 0.0 && within(*x, *y) && within(x + width, y + height) => {
            Ok(())
        }
        _ => Err(RenderError::InvalidSelection),
    }
}

#[cfg(target_os = "macos")]
async fn capture_window(bin: &str, target: &Target) -> Result<SnapshotDraft, RenderError> {
    use std::process::Stdio;
    use tokio::process::Command;
    use tokio::time::{timeout, Duration};

    let selector = selector_from_target(target)?;
    let windows = list_windows()?;
    let window = choose_window(&windows, &selector).ok_or_else(|| {
        let mut available = windows
            .iter()
            .filter(|window| window.layer == 0)
            .map(|window| window.owner.clone())
            .collect::<Vec<_>>();
        available.sort();
        available.dedup();
        available.truncate(12);
        let available = available.join(", ");
        RenderError::Capture(format!(
            "macOS window not found{}",
            if available.is_empty() {
                String::new()
            } else {
                format!("; visible windows: {available}")
            }
        ))
    })?;

    let temp = tempfile::tempdir()?;
    let output_path = temp.path().join("window.png");
    let output = timeout(
        Duration::from_secs(20),
        Command::new(bin)
            .arg("-x")
            .arg("-o")
            .arg("-t")
            .arg("png")
            .arg("-l")
            .arg(window.id.to_string())
            .arg(&output_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .map_err(|_| RenderError::Capture("macOS window capture timed out".into()))?
    .map_err(|error| RenderError::Capture(format!("spawn screencapture: {error}")))?;
    if !output.status.success() || !output_path.is_file() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(RenderError::Capture(format!(
            "screencapture failed; allow Screen & System Audio Recording for the MCP host in System Settings{}",
            if detail.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", detail.trim())
            }
        )));
    }

    let bytes = std::fs::read(&output_path)?;
    let image = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()?
        .decode()?;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 100_000_000 {
        return Err(RenderError::Capture(
            "invalid macOS window dimensions".into(),
        ));
    }
    let mut asset_bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut asset_bytes), ImageFormat::Png)?;

    let label = target.label.clone().unwrap_or_else(|| {
        if window.title.is_empty() {
            window.owner.clone()
        } else {
            format!("{} — {}", window.owner, window.title)
        }
    });
    Ok(SnapshotDraft {
        width,
        height,
        mime_type: "image/png".into(),
        asset_bytes,
        mapping: serde_json::json!({
            "version": 1,
            "renderer": "macos_window",
            "window_id": window.id,
            "owner": window.owner,
            "title": window.title,
            "capture_size": { "width": width, "height": height },
        }),
        source_label: label,
    })
}

#[cfg(not(target_os = "macos"))]
async fn capture_window(_bin: &str, _target: &Target) -> Result<SnapshotDraft, RenderError> {
    Err(RenderError::Capture(
        "macos_window is only available on macOS".into(),
    ))
}

#[cfg(target_os = "macos")]
fn list_windows() -> Result<Vec<WindowInfo>, RenderError> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::{CFString, CFStringRef};
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowLayer, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly, kCGWindowName, kCGWindowNumber, kCGWindowOwnerName,
    };

    fn value(dictionary: &CFDictionary<CFString, CFType>, key: CFStringRef) -> Option<CFType> {
        let key = unsafe { CFString::wrap_under_get_rule(key) };
        dictionary.find(&key).map(|value| (*value).clone())
    }
    fn string(dictionary: &CFDictionary<CFString, CFType>, key: CFStringRef) -> String {
        value(dictionary, key)
            .and_then(|value| value.downcast::<CFString>())
            .map(|value| value.to_string())
            .unwrap_or_default()
    }
    fn number(dictionary: &CFDictionary<CFString, CFType>, key: CFStringRef) -> Option<i64> {
        value(dictionary, key)
            .and_then(|value| value.downcast::<CFNumber>())
            .and_then(|value| value.to_i64())
    }

    let list = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )
    .ok_or_else(|| RenderError::Capture("could not read the macOS window list".into()))?;
    let mut windows = Vec::new();
    for pointer in list.get_all_values() {
        let dictionary = unsafe {
            CFDictionary::<CFString, CFType>::wrap_under_get_rule(pointer as CFDictionaryRef)
        };
        let Some(id) = number(&dictionary, unsafe { kCGWindowNumber })
            .and_then(|value| u32::try_from(value).ok())
        else {
            continue;
        };
        let layer = number(&dictionary, unsafe { kCGWindowLayer }).unwrap_or_default() as i32;
        let owner = string(&dictionary, unsafe { kCGWindowOwnerName });
        if owner.is_empty() {
            continue;
        }
        windows.push(WindowInfo {
            id,
            owner,
            title: string(&dictionary, unsafe { kCGWindowName }),
            layer,
        });
    }
    Ok(windows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_frontmost_matching_application_window() {
        let windows = vec![
            WindowInfo {
                id: 3,
                owner: "Other".into(),
                title: "Home".into(),
                layer: 0,
            },
            WindowInfo {
                id: 7,
                owner: "Retake".into(),
                title: "Retake".into(),
                layer: 0,
            },
        ];
        let selector = WindowSelector {
            id: None,
            owner: Some("Retake.app".into()),
            title: Some("take".into()),
        };
        assert_eq!(choose_window(&windows, &selector).unwrap().id, 7);
    }

    #[test]
    fn window_id_takes_precedence() {
        let windows = vec![WindowInfo {
            id: 42,
            owner: "Retake".into(),
            title: "Retake".into(),
            layer: 0,
        }];
        let selector = WindowSelector {
            id: Some(42),
            owner: Some("Other".into()),
            title: None,
        };
        assert_eq!(choose_window(&windows, &selector).unwrap().id, 42);
    }

    #[test]
    fn derives_application_name_from_bundle_path() {
        assert_eq!(application_name("/Applications/Retake.app"), "Retake");
        assert_eq!(application_name("OpenCode"), "OpenCode");
    }
}
