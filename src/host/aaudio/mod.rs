use std::cell::RefCell;
use std::cmp;
use std::convert::TryInto;
use std::time::{Duration, Instant};
use std::vec::IntoIter as VecIntoIter;

extern crate aaudio_sys;

use crate::traits::{DeviceTrait, HostTrait, StreamTrait};
use crate::{
    BackendSpecificError, BufferSize, BuildStreamError, Data, DefaultStreamConfigError,
    DeviceNameError, DevicesError, InputCallbackInfo, InputStreamTimestamp, OutputCallbackInfo,
    OutputStreamTimestamp, PauseStreamError, PlayStreamError, SampleFormat, SampleRate,
    StreamConfig, StreamError, SupportedBufferSize, SupportedStreamConfig,
    SupportedStreamConfigRange, SupportedStreamConfigsError,
};

mod android_media;
mod audio_manager;
mod convert;
mod jni_utils;

use self::android_media::{get_audio_record_min_buffer_size, get_audio_track_min_buffer_size};
use self::audio_manager::{AudioDeviceDirection, AudioDeviceInfo, AudioFormat};
use self::convert::to_stream_instant;

use self::aaudio_sys::{AAudioStream, AAudioStreamBuilder, AAudioStreamInfo};

const CHANNEL_MASKS: [i32; 8] = [
    android_media::CHANNEL_OUT_MONO,
    android_media::CHANNEL_OUT_STEREO,
    android_media::CHANNEL_OUT_STEREO | android_media::CHANNEL_OUT_FRONT_CENTER,
    android_media::CHANNEL_OUT_QUAD,
    android_media::CHANNEL_OUT_QUAD | android_media::CHANNEL_OUT_FRONT_CENTER,
    android_media::CHANNEL_OUT_5POINT1,
    android_media::CHANNEL_OUT_5POINT1 | android_media::CHANNEL_OUT_BACK_CENTER,
    android_media::CHANNEL_OUT_7POINT1_SURROUND,
];

const SAMPLE_RATES: [i32; 13] = [
    5512, 8000, 11025, 16000, 22050, 32000, 44100, 48000, 64000, 88200, 96000, 176400, 192000,
];

pub struct Host;
pub struct Device(Option<AudioDeviceInfo>);
pub struct Stream(RefCell<AAudioStream>);
pub type SupportedInputConfigs = VecIntoIter<SupportedStreamConfigRange>;
pub type SupportedOutputConfigs = VecIntoIter<SupportedStreamConfigRange>;
pub type Devices = VecIntoIter<Device>;

impl Host {
    pub fn new() -> Result<Self, crate::HostUnavailable> {
        Ok(Host)
    }
}

impl HostTrait for Host {
    type Devices = Devices;
    type Device = Device;

    fn is_available() -> bool {
        true
    }

    fn devices(&self) -> Result<Self::Devices, DevicesError> {
        if let Ok(devices) = AudioDeviceInfo::request(AudioDeviceDirection::InputOutput) {
            Ok(devices
                .into_iter()
                .map(|d| Device(Some(d)))
                .collect::<Vec<_>>()
                .into_iter())
        } else {
            Ok(vec![Device(None)].into_iter())
        }
    }

    fn default_input_device(&self) -> Option<Self::Device> {
        if let Ok(devices) = AudioDeviceInfo::request(AudioDeviceDirection::Input) {
            devices.into_iter().map(|d| Device(Some(d))).next()
        } else {
            Some(Device(None))
        }
    }

    fn default_output_device(&self) -> Option<Self::Device> {
        if let Ok(devices) = AudioDeviceInfo::request(AudioDeviceDirection::Output) {
            devices.into_iter().map(|d| Device(Some(d))).next()
        } else {
            Some(Device(None))
        }
    }
}

fn buffer_size_range_for_params(
    is_output: bool,
    sample_rate: i32,
    channel_mask: i32,
    android_format: i32,
) -> SupportedBufferSize {
    let min_buffer_size = if is_output {
        get_audio_track_min_buffer_size(sample_rate, channel_mask, android_format)
    } else {
        get_audio_record_min_buffer_size(sample_rate, channel_mask, android_format)
    };
    if min_buffer_size > 0 {
        SupportedBufferSize::Range {
            min: min_buffer_size as u32,
            max: i32::MAX as u32,
        }
    } else {
        SupportedBufferSize::Unknown
    }
}

