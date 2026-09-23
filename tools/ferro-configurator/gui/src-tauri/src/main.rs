#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

//! The desktop shell.
//!
//! Deliberately thin: every command locks the session, hands the work to
//! [`ferro_configurator_bridge`], and returns. No decision about safety is
//! taken here, so the disarmed gate cannot be bypassed by a front-end bug or
//! by calling these commands directly.
//!
//! Serial I/O blocks, so each command runs its work on the blocking pool
//! rather than the UI thread. A full blackbox download takes minutes, and
//! holding the webview thread for that long would freeze the window.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use ferro_configurator_bridge::{
    BridgeError, DEFAULT_TIMEOUT, Safety, Session, ports as discover,
};
use ferro_configurator_core::{
    DeviceSelector, FerroConfig, FlightCatalog, FlightSelector, PortInfo, SerialTransport,
};
use serde::Serialize;
use tauri::{Emitter, Manager, State};

type Shared = Arc<Mutex<Session<SerialTransport>>>;

struct AppState {
    session: Shared,
}

/// What a finished download reports back.
#[derive(Debug, Clone, Serialize)]
struct DownloadReport {
    pages: u32,
    bytes: u64,
    path: String,
}

/// One page of a download in flight.
#[derive(Debug, Clone, Serialize)]
struct DownloadProgress {
    page: u32,
    total: u32,
}

/// Runs blocking session work off the UI thread.
///
/// A panic inside the closure would poison the mutex, so it is reported as a
/// device error rather than propagated: a poisoned session should ask the
/// operator to reconnect, not take the window down.
async fn blocking<T, F>(session: &Shared, work: F) -> Result<T, BridgeError>
where
    T: Send + 'static,
    F: FnOnce(&mut Session<SerialTransport>) -> Result<T, BridgeError> + Send + 'static,
{
    let session = Arc::clone(session);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = session.lock().map_err(|_| BridgeError::Device {
            message: "the session failed and must be reconnected".to_owned(),
        })?;
        work(&mut guard)
    })
    .await
    .map_err(|error| BridgeError::Device {
        message: format!("the session task failed: {error}"),
    })?
}

#[tauri::command]
async fn list_ports() -> Result<Vec<PortInfo>, BridgeError> {
    tauri::async_runtime::spawn_blocking(discover)
        .await
        .map_err(|error| BridgeError::Device {
            message: format!("could not enumerate ports: {error}"),
        })?
}

#[tauri::command]
async fn connect(state: State<'_, AppState>, port: Option<String>) -> Result<String, BridgeError> {
    let selector = match port {
        Some(port) if !port.is_empty() => DeviceSelector::Port(port),
        _ => DeviceSelector::Auto,
    };
    blocking(&state.session, move |session| {
        session.connect(selector, DEFAULT_TIMEOUT)
    })
    .await
}

#[tauri::command]
async fn disconnect(state: State<'_, AppState>) -> Result<(), BridgeError> {
    blocking(&state.session, |session| {
        session.disconnect();
        Ok(())
    })
    .await
}

#[tauri::command]
async fn safety(state: State<'_, AppState>) -> Result<Safety, BridgeError> {
    blocking(&state.session, Session::safety).await
}

#[tauri::command]
async fn read_config(state: State<'_, AppState>) -> Result<FerroConfig, BridgeError> {
    blocking(&state.session, Session::read_config).await
}

/// Refused by the session unless the controller reports itself disarmed.
#[tauri::command]
async fn apply_config(
    state: State<'_, AppState>,
    config: FerroConfig,
) -> Result<FerroConfig, BridgeError> {
    blocking(&state.session, move |session| {
        session.apply_config(&config)
    })
    .await
}

#[tauri::command]
async fn flights(state: State<'_, AppState>) -> Result<FlightCatalog, BridgeError> {
    blocking(&state.session, Session::flights).await
}

#[tauri::command]
async fn download_flight(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    flight: String,
    output: String,
) -> Result<DownloadReport, BridgeError> {
    let selector = if flight == "latest" {
        FlightSelector::Latest
    } else {
        FlightSelector::Id(flight.parse().map_err(|_| BridgeError::Device {
            message: format!("`{flight}` is not a flight id"),
        })?)
    };
    let path = PathBuf::from(&output);
    blocking(&state.session, move |session| {
        let summary = session.download_flight(selector, &path, false, |page, total| {
            // Best effort: a dropped progress event must not fail a download
            // that is otherwise succeeding.
            let _ = app.emit("download-progress", DownloadProgress { page, total });
        })?;
        Ok(DownloadReport {
            pages: summary.pages,
            bytes: summary.bytes,
            path: output,
        })
    })
    .await
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState {
                session: Arc::new(Mutex::new(Session::new())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_ports,
            connect,
            disconnect,
            safety,
            read_config,
            apply_config,
            flights,
            download_flight,
        ])
        .run(tauri::generate_context!())
        .expect("the FerroConfigurator window failed to start");
}
