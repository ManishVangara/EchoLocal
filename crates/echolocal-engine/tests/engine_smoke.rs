//! Exercises the transcribe.cpp FFI without a model file. Real inference is
//! covered by `echolocal-bench` (it needs a downloaded model).

#![cfg(feature = "native")]

use echolocal_core::ModelId;

#[test]
fn loading_a_missing_model_fails_cleanly() {
    echolocal_engine::engine::init_backend();
    let err = echolocal_engine::load_parakeet(
        ModelId::ParakeetTdtV2,
        std::path::Path::new("/nonexistent/model.gguf"),
    )
    .err()
    .expect("loading a missing file must fail");
    assert!(err.to_string().contains("Parakeet TDT v2"), "{err}");
}

#[test]
fn loading_a_non_gguf_file_fails_cleanly() {
    let path =
        std::env::temp_dir().join(format!("echolocal-not-a-model-{}.gguf", std::process::id()));
    std::fs::write(&path, b"definitely not gguf").unwrap();
    echolocal_engine::engine::init_backend();
    assert!(echolocal_engine::load_parakeet(ModelId::ParakeetTdtV3, &path).is_err());
    let _ = std::fs::remove_file(path);
}
