use std::convert::TryInto;
use std::time::Duration;

extern crate aaudio_sys;

use crate::{
    BackendSpecificError, BuildStreamError, PauseStreamError, PlayStreamError, StreamError,
    StreamInstant,
};

pub fn to_stream_instant(duration: Duration) -> StreamInstant {
    StreamInstant::new(
        duration.as_secs().try_into().unwrap(),
        duration.subsec_nanos(),
    )
}

impl From<aaudio_sys::Error> for StreamError {
    fn from(error: aaudio_sys::Error) -> Self {
        use self::aaudio_sys::Error::*;
        match error {
            Disconnected | Unavailable => Self::DeviceNotAvailable,
            e => (BackendSpecificError {
                description: e.to_string(),
            })
            .into(),
        }
    }
}

impl From<aaudio_sys::Error> for PlayStreamError {
    fn from(error: aaudio_sys::Error) -> Self {
        use self::aaudio_sys::Error::*;
        match error {
            Disconnected | Unavailable => Self::DeviceNotAvailable,
            e => (BackendSpecificError {
                description: e.to_string(),
            })
            .into(),
        }
    }
}

impl From<aaudio_sys::Error> for PauseStreamError {
    fn from(error: aaudio_sys::Error) -> Self {
        use self::aaudio_sys::Error::*;
        match error {
            Disconnected | Unavailable => Self::DeviceNotAvailable,
            e => (BackendSpecificError {
                description: e.to_string(),
            })
            .into(),
        }
    }
}

impl From<aaudio_sys::Error> for BuildStreamError {
    fn from(error: aaudio_sys::Error) -> Self {
        use self::aaudio_sys::Error::*;
        match error {
            Disconnected | Unavailable => Self::DeviceNotAvailable,
            NoFreeHandles => Self::StreamIdOverflow,
            InvalidFormat | InvalidRate => Self::StreamConfigNotSupported,
            IllegalArgument => Self::InvalidArgument,
            e => (BackendSpecificError {
                description: e.to_string(),
            })
            .into(),
        }
    }
}
