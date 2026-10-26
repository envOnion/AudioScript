use tauri_nspanel::objc2_app_kit::{NSWindowCollectionBehavior, NSWindowStyleMask};
use tauri_nspanel::PanelHandle;

fn overlay_level() -> i64 {
    std::env::var(crate::overlay::ENV_OVERLAY_LEVEL)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000)
}

fn overlay_collection_behavior(current: NSWindowCollectionBehavior) -> NSWindowCollectionBehavior {
    let incompatible = NSWindowCollectionBehavior::MoveToActiveSpace
        | NSWindowCollectionBehavior::Managed
        | NSWindowCollectionBehavior::Transient
        | NSWindowCollectionBehavior::ParticipatesInCycle
        | NSWindowCollectionBehavior::Primary
        | NSWindowCollectionBehavior::Auxiliary
        | NSWindowCollectionBehavior::FullScreenPrimary
        | NSWindowCollectionBehavior::FullScreenNone;

    (current & !incompatible)
        | NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::IgnoresCycle
        | NSWindowCollectionBehavior::CanJoinAllApplications
        | NSWindowCollectionBehavior::FullScreenAuxiliary
}

pub fn apply_overlay_panel_attributes(panel: &PanelHandle<tauri::Wry>) {
    let ns_panel = panel.as_panel();

    let current_mask = ns_panel.styleMask();
    if let Err(e) = panel.set_style_mask(current_mask | NSWindowStyleMask::NonactivatingPanel) {
        tracing::error!("Не удалось установить NonactivatingPanel: {e:?}. Нажатие на окно субтитров прервёт полноэкранное воспроизведение");
    }

    let current_behavior = ns_panel.collectionBehavior();
    panel.set_collection_behavior(overlay_collection_behavior(current_behavior));

    panel.set_hides_on_deactivate(false);

    panel.set_level(overlay_level());

    if panel.is_visible() {
        panel.order_front_regardless();
    }

    tracing::debug!(
        level = overlay_level(),
        style_mask = ?current_mask,
        "Системные свойства окна субтитров применены"
    );
}

pub fn current_level(panel: &PanelHandle<tauri::Wry>) -> isize {
    panel.as_panel().level()
}

pub fn really_painted(panel: &PanelHandle<tauri::Wry>) -> bool {
    panel.as_panel().occlusionState().bits() & (1 << 1) != 0
}

pub fn on_active_space(panel: &PanelHandle<tauri::Wry>) -> bool {
    panel.as_panel().isOnActiveSpace()
}
