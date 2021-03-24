use std::io;
use std::sync::Arc;

use ssh2::{BlockDirections, Listener, Session};
use tokio::io::Interest;
use tokio::net::TcpStream;

use crate::AsyncChannel;

pub struct AsyncListener {
    pub(crate) listener: Listener,
    pub(crate) session: Session,
    pub(crate) io: Arc<TcpStream>,
}

impl AsyncListener {
    async fn wait_io_mut<R>(
        &mut self,
        mut op: impl FnMut(&mut Listener) -> io::Result<R>,
    ) -> io::Result<R> {
        loop {
            match op(&mut self.listener) {
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

    pub async fn accept(&mut self) -> io::Result<AsyncChannel> {
        let channel = self
            .wait_io_mut(|listener| listener.accept().map_err(Into::into))
            .await?;

        Ok(AsyncChannel {
            channel,
            session: self.session.clone(),
            io: self.io.clone(),
        })
    }
}
