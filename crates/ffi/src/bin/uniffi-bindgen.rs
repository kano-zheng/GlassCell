//! The UniFFI binding generator, run by `macos/build-engine.sh` to write the Swift bindings.

fn main() {
    uniffi::uniffi_bindgen_main()
}
