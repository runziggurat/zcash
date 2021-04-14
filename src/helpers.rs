use crate::protocol::{message::Message, payload::Version};

use tokio::{
    io,
    net::{TcpListener, TcpStream},
};

use std::net::SocketAddr;

/// Waits until an expression is true or times out.
///
/// Uses polling to cut down on time otherwise used by calling `sleep` in tests.
#[macro_export]
macro_rules! wait_until {
    ($limit_secs: expr, $condition: expr $(, $sleep_millis: expr)?) => {
        let now = std::time::Instant::now();
        loop {
            if $condition {
                break;
            }

            // Default timout.
            let sleep_millis = 10;
            // Set if present in args.
            $(let sleep_millis = $sleep_millis;)?
            tokio::time::sleep(std::time::Duration::from_millis(sleep_millis)).await;
            if now.elapsed() > std::time::Duration::from_secs($limit_secs) {
                panic!("timed out!");
            }
        }
    };
}

pub async fn handshake(listener: TcpListener) -> io::Result<TcpStream> {
    let (mut peer_stream, addr) = listener.accept().await.unwrap();

    let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
    assert!(matches!(version, Message::Version(..)));

    Message::Version(Version::new(addr, listener.local_addr().unwrap()))
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let verack = Message::read_from_stream(&mut peer_stream).await.unwrap();
    assert!(matches!(verack, Message::Verack));

    Message::Verack
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    Ok(peer_stream)
}

pub async fn handshaken_peer(node_addr: SocketAddr) -> io::Result<TcpStream> {
    let mut peer_stream = TcpStream::connect(node_addr).await?;

    Message::Version(Version::new(node_addr, peer_stream.local_addr().unwrap()))
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let version = Message::read_from_stream(&mut peer_stream).await?;
    assert!(matches!(version, Message::Version(..)));

    Message::Verack
        .write_to_stream(&mut peer_stream)
        .await
        .unwrap();

    let verack = Message::read_from_stream(&mut peer_stream).await?;
    assert!(matches!(verack, Message::Verack));

    Ok(peer_stream)
}
