use std::{
    io::{Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use ssh2::{Channel, Session};
use tokio::net::TcpStream;

use crate::{
    ensure,
    orchestrator::error::{SshError, SshResult},
};

pub struct SshCommand {
    pub command: String,
    pub background: bool,
    pub path: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
    pub timeout: Option<Duration>,
}

impl SshCommand {
    pub fn new(command: String) -> Self {
        Self {
            command,
            background: false,
            path: None,
            log_file: None,
            timeout: None,
        }
    }

    pub fn run_background(mut self) -> Self {
        self.background = true;
        self
    }

    pub fn execute_from_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    pub fn set_log_file(mut self, path: PathBuf) -> Self {
        self.log_file = Some(path);
        self
    }

    pub fn set_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
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
}

pub struct SshConnection {
    session: Session,
}

impl SshConnection {
    pub async fn new<P: AsRef<Path>>(
        address: SocketAddr,
        username: &str,
        private_key_file: P,
    ) -> SshResult<Self> {
        let tcp = TcpStream::connect(address).await?;

        let mut session = Session::new()?;
        // session.set_timeout(120_000);
        session.set_tcp_stream(tcp);
        session.handshake()?;
        session.userauth_pubkey_file(username, None, private_key_file.as_ref(), None)?;

        Ok(Self { session })
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
