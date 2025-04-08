use anyhow::Error;
use openai_api_rust::chat::*;
use openai_api_rust::*;
use regex::Regex;
use std::fs;
use std::path::PathBuf;

use crate::prompts::ERROR_ANALYSIS_PROMPT;

async fn make_openai_request(prompt: &str) -> Result<String, Error> {
    let auth = Auth::from_env().unwrap();
    let openai = OpenAI::new(auth, "https://api.openai.com/v1/");

    let body = ChatBody {
        model: "gpt-4o-mini".to_string(),
        messages: vec![Message {
            role: Role::User,
            content: prompt.to_string(),
        }],
        temperature: Some(0.3),
        top_p: None,
        n: Some(1),
        stream: Some(false),
        stop: None,
        max_tokens: None,
        presence_penalty: None,
        frequency_penalty: None,
        logit_bias: None,
        user: None,
    };

    let response = openai.chat_completion_create(&body);
    let choice = response.unwrap().choices;
    let message = &choice[0].message.as_ref().unwrap();

    Ok(message.content.clone())
}

pub fn parse_error_file_path(output: &str) -> Option<String> {
    let re = Regex::new(r".?\/([^:]+)").unwrap();
    re.captures(output)
        .map(|caps| caps.get(1).unwrap().as_str().to_string())
}

pub async fn explain_err(output: &str, err: &Error) {
    let error_file = PathBuf::from("error_output.txt");

    if let Some(file_path) = parse_error_file_path(output) {
        fs::write(
            &error_file,
            format!("Error location: {}\n {}\n {}\n", file_path, output, err),
        )
        .unwrap();

        match fs::read_to_string(&file_path) {
            Ok(contents) => {
                let prompt = ERROR_ANALYSIS_PROMPT
                    .replace("{file_path}", &file_path)
                    .replace("{error_output}", output)
                    .replace("{file_contents}", &contents);

                match make_openai_request(&prompt).await {
                    Ok(explanation) => println!("AI Explanation:\n{}", explanation),
                    Err(e) => eprintln!("Failed to get AI explanation: {}", e),
                }
            }
            Err(e) => eprintln!("Failed to read file: {}", e),
        }
    }
}
