#![allow(unused_imports, dead_code)]

use std::io;
use std::io::{Error, Read, Write};
use std::mem::MaybeUninit;
use std::os::raw::c_int;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use ssh2::{BlockDirections, File, FileStat, OpenFlags, OpenType, RenameFlags, Session, Sftp};
use tokio::io::{AsyncRead, AsyncWrite, Interest, ReadBuf};
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

    async fn wait_io_mut<R>(
        &mut self,
        mut op: impl FnMut(&mut Sftp) -> io::Result<R>,
    ) -> io::Result<R> {
        loop {
            match op(&mut self.sftp) {
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

    pub async fn open_mode(
        &self,
        filename: &Path,
        flags: OpenFlags,
        mode: i32,
        open_type: OpenType,
    ) -> io::Result<AsyncFile> {
        let file = self
            .wait_io(|sftp| {
                sftp.open_mode(filename, flags, mode, open_type)
                    .map_err(Into::into)
            })
            .await?;

        Ok(AsyncFile {
            file,
            session: self.session.clone(),
            io: self.io.clone(),
        })
    }

    pub async fn open(&self, filename: &Path) -> io::Result<AsyncFile> {
        self.open_mode(filename, OpenFlags::READ, 0o644, OpenType::File)
            .await
    }

    pub async fn create(&self, filename: &Path) -> io::Result<AsyncFile> {
        self.open_mode(
            filename,
            OpenFlags::WRITE | OpenFlags::TRUNCATE,
            0o644,
            OpenType::File,
        )
        .await
    }

    pub async fn opendir(&self, dirname: &Path) -> io::Result<AsyncFile> {
        self.open_mode(dirname, OpenFlags::READ, 0, OpenType::Dir)
            .await
    }

    pub async fn readdir(&self, dirname: &Path) -> io::Result<Vec<(PathBuf, FileStat)>> {
        let entries = self
            .wait_io(|sftp| sftp.readdir(dirname).map_err(Into::into))
            .await?;

        Ok(entries)
    }

    pub async fn mkdir(&self, filename: impl AsRef<Path>, mode: i32) -> io::Result<()> {
        let filename = filename.as_ref();
        self.wait_io(|sftp| sftp.mkdir(filename, mode).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn rmdir(&self, filename: impl AsRef<Path>) -> io::Result<()> {
        let filename = filename.as_ref();
        self.wait_io(|sftp| sftp.rmdir(filename).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn stat(&self, filename: impl AsRef<Path>) -> io::Result<()> {
        let filename = filename.as_ref();
        self.wait_io(|sftp| sftp.stat(filename).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn lstat(&self, filename: impl AsRef<Path>) -> io::Result<()> {
        let filename = filename.as_ref();
        self.wait_io(|sftp| sftp.lstat(filename).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn setstat(&self, filename: impl AsRef<Path>, stat: FileStat) -> io::Result<()> {
        let filename = filename.as_ref();
        self.wait_io(|sftp| sftp.setstat(filename, stat.clone()).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn symlink(
        &self,
        path: impl AsRef<Path>,
        target: impl AsRef<Path>,
    ) -> io::Result<()> {
        let path = path.as_ref();
        let target = target.as_ref();

        self.wait_io(|sftp| sftp.symlink(path, target).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn readlink(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        self.wait_io(|sftp| sftp.readlink(path).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn realpath(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        self.wait_io(|sftp| sftp.realpath(path).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn rename(
        &self,
        src: impl AsRef<Path>,
        dst: impl AsRef<Path>,
        flags: Option<RenameFlags>,
    ) -> io::Result<()> {
        let src = src.as_ref();
        let dst = dst.as_ref();
        self.wait_io(|sftp| sftp.rename(src, dst, flags).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn unlink(&self, file: impl AsRef<Path>) -> io::Result<()> {
        let file = file.as_ref();
        self.wait_io(|sftp| sftp.unlink(file).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn shutdown(&mut self) -> io::Result<()> {
        self.wait_io_mut(|sftp| {
            // call hidden shutdown
            // see document for ssh2::Sftp::shutdown
            sftp.shutdown().map_err(Into::into)
        })
        .await?;

        Ok(())
    }
}

pub struct AsyncFile {
    file: File,
    session: Session,
    io: Arc<TcpStream>,
}

impl AsyncFile {
    async fn wait_io<R>(&self, mut op: impl FnMut(&File) -> io::Result<R>) -> io::Result<R> {
        loop {
            match op(&self.file) {
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

    async fn wait_io_mut<R>(
        &mut self,
        mut op: impl FnMut(&mut File) -> io::Result<R>,
    ) -> io::Result<R> {
        loop {
            match op(&mut self.file) {
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

    pub async fn setstat(&mut self, stat: FileStat) -> io::Result<()> {
        self.wait_io_mut(|f| f.setstat(stat.clone()).map_err(Into::into))
            .await?;

        Ok(())
    }

    pub async fn stat(&mut self) -> io::Result<FileStat> {
        let stat = self.wait_io_mut(|f| f.stat().map_err(Into::into)).await?;

        Ok(stat)
    }

    pub async fn readdir(&mut self) -> io::Result<(PathBuf, FileStat)> {
        let res = self
            .wait_io_mut(|f| f.readdir().map_err(Into::into))
            .await?;

        Ok(res)
    }

    pub async fn fsync(&mut self) -> io::Result<()> {
        self.wait_io_mut(|f| f.fsync().map_err(Into::into)).await?;

        Ok(())
    }
}

impl AsyncRead for AsyncFile {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let io = self.io.clone();
        let file = &mut self.file;

        loop {
            match io.poll_read_ready(cx)? {
                Poll::Ready(_) => {
                    let b = unsafe {
                        &mut *(buf.unfilled_mut() as *mut [MaybeUninit<u8>] as *mut [u8])
                    };

                    match file.read(b) {
                        Ok(r) => {
                            unsafe {
                                buf.assume_init(r);
                            }
                            buf.advance(r);
                            return Poll::Ready(Ok(()));
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                        Err(e) => return Poll::Ready(Err(e)),
                    }
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl AsyncWrite for AsyncFile {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        let io = self.io.clone();
        let file = &mut self.file;

        loop {
            match io.poll_write_ready(cx)? {
                Poll::Ready(_) => match file.write(buf) {
                    Ok(r) => return Poll::Ready(Ok(r)),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Poll::Ready(Err(e)),
                },
                Poll::Pending => return Poll::Pending,
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        let io = self.io.clone();
        let file = &mut self.file;

        loop {
            match io.poll_write_ready(cx)? {
                Poll::Ready(_) => match file.flush() {
                    Ok(_) => return Poll::Ready(Ok(())),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Poll::Ready(Err(e)),
                },
                Poll::Pending => return Poll::Pending,
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        self.poll_flush(cx)
    }
}
