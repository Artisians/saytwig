use crate::pcm::{validate_rate, AudioError, Pcm16k, MAX_SECONDS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureStatus {
    Recording,
    LimitReached,
    Stopped,
    Cancelled,
    Failed(AudioError),
}

/// Device-independent capture seam. Allocates bounded storage before recording;
/// pushing complete interleaved frames allocates nothing. Native capture feeds
/// samples and device failures through this hardware-independent boundary.
pub struct CaptureBuffer {
    sample_rate: u32,
    channels: usize,
    mono: Vec<f32>,
    max_frames: usize,
    status: CaptureStatus,
}
impl CaptureBuffer {
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self, AudioError> {
        validate_rate(sample_rate)?;
        if !(1..=32).contains(&channels) {
            return Err(AudioError::UnsupportedFormat);
        }
        let max_frames = sample_rate as usize * MAX_SECONDS;
        Ok(Self {
            sample_rate,
            channels: channels as usize,
            mono: Vec::with_capacity(max_frames),
            max_frames,
            status: CaptureStatus::Recording,
        })
    }
    pub fn status(&self) -> CaptureStatus {
        self.status
    }
    pub fn frames(&self) -> usize {
        self.mono.len()
    }
    pub fn push_interleaved(&mut self, samples: &[f32]) -> Result<CaptureStatus, AudioError> {
        self.push_converted(samples, |value| value)
    }
    pub(crate) fn push_converted<T: Copy>(
        &mut self,
        samples: &[T],
        convert: impl Fn(T) -> f32,
    ) -> Result<CaptureStatus, AudioError> {
        if self.status != CaptureStatus::Recording {
            return Ok(self.status);
        }
        if !samples.len().is_multiple_of(self.channels) {
            self.fail(AudioError::InvalidSamples);
            return Err(AudioError::InvalidSamples);
        }
        for frame in samples.chunks_exact(self.channels) {
            if self.mono.len() == self.max_frames {
                self.status = CaptureStatus::LimitReached;
                break;
            }
            let mut mixed = 0.0;
            for sample in frame {
                let sample = convert(*sample);
                if !sample.is_finite() {
                    self.fail(AudioError::InvalidSamples);
                    return Err(AudioError::InvalidSamples);
                }
                mixed += sample.clamp(-1.0, 1.0) / self.channels as f32;
            }
            self.mono.push(mixed.clamp(-1.0, 1.0));
        }
        if self.mono.len() == self.max_frames {
            self.status = CaptureStatus::LimitReached;
        }
        Ok(self.status)
    }
    #[cfg(feature = "microphone")]
    pub(crate) fn mark_limit_reached(&mut self) {
        if self.status == CaptureStatus::Recording {
            self.status = CaptureStatus::LimitReached;
        }
    }
    pub fn fail(&mut self, error: AudioError) {
        self.status = CaptureStatus::Failed(error);
        self.mono.clear();
    }
    pub fn cancel(&mut self) {
        self.status = CaptureStatus::Cancelled;
        self.mono.clear();
    }
    pub fn stop(mut self) -> Result<Pcm16k, AudioError> {
        self.finish()
    }
    pub(crate) fn finish(&mut self) -> Result<Pcm16k, AudioError> {
        match self.status {
            CaptureStatus::Cancelled => Err(AudioError::Cancelled),
            CaptureStatus::Failed(error) => Err(error),
            state => {
                self.status = CaptureStatus::Stopped;
                Pcm16k::normalize(
                    std::mem::take(&mut self.mono),
                    self.sample_rate,
                    state == CaptureStatus::LimitReached,
                )
            }
        }
    }
}
