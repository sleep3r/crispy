use std::sync::{Arc, Mutex};

use crate::audio::AudioMonitorState;
use crate::recording::RecordingState;

pub struct AppState {
    pub audio: Arc<Mutex<AudioMonitorState>>,
    pub recording: Arc<Mutex<RecordingState>>,
}
