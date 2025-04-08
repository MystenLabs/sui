pub const ERROR_ANALYSIS_PROMPT: &str = r#"I got a Move compiler error in the following file:

File path: {file_path}
Error output:
{error_output}

The relevant file content is below:

{file_contents}

Please analyze this specific error only.

1. Explain clearly what caused this specific compiler error.
2. Point to the exact line(s) or construct in the Move code that triggered it.
3. Suggest a corrected version of the code.
4. Don't make assumptions beyond this file.
5. Do NOT explain unrelated or general issues in the file."#; 