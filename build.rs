//! Embed the Reaper logo in reaper.exe (Windows taskbar / Alt+Tab) and emit PNG for runtime window icon.

fn main() {
    emit_ui_build_env();

    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=static/logo-icon.svg");

    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let svg_path = manifest_dir.join("static/logo-icon.svg");
    let out_dir = manifest_dir.join("packaging/windows");
    let ico_path = out_dir.join("Reaper.ico");
    let png32_path = out_dir.join("icon-32.png");

    if !svg_path.is_file() {
        println!("cargo:warning=static/logo-icon.svg missing; skipping Windows icon embed");
        return;
    }

    std::fs::create_dir_all(&out_dir).ok();

    if needs_regenerate(&svg_path, &ico_path) {
        if let Err(e) = generate_icon_assets(&svg_path, &ico_path, &png32_path) {
            println!("cargo:warning=Windows icon generation failed: {e}");
            return;
        }
    }

    if ico_path.is_file() {
        embed_exe_icon(&ico_path);
    }
}

fn emit_ui_build_env() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let build_path = manifest_dir.join("static/BUILD");
    println!("cargo:rerun-if-changed=static/BUILD");
    let build = std::fs::read_to_string(&build_path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "0".into());
    println!("cargo:rustc-env=REAPER_UI_BUILD={build}");
}

fn needs_regenerate(svg: &std::path::Path, ico: &std::path::Path) -> bool {
    if !ico.is_file() {
        return true;
    }
    let svg_m = svg.metadata().and_then(|m| m.modified()).ok();
    let ico_m = ico.metadata().and_then(|m| m.modified()).ok();
    match (svg_m, ico_m) {
        (Some(s), Some(i)) => s > i,
        _ => true,
    }
}

fn generate_icon_assets(
    svg: &std::path::Path,
    ico: &std::path::Path,
    png32: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let svg_data = std::fs::read(svg)?;
    let tree = usvg::Tree::from_data(&svg_data, &usvg::Options::default())?;

    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [16u32, 32, 48, 256] {
        let rgba = render_svg_to_rgba(&tree, size)?;
        if size == 32 {
            write_png(png32, size, size, &rgba)?;
        }
        let image = ico::IconImage::from_rgba_data(size, size, rgba);
        icon_dir.add_entry(ico::IconDirEntry::encode(&image)?);
    }

    let mut file = std::fs::File::create(ico)?;
    icon_dir.write(&mut file)?;
    Ok(())
}

fn render_svg_to_rgba(tree: &usvg::Tree, size: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut pixmap =
        tiny_skia::Pixmap::new(size, size).ok_or("pixmap alloc failed")?;
    pixmap.fill(tiny_skia::Color::TRANSPARENT);

    let svg_w = tree.size().width();
    let svg_h = tree.size().height();
    let scale = (size as f32 / svg_w).min(size as f32 / svg_h);
    let tx = (size as f32 - svg_w * scale) / 2.0;
    let ty = (size as f32 - svg_h * scale) / 2.0;
    let transform = tiny_skia::Transform::from_translate(tx, ty).pre_scale(scale, scale);

    resvg::render(tree, transform, &mut pixmap.as_mut());
    Ok(pixmap.data().to_vec())
}

fn write_png(
    path: &std::path::Path,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    Ok(())
}

fn embed_exe_icon(ico: &std::path::Path) {
    let mut res = winres::WindowsResource::new();
    res.set_icon(ico.to_str().expect("ico path utf-8"));
    if let Err(e) = res.compile() {
        println!("cargo:warning=Could not embed Windows exe icon: {e}");
    }
}
