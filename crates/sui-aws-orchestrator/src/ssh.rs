// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::io::Write;
use std::sync::Arc;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use futures::future::try_join_all;
use russh::client::Msg;
use russh::{client, Channel};
use russh_keys::key;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::{
    client::Instance,
    ensure,
    error::{SshError, SshResult},
};

#[derive(PartialEq, Eq)]
/// The status of an ssh command running in the background.
pub enum CommandStatus {
    Running,
    Terminated,
}

impl CommandStatus {
    /// Return whether a background command is still running. Returns `Terminated` if the
    /// command is not running in the background.
    pub fn status(command_id: &str, text: &str) -> Self {
        if text.contains(command_id) {
            Self::Running
        } else {
            Self::Terminated
        }
    }
}

/// The command to execute on all specified remote machines.
#[derive(Clone, Default)]
pub struct CommandContext {
    /// Whether to run the command in the background (and return immediately). Commands
    /// running in the background are identified by a unique id.
    pub background: Option<String>,
    /// The path from where to execute the command.
    pub path: Option<PathBuf>,
    /// The log file to redirect all stdout and stderr.
    pub log_file: Option<PathBuf>,
}

impl CommandContext {
    /// Create a new ssh command.
    pub fn new() -> Self {
        Self {
            background: None,
            path: None,
            log_file: None,
        }
    }

    /// Set id of the command and indicate that it should run in the background.
    pub fn run_background(mut self, id: String) -> Self {
        self.background = Some(id);
        self
    }

    /// Set the path from where to execute the command.
    pub fn with_execute_from_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    /// Set the log file where to redirect stdout and stderr.
    pub fn with_log_file(mut self, path: PathBuf) -> Self {
        self.log_file = Some(path);
        self
    }

    /// Apply the context to a base command.
    pub fn apply<S: Into<String>>(&self, base_command: S) -> String {
        let mut str = base_command.into();
        if let Some(log_file) = &self.log_file {
            str = format!("{str} |& tee {}", log_file.as_path().display());
        }
        if let Some(id) = &self.background {
            str = format!("tmux new -d -s \"{id}\" \"{str}\"");
        }
        if let Some(exec_path) = &self.path {
            str = format!("(cd {} && {str})", exec_path.as_path().display());
        }
        str
    }
}

#[derive(Clone)]
pub struct SshConnectionManager {
    /// The ssh username.
    username: String,
    /// The ssh primate key to connect to the instances.
    private_key_file: PathBuf,
    /// The timeout value of the connection.
    timeout: Option<Duration>,
    /// The number of retries before giving up to execute the command.
    retries: usize,
}

impl SshConnectionManager {
    /// Delay before re-attempting an ssh execution.
    const RETRY_DELAY: Duration = Duration::from_secs(5);

    /// Create a new ssh manager from the instances username and private keys.
    pub fn new(username: String, private_key_file: PathBuf) -> Self {
        Self {
            username,
            private_key_file,
            timeout: None,
            retries: 0,
        }
    }

    /// Set a timeout duration for the connections.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the maximum number of times to retries to establish a connection and execute commands.
    pub fn with_retries(mut self, retries: usize) -> Self {
        self.retries = retries;
        self
    }

    /// Create a new ssh connection with the provided host.
    pub async fn connect(&self, address: SocketAddr) -> SshResult<SshConnection> {
        let mut error = None;
        for _ in 0..self.retries + 1 {
            match SshConnection::new(
                address,
                &self.username,
                self.private_key_file.clone(),
                self.timeout,
                Some(self.retries),
            )
            .await
            {
                Ok(x) => return Ok(x),
                Err(e) => error = Some(e),
            }
            sleep(Self::RETRY_DELAY).await;
        }
        Err(error.unwrap())
    }

    /// Execute the specified ssh command on all provided instances.
    pub async fn execute<I, S>(
        &self,
        instances: I,
        command: S,
        context: CommandContext,
    ) -> SshResult<Vec<(String, String)>>
    where
        I: IntoIterator<Item = Instance>,
        S: Into<String> + Clone + Send + 'static,
    {
        let targets = instances
            .into_iter()
            .map(|instance| (instance, command.clone()));
        self.execute_per_instance(targets, context).await
    }

    /// Execute the ssh command associated with each instance.
    pub async fn execute_per_instance<I, S>(
        &self,
        instances: I,
        context: CommandContext,
    ) -> SshResult<Vec<(String, String)>>
    where
        I: IntoIterator<Item = (Instance, S)>,
        S: Into<String> + Send + 'static,
    {
        let handles = self.run_per_instance(instances, context).await;

        try_join_all(handles)
            .await
            .unwrap()
            .into_iter()
            .collect::<SshResult<_>>()
    }

    async fn run_per_instance<I, S>(
        &self,
        instances: I,
        context: CommandContext,
    ) -> Vec<JoinHandle<SshResult<(String, String)>>>
    where
        I: IntoIterator<Item = (Instance, S)>,
        S: Into<String> + Send + 'static,
    {
        instances
            .into_iter()
            .map(|(instance, command)| {
                let ssh_manager = self.clone();
                let context = context.clone();

                tokio::spawn(async move {
                    let connection = ssh_manager.connect(instance.ssh_address()).await?;
                    // SshConnection::execute is a blocking call, needs to go to blocking pool
                    connection.execute(context.apply(command)).await
                })
            })
            .collect::<Vec<_>>()
    }

