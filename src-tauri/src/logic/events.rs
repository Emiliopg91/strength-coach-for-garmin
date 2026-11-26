use std::sync::Arc;

use tauri_plugin_log::log::error;

use crate::dto::{activities::ActivityLocation, devices::DeviceListItem, prompts::PromptRequest};

/// Events the logic sends to the frontend. The command layer turns each one into its Tauri event
/// (see `logic::ui::event_sink`), named after the variant in snake case.
#[derive(Clone)]
pub enum BackendEvent {
    /// The Connect session changed: the logged user, `None` once logged out.
    ConnectLogged(Option<String>),
    /// A device of the Connect account was added or changed.
    DeviceDiscovered(DeviceListItem),
    /// A Garmin device was plugged in by USB.
    DeviceConnected(DeviceListItem),
    /// A Garmin device plugged in by USB was unplugged.
    DeviceDisconnected(DeviceListItem),
    /// A background sync started: the frontend shows the loading overlay.
    StartLoading,
    /// The background sync started by the last `StartLoading` ended.
    FinishLoading,
    /// A background sync imported new activities.
    ActivitiesAdded,
    /// The steps of these workouts were downloaded from a device.
    AddedWorkoutSteps(Vec<u16>),
    /// Whether any exercise has a progression suggestion pending.
    ProgressionsAvailable(bool),
    /// An activity was named after the place where it started.
    ActivityLocationUpdate(ActivityLocation),
    /// The logic waits for the input described by the request (see `answer_prompt`).
    PromptRequested(PromptRequest),
}

type SendFn = dyn Fn(BackendEvent) -> Result<(), String> + Send + Sync;

/// Sends [`BackendEvent`]s to the frontend through the function the command layer provides.
/// Clones share that function, so it can be moved into background tasks.
#[derive(Clone)]
pub struct EventSink(Arc<SendFn>);

impl EventSink {
    pub fn new(send: impl Fn(BackendEvent) -> Result<(), String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(send))
    }

    /// Sends `event`, only logging a failure.
    pub fn emit(&self, event: BackendEvent) {
        if let Err(e) = self.try_emit(event) {
            error!("Error emitting event: {e}");
        }
    }

    /// Sends `event`, failing if it could not be delivered.
    pub fn try_emit(&self, event: BackendEvent) -> Result<(), String> {
        (self.0)(event)
    }
}
