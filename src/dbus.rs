//! D-Bus proxies for every service Nusa talks to.

use crate::*;

#[proxy(interface = "org.mpris.MediaPlayer2.Player", default_path = "/org/mpris/MediaPlayer2")]
pub(crate) trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    fn set_position(&self, track_id: &ObjectPath<'_>, position: i64) -> zbus::Result<()>;
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedVariant>>;
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn position(&self) -> zbus::Result<i64>;
}

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
pub(crate) trait Battery {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
pub(crate) trait Network {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(interface = "org.freedesktop.NetworkManager.Connection.Active", default_service = "org.freedesktop.NetworkManager")]
pub(crate) trait ActiveConnection {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<String>;
}

#[proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
pub(crate) trait BtDevice {
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn icon(&self) -> zbus::Result<String>;
}

#[proxy(interface = "org.bluez.Battery1", default_service = "org.bluez")]
pub(crate) trait BtBattery {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<u8>;
}

#[proxy(interface = "org.kde.klipper.klipper", default_service = "org.kde.klipper", default_path = "/klipper")]
// KDE's D-Bus APIs use camelCase names; zbus would default to PascalCase, so name them explicitly.
pub(crate) trait Klipper {
    #[zbus(name = "getClipboardHistoryMenu")]
    fn get_clipboard_history_menu(&self) -> zbus::Result<Vec<String>>;
    #[zbus(name = "setClipboardContents")]
    fn set_clipboard_contents(&self, s: &str) -> zbus::Result<()>;
    #[zbus(signal, name = "clipboardHistoryUpdated")]
    fn clipboard_history_updated(&self) -> zbus::Result<()>;
}

#[proxy(interface = "org.kde.kdeconnect.device.notifications.notification", default_service = "org.kde.kdeconnect")]
pub(crate) trait PhoneNotification {
    #[zbus(property, name = "appName")]
    fn app_name(&self) -> zbus::Result<String>;
    #[zbus(property, name = "title")]
    fn title(&self) -> zbus::Result<String>;
    #[zbus(property, name = "text")]
    fn text(&self) -> zbus::Result<String>;
    #[zbus(property, name = "silent")]
    fn silent(&self) -> zbus::Result<bool>;
}

#[proxy(interface = "org.kde.PlasmaShell", default_service = "org.kde.plasmashell", default_path = "/PlasmaShell")]
pub(crate) trait PlasmaShell {
    #[zbus(name = "wallpaper")]
    fn wallpaper(&self, screen: u32) -> zbus::Result<HashMap<String, OwnedVariant>>;
    #[zbus(signal, name = "wallpaperChanged")]
    fn wallpaper_changed(&self, screen: u32) -> zbus::Result<()>;
}
