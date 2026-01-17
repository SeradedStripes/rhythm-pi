fn main() {
    let config = slint_build::CompilerConfiguration::default();
    slint_build::compile_with_config("ui/main.slint", config)
        .expect("Slint compilation failed");
}
