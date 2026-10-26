use std::path::{Path, PathBuf};

use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};

use super::segment::SegmentBuilder;
use super::{AsrEngine, AsrError, EngineCaps, TARGET_SAMPLE_RATE};
use crate::config::AsrConfig;
use crate::transcript::TranscriptSegment;

const TAIL_SILENCE_SECONDS: f32 = 0.3;

#[derive(Debug, Clone)]
pub struct ModelFiles {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Precision {
    Int8,
    Fp32,
}

impl Precision {
    fn matches(self, name: &str) -> bool {
        match self {
            Precision::Int8 => name.contains(".int8."),
            Precision::Fp32 => !name.contains(".int8."),
        }
    }
}

pub fn discover_model_files(dir: &Path) -> Result<ModelFiles, AsrError> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        AsrError::ModelLoad(format!(
            "Не удалось прочитать каталог модели {}: {e}",
            dir.display()
        ))
    })?;

    let names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();

    let pick = |prefix: &str, prefer: Precision| -> Result<PathBuf, AsrError> {
        let candidates: Vec<&String> = names
            .iter()
            .filter(|n| n.starts_with(prefix) && n.ends_with(".onnx"))
            .collect();

        let chosen = candidates
            .iter()
            .find(|n| prefer.matches(n))
            .or_else(|| candidates.first())
            .ok_or_else(|| {
                AsrError::MissingModelFile(format!(
                    "{}/{}*.onnx\nСодержимое каталога：{}",
                    dir.display(),
                    prefix,
                    if names.is_empty() {
                        "(пусто)".to_string()
                    } else {
                        names.join(", ")
                    }
                ))
            })?;

        Ok(dir.join(chosen.as_str()))
    };

    let tokens = dir.join("tokens.txt");
    if !tokens.is_file() {
        return Err(AsrError::MissingModelFile(format!(
            "{}\nСодержимое каталога：{}",
            tokens.display(),
            names.join(", ")
        )));
    }

    Ok(ModelFiles {
        encoder: pick("encoder", Precision::Int8)?,
        decoder: pick("decoder", Precision::Fp32)?,
        joiner: pick("joiner", Precision::Int8)?,
        tokens,
    })
}

pub struct StreamingAdapter {
    recognizer: Option<OnlineRecognizer>,
    stream: Option<OnlineStream>,
    builder: SegmentBuilder,
    utterance: i64,
    samples_fed: u64,
    utterance_start_ms: u64,
    flushed: bool,
}

impl Default for StreamingAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingAdapter {
    pub fn new() -> Self {
        Self {
            recognizer: None,
            stream: None,
            builder: SegmentBuilder::new(),
            utterance: 0,
            samples_fed: 0,
            utterance_start_ms: 0,
            flushed: false,
        }
    }

    fn now_ms(&self) -> u64 {
        self.samples_fed * 1000 / TARGET_SAMPLE_RATE as u64
    }

    fn strm(&self) -> Result<&OnlineStream, AsrError> {
        self.stream.as_ref().ok_or(AsrError::NotInitialized)
    }

    fn drain(&mut self, out: &mut Vec<TranscriptSegment>) -> Result<(), AsrError> {
        let rec = self.recognizer.as_ref().ok_or(AsrError::NotInitialized)?;
        let stream = self.stream.as_ref().ok_or(AsrError::NotInitialized)?;

        let mut pending: Option<(i64, String)> = None;

        while rec.is_ready(stream) {
            rec.decode(stream);

            if let Some(r) = rec.get_result(stream) {
                if !r.text.trim().is_empty() {
                    pending = Some((self.utterance, r.text));
                }
            }

            if rec.is_endpoint(stream) {
                if let Some((utt, text)) = pending.take() {
                    let text = crate::asr::latin_case::restore_sentence_case(&text);
                    let end = self.now_ms();
                    out.push(
                        self.builder
                            .build(utt, &text, self.utterance_start_ms, end, true),
                    );
                    self.builder.forget(utt);
                }
                rec.reset(stream);
                self.utterance += 1;
                self.utterance_start_ms = self.now_ms();
            }
        }

        if let Some((utt, text)) = pending {
            let text = crate::asr::latin_case::restore_sentence_case(&text);
            let end = self.now_ms();
            out.push(
                self.builder
                    .build(utt, &text, self.utterance_start_ms, end, false),
            );
        }

        Ok(())
    }
}

