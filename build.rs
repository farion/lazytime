#[cfg(target_os = "windows")]
fn main() {
    use ico::{IconDir, IconDirEntry, IconImage, ResourceType};
    use image::{ImageFormat, load_from_memory_with_format};
    use std::env;
    use std::fs::File;
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let icon_png = manifest_dir.join("icon_black.png");
    println!("cargo:rerun-if-changed={}", icon_png.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("icon_white.png").display()
    );

    let bytes = std::fs::read(&icon_png).expect("failed to read icon_black.png");
    let image = load_from_memory_with_format(&bytes, ImageFormat::Png)
        .expect("failed to decode icon_black.png")
        .into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    let icon_image = IconImage::from_rgba_data(width, height, rgba);
    let icon_entry = IconDirEntry::encode(&icon_image).expect("failed to encode .ico entry");
    let mut icon_dir = IconDir::new(ResourceType::Icon);
    icon_dir.add_entry(icon_entry);

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap_or_default());
    let icon_ico = out_dir.join("lazytime.ico");
    let mut icon_file = File::create(&icon_ico).expect("failed to create generated .ico");
    icon_dir
        .write(&mut icon_file)
        .expect("failed to write generated .ico");

    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon_ico.to_string_lossy().as_ref());
    res.compile().expect("failed to compile Windows resources");
}

#[cfg(not(target_os = "windows"))]
fn main() {}
