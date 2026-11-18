mod hotkey;
mod http_client;
mod platform;

use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::azure::{self, region_id, region_index, AZURE_REGIONS};
use crate::config::{self, AppConfig};
use crate::deepl;
use crate::hotkey_spec::{CaptureFail, HotkeySpec};
use crate::language::LanguageId;
use crate::provider::ProviderKind;
use crate::router::{self, Credentials, TranslateOptions, Translation};
use crate::secrets::{self, Secrets};
use crate::ui_text::{
    self, connection_error, detected_label, format_grouped, popup_for_failure, popup_loading,
    popup_not_text, provider_note, remaining_ratio, AZURE_STATUS_PREFIX, BUSY_NOTE, COPIED_LABEL,
    COPY_FAILED, COPY_LABEL, DEEPL_STATUS_PREFIX, HOTKEY_HINT, HOTKEY_NEEDS_MODIFIER,
    HOTKEY_UNSUPPORTED, TESTING,
};
use crate::usage::{self, azure_remaining, deepl_remaining, AZURE_MONTHLY_LIMIT};

slint::include_modules!();

struct AppState {
    dir: std::path::PathBuf,
    config: AppConfig,
    secrets: Secrets,
    http: http_client::ReqwestClient,
    hotkey: hotkey::HotkeyService,
    generation: u64,
    last_source: String,
    last_explicit: Option<LanguageId>,
    current_target: Option<LanguageId>,
    busy: bool,
    tested_deepl: Option<(String, deepl::DeeplUsage)>,
    tested_azure: Option<String>,
}

pub fn run() -> Result<(), String> {
    platform::set_app_id();
    let dir = platform::app_dir();
    platform::init_log(&dir);
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        platform::log(&format!("panic: {info}"));
        default_hook(info);
    }));

    match platform::claim_single_instance() {
        Ok(true) => {}
        Ok(false) => {
            platform::log("이미 실행 중이라 기존 인스턴스에 설정을 열라고 알렸습니다.");
            return Ok(());
        }
        Err(err) => {
            platform::error_box(&err);
            return Err(err);
        }
    }

    let config_path = config::config_path(&dir);
    let first_run = !config_path.exists();
    let config = match config::load_config(&config_path) {
        Ok(config) => config,
        Err(err) => {
            platform::log(&format!("설정을 읽지 못했습니다: {err}"));
            let broken = config_path.with_extension("toml.broken");
            let _ = std::fs::rename(&config_path, &broken);
            AppConfig::default()
        }
    };
    let secrets =
        secrets::load_file(&config::secrets_path(&dir), &platform::Dpapi).unwrap_or_else(|err| {
            platform::log(&format!("저장된 키를 열지 못했습니다: {err}"));
            Secrets::default()
        });
    if let Err(err) = platform::set_autostart(config.autostart) {
        platform::log(&format!("자동 실행 경로를 갱신하지 못했습니다: {err}"));
    }

    let http = http_client::ReqwestClient::new()?;
    let hotkey = hotkey::HotkeyService::start(config.hotkey_spec())?;
    let state = Arc::new(Mutex::new(AppState {
        dir,
        config,
        secrets,
        http,
        hotkey,
        generation: 0,
        last_source: String::new(),
        last_explicit: None,
        current_target: None,
        busy: false,
        tested_deepl: None,
        tested_azure: None,
    }));

    let tray = AppTray::new().map_err(|err| err.to_string())?;
    let settings = SettingsWindow::new().map_err(|err| err.to_string())?;
    let translate = TranslateWindow::new().map_err(|err| err.to_string())?;
    install_models(&settings, &translate);
    let dark = platform::system_is_dark();
    settings.invoke_apply_theme(dark);
    translate.invoke_apply_theme(dark);
    let compact = state
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .config
        .hotkey_spec()
        .display_compact();
    tray.set_hotkey_compact(compact.as_str().into());
    tray.set_hotkey_title(hotkey_menu_title(&compact).into());

    wire_windows(&state, &tray, &settings, &translate);
    tray.show().map_err(|err| err.to_string())?;
    if first_run {
        platform::log("처음 실행이라 설정 창을 엽니다.");
        open_settings(&state, &settings);
    }

    let (activate_tx, activate_rx) = mpsc::channel();
    platform::spawn_activation_watcher(activate_tx);
    let poll = slint::Timer::default();
    {
        let state = Arc::clone(&state);
        let settings = settings.as_weak();
        let translate = translate.as_weak();
        let mut last_dark = dark;
        let mut last_check = Instant::now();
        poll.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(50),
            move || {
                let pressed = state
                    .lock()
                    .unwrap_or_else(|err| err.into_inner())
                    .hotkey
                    .drain_presses();
                if pressed {
                    if let Some(translate) = translate.upgrade() {
                        translate_clipboard(&state, &translate, None);
                    }
                }
                if activate_rx.try_recv().is_ok() {
                    if let Some(settings) = settings.upgrade() {
                        open_settings(&state, &settings);
                    }
                }
                if last_check.elapsed() >= Duration::from_secs(3) {
                    last_check = Instant::now();
                    let dark = platform::system_is_dark();
                    if dark != last_dark {
                        last_dark = dark;
                        if let Some(settings) = settings.upgrade() {
                            settings.invoke_apply_theme(dark);
                        }
                        if let Some(translate) = translate.upgrade() {
                            translate.invoke_apply_theme(dark);
                        }
                    }
                }
            },
        );
    }

    slint::run_event_loop().map_err(|err| err.to_string())?;
    if let Ok(mut state) = state.lock() {
        state.hotkey.stop();
    }
    Ok(())
}

