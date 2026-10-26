use std::sync::Mutex;

use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

use crate::config::OverlayMode;
use crate::session::{SessionState, TaskStatus};
use crate::{lifecycle, overlay};

pub const TRAY_ID: &str = "audioscript-tray";

mod id {
    pub const SHOW_MAIN: &str = "show_main";
    pub const TOGGLE_SESSION: &str = "toggle_session";
    pub const TOGGLE_OVERLAY: &str = "toggle_overlay";
    pub const OVERLAY_LOCKED: &str = "overlay_locked";
    pub const QUIT: &str = "quit";
}

pub struct TrayState {
    #[allow(dead_code)]
    icon: Mutex<tauri::tray::TrayIcon<tauri::Wry>>,
    locked_item: tauri::menu::CheckMenuItem<tauri::Wry>,
    session_item: MenuItem<tauri::Wry>,
    visibility_item: MenuItem<tauri::Wry>,
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let show_main = MenuItemBuilder::with_id(id::SHOW_MAIN, "Открыть главное окно").build(app)?;

    let capturing = app.state::<SessionState>().status() == TaskStatus::Capturing;
    let session_item =
        MenuItemBuilder::with_id(id::TOGGLE_SESSION, session_text(capturing)).build(app)?;

    let visibility_item = MenuItemBuilder::with_id(
        id::TOGGLE_OVERLAY,
        visibility_text(overlay::is_visible(app)),
    )
    .build(app)?;

    let locked_item =
        CheckMenuItemBuilder::with_id(id::OVERLAY_LOCKED, "Пропускать нажатия мыши (закрепить)")
            .checked(app.state::<overlay::OverlayState>().mode_is_click_through())
            .build(app)?;

    let quit = MenuItemBuilder::with_id(id::QUIT, "Выйти из AudioScript").build(app)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[
            &show_main,
            &session_item,
            &sep1,
            &visibility_item,
            &locked_item,
            &sep2,
            &quit,
        ])
        .build()?;

    let icon = TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray_icon(app)?)
        .icon_as_template(true)
        .tooltip("AudioScript")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_tray_event)
        .build(app)?;

    app.manage(TrayState {
        icon: Mutex::new(icon),
        locked_item,
        session_item,
        visibility_item,
    });

    Ok(())
}

fn session_text(capturing: bool) -> &'static str {
    if capturing {
        "Остановить распознавание"
    } else {
        "Начать распознавание"
    }
}

fn visibility_text(visible: bool) -> &'static str {
    if visible {
        "Скрыть окно субтитров"
    } else {
        "Показать окно субтитров"
    }
}

pub fn sync_mode(app: &AppHandle, mode: OverlayMode) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if let Err(e) = state.locked_item.set_checked(mode.is_click_through()) {
        tracing::error!("Не удалось обновить отметку в меню трея: {e}");
    }
}

pub fn sync_task_state(app: &AppHandle, status: TaskStatus) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if let Err(e) = state
        .session_item
        .set_text(session_text(status == TaskStatus::Capturing))
    {
        tracing::error!("Не удалось обновить пункт распознавания в меню трея: {e}");
    }
}

pub fn sync_visibility(app: &AppHandle, visible: bool) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    if let Err(e) = state.visibility_item.set_text(visibility_text(visible)) {
        tracing::error!("Не удалось обновить пункт субтитров в меню трея: {e}");
    }
}

fn tray_icon(app: &AppHandle) -> tauri::Result<tauri::image::Image<'static>> {
    const TEMPLATE: &[u8] = include_bytes!("../../icons/tray-template.png");

    if let Ok(img) = tauri::image::Image::from_bytes(TEMPLATE) {
        return Ok(img);
    }

    tracing::warn!("Не удалось загрузить значок трея. Используется значок приложения; в строке меню он может отображаться некорректно");
    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::AssetNotFound("Значок трея недоступен".into()))?;

    Ok(tauri::image::Image::new_owned(
        icon.rgba().to_vec(),
        icon.width(),
        icon.height(),
    ))
}

fn start_session(app: &AppHandle) {
    let p = crate::prefs::load(app);
    let args = serde_json::json!({
        "device": p.device_id,
        "source": p.audio_source,
        "config": {
            "translate": {
                "engine": p.translate_engine,
                "sourceLang": p.source_lang
            }
        }
    });

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match crate::commands::route_async(&app, "session_start", &args).await {
            Ok(_) => tracing::info!("Трей: распознавание запущено"),
            Err(e) => tracing::error!("Не удалось запустить распознавание из трея: {e}"),
        }
    });
}

fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        id::SHOW_MAIN => lifecycle::show_main(app),
        id::TOGGLE_SESSION => {
            if app.state::<SessionState>().status() == TaskStatus::Capturing {
                match crate::session::stop(app) {
                    Ok(()) => tracing::info!("Трей: запрошена остановка распознавания"),
                    Err(e) => tracing::error!("Не удалось остановить распознавание из трея: {e}"),
                }
            } else {
                start_session(app);
            }
        }
        id::TOGGLE_OVERLAY => {
            if let Err(e) = overlay::toggle(app) {
                tracing::error!("{e}");
            }
        }
        id::OVERLAY_LOCKED => {
            let next = if app
                .try_state::<TrayState>()
                .map(|s| s.locked_item.is_checked().unwrap_or(false))
                .unwrap_or(false)
            {
                OverlayMode::Locked
            } else {
                OverlayMode::Edit
            };
            if let Err(e) = overlay::set_mode(app, next) {
                tracing::error!("{e}");
                sync_mode(app, app.state::<overlay::OverlayState>().mode());
            }
        }
        id::QUIT => lifecycle::quit_app(app),
        other => tracing::debug!("Необработанный пункт меню трея: {other}"),
    }
}

fn on_tray_event(tray: &tauri::tray::TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        lifecycle::show_main(tray.app_handle());
    }
}
