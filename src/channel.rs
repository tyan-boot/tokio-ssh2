use std::io;
use std::sync::Arc;

use ssh2::{
    BlockDirections, Channel, ExitSignal, ExtendedData, PtyModes, ReadWindow, Session, Stream,
    WriteWindow,
};
use std::io::{Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, Interest, ReadBuf};
use tokio::net::TcpStream;

pub struct AsyncChannel {
    pub(crate) session: Session,
    pub(crate) channel: Channel,
    pub(crate) io: Arc<TcpStream>,
}

impl AsyncChannel {
    async fn wait_io_mut<R>(
        &mut self,
        mut op: impl FnMut(&mut Channel) -> io::Result<R>,
    ) -> io::Result<R> {
        loop {
            match op(&mut self.channel) {
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

    async fn wait_io<R>(&self, mut op: impl FnMut(&Channel) -> io::Result<R>) -> io::Result<R> {
        loop {
            match op(&self.channel) {
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

    pub async fn setenv(&mut self, var: &str, val: &str) -> io::Result<()> {
        self.wait_io_mut(|channel| channel.setenv(var, val).map_err(Into::into))
            .await
    }

    pub async fn request_pty(
        &mut self,
        term: &str,
        mode: Option<PtyModes>,
        dim: Option<(u32, u32, u32, u32)>,
    ) -> io::Result<()> {
        self.wait_io_mut(|channel| {
            channel
                .request_pty(term, mode.clone(), dim)
                .map_err(Into::into)
        })
        .await
    }

    pub async fn request_pty_size(
        &mut self,
        width: u32,
        height: u32,
        width_px: Option<u32>,
        height_px: Option<u32>,
    ) -> io::Result<()> {
        self.wait_io_mut(|channel| {
            channel
                .request_pty_size(width, height, width_px, height_px)
                .map_err(Into::into)
        })
        .await
    }

    pub async fn request_auth_agent_forwarding(&mut self) -> io::Result<()> {
        self.wait_io_mut(|channel| channel.request_auth_agent_forwarding().map_err(Into::into))
            .await
    }

    pub async fn exec(&mut self, command: &str) -> io::Result<()> {
        self.wait_io_mut(|channel| channel.exec(command).map_err(Into::into))
            .await
    }

    pub async fn shell(&mut self) -> io::Result<()> {
        self.wait_io_mut(|channel| channel.shell().map_err(Into::into))
            .await
    }

    pub async fn subsystem(&mut self, system: &str) -> io::Result<()> {
        self.wait_io_mut(|channel| channel.subsystem(system).map_err(Into::into))
            .await
    }

    pub async fn process_startup(
        &mut self,
        request: &str,
        message: Option<&str>,
    ) -> io::Result<()> {
        self.wait_io_mut(|channel| {
            channel
                .process_startup(request, message)
                .map_err(Into::into)
        })
        .await
    }

    pub fn stderr(&self) -> io::Result<AsyncStream> {
        let stream = self.channel.stderr();

        Ok(AsyncStream {
            stream,
            io: self.io.clone(),
        })
    }

    pub fn stream(&self, id: i32) -> io::Result<AsyncStream> {
        let stream = self.channel.stream(id);

        Ok(AsyncStream {
            stream,
            io: self.io.clone(),
        })
    }

    pub async fn handle_extended_data(&mut self, mode: ExtendedData) -> io::Result<()> {
        self.wait_io_mut(|channel| channel.handle_extended_data(mode).map_err(Into::into))
            .await
    }

    pub async fn exit_status(&self) -> io::Result<i32> {
        self.wait_io(|channel| channel.exit_status().map_err(Into::into))
            .await
    }

    pub async fn exit_signal(&self) -> io::Result<ExitSignal> {
        self.wait_io(|channel| channel.exit_signal().map_err(Into::into))
            .await
    }

    pub async fn read_window(&self) -> io::Result<ReadWindow> {
        self.wait_io(|channel| Ok(channel.read_window())).await
    }

    pub async fn write_window(&self) -> io::Result<WriteWindow> {
        self.wait_io(|channel| Ok(channel.write_window())).await
    }

    pub async fn adjust_receive_window(&mut self, adjust: u64, force: bool) -> io::Result<u64> {
        self.wait_io_mut(|channel| {
            channel
                .adjust_receive_window(adjust, force)
                .map_err(Into::into)
        })
        .await
    }

    pub fn eof(&self) -> bool {
        self.channel.eof()
    }

    pub async fn send_eof(&mut self) -> io::Result<()> {
        self.wait_io_mut(|channel| channel.send_eof().map_err(Into::into))
            .await
    }

    pub async fn wait_eof(&mut self) -> io::Result<()> {
        self.wait_io_mut(|channel| channel.wait_eof().map_err(Into::into))
            .await
    }

    pub async fn close(&mut self) -> io::Result<()> {
        self.wait_io_mut(|channel| channel.close().map_err(Into::into))
            .await
    }

    pub async fn wait_close(&mut self) -> io::Result<()> {
        self.wait_io_mut(|channel| channel.wait_close().map_err(Into::into))
            .await
    }
}

pub struct AsyncStream {
    stream: Stream,
    io: Arc<TcpStream>,
}

impl AsyncRead for AsyncStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let io = self.io.clone();
        let stream = &mut self.stream;
        loop {
            match io.poll_read_ready(cx)? {
                Poll::Ready(_) => match stream.read(buf.initialize_unfilled()) {
                    Ok(r) => {
                        buf.set_filled(r);
                        return Poll::Ready(Ok(()));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Poll::Ready(Err(e)),
                },
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl AsyncWrite for AsyncStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let io = self.io.clone();
        let stream = &mut self.stream;

        loop {
            match io.poll_write_ready(cx)? {
                Poll::Ready(_) => match stream.write(buf) {
                    Ok(r) => return Poll::Ready(Ok(r)),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Poll::Ready(Err(e)),
                },
                Poll::Pending => return Poll::Pending,
            };
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let io = self.io.clone();
        let stream = &mut self.stream;

        loop {
            match io.poll_write_ready(cx)? {
                Poll::Ready(_) => match stream.flush() {
                    Ok(r) => return Poll::Ready(Ok(r)),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Poll::Ready(Err(e)),
                },
                Poll::Pending => return Poll::Pending,
            };
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}
