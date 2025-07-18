use anyhow::Result;
use bluer::Address;
use tokio::sync::mpsc::{Sender, channel};

use crate::app::AppEvent;

#[derive(Debug)]
pub enum Action {
    Init,
    ToggleBluetooth,
    ToggleDevice(BTDevice),
    Scan,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BTDeviceStatus {
    Paired,
    Pairing,
    Connected,
    Connecting,
    Disconnected,
    Disconnecting,
}

#[derive(Debug, Clone)]
pub struct BTDevice {
    pub name: String,
    pub address: Address,
    pub status: BTDeviceStatus,
    pub is_paired: bool,
    pub is_trusted: bool,
    pub battery_percentage: Option<u8>,
}

impl BTDevice {
    pub fn is_on(&self) -> bool {
        self.status == BTDeviceStatus::Connected
    }
}

#[derive(Debug, Clone, Default)]
pub struct BTState {
    pub on: bool,
    pub devices: Vec<BTDevice>,
}

pub async fn init_bluetooth(app_tx: Sender<AppEvent>) -> Result<Sender<Action>> {
    let (tx, mut rx) = channel::<Action>(32);

    tokio::spawn(async move {
        while let Some(action) = rx.recv().await {
            match action {
                Action::Init => todo!(),
                Action::ToggleBluetooth => todo!(),
                Action::ToggleDevice(device) => todo!(),
                Action::Scan => todo!(),
            }
        }
    });

    Ok(tx)
}
