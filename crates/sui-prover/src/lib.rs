pub mod prove;
pub mod llm_explain;
pub mod prompts;

pub use prove::{execute, GeneralConfig, BuildConfig}; 
pub use llm_explain::explain_err;