use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct ReviewSessionCancellation {
    cancelled: watch::Sender<bool>,
}

impl Default for ReviewSessionCancellation {
    fn default() -> Self {
        let (cancelled, _) = watch::channel(false);
        Self { cancelled }
    }
}

impl ReviewSessionCancellation {
    pub fn cancel(&self) {
        self.cancelled.send_replace(true);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        *self.cancelled.borrow()
    }

    pub(crate) async fn cancelled(&self) {
        let mut cancelled = self.cancelled.subscribe();
        while !*cancelled.borrow_and_update() {
            if cancelled.changed().await.is_err() {
                break;
            }
        }
    }
}
