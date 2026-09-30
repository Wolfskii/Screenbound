use std::panic::AssertUnwindSafe;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use super::Engine;
use crate::platform::PlatformEvent;

const CALL_TIMEOUT: Duration = Duration::from_secs(5);

type Call = Box<dyn FnOnce(&mut Engine) + Send>;

pub enum EngineMessage {
    Platform(PlatformEvent),
    Call(Call),
    Stop(Sender<()>),
}

/// Cloneable handle to the engine thread. All engine access is serialized through a channel,
/// so native event handling and UI commands never race.
#[derive(Clone)]
pub struct EngineHandle {
    tx: Sender<EngineMessage>,
}

impl EngineHandle {
    pub fn spawn(mut engine: Engine) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel::<EngineMessage>();
        std::thread::Builder::new()
            .name("screenbound-engine".into())
            .spawn(move || {
                crate::platform::init_thread();
                guarded(&mut engine, "startup", |e| e.startup());
                loop {
                    let msg = match engine.next_deadline() {
                        Some(deadline) => {
                            rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                        }
                        None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                    };
                    match msg {
                        Ok(EngineMessage::Platform(event)) => {
                            guarded(&mut engine, "event", |e| e.on_platform_event(event))
                        }
                        Ok(EngineMessage::Call(call)) => guarded(&mut engine, "command", call),
                        Ok(EngineMessage::Stop(done)) => {
                            guarded(&mut engine, "shutdown", |e| e.shutdown());
                            let _ = done.send(());
                            return;
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => {
                            guarded(&mut engine, "shutdown", |e| e.shutdown());
                            return;
                        }
                    }
                    guarded(&mut engine, "evaluation", |e| e.process_due(Instant::now()));
                }
            })?;
        Ok(Self { tx })
    }

    pub fn sender(&self) -> Sender<EngineMessage> {
        self.tx.clone()
    }

    /// Runs `f` on the engine thread and waits for its result.
    pub fn call<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Engine) -> R + Send + 'static,
    ) -> Result<R, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(EngineMessage::Call(Box::new(move |engine| {
                let _ = reply_tx.send(f(engine));
            })))
            .map_err(|_| "engine is not running".to_string())?;
        reply_rx
            .recv_timeout(CALL_TIMEOUT)
            .map_err(|_| "engine did not respond".to_string())
    }

    /// Releases all managed windows and stops the engine thread.
    pub fn stop(&self, timeout: Duration) {
        let (done_tx, done_rx) = mpsc::channel();
        if self.tx.send(EngineMessage::Stop(done_tx)).is_ok()
            && done_rx.recv_timeout(timeout).is_err()
        {
            tracing::warn!(
                "engine did not stop in time; managed windows will be recovered on next start"
            );
        }
    }
}

/// A bug handling one event must not kill the engine and strand windows in their zones.
fn guarded(engine: &mut Engine, what: &str, f: impl FnOnce(&mut Engine)) {
    if let Err(panic) = std::panic::catch_unwind(AssertUnwindSafe(|| f(engine))) {
        let msg = panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        tracing::error!("engine panicked during {what}: {msg}");
    }
}
