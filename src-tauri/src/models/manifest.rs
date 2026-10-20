//! Pinned model files (KTD8). URLs name an exact upstream commit, so the
//! bytes behind them can't change; `plans/DEPENDENCY_REVIEW_M3.md` records
//! where each pin came from.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ModelKind {
    Speech,
    Llm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpec {
    pub id: String,
    pub kind: ModelKind,
    pub display_name: String,
    /// File name under `appDataDir/models`.
    pub file_name: String,
    pub url: String,
    #[ts(type = "number")]
    pub size: u64,
    /// Lowercase hex.
    pub sha256: String,
    pub license: String,
    pub license_url: String,
}

pub const WHISPER_BASE: &str = "whisper-base";
pub const GEMMA_2_2B: &str = "gemma-2-2b-it-q4km";

/// Download order: the speech model first, so recording works as soon as
/// possible while the larger LLM is still arriving.
pub fn manifest() -> Vec<ModelSpec> {
    vec![
        ModelSpec {
            id: WHISPER_BASE.into(),
            kind: ModelKind::Speech,
            display_name: "Whisper base".into(),
            file_name: "ggml-base.bin".into(),
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/\
                  5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin"
                .into(),
            size: 147_951_465,
            sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe".into(),
            license: "MIT".into(),
            license_url: "https://github.com/openai/whisper/blob/main/LICENSE".into(),
        },
        ModelSpec {
            id: GEMMA_2_2B.into(),
            kind: ModelKind::Llm,
            display_name: "Gemma 2 2B Instruct (Q4_K_M)".into(),
            file_name: "gemma-2-2b-it-Q4_K_M.gguf".into(),
            url: "https://huggingface.co/bartowski/gemma-2-2b-it-GGUF/resolve/\
                  855f67caed130e1befc571b52bd181be2e858883/gemma-2-2b-it-Q4_K_M.gguf"
                .into(),
            size: 1_708_582_752,
            sha256: "e0aee85060f168f0f2d8473d7ea41ce2f3230c1bc1374847505ea599288a7787".into(),
            license: "Gemma Terms of Use".into(),
            license_url: "https://ai.google.dev/gemma/terms".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_model_comes_first() {
        let m = manifest();
        assert_eq!(m[0].kind, ModelKind::Speech);
        assert_eq!(m[1].kind, ModelKind::Llm);
    }

    #[test]
    fn pins_are_well_formed() {
        for spec in manifest() {
            assert!(spec.url.starts_with("https://"), "{}", spec.url);
            assert!(!spec.url.contains(' '), "{}", spec.url);
            assert!(spec.url.ends_with(&spec.file_name), "{}", spec.url);
            assert_eq!(spec.sha256.len(), 64);
            assert!(spec
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        }
    }
}
