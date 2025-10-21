use std::collections::BTreeMap;

use jsonrpc::types::{JsonRpcResult, RemoteError};
use serde::{Deserialize, Serialize};

use crate::{
    package::EnvironmentID,
    schema::{
        DefaultDependency, ExternalDependency, ManifestDependencyInfo, ReplacementDependency,
    },
};

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestData {
    /// Execution should be halted with [exit_code] and return the given [stdout]/[stderr]
    Stdio(Exit),

    /// [stderr] should be printed and [output] should be included in the output
    Echo(EchoRequest),
}

#[derive(Serialize, Deserialize)]
pub struct Exit {
    pub stdout: String,

    #[serde(default)]
    pub stderr: Option<String>,

    #[serde(default)]
    pub exit_code: Option<i32>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct EchoRequest {
    pub output: BTreeMap<EnvironmentID, JsonRpcResult<serde_json::Value>>,

    #[serde(default)]
    pub stderr: Option<String>,
}

pub fn reply_with_stdout(output: impl AsRef<str>) -> Exit {
    Exit {
        stdout: output.as_ref().to_string(),
        stderr: None,
        exit_code: None,
    }
}

pub fn echo() -> EchoRequest {
    EchoRequest {
        output: BTreeMap::new(),
        stderr: None,
    }
}

impl Exit {
    pub fn stderr(mut self, output: impl AsRef<str>) -> Self {
        self.stderr = Some(output.as_ref().to_string());
        self
    }

    /// Force the external resolver to
    pub fn exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    pub fn build(self) -> ReplacementDependency {
        build_replacement_dep(RequestData::Stdio(self))
    }
}

impl EchoRequest {
    pub fn reply_with_dep(self, text: impl AsRef<str>) -> Self {
        self.reply_in_env("default", text)
    }

    pub fn reply_in_env(mut self, env: impl AsRef<str>, text: impl AsRef<str>) -> Self {
        self.output.insert(
            env.as_ref().to_string(),
            JsonRpcResult::Ok {
                result: toml_edit::de::from_str(text.as_ref()).unwrap(),
            },
        );
        self
    }

    pub fn reply_with_err(mut self, env: EnvironmentID, code: i32, text: impl AsRef<str>) -> Self {
        self.output.insert(
            env,
            JsonRpcResult::Err {
                error: RemoteError {
                    code,
                    message: text.as_ref().to_string(),
                    data: None,
                },
            },
        );
        self
    }

    pub fn stderr(mut self, output: impl AsRef<str>) -> Self {
        self.stderr = Some(output.as_ref().to_string());
        self
    }

    pub fn build(self) -> ReplacementDependency {
        build_replacement_dep(RequestData::Echo(self))
    }
}

fn build_replacement_dep(data: RequestData) -> ReplacementDependency {
    ReplacementDependency {
        dependency: Some(DefaultDependency {
            dependency_info: ManifestDependencyInfo::External(ExternalDependency {
                resolver: "mock-resolver".to_string(),
                data: toml::Value::try_from(data).unwrap(),
            }),
            is_override: false,
            rename_from: None,
        }),
        addresses: None,
        use_environment: None,
    }
}
