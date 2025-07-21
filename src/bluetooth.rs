use std::{cmp::Ordering, time::Duration};

use anyhow::Result;
use bluer::{Adapter, Address, Session};
use futures::{FutureExt, StreamExt, stream::FuturesUnordered};
use log::error;
use tokio::{
    process::Command,
    sync::mpsc::{Sender, channel},
};

use crate::app::AppEvent;

const STATE_CHANGED_FAILED_RETRY_MS: u64 = 5_000;

#[derive(Debug)]
pub enum Action {
    ToggleBluetooth,
    ToggleDevice(BTDevice),
}

#[derive(Debug)]
pub enum BTEvent {
    Init(BTState),
    Request { action: Action, state: BTState },
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

    pub async fn from_device(device: &bluer::Device) -> Self {
        let (mut name, is_paired, is_trusted, is_connected, battery_percentage) = futures::join!(
            device.name().map(|res| res
                .ok()
                .flatten()
                .unwrap_or_else(|| device.address().to_string())),
            device.is_paired().map(Result::unwrap_or_default),
            device.is_trusted().map(Result::unwrap_or_default),
            device.is_connected().map(Result::unwrap_or_default),
            device.battery_percentage().map(|res| res.ok().flatten()),
        );

        if name.is_empty() {
            name = device.address().to_string();
        };

        let status = if is_connected {
            BTDeviceStatus::Connected
        } else if is_paired {
            BTDeviceStatus::Paired
        } else {
            BTDeviceStatus::Disconnected
        };

        Self {
            name,
            address: device.address(),
            status,
            battery_percentage,
            is_paired,
            is_trusted,
        }
    }
}

impl Eq for BTDevice {}

impl Ord for BTDevice {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.status.cmp(&other.status) {
            Ordering::Equal => self.name.to_lowercase().cmp(&other.name.to_lowercase()),
            ordering => ordering,
        }
    }
}

impl PartialOrd for BTDevice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for BTDevice {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.address == other.address
    }
}

#[derive(Debug, Clone, Default)]
pub struct BTState {
    pub on: bool,
    pub devices: Vec<BTDevice>,
}

async fn toggle_bluetooth(adapter: &Adapter, on: bool) {
    //FROM: https://github.com/pop-os/cosmic-applets/blob/c539b0628be7ea66feb3840fdca60c9e59bf3c75/cosmic-applet-bluetooth/src/bluetooth.rs#L678-L710
    if let Err(e) = adapter.set_powered(!on).await {
        error!(
            "Failed to power {} bluetooth adapter. {e:?}",
            if on { "off" } else { "on" },
        );
    }

    // rfkill will be persisted after reboot
    let name = adapter.name();
    let device_id = Command::new("rfkill")
        .arg("list")
        .arg("-n")
        .arg("--output")
        .arg("ID,DEVICE")
        .output()
        .await
        .ok()
        .and_then(|o| {
            // Output looks like this:
            // 0 acer-wireless
            // 1 acer-bluetooth
            // 2 hci0
            // 3 phy0
            //
            // The adapter names are the same as the device names on the second column.
            // So we need to find the name of the deafault adapter on the second column and
            // return the ID of the adapter.
            let lines = String::from_utf8(o.stdout).ok()?;
            lines.split("\n").find_map(|row| {
                let (id, cname) = row.trim().split_once(" ")?;
                (name == cname).then_some(id.to_string())
            })
        });

    if let Some(id) = device_id {
        if let Err(e) = Command::new("rfkill")
            .arg(if on { "block" } else { "unblock" })
            .arg(id)
            .output()
            .await
        {
            error!("Failed to set bluetooth state using rfkill. {e:?}");
        };
    }

    tokio::time::sleep(Duration::from_millis(100)).await;
}

async fn toggle_device(adapter: &Adapter, address: &Address, on: bool) {
    let device = match adapter.device(*address) {
        Ok(device) => device,
        Err(e) => {
            error!("Failed to get bluetooth device. {e:?}");
            return;
        }
    };
    let res = if on {
        device.disconnect().await
    } else {
        device.connect().await
    };

    if let Err(e) = res {
        error!("Failed to set bluetooth device state. {e:?}");
    }
}

async fn listen_for_device_changes(app_tx: Sender<AppEvent>, adapter: Adapter) {
    let mut count = 0;
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut stream = loop {
        if let Ok(stream) = adapter.discover_devices_with_changes().await {
            break stream;
        };

        interval.tick().await;

        if count > 10 {
            return;
        }

        count += 1;
    };

    while (stream.next().await).is_some() {
        if let Ok(state) = build_state(&adapter).await {
            let _ = app_tx.send(AppEvent::Response(state)).await;
        }
    }
}