fn install_models(settings: &SettingsWindow, translate: &TranslateWindow) {
    let languages = string_model(LanguageId::ALL.map(LanguageId::endonym));
    settings.set_language_labels(languages.clone());
    translate.set_language_labels(languages);
    settings.set_region_labels(string_model(
        AZURE_REGIONS.iter().map(|region| region.label),
    ));
    settings.set_deepl_status_prefix(DEEPL_STATUS_PREFIX.into());
    settings.set_azure_status_prefix(AZURE_STATUS_PREFIX.into());
}

fn hotkey_menu_title(compact: &str) -> String {
    format!("단축키\t{compact}")
}

fn string_model<I, S>(items: I) -> ModelRc<SharedString>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    ModelRc::new(VecModel::from(
        items
            .into_iter()
            .map(|item| SharedString::from(item.as_ref()))
            .collect::<Vec<_>>(),
    ))
}

fn wire_windows(
    state: &Arc<Mutex<AppState>>,
    tray: &AppTray,
    settings: &SettingsWindow,
    translate: &TranslateWindow,
) {
    let state_tray = Arc::clone(state);
    let settings_weak = settings.as_weak();
    tray.on_open_settings(move || {
        if let Some(settings) = settings_weak.upgrade() {
            open_settings(&state_tray, &settings);
        }
    });

    let state_open = Arc::clone(state);
    let translate_weak = translate.as_weak();
    tray.on_open_translate(move || {
        if let Some(translate) = translate_weak.upgrade() {
            translate_clipboard(&state_open, &translate, None);
        }
    });

    let state_quit = Arc::clone(state);
    tray.on_quit(move || {
        if let Ok(mut state) = state_quit.lock() {
            state.hotkey.stop();
        }
        let _ = slint::quit_event_loop();
    });

    let state_cancel = Arc::clone(state);
    let settings_weak = settings.as_weak();
    settings.on_cancel(move || {
        if let Some(settings) = settings_weak.upgrade() {
            close_settings(&state_cancel, &settings);
        }
    });
    let state_close = Arc::clone(state);
    settings.window().on_close_requested(move || {
        if let Ok(state) = state_close.lock() {
            state.hotkey.resume();
        }
        slint::CloseRequestResponse::HideWindow
    });

    let state_save = Arc::clone(state);
    let settings_weak = settings.as_weak();
    let tray_weak = tray.as_weak();
    settings.on_save(move || {
        if let Some(settings) = settings_weak.upgrade() {
            save_settings(&state_save, &settings, &tray_weak);
        }
    });

    let state_hint = Arc::clone(state);
    let settings_weak = settings.as_weak();
    settings.on_deepl_edited(move || {
        if let Some(settings) = settings_weak.upgrade() {
            let saved = state_hint
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .secrets
                .deepl_key
                .clone();
            if settings.get_deepl_key().as_str().trim() != saved.trim() {
                settings.set_deepl_connected(false);
                settings.set_deepl_message(SharedString::from(""));
            }
            refresh_deepl_hint(&settings);
        }
    });
    let state_azure = Arc::clone(state);
    let settings_weak = settings.as_weak();
    settings.on_azure_edited(move || {
        if let Some(settings) = settings_weak.upgrade() {
            let saved = state_azure
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .secrets
                .azure_key
                .clone();
            if settings.get_azure_key().as_str().trim() != saved.trim() {
                settings.set_azure_connected(false);
                settings.set_azure_message(SharedString::from(""));
            }
        }
    });
    let settings_weak = settings.as_weak();
    settings.on_provider_changed(move || {
        if let Some(settings) = settings_weak.upgrade() {
            refresh_deepl_hint(&settings);
        }
    });

    let state_test = Arc::clone(state);
    let settings_weak = settings.as_weak();
    settings.on_test_deepl(move || {
        if let Some(settings) = settings_weak.upgrade() {
            test_deepl(&state_test, &settings);
        }
    });
    let state_test = Arc::clone(state);
    let settings_weak = settings.as_weak();
    settings.on_test_azure(move || {
        if let Some(settings) = settings_weak.upgrade() {
            test_azure(&state_test, &settings);
        }
    });

    let state_hotkey = Arc::clone(state);
    let settings_weak = settings.as_weak();
    settings.on_hotkey_captured(move |key, ctrl, alt, shift, win| {
        if let Some(settings) = settings_weak.upgrade() {
            capture_hotkey(
                &state_hotkey,
                &settings,
                key.as_str(),
                ctrl,
                alt,
                shift,
                win,
            );
        }
    });
    let state_rec = Arc::clone(state);
    let settings_weak = settings.as_weak();
    settings.on_hotkey_recording(move |active| {
        if let Some(settings) = settings_weak.upgrade() {
            let hint = if active {
                ui_text::HOTKEY_RECORDING_HINT
            } else {
                HOTKEY_HINT
            };
            settings.set_hotkey_hint(hint.into());
        }
        let state = state_rec.lock().unwrap_or_else(|err| err.into_inner());
        if active {
            state.hotkey.pause();
        } else {
            state.hotkey.resume();
        }
    });

    let state_target = Arc::clone(state);
    let translate_weak = translate.as_weak();
    translate.on_target_changed(move |value| {
        let Some(translate) = translate_weak.upgrade() else {
            return;
        };
        let Some(language) = LanguageId::from_endonym(value.as_str()) else {
            return;
        };
        let source = {
            let state = state_target.lock().unwrap_or_else(|err| err.into_inner());
            if state.busy || state.current_target == Some(language) {
                return;
            }
            state.last_source.clone()
        };
        if source.is_empty() {
            return;
        }
        translate_text(&state_target, &translate, source, Some(language));
    });

    let translate_weak = translate.as_weak();
    translate.on_copy_clicked(move || {
        if let Some(translate) = translate_weak.upgrade() {
            copy_translation(&translate);
        }
    });

    let state_retry = Arc::clone(state);
    let translate_weak = translate.as_weak();
    translate.on_retry(move || {
        if let Some(translate) = translate_weak.upgrade() {
            let (source, explicit) = {
                let state = state_retry.lock().unwrap_or_else(|err| err.into_inner());
                (state.last_source.clone(), state.last_explicit)
            };
            if source.is_empty() {
                translate_clipboard(&state_retry, &translate, explicit);
            } else {
                translate_text(&state_retry, &translate, source, explicit);
            }
        }
    });

    let translate_weak = translate.as_weak();
    translate.on_close_popup(move || {
        if let Some(translate) = translate_weak.upgrade() {
            let _ = translate.hide();
        }
    });
    translate
        .window()
        .on_close_requested(|| slint::CloseRequestResponse::HideWindow);

    let state_from_popup = Arc::clone(state);
    let settings_weak = settings.as_weak();
    translate.on_open_settings(move || {
        if let Some(settings) = settings_weak.upgrade() {
            open_settings(&state_from_popup, &settings);
        }
    });
}

