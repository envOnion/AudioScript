use tauri::{
    menu::{
        AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID,
        WINDOW_SUBMENU_ID,
    },
    AppHandle,
};

const QUIT_ID: &str = "native-quit";
const REPO_ID: &str = "native-repo";

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let about_metadata = AboutMetadata {
        name: Some("AudioScript".into()),
        version: Some(app.package_info().version.to_string()),
        copyright: app.config().bundle.copyright.clone(),
        authors: Some(vec![
            "envOnion".into(),
            "niyongsheng (исходный проект)".into(),
        ]),
        ..Default::default()
    };
    // Выход должен пройти через lifecycle: обычный системный запрос перехватывается.
    let quit = MenuItem::with_id(
        app,
        QUIT_ID,
        "Выйти из AudioScript",
        true,
        Some("CmdOrControl+Q"),
    )?;
    let repo = MenuItem::with_id(app, REPO_ID, "AudioScript на GitHub", true, None::<&str>)?;
    let window_menu = Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        "Окно",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("Свернуть"))?,
            &PredefinedMenuItem::maximize(app, Some("Развернуть"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, Some("Закрыть окно"))?,
        ],
    )?;
    let help_menu = Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        "Справка",
        true,
        &[
            &repo,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(app, Some("Об AudioScript"), Some(about_metadata))?,
        ],
    )?;
    let menu = Menu::with_items(
        app,
        &[
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app,
                "AudioScript",
                true,
                &[
                    &PredefinedMenuItem::about(app, Some("Об AudioScript"), Some(about_metadata))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::services(app, Some("Службы"))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, Some("Скрыть AudioScript"))?,
                    &PredefinedMenuItem::hide_others(app, Some("Скрыть остальные"))?,
                    &PredefinedMenuItem::show_all(app, Some("Показать все"))?,
                    &PredefinedMenuItem::separator(app)?,
                    &quit,
                ],
            )?,
            &Submenu::with_items(
                app,
                "Файл",
                true,
                &[
                    &PredefinedMenuItem::close_window(app, Some("Закрыть окно"))?,
                    #[cfg(not(target_os = "macos"))]
                    &quit,
                ],
            )?,
            &Submenu::with_items(
                app,
                "Правка",
                true,
                &[
                    &PredefinedMenuItem::undo(app, Some("Отменить"))?,
                    &PredefinedMenuItem::redo(app, Some("Повторить"))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, Some("Вырезать"))?,
                    &PredefinedMenuItem::copy(app, Some("Скопировать"))?,
                    &PredefinedMenuItem::paste(app, Some("Вставить"))?,
                    &PredefinedMenuItem::select_all(app, Some("Выбрать всё"))?,
                ],
            )?,
            #[cfg(target_os = "macos")]
            &Submenu::with_items(
                app,
                "Вид",
                true,
                &[&PredefinedMenuItem::fullscreen(
                    app,
                    Some("Полноэкранный режим"),
                )?],
            )?,
            &window_menu,
            &help_menu,
        ],
    )?;
    #[cfg(target_os = "macos")]
    app.set_menu(menu)?;
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::Manager;
        if let Some(window) = app.get_webview_window("main") {
            window.set_menu(menu)?;
        }
    }
    app.on_menu_event(|app, event| match event.id().as_ref() {
        QUIT_ID => crate::lifecycle::quit_app(app),
        REPO_ID => {
            if let Err(e) = crate::commands::route(app, "open_repo", &serde_json::json!({})) {
                tracing::error!("Не удалось открыть страницу проекта из меню: {e}");
            }
        }
        _ => {}
    });
    Ok(())
}
