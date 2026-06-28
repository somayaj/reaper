fn main() {
    let build = std::fs::read_to_string("static/BUILD")
        .unwrap_or_else(|_| "0".to_string())
        .trim()
        .to_string();
    println!("cargo:rerun-if-changed=static/BUILD");
    println!("cargo:rustc-env=REAPER_UI_BUILD={}", build);
}