fn open_settings(state: &Arc<Mutex<AppState>>, settings: &SettingsWindow) {
    {
        let mut state = state.lock().unwrap_or_else(|err| err.into_inner());
        state.tested_deepl = None;
        state.tested_azure = None;
        fill_settings(settings, &state);
    }
    let already = settings.window().is_visible();
    let _ = settings.show();
    present(
        settings.window(),
        !already,
        460.0,
        680.0,
        platform::FrameStyle::Settings,
    );
    if !already {
        let settings = settings.as_weak();
        slint::Timer::single_shot(Duration::from_millis(40), move || {
            if let Some(settings) = settings.upgrade() {
                present(
                    settings.window(),
                    true,
                    460.0,
                    680.0,
                    platform::FrameStyle::Settings,
                );
            }
        });
    }
}

fn fill_settings(settings: &SettingsWindow, state: &AppState) {
    let config = &state.config;
    settings.set_provider_index(config.provider.index());
    settings.set_deepl_key(state.secrets.deepl_key.as_str().into());
    settings.set_azure_key(state.secrets.azure_key.as_str().into());
    settings.set_region_index(region_index(&config.azure_region) as i32);
    settings.set_fallback(config.fallback_to_azure);
    settings.set_hotkey_id(config.hotkey.as_str().into());
    settings.set_hotkey_label(config.hotkey_spec().display_spaced().into());
    settings.set_hotkey_hint(HOTKEY_HINT.into());
    settings.set_recording(false);
    settings.set_lang1_index(config.language1().index());
    settings.set_lang2_index(config.language2().index());
    settings.set_autostart(config.autostart);
    settings.set_save_error(SharedString::from(""));
    settings.set_deepl_testing(false);
    settings.set_azure_testing(false);
    if config.deepl_usage_known {
        show_deepl_usage(
            settings,
            deepl_remaining(config),
            config.deepl_character_limit,
        );
    } else {
        settings.set_deepl_connected(false);
        settings.set_deepl_message(SharedString::from(""));
    }
    if config.azure_connected {
        let month = platform::local_year_month();
        show_azure_usage(settings, azure_remaining(config, &month));
    } else {
        settings.set_azure_connected(false);
        settings.set_azure_message(SharedString::from(""));
    }
    refresh_deepl_hint(settings);
}

