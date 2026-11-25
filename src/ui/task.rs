//! AR-13: tareas largas fuera del hilo de la UI. Un solo lugar para el ciclo «trabajo bloqueante, progreso
//! compartido y sondeo», que antes estaba copiado en seis vistas con intervalos y manejos de error distintos.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dioxus::prelude::*;

use crate::ui::i18n::Msg;

/// Cada cuánto se copia el progreso del trabajo a la señal de la vista.
pub const PROGRESS_POLL: Duration = Duration::from_millis(120);

/// Aviso de un trabajo que entró en pánico o se canceló: nunca se confunde con «sin resultados».
fn join_error_text(e: tokio::task::JoinError) -> Msg {
    let panic = e.is_panic();
    let detail = e.to_string();
    Msg::new(move |en| match (panic, en) {
        (true, false) => format!("Error interno inesperado: {detail}"),
        (true, true) => format!("Unexpected internal error: {detail}"),
        (false, false) => format!("La tarea se canceló: {detail}"),
        (false, true) => format!("The task was cancelled: {detail}"),
    })
}

/// Ejecuta `work` en un hilo bloqueante. `work` recibe el progreso compartido (que arranca con el valor actual de
/// `progress`, para no borrar lo que la vista ya muestra) y la señal se actualiza mientras corre y una vez más al
/// terminar. `Err` solo si el trabajo entró en pánico.
pub async fn run_with_progress<P, T, F>(mut progress: Signal<P>, work: F) -> Result<T, Msg>
where
    P: Clone + Send + 'static,
    T: Send + 'static,
    F: FnOnce(&Arc<Mutex<P>>) -> T + Send + 'static,
{
    let shared = Arc::new(Mutex::new(progress.peek().clone()));
    let worker = Arc::clone(&shared);
    let handle = tokio::task::spawn_blocking(move || work(&worker));

    let mut publish = |shared: &Arc<Mutex<P>>| {
        if let Ok(snap) = shared.lock() {
            progress.set(snap.clone());
        }
    };
    while !handle.is_finished() {
        publish(&shared);
        tokio::time::sleep(PROGRESS_POLL).await;
    }
    let res = handle.await.map_err(join_error_text);
    publish(&shared);
    res
}

/// Ejecuta `work` en un hilo bloqueante y convierte su error en un aviso para la vista. Un pánico también llega como
/// `Err` (antes algunas vistas lo convertían en «vacío»).
pub async fn run_blocking<T, E, F>(work: F) -> Result<T, Msg>
where
    T: Send + 'static,
    E: Into<Msg> + Send + 'static,
    F: FnOnce() -> Result<T, E> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(join_error_text)
        .and_then(|r| r.map_err(Into::into))
}
