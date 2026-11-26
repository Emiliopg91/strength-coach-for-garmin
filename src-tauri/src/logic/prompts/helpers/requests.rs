use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
};

use futures_util::{FutureExt, future::BoxFuture};
use tauri_plugin_log::log::{error, info, warn};
use tokio::sync::oneshot;

use crate::{
    connect::ConnectPrompter,
    dto::prompts::{PromptKind, PromptRequest},
    logic::events::{BackendEvent, EventSink},
};

pub(in crate::logic::prompts) type Answer = Option<Vec<String>>;

/// Prompts sent to the frontend that are still waiting for an answer, by id. Clones share them.
#[derive(Default, Clone)]
pub(crate) struct Prompts {
    next_id: Arc<AtomicU32>,
    pub(in crate::logic::prompts) pending: Arc<Mutex<HashMap<u32, oneshot::Sender<Answer>>>>,
}

/// Asks the frontend, through `events`, for the input described by `kind` and waits for the
/// answer. Returns `None` if the user cancels or the prompt could not be shown.
pub(crate) async fn request_prompt(
    prompts: &Prompts,
    events: &EventSink,
    kind: PromptKind,
    prefill: Option<String>,
    detail: Option<String>,
) -> Answer {
    let id = prompts.next_id.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    prompts.pending.lock().unwrap().insert(id, tx);

    info!("Requesting {:?} prompt #{}", kind, id);
    let request = PromptRequest {
        id,
        kind,
        prefill,
        detail,
    };
    if let Err(e) = events.try_emit(BackendEvent::PromptRequested(request)) {
        error!("Error emitting prompt #{}: {}", id, e);
        prompts.pending.lock().unwrap().remove(&id);
        return None;
    }

    let answer = rx.await.ok().flatten();
    if answer.is_none() {
        warn!("Prompt #{} cancelled by the user", id);
    }
    answer
}

/// Asks for the Garmin Connect inputs through the frontend dialogs.
pub(crate) struct AppPrompter {
    prompts: Prompts,
    events: EventSink,
}

impl AppPrompter {
    pub(crate) fn new(prompts: Prompts, events: EventSink) -> Self {
        Self { prompts, events }
    }
}

impl ConnectPrompter for AppPrompter {
    fn credentials(
        &self,
        email: Option<String>,
        error: Option<String>,
    ) -> BoxFuture<'static, Option<(String, String)>> {
        let (prompts, events) = (self.prompts.clone(), self.events.clone());
        async move {
            let mut values = request_prompt(
                &prompts,
                &events,
                PromptKind::ConnectCredentials,
                email,
                error,
            )
            .await?
            .into_iter();
            Some((values.next()?, values.next()?))
        }
        .boxed()
    }

    fn mfa_code(&self, method: String) -> BoxFuture<'static, Option<String>> {
        let (prompts, events) = (self.prompts.clone(), self.events.clone());
        async move {
            request_prompt(
                &prompts,
                &events,
                PromptKind::ConnectMfa,
                None,
                Some(method),
            )
            .await?
            .into_iter()
            .next()
            .map(|code| code.trim().to_string())
            .filter(|code| !code.is_empty())
        }
        .boxed()
    }
}
