fn main() {
    tauri_build::build();

    // `translator-asr` links sherpa-onnx/onnxruntime as shared libraries and the
    // build script copies them next to the produced binaries. Add an $ORIGIN
    // rpath so the loader finds them both in `target/<profile>` and in the
    // installed layout (`/usr/lib/open-translator`).
    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");

        // The .so files may change between builds; make cargo rerun this link.
        println!("cargo:rerun-if-changed=build.rs");
    }
}