fn default_supported_configs(is_output: bool) -> VecIntoIter<SupportedStreamConfigRange> {
    // Have to "brute force" the parameter combinations with getMinBufferSize
    const FORMATS: [SampleFormat; 2] = [SampleFormat::I16, SampleFormat::F32];

    let mut output = Vec::with_capacity(SAMPLE_RATES.len() * CHANNEL_MASKS.len() * FORMATS.len());
    for sample_format in &FORMATS {
        let android_format = if *sample_format == SampleFormat::I16 {
            android_media::ENCODING_PCM_16BIT
        } else {
            android_media::ENCODING_PCM_FLOAT
        };
        for mask_idx in 0..CHANNEL_MASKS.len() {
            let channel_mask = CHANNEL_MASKS[mask_idx];
            let channel_count = mask_idx + 1;
            for sample_rate in &SAMPLE_RATES {
                if let SupportedBufferSize::Range { min, max } = buffer_size_range_for_params(
                    is_output,
                    *sample_rate,
                    channel_mask,
                    android_format,
                ) {
                    output.push(SupportedStreamConfigRange {
                        channels: channel_count as u16,
                        min_sample_rate: SampleRate(*sample_rate as u32),
                        max_sample_rate: SampleRate(*sample_rate as u32),
                        buffer_size: SupportedBufferSize::Range { min, max },
                        sample_format: *sample_format,
                    });
                }
            }
        }
    }

    output.into_iter()
}

