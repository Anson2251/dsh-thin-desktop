//! Validate the `t!` i18n macro and environment-based language detection.
//!
//! These tests mutate the process-wide environment, so they must run one at a
//! time (guarded by a shared mutex) to avoid races.
use dsh_thin_desktop_lib::t;
use std::sync::{Mutex, OnceLock};

static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn clear() {
    std::env::remove_var("DSH_LANG");
    std::env::remove_var("LC_ALL");
    std::env::remove_var("LANG");
}

#[test]
fn default_lang_is_env_driven() {
    let _g = env_guard();
    clear();
    std::env::set_var("DSH_LANG", "en_US.UTF-8");
    assert_eq!(t!("notify.approval.title"), "DSH needs your approval");
    assert_eq!(t!("menu.quit"), "Quit");
    assert_eq!(t!("win.title", "Hello"), "DeepSeek Harness — Hello");
}

#[test]
fn zh_env_yields_chinese() {
    let _g = env_guard();
    clear();
    std::env::set_var("DSH_LANG", "zh_CN.UTF-8");
    assert_eq!(t!("notify.approval.title"), "DSH 需要你批准");
    assert_eq!(t!("menu.quit"), "退出");
    assert_eq!(t!("notify.approval.tool", "edit"), "工具 edit 需要权限批准");
}

#[test]
fn lc_all_overrides_lang() {
    let _g = env_guard();
    clear();
    std::env::set_var("LC_ALL", "zh_CN.UTF-8");
    std::env::set_var("LANG", "en_US.UTF-8");
    assert_eq!(t!("notify.question.title"), "DSH 提问");
}

#[test]
fn missing_lang_falls_back_to_en() {
    let _g = env_guard();
    clear();
    assert_eq!(t!("notify.approval.title"), "DSH needs your approval");
}