    /// Wait until a command running in the background returns or started.
    pub async fn wait_for_command<I>(
        &self,
        instances: I,
        command_id: &str,
        status: CommandStatus,
    ) -> SshResult<()>
    where
        I: IntoIterator<Item = Instance> + Clone,
    {
        loop {
            sleep(Self::RETRY_DELAY).await;

            let result = self
                .execute(
                    instances.clone(),
                    "(tmux ls || true)",
                    CommandContext::default(),
                )
                .await?;
            if result
                .iter()
                .all(|(stdout, _)| CommandStatus::status(command_id, stdout) == status)
            {
                break;
            }
        }
        Ok(())
    }

    pub async fn wait_for_success<I, S>(&self, instances: I)
    where
        I: IntoIterator<Item = (Instance, S)> + Clone,
        S: Into<String> + Send + 'static + Clone,
    {
        loop {
            sleep(Self::RETRY_DELAY).await;

            if self
                .execute_per_instance(instances.clone(), CommandContext::default())
                .await
                .is_ok()
            {
                break;
            }
        }
    }

    /// Kill a command running in the background of the specified instances.
    pub async fn kill<I>(&self, instances: I, command_id: &str) -> SshResult<()>
    where
        I: IntoIterator<Item = Instance>,
    {
        let ssh_command = format!("(tmux kill-session -t {command_id} || true)");
        let targets = instances.into_iter().map(|x| (x, ssh_command.clone()));
        self.execute_per_instance(targets, CommandContext::default())
            .await?;
        Ok(())
    }
}

struct Session {}

#[async_trait]
impl client::Handler for Session {
    type Error = russh::Error;

    async fn check_server_key(
        self,
        _server_public_key: &key::PublicKey,
    ) -> Result<(Self, bool), Self::Error> {
        Ok((self, true))
    }
}

/// Representation of an ssh connection.
pub struct SshConnection {
    /// The ssh session.
    session: client::Handle<Session>,
    /// The host address.
    address: SocketAddr,
    /// The number of retries before giving up to execute the command.
    retries: usize,
}

impl SshConnection {
    /// Default duration before timing out the ssh connection.
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

    /// Create a new ssh connection with a specific host.
    pub async fn new<P: AsRef<Path>>(
        address: SocketAddr,
        username: &str,
        private_key_file: P,
        inactivity_timeout: Option<Duration>,
        retries: Option<usize>,
    ) -> SshResult<Self> {
        let key = russh_keys::load_secret_key(private_key_file, None)
            .map_err(|error| SshError::PrivateKeyError { address, error })?;

        let config = client::Config {
            inactivity_timeout: inactivity_timeout.or(Some(Self::DEFAULT_TIMEOUT)),
            ..<_>::default()
        };

        let mut session = client::connect(Arc::new(config), address, Session {})
            .await
            .map_err(|error| SshError::ConnectionError { address, error })?;

        let _auth_res = session
            .authenticate_publickey(username, Arc::new(key))
            .await
            .map_err(|error| SshError::SessionError { address, error })?;

        Ok(Self {
            session,
            address,
            retries: retries.unwrap_or_default(),
        })
    }

    /// Make a useful session error from the lower level error message.
    fn make_session_error(&self, error: russh::Error) -> SshError {
        SshError::SessionError {
            address: self.address,
            error,
        }
    }

    /// Execute an ssh command on the remote machine.
    pub async fn execute(&self, command: String) -> SshResult<(String, String)> {
        let mut error = None;
        for _ in 0..self.retries + 1 {
            let channel = match self.session.channel_open_session().await {
                Ok(x) => x,
                Err(e) => {
                    error = Some(self.make_session_error(e));
                    continue;
                }
            };
            match self.execute_impl(channel, command.clone()).await {
                r @ Ok(..) => return r,
                Err(e) => error = Some(e),
            }
        }
        Err(error.unwrap())
    }

    /// Execute an ssh command on the remote machine and return both stdout and stderr.
    async fn execute_impl(
        &self,
        mut channel: Channel<Msg>,
        command: String,
    ) -> SshResult<(String, String)> {
        channel
            .exec(true, command)
            .await
            .map_err(|e| self.make_session_error(e))?;

        let mut output = Vec::new();
        let mut exit_code = None;

        while let Some(msg) = channel.wait().await {
            match msg {
                russh::ChannelMsg::Data { ref data } => output.write_all(data).unwrap(),
                russh::ChannelMsg::ExitStatus { exit_status } => exit_code = Some(exit_status),
                _ => {}
            }
        }

        channel
            .close()
            .await
            .map_err(|error| self.make_session_error(error))?;

        let output_str: String = String::from_utf8_lossy(&output).into();

        ensure!(
            exit_code.is_some() && exit_code.unwrap() == 0,
            SshError::NonZeroExitCode {
                address: self.address,
                code: exit_code.unwrap(),
                message: output_str
            }
        );

        Ok((output_str.clone(), output_str))
    }

    /// Download a file from the remote machines by doing a silly cat on the file.
    /// TODO: if the files get too big then we should leverage a simple S3 bucket instead.
    pub async fn download<P: AsRef<Path>>(&self, path: P) -> SshResult<String> {
        let mut error = None;
        for _ in 0..self.retries + 1 {
            match self
                .execute(format!("cat {}", path.as_ref().to_str().unwrap()))
                .await
            {
                Ok((file_data, _)) => return Ok(file_data),
                Err(err) => error = Some(err),
            }
        }
        Err(error.unwrap())
    }
}
