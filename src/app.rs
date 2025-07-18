use anyhow::Result;
use tokio::sync::mpsc::{Receiver, Sender, channel};

use crate::{
    bluetooth::{Action, BTState},
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
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
}
impl App {
    pub fn new() -> Self {
        let (tx, rx) = channel::<AppEvent>(32);
        Self { tx, rx }
    }

    pub fn get_sender(&self) -> Sender<AppEvent> {
        self.tx.clone()
    }

    pub async fn run(&mut self, tray_tx: Sender<TrayEvent>, bt_tx: Sender<Action>) -> Result<()> {
        while let Some(event) = self.rx.recv().await {
            match event {
                AppEvent::Request(action) => {
                    bt_tx.send(action).await?;
                }
                AppEvent::Response(state) => {
                    tray_tx.send(TrayEvent::Update(state)).await?;
                }
                AppEvent::Shutdown => break,
            }
        }

        Ok(())
    }
}
