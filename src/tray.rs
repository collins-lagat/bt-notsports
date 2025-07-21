use std::sync::LazyLock;

use anyhow::Result;
use image::GenericImageView;
use ksni::{
    MenuItem, TrayMethods,
    menu::{CheckmarkItem, StandardItem, SubMenu},
};
use log::error;
use tokio::sync::mpsc::{Sender, channel};

use crate::{
    APP_ID,
    app::AppEvent,
    bluetooth::{Action, BTState},
};

#[derive(Debug)]
pub enum TrayEvent {
    Update(BTState),
}

#[derive(Debug)]
pub struct Tray {
    app_tx: Sender<AppEvent>,
    state: BTState,
}

impl Tray {
    pub fn new(app_tx: Sender<AppEvent>) -> Tray {
        Tray {
            app_tx,
            state: BTState::default(),
        }
    }

    pub fn update(&mut self, state: BTState) {
        self.state = state;
    }

    fn send_action(&self, action: Action) -> Result<()> {
        let handle = tokio::runtime::Handle::current();

        let tx = self.app_tx.clone();
        handle.spawn(async move {
            if let Err(e) = tx.send(AppEvent::Request(action)).await {
                error!("Tray: Failed to send action: {}", e);
            }
        });
        Ok(())
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        APP_ID.to_string()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let mut icons = Vec::with_capacity(1);

        static ON_ICON: LazyLock<ksni::Icon> =
            LazyLock::new(|| get_icon_from_image_bytes(include_bytes!("../assets/on.png")));

        static OFF_ICON: LazyLock<ksni::Icon> =
            LazyLock::new(|| get_icon_from_image_bytes(include_bytes!("../assets/off.png")));

        if self.state.on {
            icons.push(ON_ICON.clone());
        } else {
            icons.push(OFF_ICON.clone());
        }

        icons
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut menu = vec![];

        menu.push(
            CheckmarkItem {
                label: "Bluetooth".to_string(),
                checked: self.state.on,
                activate: Box::new(|this: &mut Self| {
                    this.send_action(Action::ToggleBluetooth).unwrap();
                }),
                ..Default::default()
            }
            .into(),
        );

        menu.push(MenuItem::Separator);

        let mut device_submenu = Vec::<MenuItem<Tray>>::with_capacity(self.state.devices.len());

        device_submenu.push(
            StandardItem {
                label: "Devices".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        );

        device_submenu.push(MenuItem::Separator);

        for device in &self.state.devices {
            let local_device = device.clone();
            device_submenu.push(
                CheckmarkItem {
                    label: device.name.clone(),
                    checked: device.is_on(),
                    activate: Box::new(move |this: &mut Self| {
                        this.send_action(Action::ToggleDevice(local_device.clone()))
                            .unwrap();
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }

        if self.state.devices.is_empty() {
            menu.push(
                StandardItem {
                    label: "No devices found".to_string(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        } else {
            menu.push(
                SubMenu {
                    label: "Devices".to_string(),
                    submenu: device_submenu,
                    ..Default::default()
                }
                .into(),
            );
        }

        menu
    }
}

fn get_icon_from_image_bytes(image_bytes: &[u8]) -> ksni::Icon {
    let img = image::load_from_memory_with_format(image_bytes, image::ImageFormat::Png)
        .expect("valid image");
    let (width, height) = img.dimensions();
    let mut data = img.into_rgba8().into_vec();
    assert_eq!(data.len() % 4, 0);
    for pixel in data.chunks_exact_mut(4) {
        pixel.rotate_right(1) // rgba to argb
    }
    ksni::Icon {
        width: width as i32,
        height: height as i32,
        data,
    }
}

pub async fn init_tray(app_tx: Sender<AppEvent>) -> Result<Sender<TrayEvent>> {
    let tray = Tray::new(app_tx);
    let handle = match tray.spawn().await {
        Ok(handle) => handle,
        Err(e) => {
            anyhow::bail!("Failed to spawn tray: {}", e);
        }
    };

    let (tx, mut rx) = channel::<TrayEvent>(32);

    let tokio_handle = tokio::runtime::Handle::current();
    tokio_handle.spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                TrayEvent::Update(state) => {
                    handle
                        .update(|tray| {
                            tray.update(state);
                        })
                        .await;
                }
            };
        }
    });

    Ok(tx)
}