fn close_settings(state: &Arc<Mutex<AppState>>, settings: &SettingsWindow) {
    settings.set_recording(false);
    state
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .hotkey
        .resume();
    let _ = settings.hide();
}

fn save_settings(
    state: &Arc<Mutex<AppState>>,
    settings: &SettingsWindow,
    tray: &slint::Weak<AppTray>,
) {
    settings.set_save_error(SharedString::from(""));
    let draft_hotkey = settings.get_hotkey_id().to_string();
    let Some(spec) = HotkeySpec::parse(&draft_hotkey) else {
        settings.set_save_error(HOTKEY_UNSUPPORTED.into());
        return;
    };
    let mut next_config;
    let next_secrets;
    let previous_hotkey;
    {
        let state = state.lock().unwrap_or_else(|err| err.into_inner());
        next_config = state.config.clone();
        next_secrets = Secrets {
            deepl_key: settings.get_deepl_key().as_str().trim().to_string(),
            azure_key: settings.get_azure_key().as_str().trim().to_string(),
        };
        previous_hotkey = state.config.hotkey_spec();
        next_config.provider = ProviderKind::from_index(settings.get_provider_index());
        next_config.azure_region = region_id(settings.get_region_index()).to_string();
        next_config.fallback_to_azure = settings.get_fallback();
        next_config.hotkey = spec.serialize();
        next_config.language1 = LanguageId::from_index(settings.get_lang1_index())
            .unwrap_or(LanguageId::Ko)
            .id()
            .to_string();
        next_config.language2 = LanguageId::from_index(settings.get_lang2_index())
            .unwrap_or(LanguageId::Ja)
            .id()
            .to_string();
        next_config.autostart = settings.get_autostart();
        if next_secrets.deepl_key != state.secrets.deepl_key {
            if let Some((key, usage)) = &state.tested_deepl {
                if key == &next_secrets.deepl_key {
                    usage::set_deepl_usage(
                        &mut next_config,
                        usage.character_count,
                        usage.character_limit,
                    );
                } else {
                    next_config.deepl_usage_known = false;
                    next_config.deepl_character_count = 0;
                }
            } else {
                next_config.deepl_usage_known = false;
                next_config.deepl_character_count = 0;
            }
        } else if let Some((key, usage)) = &state.tested_deepl {
            if key == &next_secrets.deepl_key {
                usage::set_deepl_usage(
                    &mut next_config,
                    usage.character_count,
                    usage.character_limit,
                );
            }
        }
        if next_secrets.azure_key != state.secrets.azure_key {
            next_config.azure_usage_month = platform::local_year_month();
            next_config.azure_usage_chars = 0;
            next_config.azure_connected =
                state.tested_azure.as_deref() == Some(next_secrets.azure_key.as_str());
        } else if state.tested_azure.as_deref() == Some(next_secrets.azure_key.as_str()) {
            next_config.azure_connected = true;
        }
        if next_secrets.azure_key.is_empty() {
            next_config.azure_connected = false;
        }
    }

    // Always register, including when the field was left in recording mode
    // (the previous combo is unregistered until resume or set).
    if let Err(err) = state
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .hotkey
        .set(spec)
    {
        settings.set_save_error(err.into());
        return;
    }
    if let Err(err) = platform::set_autostart(next_config.autostart) {
        let _ = state
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .hotkey
            .set(previous_hotkey);
        settings.set_save_error(format!("자동 실행을 바꾸지 못했습니다: {err}").into());
        return;
    }
    let config_path = config::config_path(&state.lock().unwrap_or_else(|err| err.into_inner()).dir);
    let secrets_path =
        config::secrets_path(&state.lock().unwrap_or_else(|err| err.into_inner()).dir);
    if let Err(err) = config::save_config(&config_path, &next_config) {
        settings.set_save_error(format!("설정을 저장하지 못했습니다: {err}").into());
        return;
    }
    if let Err(err) = secrets::save_file(&secrets_path, &platform::Dpapi, &next_secrets) {
        settings.set_save_error(format!("키를 저장하지 못했습니다: {err}").into());
        return;
    }
    {
        let mut state = state.lock().unwrap_or_else(|err| err.into_inner());
        state.config = next_config;
        state.secrets = next_secrets;
        state.tested_deepl = None;
        state.tested_azure = None;
    }
    if let Some(tray) = tray.upgrade() {
        let compact = spec.display_compact();
        tray.set_hotkey_compact(compact.as_str().into());
        tray.set_hotkey_title(hotkey_menu_title(&compact).into());
    }
    let _ = settings.hide();
    platform::log("설정을 저장했습니다.");
}

