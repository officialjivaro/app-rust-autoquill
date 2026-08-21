fn main() {
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
