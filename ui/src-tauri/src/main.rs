// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    match retake_desktop_lib::launch_mode(&std::env::args().skip(1).collect::<Vec<_>>()) {
        retake_desktop_lib::LaunchMode::Mcp => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap_or_else(|error| {
                    eprintln!("retake: could not start MCP runtime: {error}");
                    std::process::exit(1);
                });
            if let Err(error) = runtime.block_on(retake_server::run_mcp()) {
                eprintln!("retake: MCP server failed: {error:#}");
                std::process::exit(1);
            }
        }
        retake_desktop_lib::LaunchMode::Desktop | retake_desktop_lib::LaunchMode::ReviewWindow => {
            retake_desktop_lib::run()
        }
    }
}
