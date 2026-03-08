use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
use std::path::Path;

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub fn new(model_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        if !Path::new(model_path).exists() {
            return Err(format!("Whisper model not found at: {}", model_path).into());
        }
        let ctx = WhisperContext::new_with_params(
            model_path,
            WhisperContextParameters::default(),
        )?;
        Ok(Self { ctx })
    }

    pub fn transcribe(&self, audio: &[f32]) -> Result<String, Box<dyn std::error::Error>> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_single_segment(false);

        let mut state = self.ctx.create_state()?;
        state.full(params, audio)?;

        let num_segments = state.full_n_segments();
        let mut text = String::new();
        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                let segment_text = segment.to_str()?;
                text.push_str(segment_text.trim());
                text.push(' ');
            }
        }

        Ok(text.trim().to_string())
    }
}
