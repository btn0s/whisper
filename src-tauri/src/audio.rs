use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub struct AudioCapture {
    stream: Option<cpal::Stream>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

impl AudioCapture {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        Ok(Self {
            stream: None,
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate,
            channels,
        })
    }

    pub fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;

        let config = device.default_input_config()?;
        let sr = config.sample_rate().0;
        let ch = config.channels();
        let fmt = config.sample_format();
        eprintln!("[audio] device: {}, rate: {}, channels: {}, format: {:?}",
            device.name().unwrap_or_default(), sr, ch, fmt);
        self.sample_rate = sr;
        self.channels = ch;

        self.samples.lock().unwrap().clear();

        let samples = self.samples.clone();
        let err_fn = |err| eprintln!("Audio stream error: {}", err);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    samples.lock().unwrap().extend_from_slice(data);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let converted: Vec<f32> =
                            data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                        samples.lock().unwrap().extend_from_slice(&converted);
                    },
                    err_fn,
                    None,
                )?
            }
            format => return Err(format!("Unsupported sample format: {:?}", format).into()),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) -> Vec<f32> {
        self.stream = None;
        let raw = self.samples.lock().unwrap().clone();
        convert_to_whisper_format(&raw, self.sample_rate, self.channels)
    }

    pub fn snapshot(&self) -> Vec<f32> {
        let raw = self.samples.lock().unwrap().clone();
        convert_to_whisper_format(&raw, self.sample_rate, self.channels)
    }

    /// Get recent audio levels for waveform visualization.
    /// Returns 16 amplitude values (0.0-1.0) representing the last ~100ms of audio.
    pub fn levels(&self) -> Vec<f32> {
        let raw = self.samples.lock().unwrap();
        let num_bars = 16;
        // Use last ~4800 samples (~100ms at 48kHz)
        let tail_size = (self.sample_rate as usize / 10).min(raw.len());
        if tail_size < num_bars {
            return vec![0.0; num_bars];
        }
        let tail = &raw[raw.len() - tail_size..];
        let chunk_size = tail_size / num_bars;
        tail.chunks(chunk_size)
            .take(num_bars)
            .map(|chunk| {
                let rms = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
                // Scale up aggressively for visibility
                (rms * 50.0).min(1.0)
            })
            .collect()
    }
}

fn convert_to_whisper_format(input: &[f32], sample_rate: u32, channels: u16) -> Vec<f32> {
    const TARGET_RATE: u32 = 16_000;

    let mono: Vec<f32> = input
        .chunks(channels as usize)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    if sample_rate == TARGET_RATE {
        return mono;
    }

    let ratio = sample_rate as f64 / TARGET_RATE as f64;
    let output_len = (mono.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let sample = if idx + 1 < mono.len() {
            mono[idx] as f64 * (1.0 - frac) + mono[idx + 1] as f64 * frac
        } else if idx < mono.len() {
            mono[idx] as f64
        } else {
            0.0
        };
        output.push(sample as f32);
    }

    output
}
