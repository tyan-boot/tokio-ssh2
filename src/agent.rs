use std::io;
use std::sync::Arc;

use ssh2::{Agent, BlockDirections, PublicKey, Session};
use tokio::io::Interest;
use tokio::net::TcpStream;

pub struct AsyncAgent {
    pub(crate) agent: Agent,
    pub(crate) session: Session,
    pub(crate) io: Arc<TcpStream>,
}

impl AsyncAgent {
    async fn wait_io_mut<R>(
        &mut self,
        mut op: impl FnMut(&mut Agent) -> io::Result<R>,
    ) -> io::Result<R> {
        loop {
            match op(&mut self.agent) {
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

    async fn wait_io<R>(&self, mut op: impl FnMut(&Agent) -> io::Result<R>) -> io::Result<R> {
        loop {
            match op(&self.agent) {
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

    pub async fn connect(&mut self) -> io::Result<()> {
        self.wait_io_mut(|agent| agent.connect().map_err(Into::into))
            .await
    }

    pub async fn disconnect(&mut self) -> io::Result<()> {
        self.wait_io_mut(|agent| agent.disconnect().map_err(Into::into))
            .await
    }

    pub async fn list_identities(&mut self) -> io::Result<()> {
        self.wait_io_mut(|agent| agent.list_identities().map_err(Into::into))
            .await
    }

    pub async fn identities(&self) -> io::Result<Vec<PublicKey>> {
        self.agent.identities().map_err(Into::into)
    }

    pub async fn userauth(&self, username: &str, identity: &PublicKey) -> io::Result<()> {
        self.wait_io(|agent| agent.userauth(username, identity).map_err(Into::into))
            .await
    }
}
