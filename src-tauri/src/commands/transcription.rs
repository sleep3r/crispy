// Commands for transcription: start, get result, open result window.

use crate::commands::models::SelectedModelState;
use crate::managers::transcription::{
    load_transcription_chat_history, load_transcription_metadata, load_transcription_result,
    save_transcription_chat_history, save_transcription_metadata, save_transcription_result,
    wav_to_16k_mono_f32, ChatHistoryMessage, TranscriptionManager,
};
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs,
    },
    Client,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Serialize)]
pub struct TranscriptionStatusEvent {
    pub recording_path: String,
    pub status: String, // "started" | "completed" | "error"
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionOpenEvent {
    pub recording_path: String,
}

#[tauri::command]
pub async fn start_transcription(
    app: AppHandle,
    recording_path: String,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    selected_model_state: State<'_, SelectedModelState>,
) -> Result<(), String> {
    let _ = app.emit(
        "transcription-status",
        TranscriptionStatusEvent {
            recording_path: recording_path.clone(),
            status: "started".to_string(),
            error: None,
        },
    );

    let app_clone = app.clone();
    let path_clone = recording_path.clone();
    let tm = Arc::clone(transcription_manager.inner());
    let sel = selected_model_state.0.clone();

    std::thread::spawn(move || {
        let result = run_transcription(&app_clone, &path_clone, &tm, &sel);
        let (status, err) = match result {
            Ok(()) => ("completed".to_string(), None),
            Err(e) => ("error".to_string(), Some(e.to_string())),
        };
        let _ = app_clone.emit(
            "transcription-status",
            TranscriptionStatusEvent {
                recording_path: path_clone,
                status,
                error: err,
            },
        );
    });

    Ok(())
}

fn run_transcription(
    app: &AppHandle,
    recording_path: &str,
    tm: &TranscriptionManager,
    selected_model: &Arc<std::sync::Mutex<String>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let model_id = {
        let sel = selected_model.lock().map_err(|e| e.to_string())?;
        sel.clone()
    };
    if model_id.is_empty() || model_id == "none" {
        return Err("No transcription model selected. Choose a model in Settings.".into());
    }

    let audio = wav_to_16k_mono_f32(Path::new(recording_path))?;
    if audio.is_empty() {
        save_transcription_result(app, recording_path, "")?;
        save_transcription_metadata(app, recording_path, &model_id)?;
        return Ok(());
    }

    let current = tm.get_current_model();
    if current.as_deref() != Some(model_id.as_str()) {
        tm.load_model(&model_id)?;
    }
    let text = tm.transcribe(audio)?;
    save_transcription_result(app, recording_path, &text)?;
    save_transcription_metadata(app, recording_path, &model_id)?;
    Ok(())
}

#[tauri::command]
pub async fn get_transcription_result(
    app: AppHandle,
    recording_path: String,
) -> Result<Option<String>, String> {
    load_transcription_result(&app, &recording_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_transcription_model(
    app: AppHandle,
    recording_path: String,
) -> Result<Option<String>, String> {
    load_transcription_metadata(&app, &recording_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_transcription_window(app: AppHandle, recording_path: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("transcription-result") {
        let _ = window.emit(
            "transcription-open",
            TranscriptionOpenEvent {
                recording_path: recording_path.clone(),
            },
        );
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let encoded = urlencoding::encode(&recording_path);
    let url = WebviewUrl::App(format!("index.html?recording_path={}", encoded).into());
    WebviewWindowBuilder::new(&app, "transcription-result", url)
        .title("Transcription Result")
        .inner_size(500.0, 400.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn has_transcription_result(
    app: AppHandle,
    recording_path: String,
) -> Result<bool, String> {
    let path = crate::managers::transcription::transcription_result_path(&app, &recording_path)
        .map_err(|e| e.to_string())?;
    Ok(path.exists())
}

/// Get LLM settings (endpoint and model, omit API key for security)
#[tauri::command]
pub async fn get_llm_settings(app: AppHandle) -> Result<crate::llm_settings::LlmSettingsPublic, String> {
    let settings = crate::llm_settings::load_llm_settings(&app).map_err(|e| e.to_string())?;
    Ok(crate::llm_settings::LlmSettingsPublic {
        endpoint: settings.endpoint,
        model: settings.model,
    })
}

/// Set LLM settings (endpoint, API key, model)
#[tauri::command]
pub async fn set_llm_settings(
    app: AppHandle,
    endpoint: String,
    api_key: String,
    model: String,
) -> Result<(), String> {
    let settings = crate::llm_settings::LlmSettings {
        endpoint,
        api_key,
        model,
    };
    crate::llm_settings::save_llm_settings(&app, &settings).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessageDto {
    pub role: String, // "user" | "assistant"
    pub content: String,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionChatStreamEvent {
    pub chat_id: String,
    pub delta: String,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionChatDoneEvent {
    pub chat_id: String,
}

/// Stream LLM chat responses based on transcription + conversation history
#[tauri::command]
pub async fn stream_transcription_chat(
    app: AppHandle,
    recording_path: String,
    messages: Vec<ChatMessageDto>,
    chat_id: String,
) -> Result<(), String> {
    let app_clone = app.clone();
    tokio::spawn(async move {
        if let Err(e) = do_stream_chat(&app_clone, &recording_path, messages, &chat_id).await {
            let _ = app_clone.emit(
                "transcription-chat-error",
                TranscriptionChatStreamEvent {
                    chat_id,
                    delta: format!("Error: {}", e),
                },
            );
        }
    });
    Ok(())
}

/// Load saved chat history for a transcription.
#[tauri::command]
pub async fn get_transcription_chat_history(
    app: AppHandle,
    recording_path: String,
) -> Result<Vec<ChatMessageDto>, String> {
    let messages = load_transcription_chat_history(&app, &recording_path).map_err(|e| e.to_string())?;
    Ok(messages
        .into_iter()
        .map(|m| ChatMessageDto {
            role: m.role,
            content: m.content,
        })
        .collect())
}

/// Save chat history for a transcription.
#[tauri::command]
pub async fn set_transcription_chat_history(
    app: AppHandle,
    recording_path: String,
    messages: Vec<ChatMessageDto>,
) -> Result<(), String> {
    let normalized: Vec<ChatHistoryMessage> = messages
        .into_iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| ChatHistoryMessage {
            role: m.role,
            content: m.content,
        })
        .collect();
    save_transcription_chat_history(&app, &recording_path, &normalized)
        .map_err(|e| e.to_string())
}

async fn do_stream_chat(
    app: &AppHandle,
    recording_path: &str,
    messages: Vec<ChatMessageDto>,
    chat_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let settings = crate::llm_settings::load_llm_settings(app)?;
    if settings.api_key.is_empty() {
        return Err("API key not configured. Set it in Settings.".into());
    }

    let transcription = load_transcription_result(app, recording_path)?
        .unwrap_or_else(|| "(No transcription)".to_string());

    let config = OpenAIConfig::new()
        .with_api_key(&settings.api_key)
        .with_api_base(&settings.endpoint);
    let client = Client::with_config(config);

    let mut openai_messages = vec![
        ChatCompletionRequestSystemMessageArgs::default()
            .content(format!(
                "You are a helpful assistant. The user has a transcription:\n\n{}\n\nAnswer questions about it.",
                transcription
            ))
            .build()?
            .into(),
    ];

    for msg in messages {
        let role: ChatCompletionRequestMessage = match msg.role.as_str() {
            "user" => ChatCompletionRequestUserMessageArgs::default()
                .content(msg.content)
                .build()?
                .into(),
            "assistant" => ChatCompletionRequestAssistantMessageArgs::default()
                .content(msg.content)
                .build()?
                .into(),
            _ => continue,
        };
        openai_messages.push(role);
    }

    let request = CreateChatCompletionRequestArgs::default()
        .model(&settings.model)
        .messages(openai_messages)
        .build()?;

    let mut stream = client.chat().create_stream(request).await?;

    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => {
                for choice in response.choices {
                    if let Some(ref content) = choice.delta.content {
                        let _ = app.emit(
                            "transcription-chat-stream",
                            TranscriptionChatStreamEvent {
                                chat_id: chat_id.to_string(),
                                delta: content.clone(),
                            },
                        );
                    }
                }
            }
            Err(e) => {
                return Err(format!("Stream error: {}", e).into());
            }
        }
    }

    let _ = app.emit(
        "transcription-chat-done",
        TranscriptionChatDoneEvent {
            chat_id: chat_id.to_string(),
        },
    );

    Ok(())
}
