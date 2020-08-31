use std::convert::TryInto;
use std::time::Duration;

extern crate ndk;

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

impl From<ndk::aaudio::AAudioError> for StreamError {
    fn from(error: ndk::aaudio::AAudioError) -> Self {
        use self::ndk::aaudio::AAudioError::*;
        use self::ndk::aaudio::AAudioErrorResult::*;
        match error {
            ErrorResult(Disconnected) | ErrorResult(Unavailable) => Self::DeviceNotAvailable,
            e => (BackendSpecificError {
                description: e.to_string(),
            })
            .into(),
        }
    }
}

impl From<ndk::aaudio::AAudioError> for PlayStreamError {
    fn from(error: ndk::aaudio::AAudioError) -> Self {
        use self::ndk::aaudio::AAudioError::*;
        use self::ndk::aaudio::AAudioErrorResult::*;
        match error {
            ErrorResult(Disconnected) | ErrorResult(Unavailable) => Self::DeviceNotAvailable,
            e => (BackendSpecificError {
                description: e.to_string(),
            })
            .into(),
        }
    }
}

impl From<ndk::aaudio::AAudioError> for PauseStreamError {
    fn from(error: ndk::aaudio::AAudioError) -> Self {
        use self::ndk::aaudio::AAudioError::*;
        use self::ndk::aaudio::AAudioErrorResult::*;
        match error {
            ErrorResult(Disconnected) | ErrorResult(Unavailable) => Self::DeviceNotAvailable,
            e => (BackendSpecificError {
                description: e.to_string(),
            })
            .into(),
        }
    }
}

impl From<ndk::aaudio::AAudioError> for BuildStreamError {
    fn from(error: ndk::aaudio::AAudioError) -> Self {
        use self::ndk::aaudio::AAudioError::*;
        use self::ndk::aaudio::AAudioErrorResult::*;
        match error {
            ErrorResult(Disconnected) | ErrorResult(Unavailable) => Self::DeviceNotAvailable,
            ErrorResult(NoFreeHandles) => Self::StreamIdOverflow,
            ErrorResult(InvalidFormat) | ErrorResult(InvalidRate) => Self::StreamConfigNotSupported,
            ErrorResult(IllegalArgument) => Self::InvalidArgument,
            e => (BackendSpecificError {
                description: e.to_string(),
            })
            .into(),
        }
    }
}
