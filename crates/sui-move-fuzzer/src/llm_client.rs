// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared LLM API client used by llm_seed_gen and fuzz_loop.

/// Call an LLM via the OpenAI-compatible chat completions API (OpenRouter default).
///
/// OpenRouter endpoint:  https://openrouter.ai/api/v1/chat/completions
/// Anthropic direct:     https://api.anthropic.com/v1/messages  (different shape)
pub fn call_llm_api(
    api_key: &str,
    model: &str,
    api_url: &str,
    prompt: &str,
) -> Result<String, String> {
    let client = reqwest::blocking::Client::new();

    // OpenAI-compatible format used by OpenRouter (and most other providers).
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "messages": [
            {"role": "user", "content": prompt}
        ]
    });

    let response = client
        .post(api_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        // OpenRouter asks for these headers for routing/analytics.
        .header("HTTP-Referer", "https://github.com/MystenLabs/sui")
        .header("X-Title", "sui-move-fuzzer")
        .json(&body)
        .send()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status();
    let text = response
        .text()
        .map_err(|e| format!("failed to read response body: {e}"))?;

    if !status.is_success() {
        return Err(format!("API returned {status}: {text}"));
    }

    let parsed: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("failed to parse response JSON: {e}"))?;

    // OpenAI-compatible response: choices[0].message.content
    parsed["choices"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|choice| choice["message"]["content"].as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("unexpected response shape: {text}"))
}
