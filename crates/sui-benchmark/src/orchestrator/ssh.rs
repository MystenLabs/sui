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
    orchestrator::error::{SshError, SshResult},
};

use super::state::Instance;

#[derive(Clone)]
pub struct SshCommand<C: Fn(usize) -> String> {
    pub command: C,
    pub background: Option<String>,
    pub path: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
    pub timeout: Option<Duration>,
}

impl<C: Fn(usize) -> String> SshCommand<C> {
    pub fn new(command: C) -> Self {
        Self {
            command,
            background: None,
            path: None,
            log_file: None,
            timeout: None,
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

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
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
}

impl SshConnectionManager {
    pub fn new(username: String, private_key_file: PathBuf) -> Self {
        Self {
            username,
            private_key_file,
        }
    }

    pub async fn connect(&self, address: SocketAddr) -> SshResult<SshConnection> {
        SshConnection::new(address, &self.username, self.private_key_file.clone()).await
    }

    pub async fn execute<C>(
        &self,
        instances: &[Instance],
        command: SshCommand<C>,
    ) -> SshResult<Vec<(String, String)>>
    where
        C: Fn(usize) -> String + Clone + Send + 'static,
    {
        let handles = instances
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, instance)| {
                let ssh_manager = self.clone();
                let command = command.clone();

                tokio::spawn(async move {
                    ssh_manager
                        .connect(instance.ssh_address())
                        .await?
                        .with_timeout(&command.timeout)
                        .execute(command.stringify(i))
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
}

impl SshConnection {
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

    pub async fn new<P: AsRef<Path>>(
        address: SocketAddr,
        username: &str,
        private_key_file: P,
    ) -> SshResult<Self> {
        let tcp = TcpStream::connect(address).await?;

        let mut session = Session::new()?;
        session.set_timeout(Self::DEFAULT_TIMEOUT.as_millis() as u32);
        session.set_tcp_stream(tcp);
        session.handshake()?;
        session.userauth_pubkey_file(username, None, private_key_file.as_ref(), None)?;

        Ok(Self { session })
    }

    pub fn with_timeout(mut self, timeout: &Option<Duration>) -> Self {
        let duration = match timeout {
            Some(value) => value,
            None => &Self::DEFAULT_TIMEOUT,
        };
        self.session.set_timeout(duration.as_millis() as u32);
        self
    }

    pub fn execute(&self, command: String) -> SshResult<(String, String)> {
        let channel = self.session.channel_session()?;
        Self::execute_impl(channel, command)
    }

    pub fn execute_from_path<P: AsRef<Path>>(
        &self,
        command: String,
        path: P,
    ) -> SshResult<(String, String)> {
        let channel = self.session.channel_session()?;
        let command = format!("(cd {} && {command})", path.as_ref().display().to_string());
        Self::execute_impl(channel, command)
    }

    fn execute_impl(mut channel: Channel, command: String) -> SshResult<(String, String)> {
        channel.exec(&command)?;
        // println!("{command}");

        let mut stdout = String::new();
        channel.read_to_string(&mut stdout)?;
        // println!("{stdout}");

        let mut stderr = String::new();
        channel.stderr().read_to_string(&mut stderr)?;
        // println!("{stderr}");

        channel.close()?;
        channel.wait_close()?;

        let exit_status = channel.exit_status()?;
        ensure!(
            exit_status == 0,
            SshError::NonZeroExitCode(exit_status, stderr.clone())
        );

        Ok((stdout, stderr))
    }

    pub fn upload<P: AsRef<Path>>(&self, path: P, content: &[u8]) -> SshResult<()> {
        let size = content.len() as u64;
        let mut channel = self.session.scp_send(path.as_ref(), 0o644, size, None)?;
        channel.write_all(content).unwrap();
        Ok(())
    }

    pub fn download<P: AsRef<Path>>(&self, path: P) -> SshResult<String> {
        let (mut channel, _stats) = self.session.scp_recv(path.as_ref())?;
        // println!("2: {}", path.as_ref().display());
        let mut content = String::new();
        // println!("{content}");
        channel.read_to_string(&mut content).unwrap();
        Ok(content)
    }
}
