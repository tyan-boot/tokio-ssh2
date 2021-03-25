use std::io;
use std::net::TcpStream as StdTcpStream;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::Path;
use std::sync::Arc;

use ssh2::{
    BlockDirections, DisconnectCode, HashType, HostKeyType, KeyboardInteractivePrompt, KnownHosts,
    MethodType, ScpFileStat, Session,
};
use tokio::io::Interest;
use tokio::net::TcpStream;

use crate::agent::AsyncAgent;
use crate::channel::AsyncChannel;
use crate::sftp::AsyncSftp;
use crate::AsyncListener;

pub struct AsyncSession {
    session: Session,
    io: Arc<TcpStream>,
}

impl AsyncSession {
    pub fn new(stream: StdTcpStream) -> io::Result<Self> {
        let mut session = Session::new()?;
        session.set_blocking(false);
        session.set_tcp_stream(stream);

        let fd = session.as_raw_fd();
        let stream = unsafe { StdTcpStream::from_raw_fd(fd) };
        let stream = Arc::new(TcpStream::from_std(stream)?);

        Ok(AsyncSession {
            session,
            io: stream,
        })
    }

    async fn wait_io_mut<R>(
        &mut self,
        mut op: impl FnMut(&mut Session) -> io::Result<R>,
    ) -> io::Result<R> {
        loop {
            match op(&mut self.session) {
                Ok(r) => {
                    return Ok(r);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    let dir = self.session.block_directions();
                    let interest = match dir {
                        BlockDirections::None => {
                            panic!()
                        }
                        BlockDirections::Inbound => Interest::READABLE,
                        BlockDirections::Outbound => Interest::WRITABLE,
                        BlockDirections::Both => Interest::READABLE.add(Interest::WRITABLE),
                    };
                    self.io.ready(interest).await?;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn wait_io<'a, R: 'a>(
        &'a self,
        mut op: impl FnMut(&'a Session) -> io::Result<R>,
    ) -> io::Result<R> {
        loop {
            match op(&self.session) {
                Ok(r) => {
                    return Ok(r);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    let dir = self.session.block_directions();
                    let interest = match dir {
                        BlockDirections::None => {
                            panic!()
                        }
                        BlockDirections::Inbound => Interest::READABLE,
                        BlockDirections::Outbound => Interest::WRITABLE,
                        BlockDirections::Both => Interest::READABLE.add(Interest::WRITABLE),
                    };
                    self.io.ready(interest).await?;
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub async fn handshake(&mut self) -> io::Result<()> {
        self.wait_io_mut(|session| session.handshake().map_err(Into::into))
            .await
    }

    pub async fn userauth_password(&self, username: &str, password: &str) -> io::Result<()> {
        self.wait_io(|session| {
            session
                .userauth_password(username, password)
                .map_err(Into::into)
        })
        .await
    }

    pub async fn userauth_keyboard_interactive<P: KeyboardInteractivePrompt>(
        &self,
        username: &str,
        prompter: &mut P,
    ) -> io::Result<()> {
        self.wait_io(|session| {
            session
                .userauth_keyboard_interactive(username, prompter)
                .map_err(Into::into)
        })
        .await
    }

    pub async fn userauth_agent(&self, username: &str) -> io::Result<()> {
        self.wait_io(|session| session.userauth_agent(username).map_err(Into::into))
            .await
    }

    pub async fn userauth_pubkey_file(
        &self,
        username: &str,
        pubkey: Option<&Path>,
        privatekey: &Path,
        passphrase: Option<&str>,
    ) -> io::Result<()> {
        self.wait_io(|session| {
            session
                .userauth_pubkey_file(username, pubkey, privatekey, passphrase)
                .map_err(Into::into)
        })
        .await
    }

    pub async fn userauth_pubkey_memory(
        &self,
        username: &str,
        pubkeydata: Option<&str>,
        privatekeydata: &str,
        passphrase: Option<&str>,
    ) -> io::Result<()> {
        self.wait_io(|session| {
            session
                .userauth_pubkey_memory(username, pubkeydata, privatekeydata, passphrase)
                .map_err(Into::into)
        })
        .await
    }

    pub async fn userauth_hostbased_file(
        &self,
        username: &str,
        publickey: &Path,
        privatekey: &Path,
        passphrase: Option<&str>,
        hostname: &str,
        local_username: Option<&str>,
    ) -> io::Result<()> {
        self.wait_io(|session| {
            session
                .userauth_hostbased_file(
                    username,
                    publickey,
                    privatekey,
                    passphrase,
                    hostname,
                    local_username,
                )
                .map_err(Into::into)
        })
        .await
    }

    pub fn authenticated(&self) -> bool {
        self.session.authenticated()
    }

    pub async fn auth_methods(&self, username: &str) -> io::Result<&str> {
        self.wait_io(|session| session.auth_methods(username).map_err(Into::into))
            .await
    }

    pub async fn method_pref(&self, method_type: MethodType, prefs: &str) -> io::Result<()> {
        self.wait_io(|session| session.method_pref(method_type, prefs).map_err(Into::into))
            .await
    }

    pub fn methods(&self, method_type: MethodType) -> Option<&str> {
        self.session.methods(method_type)
    }

    pub async fn supported_algs(&self, method_type: MethodType) -> io::Result<Vec<&str>> {
        self.wait_io(|session| session.supported_algs(method_type).map_err(Into::into))
            .await
    }

    pub async fn agent(&self) -> io::Result<AsyncAgent> {
        let agent = self
            .wait_io(|session| session.agent().map_err(Into::into))
            .await?;

        Ok(AsyncAgent {
            agent,
            session: self.session.clone(),
            io: self.io.clone(),
        })
    }

    pub fn known_hosts(&self) -> io::Result<KnownHosts> {
        self.session.known_hosts().map_err(Into::into)
    }

    pub async fn channel_session(&self) -> io::Result<AsyncChannel> {
        let channel = self
            .wait_io(|session| session.channel_session().map_err(Into::into))
            .await?;

        Ok(AsyncChannel {
            channel,
            session: self.session.clone(),
            io: self.io.clone(),
        })
    }

    pub async fn channel_direct_tcpip(
        &self,
        host: &str,
        port: u16,
        src: Option<(&str, u16)>,
    ) -> io::Result<AsyncChannel> {
        let channel = self
            .wait_io(|session| {
                session
                    .channel_direct_tcpip(host, port, src)
                    .map_err(Into::into)
            })
            .await?;

        Ok(AsyncChannel {
            channel,
            session: self.session.clone(),
            io: self.io.clone(),
        })
    }

    pub async fn channel_forward_listen(
        &self,
        remote_port: u16,
        host: Option<&str>,
        queue_maxsize: Option<u32>,
    ) -> io::Result<(AsyncListener, u16)> {
        let (listener, port) = self
            .wait_io(|session| {
                session
                    .channel_forward_listen(remote_port, host, queue_maxsize)
                    .map_err(Into::into)
            })
            .await?;

        Ok((
            AsyncListener {
                listener,
                session: self.session.clone(),
                io: self.io.clone(),
            },
            port,
        ))
    }

    pub async fn scp_recv(&self, path: &Path) -> io::Result<(AsyncChannel, ScpFileStat)> {
        let (channel, stat) = self
            .wait_io(|session| session.scp_recv(path).map_err(Into::into))
            .await?;

        Ok((
            AsyncChannel {
                channel,
                session: self.session.clone(),
                io: self.io.clone(),
            },
            stat,
        ))
    }

    pub async fn scp_send(
        &self,
        remote_path: &Path,
        mode: i32,
        size: u64,
        times: Option<(u64, u64)>,
    ) -> io::Result<AsyncChannel> {
        let channel = self
            .wait_io(|session| {
                session
                    .scp_send(remote_path, mode, size, times)
                    .map_err(Into::into)
            })
            .await?;

        Ok(AsyncChannel {
            channel,
            session: self.session.clone(),
            io: self.io.clone(),
        })
    }

    pub async fn sftp(&self) -> io::Result<AsyncSftp> {
        let sftp = self
            .wait_io(|session| session.sftp().map_err(Into::into))
            .await?;

        Ok(AsyncSftp {
            sftp,
            session: self.session.clone(),
            io: self.io.clone(),
        })
    }

    pub async fn channel_open(
        &self,
        channel_type: &str,
        window_size: u32,
        packet_size: u32,
        message: Option<&str>,
    ) -> io::Result<AsyncChannel> {
        let channel = self
            .wait_io(|session| {
                session
                    .channel_open(channel_type, window_size, packet_size, message)
                    .map_err(Into::into)
            })
            .await?;

        Ok(AsyncChannel {
            channel,
            session: self.session.clone(),
            io: self.io.clone(),
        })
    }

    pub fn banner(&self) -> Option<&str> {
        self.session.banner()
    }

    pub fn host_key(&self) -> Option<(&[u8], HostKeyType)> {
        self.session.host_key()
    }

    pub fn host_key_hash(&self, hash: HashType) -> Option<&[u8]> {
        self.session.host_key_hash(hash)
    }

    pub async fn keepalive_send(&self) -> io::Result<u32> {
        self.wait_io(|session| session.keepalive_send().map_err(Into::into))
            .await
    }

    pub async fn disconnect(
        &self,
        reason: Option<DisconnectCode>,
        description: &str,
        lang: Option<&str>,
    ) -> io::Result<()> {
        self.wait_io(|session| {
            session
                .disconnect(reason, description, lang)
                .map_err(Into::into)
        })
        .await
    }
}
