use anyhow::Error;
use regex::Regex;
use std::fs;
use openai_api_rust::*;
use openai_api_rust::chat::*;

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
    re.captures(output).map(|caps| caps.get(1).unwrap().as_str().to_string())
}

pub async fn explain_err(output: &str, err: &Error) {
    println!("Error output: {}", output);
    eprintln!("Error itself: {}", err);

    if let Some(file_path) = parse_error_file_path(output) {
        println!("Error location: {}", file_path);

        match fs::read_to_string(&file_path) {
            Ok(contents) => {
                let prompt = format!(
                    "I got a Move compiler error in the following file:\n\n\
                    File path: {}\n\
                    Error output:\n\
                    {}\n\n\
                    The relevant file content is below:\n\n\
                    {}\n\n\
                    Please analyze this specific error only.\n\n\
                    1. Explain clearly what caused this specific compiler error.\n\
                    2. Point to the exact line(s) or construct in the Move code that triggered it.\n\
                    3. Suggest a corrected version of the code.\n\
                    4. Don't make assumptions beyond this file.\n\
                    5. Do NOT explain unrelated or general issues in the file.",
                    file_path,
                    output,
                    contents
                );

                match make_openai_request(&prompt).await {
                    Ok(explanation) => println!("AI Explanation:\n{}", explanation),
                    Err(e) => eprintln!("Failed to get AI explanation: {}", e),
                }
            }
            Err(e) => eprintln!("Failed to read file: {}", e),
        }
    }
}
