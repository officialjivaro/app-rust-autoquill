fn main() {
    compile_windows_resources();

    let mut config = slint_build::CompilerConfiguration::new()
        .with_style("fluent-dark".into())
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles);

    // Element metadata makes UI tests stable and accessibility-aware, but release executables do
    // not need it. Keeping it out of both release profiles protects the portable binary size.
    let profile = std::env::var("PROFILE").unwrap_or_default();
    if !profile.starts_with("release") {
        config = config.with_debug_info(true);
    }

    slint_build::compile_with_config("ui/app-window.slint", config)
        .expect("failed to compile the AutoQuill UI");
}

#[cfg(windows)]
fn compile_windows_resources() {
    use winresource::VersionInfo;

    println!("cargo:rerun-if-changed=assets/icon.ico");
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("assets/icon.ico")
        .set_version_info(VersionInfo::FILEVERSION, 0x0000_0010_0000_0001)
        .set_version_info(VersionInfo::PRODUCTVERSION, 0x0000_0010_0000_0001);
    resource
        .compile()
        .expect("failed to compile Windows icon and version resources");
}

#[cfg(not(windows))]
fn compile_windows_resources() {}
