use anyhow::Result;
use log::{info, error};
use shared::WhisperConfig;
use std::time::{SystemTime, Duration};
use std::sync::Arc;
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

pub struct WhisperManager {
    config: WhisperConfig,
    model: Option<Arc<WhisperContext>>,
    last_used: Option<SystemTime>,
}

impl WhisperManager {
    pub fn new(config: &WhisperConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            model: None,
            last_used: None,
        })
    }
    
    pub async fn ensure_loaded(&mut self) -> Result<()> {
        if self.model.is_none() {
            info!("Loading Whisper model from {:?}", self.config.model_path);
            self.load_model().await?;
        }
        
        self.last_used = Some(SystemTime::now());
        Ok(())
    }
    
    pub fn is_loaded(&self) -> bool {
        self.model.is_some()
    }
    
    async fn load_model(&mut self) -> Result<()> {
        // Check if model file exists
        if !self.config.model_path.exists() {
            return Err(anyhow::anyhow!(
                "Whisper model not found at {:?}. Please download a model file.",
                self.config.model_path
            ));
        }
        
        info!("Loading Whisper model from {:?}", self.config.model_path);
        
        // Load the model using whisper-rs in a blocking task
        let model_path = self.config.model_path.clone();
        let ctx = tokio::task::spawn_blocking(move || {
            let params = WhisperContextParameters::default();
            WhisperContext::new_with_params(&model_path.to_string_lossy(), params)
        }).await??;
        
        self.model = Some(Arc::new(ctx));
        
        info!("Whisper model loaded successfully");
        Ok(())
    }
    
    pub async fn unload_if_timeout(&mut self) -> Result<()> {
        if let Some(last_used) = self.last_used {
            let timeout_duration = Duration::from_secs(self.config.model_timeout_seconds);
            
            if last_used.elapsed().unwrap_or(Duration::ZERO) > timeout_duration {
                info!("Unloading Whisper model due to timeout");
                self.model = None;
                self.last_used = None;
            }
        }
        
        Ok(())
    }
    
    pub async fn transcribe_audio(&mut self, audio_data: &[f32]) -> Result<String> {
        let ctx = self.model.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Whisper model not loaded"))?;
        
        // Clone data needed for the blocking task
        let ctx_clone = Arc::clone(ctx);
        let audio_data = audio_data.to_vec();
        let language = self.config.language.clone();
        
        // Run transcription in a blocking task since whisper-rs is sync
        let transcription_result = Self::transcribe_blocking(ctx_clone, audio_data, language).await?;
        
        // Update last used time after transcription
        self.last_used = Some(SystemTime::now());
        
        match transcription_result {
            Ok(transcription) => {
                if transcription.trim().is_empty() {
                    Ok("[No speech detected]".to_string())
                } else {
                    Ok(transcription.trim().to_string())
                }
            }
            Err(e) => {
                error!("Transcription failed: {:?}", e);
                Err(anyhow::anyhow!("Transcription failed: {:?}", e))
            }
        }
    }
    
    async fn transcribe_blocking(
        ctx: Arc<WhisperContext>,
        audio_data: Vec<f32>,
        language: Option<String>,
    ) -> Result<Result<String, whisper_rs::WhisperError>> {
        tokio::task::spawn_blocking(move || {
            // Create a state from the context
            let mut state = ctx.create_state()?;
            
            // Create parameters for transcription
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            
            // Set language if specified
            if let Some(ref lang) = language {
                params.set_language(Some(lang));
            }
            
            // Set other parameters based on config
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            
            // Run the transcription
            state.full(params, &audio_data)?;
            
            // Get the transcribed text
            let num_segments = state.full_n_segments()?;
            let mut transcription = String::new();
            
            for i in 0..num_segments {
                let segment_text = state.full_get_segment_text(i)?;
                transcription.push_str(&segment_text);
            }
            
            Ok(transcription)
        }).await.map_err(|e| anyhow::anyhow!("Task failed: {}", e))
    }
}