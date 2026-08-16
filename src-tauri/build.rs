fn main() {
    tauri_build::build();
    // Re-run (and recompile) whenever icons change: `generate_context!` reads
    // the icon files at compile time, so without this trigger editing an icon
    // would silently leave the embedded window/tray icon stale.
    println!("cargo:rerun-if-changed=icons");
}
