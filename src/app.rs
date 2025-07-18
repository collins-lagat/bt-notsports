#[derive(Debug, Clone)]
pub enum Event {
    Update,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum Action {
    ToggleBluetooth,
    ToggleDevice,
    Scan,
}
