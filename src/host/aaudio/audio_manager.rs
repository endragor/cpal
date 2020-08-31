extern crate jni;

use self::jni::{errors::Result as JResult, objects::JObject, JNIEnv};

use super::jni_utils::{
    call_method_no_args_ret_bool, call_method_no_args_ret_char_sequence,
    call_method_no_args_ret_int, call_method_no_args_ret_int_array, call_method_no_args_ret_string,
    get_system_service, with_attached,
};

extern crate ndk_glue;

extern crate num_derive;
extern crate num_traits;
use self::num_derive::FromPrimitive;
use self::num_traits::FromPrimitive;

pub struct AudioManager;

impl AudioManager {
    pub const GET_DEVICES_INPUTS: i32 = 1 << 0;
    pub const GET_DEVICES_OUTPUTS: i32 = 1 << 1;
    pub const GET_DEVICES_ALL: i32 = Self::GET_DEVICES_INPUTS | Self::GET_DEVICES_OUTPUTS;
}

/**
 * The Android audio device info
 */
#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    /**
     * Device identifier
     */
    pub id: i32,

    /**
     * The type of device
     */
    pub device_type: AudioDeviceType,

    /**
     * The device can be used for playback and/or capture
     */
    pub direction: AudioDeviceDirection,

    /**
     * Device address
     */
    pub address: String,

    /**
     * Device product name
     */
    pub product_name: String,

    /**
     * Available channel configurations
     */
    pub channel_counts: Vec<i32>,

    /**
     * Supported sample rates
     */
    pub sample_rates: Vec<i32>,

    /**
     * Supported audio formats
     */
    pub formats: Vec<AudioFormat>,
}

/**
 * The type of audio device
 */
#[derive(Debug, Clone, Copy, FromPrimitive)]
#[repr(i32)]
pub enum AudioDeviceType {
    Unknown = 0,
    AuxLine = 19,
    BluetoothA2DP = 8,
    BluetoothSCO = 7,
    BuiltinEarpiece = 1,
    BuiltinMic = 15,
    BuiltinSpeaker = 2,
    Bus = 21,
    Dock = 13,
    Fm = 14,
    FmTuner = 16,
    Hdmi = 9,
    HdmiArc = 10,
    HearingAid = 23,
    Ip = 20,
    LineAnalog = 5,
    LineDigital = 6,
    Telephony = 18,
    TvTuner = 17,
    UsbAccessory = 12,
    UsbDevice = 11,
    UsbHeadset = 22,
    UsbHeadphones = 4,
    WiredHeadset = 3,
}

/**
 * The direction of audio device
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum AudioDeviceDirection {
    Input = AudioManager::GET_DEVICES_INPUTS,
    Output = AudioManager::GET_DEVICES_OUTPUTS,
    InputOutput = AudioManager::GET_DEVICES_ALL,
}

impl AudioDeviceDirection {
    pub fn new(is_input: bool, is_output: bool) -> Option<Self> {
        use self::AudioDeviceDirection::*;
        match (is_input, is_output) {
            (true, true) => Some(InputOutput),
            (false, true) => Some(Output),
            (true, false) => Some(Input),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    I16,
    F32,
}

impl AudioFormat {
    const ENCODING_PCM_16BIT: i32 = 2;
    const ENCODING_PCM_FLOAT: i32 = 4;

    pub(crate) fn from_encoding(encoding: i32) -> Option<AudioFormat> {
        match encoding {
            AudioFormat::ENCODING_PCM_16BIT => Some(AudioFormat::I16),
            AudioFormat::ENCODING_PCM_FLOAT => Some(AudioFormat::F32),
            _ => None,
        }
    }
}

impl AudioDeviceInfo {
    /**
     * Request audio devices using Android Java API
     */
    pub fn request(direction: AudioDeviceDirection) -> Result<Vec<AudioDeviceInfo>, String> {
        let activity = ndk_glue::native_activity();
        let sdk_version = activity.sdk_version();

        if sdk_version >= 23 {
            with_attached(|env, activity| try_request_devices_info(env, activity, direction))
                .map_err(|error| error.to_string())
        } else {
            Err("Method unsupported".into())
        }
    }
}

fn get_devices<'a: 'b, 'b>(
    env: &'b JNIEnv<'a>,
    subject: JObject<'a>,
    flags: i32,
) -> JResult<JObject<'a>> {
    env.call_method(
        subject,
        "getDevices",
        "(I)[Landroid/media/AudioDeviceInfo;",
        &[flags.into()],
    )?
    .l()
}

fn try_request_devices_info<'a>(
    env: &JNIEnv<'a>,
    activity: JObject,
    direction: AudioDeviceDirection,
) -> JResult<Vec<AudioDeviceInfo>> {
    let audio_manager = get_system_service(env, activity, "audio")?;

    let devices = env.auto_local(get_devices(&env, audio_manager, direction as i32)?);

    let raw_devices = devices.as_obj().into_inner();

    let length = env.get_array_length(raw_devices)?;

    (0..length)
        .into_iter()
        .map(|index| {
            let device = env.get_object_array_element(raw_devices, index)?;

            Ok(AudioDeviceInfo {
                id: call_method_no_args_ret_int(&env, device, "getId")?,
                address: call_method_no_args_ret_string(&env, device, "getAddress")?,
                product_name: call_method_no_args_ret_char_sequence(
                    &env,
                    device,
                    "getProductName",
                )?,
                device_type: FromPrimitive::from_i32(call_method_no_args_ret_int(
                    &env, device, "getType",
                )?)
                .unwrap(),
                direction: AudioDeviceDirection::new(
                    call_method_no_args_ret_bool(&env, device, "isSource")?,
                    call_method_no_args_ret_bool(&env, device, "isSink")?,
                )
                .ok_or_else(|| "Invalid device direction")?,
                channel_counts: call_method_no_args_ret_int_array(
                    &env,
                    device,
                    "getChannelCounts",
                )?,
                sample_rates: call_method_no_args_ret_int_array(&env, device, "getSampleRates")?,
                formats: call_method_no_args_ret_int_array(&env, device, "getEncodings")?
                    .into_iter()
                    .map(AudioFormat::from_encoding)
                    .filter(Option::is_some)
                    .map(Option::unwrap)
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Result<Vec<_>, _>>()
}
