//! The frontend's `bindings.ts` must match the Rust commands. Run with
//! `UPDATE_BINDINGS=1` (or `just bindings`) to regenerate it.

use std::path::Path;

#[test]
fn bindings_are_up_to_date() {
    let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings.ts");
    if std::env::var_os("UPDATE_BINDINGS").is_some() {
        keybund_app_lib::export_bindings(&committed).unwrap();
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let fresh = dir.path().join("bindings.ts");
    keybund_app_lib::export_bindings(&fresh).unwrap();
    let expected = std::fs::read_to_string(&fresh).unwrap();
    let actual = std::fs::read_to_string(&committed).unwrap_or_default();
    assert!(actual == expected, "bindings.ts is outdated – run `just bindings`");
}
