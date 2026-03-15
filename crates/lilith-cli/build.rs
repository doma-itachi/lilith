fn main() {
    println!("cargo:rerun-if-changed=../../assets/image/icon.ico");

    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../../assets/image/icon.ico");
        resource
            .compile()
            .expect("failed to compile Windows icon resource");
    }
}
