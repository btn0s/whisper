use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
}

pub struct LlmClient {
    client: reqwest::Client,
    model: String,
    base_url: String,
}

impl LlmClient {
    pub fn new(model: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            model: model.to_string(),
            base_url: "http://localhost:11434".to_string(),
        }
    }

    pub async fn reformat(
        &self,
        raw_transcript: &str,
        file_context: Option<&str>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let system = format!(
            "You are a dictation assistant. Clean up the following raw speech transcript into \
             well-formed text. Fix grammar, punctuation, and formatting. Preserve the speaker's \
             intent exactly — do not add, remove, or rephrase content.\n\
             \n\
             If the transcript references code or file names, use the project context below to \
             resolve them to their correct names.\n\
             \n\
             Output ONLY the cleaned text, nothing else.\n\
             \n\
             {}",
            file_context
                .map(|ctx| format!("Project files:\n{}", ctx))
                .unwrap_or_default()
        );

        let request = OllamaRequest {
            model: self.model.clone(),
            prompt: raw_transcript.to_string(),
            stream: false,
            system: Some(system),
        };

        let resp = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("Ollama returned status: {}", resp.status()).into());
        }

        let body: OllamaResponse = resp.json().await?;
        Ok(body.response.trim().to_string())
    }
}
