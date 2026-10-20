//! The default input device through cpal. The stream lives on its own
//! thread, so nothing here depends on whether a cpal `Stream` is `Send`.

use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};

use super::{AudioError, Capture, Format, Privacy, Result, Sink};

pub trait Microphone: Send + Sync {
    fn open(&self, sink: Sink) -> Result<Box<dyn Capture>>;
}

/// The system default input at its default format.
pub struct CpalMicrophone;

impl Microphone for CpalMicrophone {
    fn open(&self, sink: Sink) -> Result<Box<dyn Capture>> {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let lost = stop_tx.clone();
        let thread = thread::Builder::new().name("mic".into()).spawn(move || {
            let stream = match start(sink, lost) {
                Ok((stream, format)) => {
                    let _ = ready_tx.send(Ok(format));
                    stream
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            // Returns when `MicCapture` drops or the device is lost; either
            // way dropping the stream drops the sink, ending its writer.
            let _ = stop_rx.recv();
            drop(stream);
        })?;
        let format = ready_rx
            .recv()
            .map_err(|_| AudioError::Device("microphone thread exited".into()))?;
        let format = match format {
            Ok(format) => format,
            Err(e) => {
                let _ = thread.join();
                return Err(e);
            }
        };
        Ok(Box::new(MicCapture {
            format,
            stop: Some(stop_tx),
            thread: Some(thread),
        }))
    }
}

struct MicCapture {
    format: Format,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Capture for MicCapture {
    fn format(&self) -> Format {
        self.format
    }
}

impl Drop for MicCapture {
    fn drop(&mut self) {
        // The stream's error callback holds a sender too, so dropping this
        // one alone would not wake the thread.
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn device_error(e: cpal::Error) -> AudioError {
    match e.kind() {
        cpal::ErrorKind::PermissionDenied => AudioError::PermissionDenied(Privacy::Microphone),
        _ => AudioError::Device(e.to_string()),
    }
}

/// Errors after which the stream delivers nothing more. Glitches, reroutes
/// and refused real-time scheduling leave it running.
fn is_fatal(e: &cpal::Error) -> bool {
    !matches!(
        e.kind(),
        cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied
    )
}

/// Starts the default input; a fatal stream error sends on `lost`.
fn start(sink: Sink, lost: mpsc::Sender<()>) -> Result<(cpal::Stream, Format)> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or(AudioError::NoInputDevice)?;
    let supported = device.default_input_config().map_err(device_error)?;
    let config = supported.config();
    let format = Format {
        sample_rate: config.sample_rate,
        channels: config.channels,
    };
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config, sink, lost),
        SampleFormat::I16 => build::<i16>(&device, config, sink, lost),
        SampleFormat::I32 => build::<i32>(&device, config, sink, lost),
        other => {
            return Err(AudioError::Device(format!(
                "unsupported sample format {other:?}"
            )))
        }
    }
    .map_err(device_error)?;
    stream.play().map_err(device_error)?;
    Ok((stream, format))
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut sink: Sink,
    lost: mpsc::Sender<()>,
) -> std::result::Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let mut buf = Vec::new();
    device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _| {
            buf.clear();
            buf.extend(data.iter().map(|&s| s.to_sample::<f32>()));
            sink(&buf);
        },
        move |e| {
            eprintln!("microphone: {e}");
            if is_fatal(&e) {
                let _ = lost.send(());
            }
        },
        None,
    )
}
