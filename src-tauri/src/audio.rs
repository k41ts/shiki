//! Microphone capture into an in-memory 16-bit mono WAV.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use std::{
    io::Cursor,
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct Recording {
    stop: mpsc::Sender<()>,
    handle: JoinHandle<Option<Vec<u8>>>,
}

impl Recording {
    /// Returns WAV bytes, or None if it was too short or silent.
    pub fn stop(self) -> Option<Vec<u8>> {
        let _ = self.stop.send(());
        self.handle.join().ok().flatten()
    }
}

type LevelFn = Arc<dyn Fn(f32) + Send + Sync>;

struct Sink {
    samples: Arc<Mutex<Vec<i16>>>,
    on_level: LevelFn,
    last: Mutex<Instant>,
}

impl Sink {
    fn push<T: SizedSample>(&self, data: &[T], channels: usize)
    where
        f32: FromSample<T>,
    {
        let mut buf = self.samples.lock().unwrap();
        let mut sum = 0.0f32;
        let mut n = 0usize;
        for frame in data.chunks(channels) {
            let v = frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32;
            buf.push((v.clamp(-1.0, 1.0) * 32767.0) as i16);
            sum += v * v;
            n += 1;
        }
        drop(buf);
        let mut last = self.last.lock().unwrap();
        if n > 0 && last.elapsed() >= Duration::from_millis(50) {
            *last = Instant::now();
            (self.on_level)((sum / n as f32).sqrt());
        }
    }
}

fn build<T: SizedSample>(
    dev: &cpal::Device,
    cfg: &cpal::StreamConfig,
    sink: Arc<Sink>,
) -> Result<cpal::Stream, cpal::Error>
where
    f32: FromSample<T>,
{
    let channels = cfg.channels as usize;
    dev.build_input_stream(
        cfg.clone(),
        move |data: &[T], _: &cpal::InputCallbackInfo| sink.push(data, channels),
        |e| eprintln!("audio error: {e}"),
        None,
    )
}

pub fn start(on_level: impl Fn(f32) + Send + Sync + 'static) -> Result<Recording, String> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let on_level: LevelFn = Arc::new(on_level);

    // cpal streams aren't Send, so the stream lives entirely on this thread.
    let handle = thread::spawn(move || -> Option<Vec<u8>> {
        let setup = || -> Result<(cpal::Stream, u32, Arc<Mutex<Vec<i16>>>), String> {
            let dev = cpal::default_host()
                .default_input_device()
                .ok_or("Tidak ada mikrofon yang terdeteksi.")?;
            let supported = dev.default_input_config().map_err(|e| e.to_string())?;
            let format = supported.sample_format();
            let cfg: cpal::StreamConfig = supported.into();
            let samples = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::new(Sink { samples: samples.clone(), on_level, last: Mutex::new(Instant::now()) });
            let stream = match format {
                cpal::SampleFormat::F32 => build::<f32>(&dev, &cfg, sink),
                cpal::SampleFormat::I16 => build::<i16>(&dev, &cfg, sink),
                cpal::SampleFormat::U16 => build::<u16>(&dev, &cfg, sink),
                cpal::SampleFormat::I32 => build::<i32>(&dev, &cfg, sink),
                other => return Err(format!("Format mikrofon tidak didukung: {other:?}")),
            }
            .map_err(|e| e.to_string())?;
            stream.play().map_err(|e| e.to_string())?;
            Ok((stream, cfg.sample_rate as u32, samples))
        };

        let (stream, rate, samples) = match setup() {
            Ok(v) => {
                let _ = ready_tx.send(Ok(()));
                v
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                return None;
            }
        };
        let _ = stop_rx.recv();
        drop(stream);

        let samples = samples.lock().unwrap();
        let peak = samples.iter().map(|s| (*s as i32).abs()).max().unwrap_or(0);
        if (samples.len() as f32) < rate as f32 * 0.4 || peak < 300 {
            return None;
        }
        let mut out = Cursor::new(Vec::new());
        let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::new(&mut out, spec).ok()?;
        for s in samples.iter() {
            w.write_sample(*s).ok()?;
        }
        w.finalize().ok()?;
        Some(out.into_inner())
    });

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(Recording { stop: stop_tx, handle }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Mikrofon gagal dibuka.".into()),
    }
}
