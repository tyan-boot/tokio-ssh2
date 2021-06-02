# tokio-ssh2

Asynchronous ssh2 client for tokio.

This crate is build on top of `tokio` and [`ssh2`](https://github.com/alexcrichton/ssh2-rs).
All api are same as `ssh2` but support `async`.

## Example
```rust
use tokio_ssh2::AsyncSession;
use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<()> {
    let tcp = std::net::TcpStream::connect("127.0.0.1:22")?;
    let mut session = AsyncSession::new(tcp)?;
    
    session.handshake().await?;
    session.userauth_password("root", "root").await?;
    
    let mut channel = session.channel_session().await?;
    channel.request_pty("xterm-256color", None, None).await?;
    channel.shell().await?;
    
    let stream = channel.stream(0)?;
    let mut line = String::new();
    
    stream.write_all("ls".as_bytes()).await?;
    stream.read_to_string(&mut line).await?;
    
    println!("{}", line);
    
    Ok(())
}
```

## Limitation
* Only support std `TcpStream`, not `tokio::net::TcpStream` or serial. Will be supported once `AsyncHandle` stable in `tokio`. See [issue 3781](https://github.com/tokio-rs/tokio/issues/3781).
* Only unix, Windows will be support soon.