pub mod synthetic_peers;

use crate::protocol::{
    message::{filter::MessageFilter, Message},
    payload::Version,
};

use tokio::{
    io,
    net::{TcpListener, TcpStream},
    time::timeout,
};

use std::{net::SocketAddr, time::Duration};

pub fn enable_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    fmt()
        .with_test_writer()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

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

/// Helper to respond to a handshake initiated by the node on connection to the supplied listener,
/// returns the tcp stream.
///
/// Note, the listener's adddress must be set on the node as an initial peer.
pub async fn respond_to_handshake(listener: TcpListener) -> io::Result<TcpStream> {
    let (mut peer_stream, addr) = listener.accept().await?;

    let version = Message::read_from_stream(&mut peer_stream).await?;
    assert!(matches!(version, Message::Version(..)));

    Message::Version(Version::new(addr, listener.local_addr().unwrap()))
        .write_to_stream(&mut peer_stream)
        .await?;

    let verack = Message::read_from_stream(&mut peer_stream).await?;
    assert!(matches!(verack, Message::Verack));

    Message::Verack.write_to_stream(&mut peer_stream).await?;

    Ok(peer_stream)
}

/// Connects to the node at the given address, handshakes and returns the established stream.
pub async fn initiate_handshake(node_addr: SocketAddr) -> io::Result<TcpStream> {
    let mut peer_stream = initiate_version_exchange(node_addr).await?;

    Message::Verack.write_to_stream(&mut peer_stream).await?;

    let verack = Message::read_from_stream(&mut peer_stream).await?;
    assert!(matches!(verack, Message::Verack));

    Ok(peer_stream)
}

// Connects to the node at the given address, completes the version exchange and returns the
// established stream.
pub async fn initiate_version_exchange(node_addr: SocketAddr) -> io::Result<TcpStream> {
    let mut peer_stream = TcpStream::connect(node_addr).await?;

    // Send and receive Version.
    Message::Version(Version::new(node_addr, peer_stream.local_addr().unwrap()))
        .write_to_stream(&mut peer_stream)
        .await?;

    let version = Message::read_from_stream(&mut peer_stream).await?;
    assert!(matches!(version, Message::Version(..)));

    Ok(peer_stream)
}

// Returns true if the error kind is one that indicates that the connection has
// been terminated.
pub fn is_termination_error(err: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(
        err.kind(),
        ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof
    )
}

// Autoresponds to a maximum of 10 messages while expecting the stream to disconnect.
pub async fn autorespond_and_expect_disconnect(stream: &mut TcpStream) {
    let auto_responder = MessageFilter::with_all_auto_reply().enable_logging();

    let mut is_disconnect = false;

    // Read a maximum of 10 messages before exiting.
    for _ in 0usize..10 {
        let result = timeout(
            Duration::from_secs(5),
            auto_responder.read_from_stream(stream),
        )
        .await;

        match result {
            Err(elapsed) => panic!("Timeout after {}", elapsed),
            Ok(Ok(message)) => println!("Received unfiltered message: {:?}", message),
            Ok(Err(err)) => {
                if is_termination_error(&err) {
                    is_disconnect = true;
                    break;
                }
            }
        }
    }

    assert!(is_disconnect);
}
