#![allow(unused_imports, dead_code)]
use std::io;
use std::path::Path;
use std::sync::Arc;

use ssh2::{BlockDirections, OpenFlags, OpenType, Session, Sftp};
use tokio::io::Interest;
use tokio::net::TcpStream;

pub struct AsyncSftp {
    pub(crate) sftp: Sftp,
    pub(crate) session: Session,
    pub(crate) io: Arc<TcpStream>,
}

impl AsyncSftp {
    async fn wait_io<R>(&self, mut op: impl FnMut(&Sftp) -> io::Result<R>) -> io::Result<R> {
        loop {
            match op(&self.sftp) {
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
}
