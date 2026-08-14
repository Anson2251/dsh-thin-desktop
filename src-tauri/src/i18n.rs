//! Minimal i18n for the few user-facing strings (menu, tray, notifications).
//!
//! A `t!` macro + a compile-time key table. Language is chosen at runtime by
//! reading environment variables (classic `LANG`/`LC_ALL`, with `DSH_LANG`
//! as an explicit override), so it works on macOS, Linux and Windows without
//! any per-platform code.

/// Supported languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

/// Detect the language from the environment.
///
/// Precedence:
///   1. `DSH_LANG`  (explicit override)
///   2. `LC_ALL`    (classic, overrides LANG)
///   3. `LANG`      (classic, POSIX/Unix; also set by many tools on Windows shells)
///
/// Anything whose region/script resolves to Chinese yields `Lang::Zh`,
/// otherwise `Lang::En`.
pub fn detect() -> Lang {
    let probe = |var: &str| std::env::var_os(var).map(|v| v.to_string_lossy().to_lowercase());
    let locale = probe("DSH_LANG")
        .or_else(|| probe("LC_ALL"))
        .or_else(|| probe("LANG"));
    match locale {
        Some(l) if l.contains("zh") => Lang::Zh,
        _ => Lang::En,
    }
}

/// Whether Chinese is the active language (useful for JS/HOST injection).
#[allow(dead_code)]
pub fn is_zh() -> bool {
    detect() == Lang::Zh
}

/// A short language code (`"zh"` or `"en"`) for the frontend.
pub fn lang_code() -> &'static str {
    match detect() {
        Lang::Zh => "zh",
        Lang::En => "en",
    }
}

/// Look up a key for the active language and substitute `{}` placeholders.
pub fn get_fmt(key: &'static str, args: &[String]) -> String {
    let mut s = get(key).to_string();
    for arg in args {
        if let Some(pos) = s.find("{}") {
            s.replace_range(pos..pos + 2, arg);
        } else {
            // Extra args are appended so nothing is silently dropped.
            s.push(' ');
            s.push_str(arg);
        }
    }
    s
}

/// Look up a key for the active language.
pub fn get(key: &'static str) -> &'static str {
    let lang = detect();
    match key {
        // Notifications
        "notify.approval.title" => pick(lang, "DSH 需要你批准", "DSH needs your approval"),
        "notify.approval.tool" => pick(lang, "工具 {} 需要权限批准", "Tool {} needs permission approval"),
        "notify.approval.generic" => pick(lang, "操作需要权限批准", "An operation needs permission approval"),
        "notify.question.title" => pick(lang, "DSH 提问", "DSH Question"),
        "notify.question.fallback" => pick(lang, "Agent 正在问你一个问题", "The agent is asking you a question"),

        // Window / titles
        "win.title" => pick(lang, "DeepSeek Harness — {}", "DeepSeek Harness — {}"),

        // Menu / tray
        "menu.show_hide" => pick(lang, "显示 / 隐藏窗口", "Show / Hide Window"),
        "menu.reload" => pick(lang, "Reload", "Reload"),
        "menu.restart" => pick(lang, "Start / Restart Backend", "Start / Restart Backend"),
        "menu.quit" => pick(lang, "退出", "Quit"),
        "menu.toggle_devtools" => pick(lang, "Toggle DevTools", "Toggle DevTools"),

        _ => key,
    }
}

fn pick(lang: Lang, zh: &'static str, en: &'static str) -> &'static str {
    match lang {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

/// Translate a key. Use `t!` — this is the backing function.
#[doc(hidden)]
pub fn __t(key: &'static str) -> &'static str {
    get(key)
}

/// Translate a key with `{}` placeholder substitution.
#[doc(hidden)]
pub fn __t_fmt(key: &'static str, args: Vec<String>) -> String {
    get_fmt(key, &args)
}

/// Convenient `t!` macro.
///
/// ```ignore
/// let s = t!("notify.approval.title");
/// let s2 = t!("notify.approval.tool", "edit");
/// let title = t!("win.title", session_name);
/// ```
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::__t($key)
    };
    ($key:literal, $($arg:expr),+ $(,)?) => {
        $crate::i18n::__t_fmt($key, vec![$($arg.to_string()),+])
    };
}