fn capture_hotkey(
    state: &Arc<Mutex<AppState>>,
    settings: &SettingsWindow,
    key: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
) {
    match HotkeySpec::capture(ctrl, alt, shift, win, key) {
        Ok(spec) => {
            settings.set_hotkey_id(spec.serialize().into());
            settings.set_hotkey_label(spec.display_spaced().into());
            settings.set_hotkey_hint(HOTKEY_HINT.into());
            settings.set_recording(false);
            state
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .hotkey
                .resume();
        }
        Err(CaptureFail::Ignore) => {}
        Err(CaptureFail::Cancelled) => {
            settings.set_recording(false);
            settings.set_hotkey_hint(HOTKEY_HINT.into());
            state
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .hotkey
                .resume();
        }
        Err(CaptureFail::NeedsModifier) => {
            settings.set_hotkey_hint(HOTKEY_NEEDS_MODIFIER.into());
        }
        Err(CaptureFail::Unsupported) => {
            settings.set_hotkey_hint(HOTKEY_UNSUPPORTED.into());
        }
    }
}

fn test_deepl(state: &Arc<Mutex<AppState>>, settings: &SettingsWindow) {
    let key = settings.get_deepl_key().as_str().trim().to_string();
    if key.is_empty() {
        settings.set_deepl_connected(false);
        settings.set_deepl_message("키를 입력한 뒤 연결 테스트를 눌러 주세요.".into());
        refresh_deepl_hint(settings);
        return;
    }
    settings.set_deepl_testing(true);
    settings.set_deepl_connected(false);
    settings.set_deepl_message(TESTING.into());
    let http = state
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .http
        .clone();
    let state = Arc::clone(state);
    let settings = settings.as_weak();
    std::thread::spawn(move || {
        let result = deepl::usage(&http, &key);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(settings) = settings.upgrade() else {
                return;
            };
            settings.set_deepl_testing(false);
            match result {
                Ok(usage) => {
                    let remaining = usage.character_limit.saturating_sub(usage.character_count);
                    show_deepl_usage(&settings, remaining, usage.character_limit);
                    settings.set_deepl_message(SharedString::from(""));
                    state
                        .lock()
                        .unwrap_or_else(|err| err.into_inner())
                        .tested_deepl = Some((key, usage));
                }
                Err(err) => {
                    platform::log(&format!("DeepL 연결 테스트 실패: {err}"));
                    settings.set_deepl_connected(false);
                    settings.set_deepl_message(connection_error(&err).into());
                }
            }
            refresh_deepl_hint(&settings);
        });
    });
}

