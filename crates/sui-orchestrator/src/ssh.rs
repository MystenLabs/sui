use std::{
    io::{Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use futures::future::try_join_all;
use ssh2::{Channel, Session};
use tokio::net::TcpStream;

use crate::{
    ensure,
    error::{SshError, SshResult},
};

use super::client::Instance;

#[derive(Clone)]
pub struct SshCommand<C: Fn(usize) -> String> {
    pub command: C,
    pub background: Option<String>,
    pub path: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
}

impl<C: Fn(usize) -> String> SshCommand<C> {
    pub fn new(command: C) -> Self {
        Self {
            command,
            background: None,
            path: None,
            log_file: None,
        }
    }

    pub fn run_background(mut self, id: String) -> Self {
        self.background = Some(id);
        self
    }

    pub fn with_execute_from_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    pub fn with_log_file(mut self, path: PathBuf) -> Self {
        self.log_file = Some(path);
        self
    }

    pub fn stringify(&self, index: usize) -> String {
        let mut str = (self.command)(index);
        if let Some(log_file) = &self.log_file {
            str = format!("{str} |& tee {}", log_file.as_path().display().to_string());
        }
        if let Some(id) = &self.background {
            str = format!("tmux new -d -s \"{id}\" \"{str}\"");
        }
        if let Some(exec_path) = &self.path {
            str = format!(
                "(cd {} && {str})",
                exec_path.as_path().display().to_string()
            );
        }
        str
    }
}

#[derive(Clone)]
pub struct SshConnectionManager {
    username: String,
    private_key_file: PathBuf,
    timeout: Option<Duration>,
    retries: usize,
}

impl SshConnectionManager {
    pub fn new(username: String, private_key_file: PathBuf) -> Self {
        Self {
            username,
            private_key_file,
            timeout: None,
            retries: 0,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_retries(mut self, retries: usize) -> Self {
        self.retries = retries;
        self
    }

    /// Create a new ssh connection with the provided host.
    pub async fn connect(&self, address: SocketAddr) -> SshResult<SshConnection> {
        SshConnection::new(address, &self.username, self.private_key_file.clone())
            .await
            .map(|x| x.with_timeout(&self.timeout))
    }

    /// Execute the specified ssh command on all provided instances.
    pub async fn execute<'a, I, C>(
        &self,
        instances: I,
        command: SshCommand<C>,
    ) -> SshResult<Vec<(String, String)>>
    where
        I: Iterator<Item = &'a Instance>,
        C: Fn(usize) -> String + Clone + Send + 'static,
    {
        let handles = instances
            .cloned()
            .enumerate()
            .map(|(i, instance)| {
                let ssh_manager = self.clone();
                let command = command.clone();

                tokio::spawn(async move {
                    let mut error = None;
                    for _ in 0..ssh_manager.retries {
                        let connection = match ssh_manager.connect(instance.ssh_address()).await {
                            Ok(x) => x,
                            Err(e) => {
                                error = Some(e);
                                continue;
                            }
                        };

                        match connection.execute(command.stringify(i)) {
                            r @ Ok(..) => return r,
                            Err(e) => error = Some(e),
                        }
                    }
                    Err(error.unwrap())
                })
            })
            .collect::<Vec<_>>();

        try_join_all(handles)
            .await
            .unwrap()
            .into_iter()
            .collect::<SshResult<_>>()
    }
}

pub struct SshConnection {
    session: Session,
    address: SocketAddr,
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
            .map_err(|error| SshError::ConnectionError { ip: address, error })?;

        let mut session = Session::new()?;
        session.set_timeout(Self::DEFAULT_TIMEOUT.as_millis() as u32);
        session.set_tcp_stream(tcp);
        session.handshake()?;
        session.userauth_pubkey_file(username, None, private_key_file.as_ref(), None)?;

        Ok(Self { session, address })
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

    fn make_connection_error(&self, error: std::io::Error) -> SshError {
        SshError::ConnectionError {
            ip: self.address.clone(),
            error,
        }
    }

    /// Execute a ssh command on the remote machine.
    pub fn execute(&self, command: String) -> SshResult<(String, String)> {
        let channel = self.session.channel_session()?;
        self.execute_impl(channel, command)
    }

    /// Execute a ssh command from a given path.
    /// TODO: Eventually remove this function and use [`execute`] through the ssh manager instead.
    pub fn execute_from_path<P: AsRef<Path>>(
        &self,
        command: String,
        path: P,
    ) -> SshResult<(String, String)> {
        let channel = self.session.channel_session()?;
        let command = format!("(cd {} && {command})", path.as_ref().display().to_string());
        self.execute_impl(channel, command)
    }

    /// Execute an ssh command on the remote machine and return both stdout and stderr.
    fn execute_impl(&self, mut channel: Channel, command: String) -> SshResult<(String, String)> {
        channel.exec(&command)?;

        let mut stdout = String::new();
        channel
            .read_to_string(&mut stdout)
            .map_err(|e| self.make_connection_error(e))?;

        let mut stderr = String::new();
        channel
            .stderr()
            .read_to_string(&mut stderr)
            .map_err(|e| self.make_connection_error(e))?;

        channel.close()?;
        channel.wait_close()?;

        let exit_status = channel.exit_status()?;
        ensure!(
            exit_status == 0,
            SshError::NonZeroExitCode(exit_status, stderr.clone())
        );

        Ok((stdout, stderr))
    }

    /// Upload a file to the remote machines through scp.
    pub fn upload<P: AsRef<Path>>(&self, path: P, content: &[u8]) -> SshResult<()> {
        let size = content.len() as u64;
        let mut channel = self.session.scp_send(path.as_ref(), 0o644, size, None)?;
        channel
            .write_all(content)
            .map_err(|e| self.make_connection_error(e))?;
        Ok(())
    }

    /// Download a file from the remote machines through scp.
    pub fn download<P: AsRef<Path>>(&self, path: P) -> SshResult<String> {
        let (mut channel, _stats) = self.session.scp_recv(path.as_ref())?;
        let mut content = String::new();
        channel
            .read_to_string(&mut content)
            .map_err(|e| self.make_connection_error(e))?;
        Ok(content)
    }
}
