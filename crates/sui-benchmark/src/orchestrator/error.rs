// #[macro_export]
// macro_rules! bail {
//     ($e:expr) => {
//         return Err($e)
//     };
// }

#[macro_export(local_inner_macros)]
macro_rules! ensure {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            return Err($e);
        }
    };
}

pub type SettingsResult<T> = Result<T, SettingsError>;

#[derive(thiserror::Error, Debug)]
pub enum SettingsError {
    #[error("Failed to read settings file '{file:?}': {message}")]
    InvalidSettings { file: String, message: String },

    #[error("Failed to read token file '{file:?}': {message}")]
    InvalidTokenFile { file: String, message: String },

    #[error("Failed to read ssh public key file '{file:?}': {message}")]
    InvalidSshPublicKeyFile { file: String, message: String },
}

pub type CloudProviderResult<T> = Result<T, CloudProviderError>;

#[derive(thiserror::Error, Debug)]
pub enum CloudProviderError {
    #[error("Failed to send server request: {0}")]
    RequestError(String),

    #[error("Unexpected response: {0}")]
    UnexpectedResponse(String),

    #[error("Received error status code ({0}): {1}")]
    FailureResponseCode(String, String),

    #[error("SSH key \"{0}\" not found")]
    SshKeyNotFound(String),
}

pub type SshResult<T> = Result<T, SshError>;

#[derive(thiserror::Error, Debug)]
pub enum SshError {
    #[error("Failed to create ssh session: {0}")]
    SessionError(#[from] ssh2::Error),

    #[error("Failed to connect to instance: {0}")]
    ConnectionError(#[from] std::io::Error),

    #[error("Remote execution returned exit code ({0}): {1}")]
    NonZeroExitCode(i32, String),
}

pub type TestbedResult<T> = Result<T, TestbedError>;

#[derive(thiserror::Error, Debug)]
pub enum TestbedError {
    #[error(transparent)]
    SettingsError(#[from] SettingsError),

    #[error(transparent)]
    CloudProviderError(#[from] CloudProviderError),

    #[error(transparent)]
    SshError(#[from] SshError),

    #[error("Missing instances: {0}")]
    InsufficientCapacity(String),
}
