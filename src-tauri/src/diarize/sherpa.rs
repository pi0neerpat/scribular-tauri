use std::path::PathBuf;

use sherpa_onnx::{SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig};

use super::{SpeakerEmbedder, SpeakerEmbedderLoader};

pub struct SherpaLoader(Option<PathBuf>);

impl SherpaLoader {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self(path)
    }
}

impl SpeakerEmbedderLoader for SherpaLoader {
    fn load(&self) -> Result<Box<dyn SpeakerEmbedder>, String> {
        let model = self
            .0
            .as_ref()
            .filter(|path| path.is_file())
            .ok_or("speaker model isn't ready")?;
        let extractor = SpeakerEmbeddingExtractor::create(&SpeakerEmbeddingExtractorConfig {
            model: Some(model.to_string_lossy().into_owned()),
            num_threads: 4,
            debug: false,
            provider: Some("cpu".into()),
        })
        .ok_or("couldn't load the speaker model")?;
        Ok(Box::new(SherpaEmbedder(extractor)))
    }
}

struct SherpaEmbedder(SpeakerEmbeddingExtractor);

impl SpeakerEmbedder for SherpaEmbedder {
    fn embed(&self, audio: &[f32]) -> Result<Vec<f32>, String> {
        let stream = self
            .0
            .create_stream()
            .ok_or("couldn't create speaker stream")?;
        stream.accept_waveform(16_000, audio);
        if !self.0.is_ready(&stream) {
            return Err("speaker segment is too short".into());
        }
        self.0
            .compute(&stream)
            .ok_or_else(|| "speaker embedding failed".into())
    }
}