impl AsrEngine for StreamingAdapter {
    fn init(&mut self, cfg: &AsrConfig) -> Result<(), AsrError> {
        let dir = cfg
            .model_dir
            .as_ref()
            .ok_or_else(|| AsrError::InvalidConfig("Каталог model_dir не задан".into()))?;
        let files = discover_model_files(Path::new(dir))?;

        let mut config = OnlineRecognizerConfig::default();

        config.model_config.transducer.encoder = Some(files.encoder.to_string_lossy().into_owned());
        config.model_config.transducer.decoder = Some(files.decoder.to_string_lossy().into_owned());
        config.model_config.transducer.joiner = Some(files.joiner.to_string_lossy().into_owned());
        config.model_config.tokens = Some(files.tokens.to_string_lossy().into_owned());
        config.model_config.provider = Some("cpu".into());

        config.decoding_method = Some("greedy_search".to_string());

        let (rule1, rule2, rule3) = cfg.endpoint.to_recognizer_rules();
        config.enable_endpoint = true;
        config.rule1_min_trailing_silence = rule1;
        config.rule2_min_trailing_silence = rule2;
        config.rule3_min_utterance_length = rule3;

        let recognizer = OnlineRecognizer::create(&config).ok_or_else(|| {
            AsrError::ModelLoad(format!(
                "Не удалось создать потоковый движок (каталог модели {})",
                Path::new(dir).display()
            ))
        })?;

        let stream = recognizer.create_stream();

        self.recognizer = Some(recognizer);
        self.stream = Some(stream);
        self.utterance = 0;
        self.samples_fed = 0;
        self.utterance_start_ms = 0;
        self.flushed = false;
        Ok(())
    }

    fn capabilities(&self) -> EngineCaps {
        EngineCaps::STREAMING_TRANSDUCER
    }

    fn feed_pcm(&mut self, pcm: &[f32]) -> Result<Vec<TranscriptSegment>, AsrError> {
        if self.recognizer.is_none() {
            return Err(AsrError::NotInitialized);
        }
        let stream = self.strm()?;
        stream.accept_waveform(TARGET_SAMPLE_RATE as i32, pcm);
        self.samples_fed += pcm.len() as u64;

        let mut out = Vec::new();
        self.drain(&mut out)?;
        Ok(out)
    }

    fn flush(&mut self) -> Result<Vec<TranscriptSegment>, AsrError> {
        if self.recognizer.is_none() {
            return Err(AsrError::NotInitialized);
        }
        if self.flushed {
            return Ok(Vec::new());
        }
        self.flushed = true;

        let tail_len = (TARGET_SAMPLE_RATE as f32 * TAIL_SILENCE_SECONDS) as usize;
        let tail = vec![0.0f32; tail_len];

        {
            let stream = self.strm()?;
            stream.accept_waveform(TARGET_SAMPLE_RATE as i32, &tail);
            stream.input_finished();
        }
        self.samples_fed += tail_len as u64;

        let mut out = Vec::new();
        self.drain(&mut out)?;

        if let Some(last) = out.last_mut() {
            if last.state == crate::transcript::SegmentState::Partial {
                last.state = crate::transcript::SegmentState::Final;
            }
        }
        Ok(out)
    }

