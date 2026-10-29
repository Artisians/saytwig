use rubato::{FftFixedInOut, Resampler};
use thiserror::Error;
pub const SAMPLE_RATE: u32 = 16_000;
pub const MAX_SECONDS: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum AudioError {
    #[error("The selected microphone is unavailable")]
    DeviceUnavailable,
    #[error("The microphone configuration is unsupported")]
    UnsupportedFormat,
    #[error("The microphone could not be started")]
    StartFailed,
    #[error("The microphone stopped unexpectedly")]
    DeviceLost,
    #[error("The recording was cancelled")]
    Cancelled,
    #[error("Audio contains invalid samples or an incomplete frame")]
    InvalidSamples,
    #[error("The recording exceeded the permitted duration")]
    TooLong,
    #[error("Audio capture could not keep up with the device")]
    Overrun,
    #[error("Audio normalization failed")]
    ResamplingFailed,
    #[error("The recording worker stopped unexpectedly")]
    WorkerStopped,
}

/// Validated mono 16 kHz audio. Deliberately not Debug: samples are private.
pub struct Pcm16k {
    samples: Vec<f32>,
    limit_reached: bool,
}
impl Pcm16k {
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }
    pub fn limit_reached(&self) -> bool {
        self.limit_reached
    }
    pub fn duration_seconds(&self) -> f64 {
        self.samples.len() as f64 / SAMPLE_RATE as f64
    }
    pub fn from_mono(samples: Vec<f32>, sample_rate: u32) -> Result<Self, AudioError> {
        Self::normalize(samples, sample_rate, false)
    }
    pub(crate) fn normalize(
        samples: Vec<f32>,
        sample_rate: u32,
        limit_reached: bool,
    ) -> Result<Self, AudioError> {
        validate_rate(sample_rate)?;
        if samples.len() > sample_rate as usize * MAX_SECONDS {
            return Err(AudioError::TooLong);
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(AudioError::InvalidSamples);
        }
        let samples: Vec<f32> = samples
            .into_iter()
            .map(|sample| sample.clamp(-1.0, 1.0))
            .collect();
        if samples.is_empty() || sample_rate == SAMPLE_RATE {
            return Ok(Self {
                samples,
                limit_reached,
            });
        }
        let expected =
            (samples.len() as u64 * SAMPLE_RATE as u64).div_ceil(sample_rate as u64) as usize;
        let mut converter =
            FftFixedInOut::<f32>::new(sample_rate as usize, SAMPLE_RATE as usize, 1024, 1)
                .map_err(|_| AudioError::ResamplingFailed)?;
        let delay = converter.output_delay();
        let block = converter.input_frames_next();
        let mut output = Vec::with_capacity(expected + delay + converter.output_frames_max());
        for chunk in samples.chunks(block) {
            let produced = if chunk.len() == block {
                converter.process(&[chunk], None)
            } else {
                converter.process_partial(Some(&[chunk]), None)
            }
            .map_err(|_| AudioError::ResamplingFailed)?;
            output.extend_from_slice(&produced[0]);
        }
        // Flush the finite filter tail, then remove only the documented delay.
        while output.len() < expected + delay {
            let produced = converter
                .process_partial::<&[f32]>(None, None)
                .map_err(|_| AudioError::ResamplingFailed)?;
            if produced[0].is_empty() {
                return Err(AudioError::ResamplingFailed);
            }
            output.extend_from_slice(&produced[0]);
        }
        let samples = output[delay..delay + expected]
            .iter()
            .map(|sample| sample.clamp(-1.0, 1.0))
            .collect();
        Ok(Self {
            samples,
            limit_reached,
        })
    }
}
pub(crate) fn validate_rate(rate: u32) -> Result<(), AudioError> {
    if (8_000..=192_000).contains(&rate) {
        Ok(())
    } else {
        Err(AudioError::UnsupportedFormat)
    }
}
