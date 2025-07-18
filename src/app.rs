use anyhow::Result;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use crate::{
    bluetooth::{Action, BTEvent, BTState},
    tray::TrayEvent,
};

#[derive(Debug)]
pub enum AppEvent {
    Request(Action),
    Response(BTState),
    Shutdown,
}

#[derive(Debug)]
pub struct App {
    state: BTState,
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
}
impl App {
    pub fn new() -> Self {
        let (tx, rx) = channel::<AppEvent>(32);
        let state = BTState::default();
        Self { tx, rx, state }
    }

    pub fn get_sender(&self) -> Sender<AppEvent> {
        self.tx.clone()
    }

    pub async fn run(&mut self, tray_tx: Sender<TrayEvent>, bt_tx: Sender<BTEvent>) -> Result<()> {
        while let Some(event) = self.rx.recv().await {
            match event {
                AppEvent::Request(action) => {
                    bt_tx
                        .send(BTEvent::Request {
                            action,
                            state: self.state.clone(),
                        })
                        .await?;
                }
                AppEvent::Response(state) => {
                    self.state = state.clone();
                    tray_tx.send(TrayEvent::Update(state)).await?;
                }
                AppEvent::Shutdown => break,
            }
        }

        Ok(())
    }
}
