use std::{
    collections::HashMap,
    io::{Read, Write},
    net::SocketAddr,
    path::Path,
};

use ssh2::{Channel, Session};
use tokio::{net::TcpStream, task::JoinHandle};

use crate::{
    ensure,
    orchestrator::{
        error::{SshError, SshResult},
        settings::Settings,
    },
};

pub struct SshConnectionPool {
    username: String,
    settings: Settings,
    connections: HashMap<SocketAddr, SshConnection>,
}

impl SshConnectionPool {
    pub fn new(username: String, settings: Settings) -> Self {
        Self {
            username,
            settings,
            connections: HashMap::new(),
        }
    }

    async fn reconnect(&mut self, address: SocketAddr) -> SshResult<&SshConnection> {
        let private_key_file = self.settings.ssh_private_key_file.clone();
        let connection = SshConnection::new(address, &self.username, private_key_file).await?;
        self.connections.insert(address, connection);
        let connection = self.connections.get(&address).unwrap();
        Ok(connection)
    }

    pub async fn execute(
        &mut self,
        address: SocketAddr,
        command: String,
    ) -> SshResult<(String, String)> {
        match self.connections.get(&address) {
            Some(x) => x,
            None => self.reconnect(address).await?,
        }
        .execute(command)
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
        session.set_tcp_stream(tcp);
        session.handshake()?;
        session.userauth_pubkey_file(username, None, private_key_file.as_ref(), None)?;

        Ok(Self { session })
    }

    pub fn execute(&self, command: String) -> SshResult<(String, String)> {
        let channel = self.session.channel_session()?;
        Self::execute_impl(channel, command)
    }

    pub fn background_execute(
        &self,
        command: String,
    ) -> SshResult<JoinHandle<SshResult<(String, String)>>> {
        let channel = self.session.channel_session()?;

        Ok(tokio::spawn(
            async move { Self::execute_impl(channel, command) },
        ))
    }

    fn execute_impl(mut channel: Channel, command: String) -> SshResult<(String, String)> {
        channel.exec(&command)?;

        let mut stdout = String::new();
        channel.read_to_string(&mut stdout)?;

        let mut stderr = String::new();
        channel.stderr().read_to_string(&mut stderr)?;

        channel.close()?;
        channel.wait_close()?;

        let exit_status = channel.exit_status()?;
        ensure!(exit_status == 0, SshError::NonZeroExitCode(exit_status));

        Ok((stdout, stderr))
    }

    pub fn upload<P: AsRef<Path>>(&self, path: P, content: &[u8]) -> SshResult<()> {
        let size = content.len() as u64;
        let mut channel = self.session.scp_send(path.as_ref(), 0o644, size, None)?;
        channel.write_all(content).unwrap();
        Ok(())
    }
}
