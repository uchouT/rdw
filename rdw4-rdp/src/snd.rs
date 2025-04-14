use std::{borrow::Cow, sync::OnceLock};

use ironrdp::rdpsnd::{
    client::RdpsndClientHandler,
    pdu::{AudioFormat, AudioFormatFlags, PitchPdu, VolumePdu, WaveFormat},
};
use rdw::GstAudio;
use tracing::{debug, instrument, warn};

use crate::{Error, Result};

#[derive(Debug)]
pub(crate) struct RdwSndBackend {
    gst: GstAudio,
    format_no: Option<usize>,
}

impl RdwSndBackend {
    pub fn new() -> Result<Self> {
        let gst = GstAudio::new().map_err(|e| Error::Other(e.to_string()))?;

        Ok(Self {
            gst,
            format_no: None,
        })
    }
}

impl RdpsndClientHandler for RdwSndBackend {
    #[instrument]
    fn get_flags(&self) -> AudioFormatFlags {
        AudioFormatFlags::VOLUME
    }

    #[instrument]
    fn get_formats(&self) -> &[AudioFormat] {
        static FORMATS: OnceLock<Vec<AudioFormat>> = OnceLock::new();

        FORMATS.get_or_init(|| {
            let mut res = vec![
                AudioFormat {
                    format: WaveFormat::PCM,
                    n_channels: 2,
                    n_samples_per_sec: 48000,
                    n_avg_bytes_per_sec: 192000,
                    n_block_align: 4,
                    bits_per_sample: 16,
                    data: None,
                },
                AudioFormat {
                    format: WaveFormat::PCM,
                    n_channels: 2,
                    n_samples_per_sec: 44100,
                    n_avg_bytes_per_sec: 176400,
                    n_block_align: 4,
                    bits_per_sample: 16,
                    data: None,
                },
                AudioFormat {
                    format: WaveFormat::PCM,
                    n_channels: 2,
                    n_samples_per_sec: 22050,
                    n_avg_bytes_per_sec: 88200,
                    n_block_align: 4,
                    bits_per_sample: 16,
                    data: None,
                },
                AudioFormat {
                    format: WaveFormat::PCM,
                    n_channels: 1,
                    n_samples_per_sec: 22050,
                    n_avg_bytes_per_sec: 44100,
                    n_block_align: 2,
                    bits_per_sample: 16,
                    data: None,
                },
            ];

            if self.gst.can_decode("audio/x-opus").unwrap_or(false) {
                res.push(AudioFormat {
                    format: WaveFormat::OPUS,
                    n_channels: 2,
                    n_samples_per_sec: 48000,
                    n_avg_bytes_per_sec: 192000,
                    n_block_align: 4,
                    bits_per_sample: 16,
                    data: None,
                });
            }

            res
        })
    }

    fn wave(&mut self, format_no: usize, _ts: u32, data: Cow<'_, [u8]>) {
        if self.format_no != Some(format_no) {
            debug!(?format_no, "New audio format");
            self.gst.fini_out(0);
            self.format_no = Some(format_no);
            let Some(format) = self.get_formats().get(format_no) else {
                warn!(?format_no, "Invalid audio format_no");
                return;
            };
            let Some(caps) = format_to_caps(format) else {
                warn!("Unsupported audio format");
                return;
            };
            if let Err(err) = self.gst.init_out(0, &caps) {
                warn!("Failed to initialize audio output: {}", err);
                return;
            }
            if let Err(err) = self.gst.set_enabled_out(0, true) {
                warn!("Failed to enable audio output: {}", err);
                return;
            }
        }

        if let Err(err) = self.gst.write_out(0, data.to_vec()) {
            warn!("Failed to write audio data: {}", err);
        };
    }

    fn set_volume(&mut self, volume: VolumePdu) {
        let vol = volume.volume_left as f64 / 0xffff as f64;
        if let Err(err) = self.gst.set_volume_out(0, false, Some(vol)) {
            warn!("Failed to set volume: {}", err);
        }
    }

    fn set_pitch(&mut self, pitch: PitchPdu) {
        debug!(?pitch);
    }

    #[instrument]
    fn close(&mut self) {
        self.gst.fini_out(0);
    }
}

fn format_to_caps(fmt: &AudioFormat) -> Option<String> {
    let caps = match fmt.format {
        WaveFormat::PCM => format!(
            "audio/x-raw,channels={},rate={},format=S{}LE,layout=interleaved",
            fmt.n_channels, fmt.n_samples_per_sec, fmt.bits_per_sample,
        ),
        WaveFormat::OPUS => format!(
            "audio/x-opus,channels={},rate={},channel-mapping-family=0",
            fmt.n_channels, fmt.n_samples_per_sec,
        ),
        _ => return None,
    };

    Some(caps)
}