fn device_supported_configs(
    device: &AudioDeviceInfo,
    is_output: bool,
) -> VecIntoIter<SupportedStreamConfigRange> {
    let sample_rates = if !device.sample_rates.is_empty() {
        device.sample_rates.as_slice()
    } else {
        &SAMPLE_RATES
    };

    const ALL_CHANNELS: [i32; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let channel_counts = if !device.channel_counts.is_empty() {
        device.channel_counts.as_slice()
    } else {
        &ALL_CHANNELS
    };

    const ALL_FORMATS: [AudioFormat; 2] = [AudioFormat::I16, AudioFormat::F32];
    let formats = if !device.formats.is_empty() {
        device.formats.as_slice()
    } else {
        &ALL_FORMATS
    };

    let mut output = Vec::with_capacity(sample_rates.len() * channel_counts.len() * formats.len());
    for sample_rate in sample_rates {
        for channel_count in channel_counts {
            assert!(*channel_count > 0);
            if *channel_count > (CHANNEL_MASKS.len() as i32) {
                continue;
            }
            let channel_mask = CHANNEL_MASKS[*channel_count as usize - 1];
            for format in formats {
                let (android_format, sample_format) = match format {
                    AudioFormat::I16 => (android_media::ENCODING_PCM_16BIT, SampleFormat::I16),
                    AudioFormat::F32 => (android_media::ENCODING_PCM_FLOAT, SampleFormat::F32),
                };
                let buffer_size = buffer_size_range_for_params(
                    is_output,
                    *sample_rate,
                    channel_mask,
                    android_format,
                );
                output.push(SupportedStreamConfigRange {
                    channels: cmp::min(*channel_count as u16, 2u16),
                    min_sample_rate: if *sample_rate == 0 {
                        SampleRate(0)
                    } else {
                        SampleRate(*sample_rate as u32)
                    },
                    max_sample_rate: if *sample_rate == 0 {
                        SampleRate(i32::MAX as u32)
                    } else {
                        SampleRate(*sample_rate as u32)
                    },
                    buffer_size,
                    sample_format,
                });
            }
        }
    }

    output.into_iter()
}

fn builder_for_device(
    device: &Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    direction: aaudio_sys::Direction,
) -> Result<AAudioStreamBuilder, BuildStreamError> {
    let format = match sample_format {
        SampleFormat::I16 => aaudio_sys::Format::I16,
        SampleFormat::F32 => aaudio_sys::Format::F32,
        SampleFormat::U16 => {
            return Err(BackendSpecificError {
                description: "U16 format is not supported on Android.".to_owned(),
            }
            .into())
        }
    };
    let mut builder = AAudioStreamBuilder::new()?
        .set_direction(direction)
        .set_format(format)
        .set_channel_count(i32::from(config.channels));
    builder = if let Some(info) = &device.0 {
        builder.set_device_id(info.id)
    } else {
        builder
    };
    builder = builder.set_sample_rate(config.sample_rate.0.try_into().unwrap());
    builder = match &config.buffer_size {
        BufferSize::Default => builder,
        BufferSize::Fixed(size) => builder.set_buffer_capacity_in_frames(*size as i32),
    };
    Ok(builder)
}

fn get_input_callback_info(
    stream: &AAudioStreamInfo,
    creation_time: &Instant,
) -> InputCallbackInfo {
    let timestamp = stream
        .get_timestamp_monotonic()
        .unwrap_or(aaudio_sys::Timestamp {
            frame_position: 0,
            time_nanos: 0,
        });
    InputCallbackInfo {
        timestamp: InputStreamTimestamp {
            callback: to_stream_instant(creation_time.elapsed()),
            capture: to_stream_instant(Duration::from_nanos(timestamp.time_nanos as u64)),
        },
    }
}

fn get_output_callback_info(
    stream: &AAudioStreamInfo,
    creation_time: &Instant,
) -> OutputCallbackInfo {
    let timestamp = stream
        .get_timestamp_monotonic()
        .unwrap_or(aaudio_sys::Timestamp {
            frame_position: 0,
            time_nanos: 0,
        });
    OutputCallbackInfo {
        timestamp: OutputStreamTimestamp {
            callback: to_stream_instant(creation_time.elapsed()),
            playback: to_stream_instant(Duration::from_nanos(timestamp.time_nanos as u64)),
        },
    }
}

fn to_sample_format(format: aaudio_sys::Format) -> SampleFormat {
    match format {
        aaudio_sys::Format::Unspecified => panic!("Sample format must be specified here"),
        aaudio_sys::Format::I16 => SampleFormat::I16,
        aaudio_sys::Format::F32 => SampleFormat::F32,
    }
}

impl DeviceTrait for Device {
    type SupportedInputConfigs = SupportedInputConfigs;
    type SupportedOutputConfigs = SupportedOutputConfigs;
    type Stream = Stream;

    fn name(&self) -> Result<String, DeviceNameError> {
        match &self.0 {
            None => Ok("default".to_owned()),
            Some(info) => Ok(info.product_name.clone()),
        }
    }

    fn supported_input_configs(
        &self,
    ) -> Result<Self::SupportedInputConfigs, SupportedStreamConfigsError> {
        if let Some(info) = &self.0 {
            Ok(device_supported_configs(info, false))
        } else {
            Ok(default_supported_configs(false))
        }
    }

    fn supported_output_configs(
        &self,
    ) -> Result<Self::SupportedOutputConfigs, SupportedStreamConfigsError> {
        if let Some(info) = &self.0 {
            Ok(device_supported_configs(info, true))
        } else {
            Ok(default_supported_configs(true))
        }
    }

    fn default_input_config(&self) -> Result<SupportedStreamConfig, DefaultStreamConfigError> {
        let mut configs: Vec<_> = self.supported_input_configs().unwrap().collect();
        configs.sort_by(|a, b| b.cmp_default_heuristics(a));
        let config = configs
            .into_iter()
            .next()
            .ok_or(DefaultStreamConfigError::StreamTypeNotSupported)?
            .with_max_sample_rate();

        Ok(config)
    }

    fn default_output_config(&self) -> Result<SupportedStreamConfig, DefaultStreamConfigError> {
        let mut configs: Vec<_> = self.supported_output_configs().unwrap().collect();
        configs.sort_by(|a, b| b.cmp_default_heuristics(a));
        let config = configs
            .into_iter()
            .next()
            .ok_or(DefaultStreamConfigError::StreamTypeNotSupported)?
            .with_max_sample_rate();
        Ok(config)
    }

    fn build_input_stream_raw<D, E>(
        &self,
        config: &StreamConfig,
        sample_format: SampleFormat,
        mut data_callback: D,
        mut error_callback: E,
    ) -> Result<Self::Stream, BuildStreamError>
    where
        D: FnMut(&Data, &InputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        let builder =
            builder_for_device(self, config, sample_format, aaudio_sys::Direction::Input)?;
        let creation_time = Instant::now();
        let stream = builder
            .set_callbacks(
                move |stream, data, _num_frames| {
                    let sample_format = to_sample_format(stream.get_format());
                    data_callback(
                        &unsafe {
                            Data::from_parts(
                                data.as_ptr() as *mut _,
                                data.len() / sample_format.sample_size(),
                                sample_format,
                            )
                        },
                        &get_input_callback_info(stream, &creation_time),
                    );
                    aaudio_sys::CallbackResult::Continue
                },
                move |_stream, err| error_callback(StreamError::from(err)),
            )
            .open_stream()?;
        Ok(Stream(RefCell::new(stream)))
    }

    fn build_output_stream_raw<D, E>(
        &self,
        config: &StreamConfig,
        sample_format: SampleFormat,
        mut data_callback: D,
        mut error_callback: E,
    ) -> Result<Self::Stream, BuildStreamError>
    where
        D: FnMut(&mut Data, &OutputCallbackInfo) + Send + 'static,
        E: FnMut(StreamError) + Send + 'static,
    {
        let builder =
            builder_for_device(self, config, sample_format, aaudio_sys::Direction::Output)?;
        let creation_time = Instant::now();
        let stream = builder
            .set_callbacks(
                move |stream, data, _num_frames| {
                    let sample_format = to_sample_format(stream.get_format());
                    data_callback(
                        &mut unsafe {
                            Data::from_parts(
                                data.as_ptr() as *mut _,
                                data.len() / sample_format.sample_size(),
                                sample_format,
                            )
                        },
                        &get_output_callback_info(stream, &creation_time),
                    );
                    aaudio_sys::CallbackResult::Continue
                },
                move |_stream, err| error_callback(StreamError::from(err)),
            )
            .open_stream()?;
        Ok(Stream(RefCell::new(stream)))
    }
}

impl StreamTrait for Stream {
    fn play(&self) -> Result<(), PlayStreamError> {
        self.0
            .borrow_mut()
            .request_start()
            .map_err(PlayStreamError::from)
    }

    fn pause(&self) -> Result<(), PauseStreamError> {
        self.0
            .borrow_mut()
            .request_pause()
            .map_err(PauseStreamError::from)
    }
}
