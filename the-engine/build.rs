// build.rs — compile-time guard: reject WASM targets for the-engine.
fn main() {
    let target = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target == "wasm32" {
        panic!(
            "the-engine cannot be compiled for wasm32. \
             It is a native Tokio daemon. \
             Use `cargo build --package the-anchor` for the WASM contract."
        );
    }
}
