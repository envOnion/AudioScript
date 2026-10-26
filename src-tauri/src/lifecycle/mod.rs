use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, WindowEvent};

pub const MAIN_LABEL: &str = "main";

#[derive(Debug, Default)]
pub struct LifecycleState {
    quitting: AtomicBool,
}

impl LifecycleState {
    fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::SeqCst)
    }

    fn mark_quitting(&self) {
        self.quitting.store(true, Ordering::SeqCst);
    }
}

#[cfg(target_os = "macos")]
fn set_dock_visible(app: &AppHandle, visible: bool) {
    let policy = if visible {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    };
    if let Err(e) = app.set_activation_policy(policy) {
        tracing::error!(visible, "Не удалось изменить политику активации: {e}");
    }
}

pub fn show_main(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    set_dock_visible(app, true);

    let Some(window) = app.get_webview_window(MAIN_LABEL) else {
        tracing::error!("Главное окно не найдено (label={MAIN_LABEL})");
        return;
    };

    let _ = window.unminimize();
    if let Err(e) = window.show().and_then(|_| window.set_focus()) {
        tracing::error!("Не удалось открыть главное окно: {e}");
    }
}

pub fn quit_app(app: &AppHandle) {
    tracing::info!("Пользователь запросил выход (единственный путь завершения)");
    if let Some(state) = app.try_state::<LifecycleState>() {
        state.mark_quitting();
    }
    app.exit(0);
}

pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };

    let quitting = window
        .app_handle()
        .try_state::<LifecycleState>()
        .map(|s| s.is_quitting())
        .unwrap_or(false);

    if quitting {
        return;
    }

    api.prevent_close();

    if let Err(e) = window.hide() {
        tracing::error!(label = window.label(), "Не удалось скрыть окно: {e}");
    } else {
        #[cfg(target_os = "macos")]
        {
            if window.label() == MAIN_LABEL {
                set_dock_visible(window.app_handle(), false);
            }
        }
        tracing::debug!(label = window.label(), "Окно свёрнуто в трей");
    }
}

pub fn on_run_event(app: &AppHandle, event: &tauri::RunEvent) -> bool {
    if let tauri::RunEvent::ExitRequested { .. } = event {
        let quitting = app
            .try_state::<LifecycleState>()
            .map(|s| s.is_quitting())
            .unwrap_or(false);
        if !quitting {
            tracing::debug!("Запрос выхода перехвачен: приложение продолжает работать в фоне");
            return true;
        }
    }
    false
}
