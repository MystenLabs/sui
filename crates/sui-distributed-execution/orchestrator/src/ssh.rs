// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    io::{Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use futures::future::try_join_all;
use ssh2::{Channel, Session};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio::{net::TcpStream, time::sleep};

use crate::{
    client::Instance,
    ensure,
    error::{SshError, SshResult},
};

#[derive(PartialEq, Eq)]
/// The status of a ssh command running in the background.
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
            match SshConnection::new(address, &self.username, self.private_key_file.clone()).await {
                Ok(x) => return Ok(x.with_timeout(&self.timeout).with_retries(self.retries)),
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
        let handles = self.run_per_instance(instances, context);

        try_join_all(handles)
            .await
            .unwrap()
            .into_iter()
            .collect::<SshResult<_>>()
    }

    pub fn run_per_instance<I, S>(
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
                    Handle::current()
                        .spawn_blocking(move || connection.execute(context.apply(command)))
                        .await
                        .unwrap()
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

/// Representation of an ssh connection.
pub struct SshConnection {
    /// The ssh session.
    session: Session,
    /// The host address.
    address: SocketAddr,
    /// The number of retries before giving up to execute the command.
    retries: usize,
}

impl SshConnection {
    /// Default duration before timing out the ssh connection.
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

    /// Create a new ssh connection with a specific host.
    pub async fn new<P: AsRef<Path>>(
        address: SocketAddr,
        username: &str,
        private_key_file: P,
    ) -> SshResult<Self> {
        let tcp = TcpStream::connect(address)
            .await
            .map_err(|error| SshError::ConnectionError { address, error })?;

        let mut session =
            Session::new().map_err(|error| SshError::SessionError { address, error })?;
        session.set_timeout(Self::DEFAULT_TIMEOUT.as_millis() as u32);
        session.set_tcp_stream(tcp);
        session
            .handshake()
            .map_err(|error| SshError::SessionError { address, error })?;
        session
            .userauth_pubkey_file(username, None, private_key_file.as_ref(), None)
            .map_err(|error| SshError::SessionError { address, error })?;

        Ok(Self {
            session,
            address,
            retries: 0,
        })
    }

    /// Set a timeout for the ssh connection. If no timeouts are specified, reset it to the
    /// default value.
    pub fn with_timeout(self, timeout: &Option<Duration>) -> Self {
        let duration = match timeout {
            Some(value) => value,
            None => &Self::DEFAULT_TIMEOUT,
        };
        self.session.set_timeout(duration.as_millis() as u32);
        self
    }

    /// Set the maximum number of times to retries to establish a connection and execute commands.
    pub fn with_retries(mut self, retries: usize) -> Self {
        self.retries = retries;
        self
    }

    /// Make a useful session error from the lower level error message.
    fn make_session_error(&self, error: ssh2::Error) -> SshError {
        SshError::SessionError {
            address: self.address,
            error,
        }
    }

    /// Make a useful connection error from the lower level error message.
    fn make_connection_error(&self, error: std::io::Error) -> SshError {
        SshError::ConnectionError {
            address: self.address,
            error,
        }
    }

    /// Execute a ssh command on the remote machine.
    pub fn execute(&self, command: String) -> SshResult<(String, String)> {
        let mut error = None;
        for _ in 0..self.retries + 1 {
            let channel = match self.session.channel_session() {
                Ok(x) => x,
                Err(e) => {
                    error = Some(self.make_session_error(e));
                    continue;
                }
            };
            match self.execute_impl(channel, command.clone()) {
                r @ Ok(..) => return r,
                Err(e) => error = Some(e),
            }
        }
        Err(error.unwrap())
    }

    /// Execute an ssh command on the remote machine and return both stdout and stderr.
    fn execute_impl(&self, mut channel: Channel, command: String) -> SshResult<(String, String)> {
        channel
            .exec(&command)
            .map_err(|e| self.make_session_error(e))?;

        let mut stdout = String::new();
        channel
            .read_to_string(&mut stdout)
            .map_err(|e| self.make_connection_error(e))?;

        let mut stderr = String::new();
        channel
            .stderr()
            .read_to_string(&mut stderr)
            .map_err(|e| self.make_connection_error(e))?;

        channel.close().map_err(|e| self.make_session_error(e))?;
        channel
            .wait_close()
            .map_err(|e| self.make_session_error(e))?;

        let exit_status = channel
            .exit_status()
            .map_err(|e| self.make_session_error(e))?;

        ensure!(
            exit_status == 0,
            SshError::NonZeroExitCode {
                address: self.address,
                code: exit_status,
                message: stderr.clone()
            }
        );

        Ok((stdout, stderr))
    }

    /// Upload a file to the remote machines through scp.
    #[allow(dead_code)]
    pub fn upload<P: AsRef<Path>>(&self, path: P, content: &[u8]) -> SshResult<()> {
        let size = content.len() as u64;
        let mut error = None;
        for _ in 0..self.retries + 1 {
            let mut channel = match self.session.scp_send(path.as_ref(), 0o644, size, None) {
                Ok(x) => x,
                Err(e) => {
                    error = Some(self.make_session_error(e));
                    continue;
                }
            };
            match channel
                .write_all(content)
                .map_err(|e| self.make_connection_error(e))
            {
                r @ Ok(..) => return r,
                Err(e) => error = Some(e),
            }
        }
        Err(error.unwrap())
    }

    /// Download a file from the remote machines through scp.
    pub fn download<P: AsRef<Path>>(&self, path: P) -> SshResult<String> {
        let mut error = None;
        for _ in 0..self.retries + 1 {
            let (mut channel, _stats) = match self.session.scp_recv(path.as_ref()) {
                Ok(x) => x,
                Err(e) => {
                    error = Some(self.make_session_error(e));
                    continue;
                }
            };

            let mut content = String::new();
            match channel
                .read_to_string(&mut content)
                .map_err(|e| self.make_connection_error(e))
            {
                Ok(..) => return Ok(content),
                Err(e) => error = Some(e),
            }
        }
        Err(error.unwrap())
    }
}
