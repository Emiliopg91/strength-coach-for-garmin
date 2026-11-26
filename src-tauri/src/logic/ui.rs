//! Tauri side of the command layer: events, windows and dialogs. Only the commands (and the app
//! setup) use it; the logic gets what it needs from them as plain values and functions.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_log::log::{error, info};
use tokio::sync::oneshot;

use crate::{
    SettingsLock,
    dto::{activities::ActivityLocation, devices::DeviceListItem, prompts::PromptRequest},
    logic::{
        ensure_extension,
        events::{BackendEvent, EventSink},
        settings_language,
    },
    utils::translations::translate,
};

/// Sink that emits each [`BackendEvent`] to the frontend as its Tauri event. Every event is
/// emitted only here, with its payload bound to a typed local: the TypeScript generator reads the
/// payload type from that binding and generates one listener per event.
pub(crate) fn event_sink<R: Runtime>(app: &AppHandle<R>) -> EventSink {
    let app = app.clone();
    EventSink::new(move |event| {
        let res = match event {
            BackendEvent::ConnectLogged(user) => {
                let connect_user: Option<String> = user;
                app.emit("connect_logged", connect_user)
            }
            BackendEvent::DeviceDiscovered(device) => {
                let discovered: DeviceListItem = device;
                app.emit("device_discovered", discovered)
            }
            BackendEvent::DeviceConnected(device) => {
                let connected: DeviceListItem = device;
                app.emit("device_connected", connected)
            }
            BackendEvent::DeviceDisconnected(device) => {
                let disconnected: DeviceListItem = device;
                app.emit("device_disconnected", disconnected)
            }
            BackendEvent::StartLoading => app.emit("start_loading", ()),
            BackendEvent::FinishLoading => app.emit("finish_loading", ()),
            BackendEvent::ActivitiesAdded => app.emit("activities_added", ()),
            BackendEvent::AddedWorkoutSteps(workouts) => {
                let workout_ids: Vec<u16> = workouts;
                app.emit("added_workout_steps", workout_ids)
            }
            BackendEvent::ProgressionsAvailable(any) => {
                let any_progression: bool = any;
                app.emit("progressions_available", any_progression)
            }
            BackendEvent::ActivityLocationUpdate(location) => {
                let activity_location: ActivityLocation = location;
                app.emit("activity_location_update", activity_location)
            }
            BackendEvent::PromptRequested(request) => {
                // The dialog lives in the main window, which may be minimized. A recreated window
                // would not be listening yet, so only an existing one is shown
                if app.get_webview_window("main").is_some() {
                    let _ = show_main_window(&app);
                }
                let prompt_request: PromptRequest = request;
                app.emit("prompt_requested", prompt_request)
            }
        };
        res.map_err(|e| e.to_string())
    })
}

/// Shows, unminimizes and focuses the main window, creating it if it was closed.
pub(crate) fn show_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("main") {
        w.show()?;
        w.unminimize()?;
        w.set_focus()?;

        return Ok(());
    }

    open_window(app, "main", None)?.set_focus()?;

    Ok(())
}

/// Opens the window `label` declared in `tauri.conf.json`, with `query` appended to its URL if
/// given.
///
/// # Panics
/// If `tauri.conf.json` declares no window `label`.
pub(crate) fn open_window<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    query: Option<&str>,
) -> tauri::Result<WebviewWindow<R>> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == label)
        .unwrap_or_else(|| panic!("no '{label}' window in tauri.conf.json"))
        .clone();

    if let Some(query) = query {
        config.url = tauri::WebviewUrl::App(format!("{}?{query}", config.url).into());
    }

    WebviewWindowBuilder::from_config(app, &config)?.build()
}