fn test_azure(state: &Arc<Mutex<AppState>>, settings: &SettingsWindow) {
    let key = settings.get_azure_key().as_str().trim().to_string();
    let region = region_id(settings.get_region_index()).to_string();
    if key.is_empty() {
        settings.set_azure_connected(false);
        settings.set_azure_message("키를 입력한 뒤 연결 테스트를 눌러 주세요.".into());
        return;
    }
    settings.set_azure_testing(true);
    settings.set_azure_connected(false);
    settings.set_azure_message(TESTING.into());
    let http = state
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .http
        .clone();
    let saved_key = state
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .secrets
        .azure_key
        .clone();
    let remaining_now = {
        let state = state.lock().unwrap_or_else(|err| err.into_inner());
        azure_remaining(&state.config, &platform::local_year_month())
    };
    let state = Arc::clone(state);
    let settings = settings.as_weak();
    std::thread::spawn(move || {
        let result = azure::detect(&http, &key, &region);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(settings) = settings.upgrade() else {
                return;
            };
            settings.set_azure_testing(false);
            match result {
                Ok(()) => {
                    let remaining = if key == saved_key {
                        remaining_now
                    } else {
                        AZURE_MONTHLY_LIMIT
                    };
                    show_azure_usage(&settings, remaining);
                    settings.set_azure_message(SharedString::from(""));
                    state
                        .lock()
                        .unwrap_or_else(|err| err.into_inner())
                        .tested_azure = Some(key);
                }
                Err(err) => {
                    platform::log(&format!("Azure 연결 테스트 실패: {err}"));
                    settings.set_azure_connected(false);
                    settings.set_azure_message(connection_error(&err).into());
                }
            }
        });
    });
}

fn show_deepl_usage(settings: &SettingsWindow, remaining: u64, limit: u64) {
    settings.set_deepl_connected(true);
    settings.set_deepl_remaining(format_grouped(remaining).into());
    settings.set_deepl_limit(format_grouped(limit).into());
    settings.set_deepl_progress(remaining_ratio(remaining, limit));
}

fn show_azure_usage(settings: &SettingsWindow, remaining: u64) {
    settings.set_azure_connected(true);
    settings.set_azure_remaining(format_grouped(remaining).into());
    settings.set_azure_limit(format_grouped(AZURE_MONTHLY_LIMIT).into());
    settings.set_azure_progress(remaining_ratio(remaining, AZURE_MONTHLY_LIMIT));
}

