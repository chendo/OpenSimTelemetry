//! Build script that concatenates the split UI source files into a single
//! `src/ui.html` file. The server embeds this via `include_str!("ui.html")`.

use std::fs;
use std::path::Path;

fn main() {
    let ui_dir = Path::new("src/ui");
    let output = Path::new("src/ui.html");

    // Re-run if any source file changes
    println!("cargo::rerun-if-changed=src/ui/");

    let css = fs::read_to_string(ui_dir.join("styles.css")).unwrap();
    let js_files = [
        "constants.js",
        "telemetry-store.js",
        "widgets-base.js",
        "widgets-vehicle.js",
        "widgets-graph.js",
        "widgets-data.js",
        "widgets-replay.js",
        "app.js",
    ];

    let js: String = js_files
        .iter()
        .map(|f| fs::read_to_string(ui_dir.join(f)).unwrap())
        .collect::<Vec<_>>()
        .join("\n");

    let html = fs::read_to_string(ui_dir.join("index.html")).unwrap();
    let result = format!(
        "<!-- GENERATED FILE — DO NOT EDIT. Edit files in src/ui/ instead. -->\n{}",
        html.replace("/* __STYLES__ */", &css)
            .replace("/* __SCRIPTS__ */", &js),
    );

    // Only write if content changed (avoids unnecessary recompilation)
    let current = fs::read_to_string(output).unwrap_or_default();
    if current != result {
        fs::write(output, result).unwrap();
    }
}