/// Asks where to save a file, starting at `default_file` and offering only files with
/// `extension`, which is appended if missing. Returns `None` if the user cancels.
pub(crate) async fn ask_save_path<R: Runtime>(
    app: &AppHandle<R>,
    default_file: &Path,
    filter_name: &str,
    extension: &str,
    title: &str,
) -> Result<Option<PathBuf>, String> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title(title)
        .add_filter(filter_name, &[extension]);
    if let Some(dir) = default_file.parent() {
        dialog = dialog.set_directory(dir);
    }
    if let Some(name) = default_file.file_name() {
        dialog = dialog.set_file_name(name.to_string_lossy());
    }

    let (tx, rx) = oneshot::channel();
    dialog.save_file(move |path| {
        let _ = tx.send(path);
    });

    match rx.await.map_err(|e| e.to_string())? {
        Some(path) => Ok(Some(ensure_extension(
            path.into_path().map_err(|e| e.to_string())?,
            extension,
        ))),
        None => Ok(None),
    }
}

/// Asks for one or more existing files with `extension`. Empty if the user cancels.
pub(crate) async fn ask_open_paths<R: Runtime>(
    app: &AppHandle<R>,
    filter_name: &str,
    extension: &str,
    title: &str,
) -> Result<Vec<PathBuf>, String> {
    let (tx, rx) = oneshot::channel();
    app.dialog()
        .file()
        .set_title(title)
        .add_filter(filter_name, &[extension])
        .pick_files(move |paths| {
            let _ = tx.send(paths);
        });

    rx.await
        .map_err(|e| e.to_string())?
        .unwrap_or_default()
        .into_iter()
        .map(|path| path.into_path().map_err(|e| e.to_string()))
        .collect()
}

/// Asks where to save `w` as a PDF, starting at `<downloads>/<default_name>.pdf`, prints it there
/// and opens it. The window is closed once printing ends, on cancel or on error.
pub(crate) async fn print_to_pdf<R: Runtime>(
    w: WebviewWindow<R>,
    default_name: &str,
) -> Result<(), String> {
    let res = start_pdf_print(w.clone(), default_name).await;
    // On success the window is closed once the print operation ends
    if res.is_err() {
        let _ = w.close();
    }
    res
}

async fn start_pdf_print<R: Runtime>(
    w: WebviewWindow<R>,
    default_name: &str,
) -> Result<(), String> {
    use webkit2gtk::{PrintOperation, PrintOperationExt};

    let lang = settings_language(&w.state::<SettingsLock>());
    let default_file = w
        .path()
        .download_dir()
        .map_err(|e| e.to_string())?
        .join(format!("{default_name}.pdf"));

    let Some(file) = ask_save_path(
        w.app_handle(),
        &default_file,
        "PDF",
        "pdf",
        &translate("select_file_save_pdf", lang),
    )
    .await?
    else {
        // Cancelled by the user
        let _ = w.close();
        return Ok(());
    };

    let uri = format!("file://{}", file.display());
    w.clone()
        .with_webview(move |pw| {
            let settings = gtk::PrintSettings::new();
            // GTK registers its file backend under a localized name
            settings.set_printer(&gtk::glib::dgettext(Some("gtk30"), "Print to File"));
            settings.set(gtk::PRINT_SETTINGS_OUTPUT_FILE_FORMAT, Some("pdf"));
            settings.set(gtk::PRINT_SETTINGS_OUTPUT_URI, Some(&uri));

            let page_setup = gtk::PageSetup::new();
            page_setup.set_paper_size(&gtk::PaperSize::new(Some(gtk::PAPER_NAME_A4)));
            page_setup.set_top_margin(15.0, gtk::Unit::Mm);
            page_setup.set_bottom_margin(15.0, gtk::Unit::Mm);
            page_setup.set_left_margin(15.0, gtk::Unit::Mm);
            page_setup.set_right_margin(15.0, gtk::Unit::Mm);

            let op = PrintOperation::new(&pw.inner());
            op.set_print_settings(&settings);
            op.set_page_setup(&page_setup);
            op.connect_failed(move |_, e| {
                error!("PDF print failed: {e}");
            });
            // Emitted after `failed` too, so the hidden window is always released here
            op.connect_finished(move |_| {
                info!("PDF printed to {uri}");
                // Runs on the GTK main thread, so the viewer is launched without waiting for it
                let uri = uri.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = tokio::process::Command::new("xdg-open")
                        .arg(&uri)
                        .status()
                        .await;
                });
                let _ = w.close();
            });
            op.print();
        })
        .map_err(|e| e.to_string())
}