fn refresh_deepl_hint(settings: &SettingsWindow) {
    let primary = ProviderKind::from_index(settings.get_provider_index());
    settings.set_deepl_hint(
        ui_text::deepl_hint(
            settings.get_deepl_key().as_str(),
            primary,
            settings.get_deepl_connected(),
        )
        .into(),
    );
}

fn translate_clipboard(
    state: &Arc<Mutex<AppState>>,
    translate: &TranslateWindow,
    explicit: Option<LanguageId>,
) {
    match read_clipboard() {
        ClipboardRead::Text(text) => translate_text(state, translate, text, explicit),
        ClipboardRead::Empty => {
            show_failure_popup(translate, &router::TranslateFailure::Empty);
            reveal_translate(translate);
        }
        ClipboardRead::NotText => {
            show_static_popup(translate, popup_not_text(), PopupMode::NotText);
            reveal_translate(translate);
        }
    }
}

fn translate_text(
    state: &Arc<Mutex<AppState>>,
    translate: &TranslateWindow,
    source: String,
    explicit: Option<LanguageId>,
) {
    let compact = translate.get_mode() != PopupMode::Result || explicit.is_none();
    let generation = {
        let mut state = state.lock().unwrap_or_else(|err| err.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.last_source = source.clone();
        state.last_explicit = explicit;
        state.busy = true;
        if let Some(language) = explicit {
            state.current_target = Some(language);
        }
        state.generation
    };
    if compact {
        let loading = popup_loading();
        translate.set_mode(PopupMode::Loading);
        translate.set_error_title(loading.title.into());
        translate.set_error_detail(loading.detail.into());
        translate.set_show_settings_button(false);
        translate.set_show_retry_button(false);
        translate.set_busy(true);
        reveal_translate(translate);
    } else {
        translate.set_busy(true);
        translate.set_provider_note(BUSY_NOTE.into());
        translate.set_copy_label(COPY_LABEL.into());
        let _ = translate.show();
        platform::focus_window(translate.window());
        translate.invoke_focus_popup();
    }

    let (http, credentials, options) = {
        let state = state.lock().unwrap_or_else(|err| err.into_inner());
        (
            state.http.clone(),
            Credentials {
                deepl_key: state.secrets.deepl_key.clone(),
                azure_key: state.secrets.azure_key.clone(),
                azure_region: state.config.azure_region.clone(),
            },
            TranslateOptions {
                primary: state.config.provider,
                fallback_to_azure: state.config.fallback_to_azure,
                language1: state.config.language1(),
                language2: state.config.language2(),
            },
        )
    };
    let month = platform::local_year_month();
    let dir = state
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .dir
        .clone();
    let state = Arc::clone(state);
    let translate = translate.as_weak();
    std::thread::spawn(move || {
        let result = router::translate(&http, &source, explicit, &credentials, &options);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(translate) = translate.upgrade() else {
                return;
            };
            let mut state = state.lock().unwrap_or_else(|err| err.into_inner());
            if state.generation != generation {
                return;
            }
            state.busy = false;
            match result {
                Ok(translation) => {
                    for (provider, chars) in &translation.usage {
                        usage::record_usage(&mut state.config, *provider, *chars, &month);
                    }
                    if let Err(err) = config::save_config(&config::config_path(&dir), &state.config)
                    {
                        platform::log(&format!("사용량을 저장하지 못했습니다: {err}"));
                    }
                    if translate.window().is_visible() {
                        show_translation(&translate, &mut state, &translation);
                        reveal_translate(&translate);
                    }
                }
                Err(err) => {
                    platform::log(&format!("번역 실패: {err}"));
                    if translate.window().is_visible() {
                        show_failure_popup(&translate, &err);
                        reveal_translate(&translate);
                    }
                }
            }
        });
    });
}

