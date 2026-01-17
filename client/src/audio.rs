use anyhow::Result;
use rodio::{Decoder, OutputStream, Sink};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

// Wrapper to make AudioContext Send + Sync
pub struct AudioContext {
    _stream: Arc<Mutex<Option<OutputStream>>>,
    sink: Arc<Mutex<Sink>>,
}

impl AudioContext {
    pub fn new() -> Result<Self> {
        // Initialize rodio for playback
        let (stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;
        
        Ok(Self {
            _stream: Arc::new(Mutex::new(Some(stream))),
            sink: Arc::new(Mutex::new(sink)),
        })
    }
    
    /// Play audio from bytes buffer
    pub fn play_bytes(&self, data: Vec<u8>) -> Result<()> {
        let cursor = Cursor::new(data);
        let source = Decoder::new(cursor)?;
        
        let sink = self.sink.lock().unwrap();
        sink.append(source);
        sink.play();
        
        Ok(())
    }
    
    /// Check if audio is playing
    pub fn is_playing(&self) -> bool {
        if let Ok(sink) = self.sink.lock() {
            !sink.empty()
        } else {
            false
        }
    }
    
    pub fn stop(&self) {
        if let Ok(sink) = self.sink.lock() {
            sink.stop();
        }
    }
    
    pub fn pause(&self) {
        if let Ok(sink) = self.sink.lock() {
            sink.pause();
        }
    }
    
    pub fn resume(&self) {
        if let Ok(sink) = self.sink.lock() {
            sink.play();
        }
    }
}

// Safety: OutputStream contains non-Send raw pointers but we're keeping it behind a Mutex
// and never moving it, so it's safe to share across threads
unsafe impl Send for AudioContext {}
unsafe impl Sync for AudioContext {}
