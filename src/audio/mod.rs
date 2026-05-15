use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;

pub struct AudioManager {
    inner:  Option<Inner>,
    volume: f32,
}

struct Inner {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    sink: Option<Sink>,
}

impl AudioManager {
    pub fn new() -> Self {
        let inner = match OutputStream::try_default() {
            Ok((stream, handle)) => {
                println!("[audio] Output device initialised");
                Some(Inner { _stream: stream, handle, sink: None })
            }
            Err(e) => {
                eprintln!("[audio] No audio output device: {e}");
                None
            }
        };
        Self { inner, volume: 1.0 }
    }

    pub fn play_music(&mut self, path: &str) {
        let Some(inner) = &mut self.inner else {
            eprintln!("[audio] play_music called but no device");
            return;
        };
        if let Some(s) = inner.sink.take() { s.stop(); }

        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => { eprintln!("[audio] Cannot open '{path}': {e}"); return; }
        };
        let source = match Decoder::new(BufReader::new(file)) {
            Ok(d) => d.buffered().repeat_infinite(),
            Err(e) => { eprintln!("[audio] Cannot decode '{path}': {e}"); return; }
        };
        match Sink::try_new(&inner.handle) {
            Ok(sink) => {
                sink.set_volume(self.volume);
                sink.append(source);
                inner.sink = Some(sink);
                println!("[audio] Playing '{path}'");
            }
            Err(e) => eprintln!("[audio] Sink error: {e}"),
        }
    }

    pub fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 1.0);
        if let Some(inner) = &self.inner {
            if let Some(sink) = &inner.sink {
                sink.set_volume(self.volume);
            }
        }
    }

    pub fn play_sound(&self, path: &str) {
        let Some(inner) = &self.inner else { return; };
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => { eprintln!("[audio] Cannot open '{path}': {e}"); return; }
        };
        let source = match Decoder::new(BufReader::new(file)) {
            Ok(d) => d,
            Err(e) => { eprintln!("[audio] Cannot decode '{path}': {e}"); return; }
        };
        match Sink::try_new(&inner.handle) {
            Ok(sink) => { sink.append(source); sink.detach(); }
            Err(e) => eprintln!("[audio] Sink error: {e}"),
        }
    }

    pub fn stop_music(&mut self) {
        if let Some(inner) = &mut self.inner {
            if let Some(s) = inner.sink.take() { s.stop(); }
        }
    }
}