    fn shutdown(&mut self) {
        self.stream = None;
        self.recognizer = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_reports_missing_dir_clearly() {
        let err = discover_model_files(Path::new("/nonexistent/model/dir")).unwrap_err();
        assert!(matches!(err, AsrError::ModelLoad(_)));
    }

    fn temp_model_dir(files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("asr-disc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in files {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        dir
    }

    #[test]
    fn discover_uses_mixed_precision_matching_upstream() {
        let dir = temp_model_dir(&[
            "encoder-epoch-99-avg-1.onnx",
            "encoder-epoch-99-avg-1.int8.onnx",
            "decoder-epoch-99-avg-1.onnx",
            "decoder-epoch-99-avg-1.int8.onnx",
            "joiner-epoch-99-avg-1.onnx",
            "joiner-epoch-99-avg-1.int8.onnx",
            "tokens.txt",
        ]);

        let f = discover_model_files(&dir).unwrap();
        assert!(
            f.encoder.to_string_lossy().contains(".int8."),
            "encoder 应用 int8"
        );
        assert!(
            !f.decoder.to_string_lossy().contains(".int8."),
            "decoder 应保持 fp32"
        );
        assert!(
            f.joiner.to_string_lossy().contains(".int8."),
            "joiner 应用 int8"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn discover_falls_back_when_preferred_precision_is_absent() {
        let dir = temp_model_dir(&[
            "encoder-epoch-99-avg-1.onnx",
            "decoder-epoch-99-avg-1.int8.onnx",
            "joiner-epoch-99-avg-1.onnx",
            "tokens.txt",
        ]);
        assert!(discover_model_files(&dir).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn discover_error_lists_actual_directory_contents() {
        let dir = temp_model_dir(&["encoder-epoch-99-avg-1.int8.onnx", "tokens.txt"]);
        let msg = discover_model_files(&dir).unwrap_err().to_string();
        assert!(msg.contains("decoder"), "{msg}");
        assert!(msg.contains("encoder-epoch-99-avg-1.int8.onnx"), "{msg}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn discover_ignores_chunk_variant_subdirectories() {
        let dir = temp_model_dir(&[
            "encoder-epoch-99-avg-1.int8.onnx",
            "decoder-epoch-99-avg-1.onnx",
            "joiner-epoch-99-avg-1.int8.onnx",
            "tokens.txt",
        ]);
        std::fs::create_dir_all(dir.join("64")).unwrap();
        std::fs::write(dir.join("64").join("encoder-x.onnx"), b"x").unwrap();

        let f = discover_model_files(&dir).unwrap();
        assert!(!f.encoder.to_string_lossy().contains("/64/"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn feed_before_init_is_an_error_not_a_panic() {
        let mut a = StreamingAdapter::new();
        assert!(matches!(
            a.feed_pcm(&[0.0; 1600]),
            Err(AsrError::NotInitialized)
        ));
    }

    #[test]
    fn capabilities_declare_partial_support() {
        let a = StreamingAdapter::new();
        let c = a.capabilities();
        assert!(c.streaming && c.produces_partial && c.supports_endpoint);
        assert!(!c.requires_full_utterance);
    }
    #[test]
    #[ignore = "Requires bundled ASR models and official samples in .cache/asr-canary"]
    fn bundled_asr_audio_canary() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        for language in ["en", "ru"] {
            let bytes =
                std::fs::read(project.join(format!(".cache/asr-canary/{language}.wav"))).unwrap();
            assert_eq!(&bytes[0..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WAVE");
            let mut offset = 12;
            let mut pcm = Vec::new();
            while offset + 8 <= bytes.len() {
                let size =
                    u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
                let start = offset + 8;
                if &bytes[offset..offset + 4] == b"fmt " {
                    assert_eq!(
                        u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()),
                        1
                    );
                    assert_eq!(
                        u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap()),
                        1
                    );
                    assert_eq!(
                        u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap()),
                        TARGET_SAMPLE_RATE
                    );
                    assert_eq!(
                        u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap()),
                        16
                    );
                }
                if &bytes[offset..offset + 4] == b"data" {
                    pcm.extend(
                        bytes[start..start + size]
                            .chunks_exact(2)
                            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0),
                    );
                }
                offset = start + size + size % 2;
            }
            assert!(!pcm.is_empty());
            let mut engine = StreamingAdapter::new();
            engine
                .init(&AsrConfig {
                    source_lang: language.into(),
                    model_dir: Some(
                        project
                            .join(format!("src-tauri/assets/models/asr-builtin/{language}"))
                            .to_string_lossy()
                            .into_owned(),
                    ),
                    ..Default::default()
                })
                .unwrap();
            let mut last = String::new();
            let mut saw_partial = false;
            let mut saw_endpoint = false;
            let mut recognized = String::new();
            for chunk in pcm.chunks(1600) {
                for segment in engine.feed_pcm(chunk).unwrap() {
                    saw_partial |= segment.state == crate::transcript::SegmentState::Partial;
                    if segment.state == crate::transcript::SegmentState::Final {
                        saw_endpoint = true;
                        recognized.push_str(&segment.text);
                        recognized.push(' ');
                    }
                    last = segment.text;
                }
            }
            for _ in 0..30 {
                for segment in engine.feed_pcm(&[0.0; 1600]).unwrap() {
                    if segment.state == crate::transcript::SegmentState::Final {
                        saw_endpoint = true;
                        recognized.push_str(&segment.text);
                        recognized.push(' ');
                    }
                    last = segment.text;
                }
            }
            assert!(
                saw_endpoint,
                "Streaming silence did not trigger an endpoint"
            );
            for segment in engine.flush().unwrap() {
                last = segment.text;
            }
            let text = if recognized.trim().is_empty() {
                last.clone()
            } else {
                recognized.trim().to_owned()
            };
            println!("ASR {language}: {text} (streamed partial: {saw_partial}; endpoint: {saw_endpoint})");
            assert!(saw_partial, "Streaming model did not emit a partial result");
            assert!(
                last.chars().filter(|c| c.is_alphabetic()).count() >= 8,
                "{last}"
            );
            if language == "en" {
                let reference = std::fs::read_to_string(project.join(".cache/asr-canary/en.txt"))
                    .unwrap()
                    .to_lowercase();
                let expected = reference
                    .lines()
                    .find(|line| line.starts_with("0.wav") || line.starts_with("0 "))
                    .unwrap_or_else(|| reference.lines().next().unwrap());
                let words: Vec<_> = expected
                    .split_whitespace()
                    .skip(1)
                    .filter(|word| word.len() >= 4)
                    .collect();
                let recognized = format!("{recognized} {last}").to_lowercase();
                let hits = words
                    .iter()
                    .filter(|word| recognized.contains(**word))
                    .count();
                assert!(
                    hits >= 3,
                    "Too few reference words in {recognized}; expected {expected}"
                );
            }
            if language == "ru" {
                assert!(text.to_lowercase().contains("люблю"), "{text}");
                assert!(last.chars().any(|c| ('а'..='я').contains(&c)), "{last}");
            } else {
                assert!(last.chars().any(|c| c.is_ascii_alphabetic()), "{last}");
            }
            engine.shutdown();
            use crate::translate::Translator;
            let target = crate::config::target_lang_for(language);
            let mut translator = crate::translate::local_nmt::LocalNmt::new(
                project.join("src-tauri/assets/models/mt"),
                crate::config::MtModelTier::Base,
            );
            translator
                .init(&crate::config::TranslateConfig {
                    source_lang: language.into(),
                    target_lang: target.into(),
                    ..Default::default()
                })
                .unwrap();
            let translated = translator.translate(&text).unwrap();
            println!("Audio → ASR → MT {language} → {target}: {translated}");
            if target == "ru" {
                assert!(
                    translated.chars().any(|c| ('а'..='я').contains(&c)),
                    "{translated}"
                );
            } else {
                assert!(translated.to_lowercase().contains("love"), "{translated}");
            }
            translator.shutdown();
        }
    }
}