pub async fn init_bluetooth(app_tx: Sender<AppEvent>) -> Result<Sender<BTEvent>> {
    let (tx, mut rx) = channel::<BTEvent>(32);

    // FROM: https://github.com/pop-os/cosmic-applets/blob/c171f048a6dff1a032eb5edf8f343cac60971ac5/cosmic-applet-bluetooth/src/bluetooth.rs#L82,L97
    //
    // ChatGPT says this code is attempting to establish a session with retry logic, using exponential backoff.
    // - 2_u64 is just the literal integer (i32) 2, but instucts the compiler treat it as a u64.
    //   saturating_pow is implemented for u64 and not i32. the rust compiler would assume the
    //   integer was i32 if it was not explicitly specified.
    // - u32 is the same as u64 but is a smaller integer type.
    // - each iteration of the loop adds 1 to the retry_count, meaning the result of 2^retry_count
    //   increases exponentially in each iteration.
    // - 2^retry_count (exponential growth)
    // - they use .saturating_pow(u32) instead of .pow(u32) to avoid overflowing. it returns the
    //   max or min bound when the result is too large to fit in the return type.
    // - 68_719_476_734 is the time in milliseconds. This is actially 2.18 years.
    // - the original code chained `.max(68719476734)` on to the result of the pow call, but
    //   ChatGPT (which I also agree after looking at the docs) says that it's possibly a
    //   mistake/bug. `.max(68719476734)` compares the result of the pow call to 68719476734,
    //   and returns the larger of the two. Meaning, on the first iteration, the result of the
    //   pow call will be 2, which is less than 68719476734, so 68719476734 will be returned.
    //   Therefore, the loop will wait for 2 years before retrying!
    //   I have created a [PR](https://github.com/pop-os/cosmic-applets/issues/997) to fix this.
    // - ChatGPT suggest that `.min(68719476734)` should be used instead of `.max(68719476734)`.
    //   This will enable the exponential backoff to work correctly as it will exponentially
    //   increase from 2 to 68719476734.

    let mut retry_count = 0u32;

    // Initialize connection.
    let session = loop {
        if let Ok(session) = Session::new().await {
            break session;
        }

        // will run up to retry_count = 16 which 65,536 milliseconds which is roughly 1.1 seconds.
        if retry_count >= 16 {
            anyhow::bail!("Failed to connect to Bluetooth session");
        }

        retry_count = retry_count.saturating_add(1);
        _ = tokio::time::sleep(Duration::from_millis(
            2_u64.saturating_pow(retry_count).min(65_536),
        ))
        .await;
    };

    let adapter = session.default_adapter().await?;
    let state = build_state(&adapter).await?;

    tx.send(BTEvent::Init(state)).await?;

    tokio::spawn(listen_for_device_changes(app_tx.clone(), adapter.clone()));

    tokio::spawn(async move {
        while let Some(action) = rx.recv().await {
            match action {
                BTEvent::Init(btstate) => {
                    if let Err(e) = app_tx.send(AppEvent::Response(btstate)).await {
                        error!("Failed to send BTState to AppEvent::Response: {e}");
                    };
                }
                BTEvent::Request { action, state } => {
                    match action {
                        Action::ToggleBluetooth => {
                            toggle_bluetooth(&adapter, state.on).await;

                            // There's a significant delay when turning off the adapter. Borrowing some ideas from GNOME's
                            // bluetooth applet.
                            // FROM: https://github.com/GNOME/gnome-shell/blob/4272916830120c0ff858e9b9de5d242a04932632/js/ui/status/bluetooth.js#L123-L140
                            let app_tx = app_tx.clone();
                            let local_adapter = adapter.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(Duration::from_millis(
                                    STATE_CHANGED_FAILED_RETRY_MS,
                                ))
                                .await;

                                if let Ok(state) = build_state(&local_adapter).await {
                                    let _ = app_tx.send(AppEvent::Response(state)).await;
                                }
                            });
                        }
                        Action::ToggleDevice(device) => {
                            toggle_device(&adapter, &device.address, device.is_on()).await
                        }
                    }

                    if let Ok(state) = build_state(&adapter).await {
                        let _ = app_tx.send(AppEvent::Response(state)).await;
                    }
                }
            }
        }
    });

    Ok(tx)
}

async fn build_state(adapter: &Adapter) -> Result<BTState> {
    let on = adapter.is_powered().await?;
    let addresses = adapter.device_addresses().await.unwrap_or_default();

    let mut devices = Vec::with_capacity(addresses.len());

    let mut device_stream = addresses
        .into_iter()
        .filter_map(|address| adapter.device(address).ok())
        .map(async |device| BTDevice::from_device(&device).await)
        .collect::<FuturesUnordered<_>>();

    while let Some(device) = device_stream.next().await {
        devices.push(device)
    }

    devices.sort();

    Ok(BTState { on, devices })
}
