use std::sync::LazyLock;

use anyhow::Result;
use image::GenericImageView;
use ksni::{
    MenuItem,
    menu::{CheckmarkItem, StandardItem},
};
use log::error;
use tokio::sync::mpsc::Sender;

use crate::{APP_ID, app::Action};

#[derive(Clone, Debug)]
pub struct Device {
    name: String,
    on: bool,
}
pub struct Tray {
    action_tx: Sender<Action>,
    on: bool,
    devices: Vec<Device>,
}

impl Tray {
    pub fn new(action_tx: Sender<Action>) -> Tray {
        Tray {
            on: false,
            devices: Vec::new(),
            action_tx,
        }
    }

    fn send_action(&self, action: Action) -> Result<()> {
        let handle = tokio::runtime::Handle::current();

        let tx = self.action_tx.clone();
        handle.spawn(async move {
            if let Err(e) = tx.send(action).await {
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

        if self.on {
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
                label: "On".to_string(),
                checked: self.on,
                activate: Box::new(|this: &mut Self| {
                    this.send_action(Action::ToggleBluetooth).unwrap();
                }),
                ..Default::default()
            }
            .into(),
        );

        menu.push(MenuItem::Separator);

        menu.push(
            StandardItem {
                label: "Scan for Devices".to_string(),
                activate: Box::new(|this: &mut Self| {
                    this.send_action(Action::Scan).unwrap();
                }),
                ..Default::default()
            }
            .into(),
        );

        menu.push(MenuItem::Separator);

        for device in &self.devices {
            let local_device = device.clone();
            menu.push(
                CheckmarkItem {
                    label: device.name.clone(),
                    checked: device.on,
                    activate: Box::new(move |this: &mut Self| {
                        this.send_action(Action::ToggleDevice(local_device.clone()))
                            .unwrap();
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }

        if self.devices.is_empty() {
            menu.push(
                StandardItem {
                    label: "No devices found".to_string(),
                    enabled: false,
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
