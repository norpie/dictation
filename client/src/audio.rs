use anyhow::{Result, Context};
use cpal::{Device, Host, StreamConfig, SampleFormat, SampleRate};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{info, debug, error, warn};
use shared::{AudioChunk, AudioConfig};
use tokio::sync::mpsc;
use std::time::SystemTime;
use uuid::Uuid;

pub struct AudioCapture {
    config: AudioConfig,
    session_id: Option<Uuid>,
    host: Host,
    device: Device,
}

impl AudioCapture {
    pub fn new(config: &AudioConfig) -> Result<Self> {
        let host = cpal::default_host();
        
        let device = if let Some(device_name) = &config.device {
            // Try to find the specified device
            host.input_devices()?
                .find(|d| d.name().map(|n| n == *device_name).unwrap_or(false))
                .context(format!("Audio device '{}' not found", device_name))?
        } else {
            // Use default device
            host.default_input_device()
                .context("No default input device available")?
        };
        
        info!("Using audio device: {}", device.name().unwrap_or_else(|_| "Unknown".to_string()));
        
        Ok(Self {
            config: config.clone(),
            session_id: None,
            host,
            device,
        })
    }
    
    pub fn start_recording(
        &mut self,
        session_id: Uuid,
    ) -> Result<mpsc::Receiver<AudioChunk>> {
        if self.session_id.is_some() {
            return Err(anyhow::anyhow!("Recording already active"));
        }
        
        self.session_id = Some(session_id);
        
        // Get supported configs and pick the best one
        let mut supported_configs = self.device.supported_input_configs()?;
        let supported_config = supported_configs
            .next()
            .context("No supported audio config found")?;
        
        // Configure stream settings
        let sample_rate = if self.config.sample_rate == 0 {
            supported_config.min_sample_rate().0.max(16000) // Prefer 16kHz for Whisper
        } else {
            self.config.sample_rate
        };
        
        let config = StreamConfig {
            channels: self.config.channels,
            sample_rate: SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Fixed(self.config.buffer_size as u32),
        };
        
        info!("Audio config: {:?}", config);
        
        let (tx, rx) = mpsc::channel(32);
        let session_id_clone = session_id;
        let sample_rate_clone = sample_rate;
        let channels_clone = self.config.channels;
        
        // Create the audio stream based on the sample format
        let stream = match supported_config.sample_format() {
            SampleFormat::I8 => {
                self.create_stream::<i8>(&config, tx, session_id_clone, sample_rate_clone, channels_clone)?
            }
            SampleFormat::I16 => {
                self.create_stream::<i16>(&config, tx, session_id_clone, sample_rate_clone, channels_clone)?
            }
            SampleFormat::I32 => {
                self.create_stream::<i32>(&config, tx, session_id_clone, sample_rate_clone, channels_clone)?
            }
            SampleFormat::I64 => {
                self.create_stream::<i64>(&config, tx, session_id_clone, sample_rate_clone, channels_clone)?
            }
            SampleFormat::U8 => {
                self.create_stream::<u8>(&config, tx, session_id_clone, sample_rate_clone, channels_clone)?
            }
            SampleFormat::U16 => {
                self.create_stream::<u16>(&config, tx, session_id_clone, sample_rate_clone, channels_clone)?
            }
            SampleFormat::U32 => {
                self.create_stream::<u32>(&config, tx, session_id_clone, sample_rate_clone, channels_clone)?
            }
            SampleFormat::U64 => {
                self.create_stream::<u64>(&config, tx, session_id_clone, sample_rate_clone, channels_clone)?
            }
            SampleFormat::F32 => {
                self.create_stream::<f32>(&config, tx, session_id_clone, sample_rate_clone, channels_clone)?
            }
            SampleFormat::F64 => {
                self.create_stream::<f64>(&config, tx, session_id_clone, sample_rate_clone, channels_clone)?
            }
            _ => {
                return Err(anyhow::anyhow!("Unsupported sample format: {:?}", supported_config.sample_format()));
            }
        };
        
        stream.play()?;
        
        // Keep the stream alive by storing it
        // The stream will be automatically dropped when the AudioCapture is dropped
        std::mem::forget(stream);
        
        Ok(rx)
    }
    
    fn create_stream<T>(
        &self,
        config: &StreamConfig,
        tx: mpsc::Sender<AudioChunk>,
        session_id: Uuid,
        sample_rate: u32,
        channels: u16,
    ) -> Result<cpal::Stream>
    where
        T: cpal::Sample + cpal::SizedSample + Send + 'static,
        f32: cpal::FromSample<T>,
    {
        let stream = self.device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                // Convert samples to f32
                let f32_data: Vec<f32> = data.iter().map(|&sample| cpal::Sample::to_sample(sample)).collect();
                
                debug!("Captured {} audio samples", f32_data.len());
                
                let chunk = AudioChunk {
                    session_id,
                    data: f32_data,
                    sample_rate,
                    channels,
                    timestamp: SystemTime::now(),
                };
                
                // Send chunk (non-blocking)
                if let Err(e) = tx.try_send(chunk) {
                    warn!("Failed to send audio chunk: {}", e);
                }
            },
            move |err| {
                error!("Audio stream error: {}", err);
            },
            None,
        )?;
        
        Ok(stream)
    }
    
    pub fn stop_recording(&mut self) {
        self.session_id = None;
        info!("Recording stopped");
    }
    
    pub fn is_recording(&self) -> bool {
        self.session_id.is_some()
    }
    
    pub fn list_input_devices() -> Result<Vec<String>> {
        let host = cpal::default_host();
        let devices: Result<Vec<String>, _> = host
            .input_devices()?
            .map(|device| device.name().map_err(|e| e.into()))
            .collect();
        devices
    }
}