fn show_translation(translate: &TranslateWindow, state: &mut AppState, translation: &Translation) {
    state.current_target = Some(translation.target);
    state.busy = false;
    translate.set_busy(false);
    translate.set_mode(PopupMode::Result);
    translate.set_source_text(state.last_source.as_str().into());
    translate.set_translated_text(translation.text.as_str().into());
    translate
        .set_detected_label(detected_label(translation.detected, &translation.detected_raw).into());
    translate.set_provider_note(provider_note(translation.provider, translation.fallback).into());
    translate.set_target_index(translation.target.index());
    translate.set_copy_label(COPY_LABEL.into());
    translate.set_show_settings_button(false);
    translate.set_show_retry_button(false);
}

fn show_failure_popup(translate: &TranslateWindow, failure: &router::TranslateFailure) {
    let mode = match failure {
        router::TranslateFailure::Empty => PopupMode::Empty,
        router::TranslateFailure::NoApiKey => PopupMode::NoKey,
        router::TranslateFailure::QuotaExceeded => PopupMode::Quota,
        router::TranslateFailure::Network => PopupMode::Network,
        router::TranslateFailure::TooLong => PopupMode::TooLong,
        router::TranslateFailure::Auth | router::TranslateFailure::Unavailable => PopupMode::Failed,
    };
    show_static_popup(translate, popup_for_failure(failure), mode);
}

fn show_static_popup(translate: &TranslateWindow, text: ui_text::PopupText, mode: PopupMode) {
    translate.set_mode(mode);
    translate.set_error_title(text.title.into());
    translate.set_error_detail(text.detail.into());
    translate.set_show_settings_button(text.settings_button);
    translate.set_show_retry_button(text.retry_button);
    translate.set_busy(false);
}

fn copy_translation(translate: &TranslateWindow) {
    let text = translate.get_translated_text().to_string();
    if text.is_empty() {
        return;
    }
    match write_clipboard(&text) {
        Ok(()) => {
            translate.set_copy_label(COPIED_LABEL.into());
            let translate = translate.as_weak();
            slint::Timer::single_shot(Duration::from_millis(1200), move || {
                if let Some(translate) = translate.upgrade() {
                    if translate.get_copy_label().as_str() == COPIED_LABEL {
                        translate.set_copy_label(COPY_LABEL.into());
                    }
                }
            });
        }
        Err(err) => {
            platform::log(&format!("복사 실패: {err}"));
            translate.set_copy_label(COPY_FAILED.into());
        }
    }
}

fn reveal_translate(translate: &TranslateWindow) {
    let (width, height) = if translate.get_mode() == PopupMode::Result {
        (520.0, 480.0)
    } else {
        (400.0, 260.0)
    };
    let already = translate.window().is_visible();
    let _ = translate.show();
    present(
        translate.window(),
        !already,
        width,
        height,
        platform::FrameStyle::Translate,
    );
    translate.invoke_focus_popup();
    if !already {
        let translate = translate.as_weak();
        slint::Timer::single_shot(Duration::from_millis(40), move || {
            if let Some(translate) = translate.upgrade() {
                present(
                    translate.window(),
                    true,
                    width,
                    height,
                    platform::FrameStyle::Translate,
                );
                translate.invoke_focus_popup();
            }
        });
    }
}

fn present(
    window: &slint::Window,
    tune: bool,
    width: f32,
    height: f32,
    style: platform::FrameStyle,
) {
    window.set_size(slint::LogicalSize::new(width, height));
    if tune {
        platform::tune_frame(window, style);
    }
    platform::center_on_cursor(window);
    platform::focus_window(window);
}

enum ClipboardRead {
    Text(String),
    Empty,
    NotText,
}

fn read_clipboard() -> ClipboardRead {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(clipboard) => clipboard,
        Err(err) => {
            platform::log(&format!("클립보드를 열지 못했습니다: {err}"));
            return ClipboardRead::NotText;
        }
    };
    match clipboard.get_text() {
        Ok(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                ClipboardRead::Empty
            } else {
                ClipboardRead::Text(trimmed.to_string())
            }
        }
        Err(_) => ClipboardRead::NotText,
    }
}

fn write_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|err| err.to_string())?;
    clipboard.set_text(text).map_err(|err| err.to_string())
}
