use anyhow::Result;
use log::info;
use tokio::sync::mpsc::{Receiver, Sender};

use crate::tray::Device;

#[derive(Debug, Clone)]
pub enum Event {
    Update,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum Action {
    ToggleBluetooth,
    ToggleDevice(Device),
    Scan,
}

#[derive(Debug, Clone)]
pub struct App {
    event_tx: Sender<Event>,
}
impl App {
    pub fn new(event_tx: Sender<Event>) -> Self {
        Self { event_tx }
    }

    pub async fn send_event(&self, event: Event) -> Result<()> {
        self.event_tx.send(event).await?;
        Ok(())
    }

    pub async fn run(
        &self,
        mut event_rx: Receiver<Event>,
        mut action_rx: Receiver<Action>,
    ) -> anyhow::Result<()> {
        let _app = self.clone();
        tokio::spawn(async move {
            while let Some(action) = action_rx.recv().await {
                match action {
                    Action::ToggleBluetooth => todo!(),
                    Action::ToggleDevice(device) => todo!(),
                    Action::Scan => todo!(),
                }
            }
        });

        while let Some(event) = event_rx.recv().await {
            match event {
                Event::Update => {
                    info!("Updating tray");
                }
                Event::Shutdown => break,
            }
        }

        Ok(())
    }
}
