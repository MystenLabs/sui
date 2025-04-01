pub mod prove;
pub mod llm_explain;

pub use prove::{execute, GeneralConfig, BuildConfig}; 
pub use llm_explain::explain_err;