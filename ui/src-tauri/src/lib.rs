mod installer;

use serde::Deserialize;
use std::io::{BufRead, Write};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchMode {
    Desktop,
    Mcp,
    ReviewWindow,
}

pub fn launch_mode(args: &[String]) -> LaunchMode {
    if args.iter().any(|argument| argument == "--review-window") {
        return LaunchMode::ReviewWindow;
    }
    match args.first().map(String::as_str) {
        Some("mcp" | "--mcp") => LaunchMode::Mcp,
        _ => LaunchMode::Desktop,
    }
}

#[derive(Deserialize)]
struct OpenReviewRequest {
    url: String,
}

fn parse_review_url(value: &str) -> Result<tauri::Url, String> {
    let url = value
        .parse::<tauri::Url>()
        .map_err(|error| format!("invalid review URL: {error}"))?;
    let review_id = url.path().strip_prefix("/review/").unwrap_or_default();
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || review_id.is_empty()
        || !review_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err("expected a local Retake review URL".into());
    }
    Ok(url)
}

fn review_url_from_stdin() -> Result<tauri::Url, String> {
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|error| format!("could not read review URL: {error}"))?;
    let request: OpenReviewRequest =
        serde_json::from_str(&line).map_err(|error| format!("invalid review request: {error}"))?;
    parse_review_url(&request.url)
}

fn listen_for_review_commands(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            match line.as_deref() {
                Ok("focus") => {
                    if let Some(window) = app.get_webview_window("review") {
                        let _ = window.unminimize();
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                Ok("close") | Err(_) => {
                    app.exit(0);
                    return;
                }
                Ok(_) => {}
            }
        }
        app.exit(0);
    });
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let review_mode =
        launch_mode(&std::env::args().skip(1).collect::<Vec<_>>()) == LaunchMode::ReviewWindow;
    let review_url = if review_mode {
        Some(review_url_from_stdin().unwrap_or_else(|error| {
            eprintln!("retake desktop: {error}");
            std::process::exit(2);
        }))
    } else {
        None
    };

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            installer::installer_status,
            installer::install_clients,
            installer::remove_clients,
            quit_app
        ])
        .setup(move |app| {
            let (label, webview_url, width, height) = match review_url.clone() {
                Some(url) => ("review", WebviewUrl::External(url), 1280.0, 820.0),
                None => ("main", WebviewUrl::default(), 820.0, 680.0),
            };
            WebviewWindowBuilder::new(app, label, webview_url)
                .title("Retake")
                .inner_size(width, height)
                .min_inner_size(640.0, 420.0)
                .build()?;

            if review_mode {
                listen_for_review_commands(app.handle().clone());
                println!("ready");
                std::io::stdout().flush()?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Retake");
}

#[cfg(test)]
mod tests {
    use super::{LaunchMode, launch_mode, parse_review_url};

    #[test]
    fn dispatches_mcp_without_entering_tauri_and_preserves_review_mode() {
        assert_eq!(launch_mode(&["mcp".into()]), LaunchMode::Mcp);
        assert_eq!(launch_mode(&["--mcp".into()]), LaunchMode::Mcp);
        assert_eq!(
            launch_mode(&["--other-arg".into(), "--review-window".into()]),
            LaunchMode::ReviewWindow
        );
        assert_eq!(launch_mode(&[]), LaunchMode::Desktop);
    }

    #[test]
    fn accepts_tokenized_loopback_review_urls() {
        let url = parse_review_url(
            "http://127.0.0.1:49152/review/77c42613-27bb-4239-a423-2cde3ed9734b#token=secret",
        )
        .unwrap();
        assert_eq!(url.port(), Some(49152));
    }

    #[test]
    fn rejects_other_hosts_and_paths() {
        assert!(parse_review_url("http://localhost:49152/review/abc").is_err());
        assert!(parse_review_url("https://127.0.0.1/review/abc").is_err());
        assert!(parse_review_url("http://127.0.0.1:49152/api/reviews/abc").is_err());
        assert!(parse_review_url("http://127.0.0.1:49152/review/abc/extra").is_err());
    }
}
