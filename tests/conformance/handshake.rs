use ziggurat::common::message::Message;
use ziggurat::common::message::MessageHeader;
use ziggurat::common::message::Version;

use bytes::BytesMut;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::{io::AsyncWriteExt, net::TcpStream};

use std::convert::TryInto;
use std::net::SocketAddr;

// #[tokio::test]
// async fn handshake_responder_side() {
//     // 1. Configure and run node.
//     // 2. Send a Version message to the node.
//     // 3. Expect a Version back and send Verack.
//     // 4. Expect Verack back.
//
//     let node_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
//     let mut peer_stream = TcpStream::connect(node_addr).await.unwrap();
//
//     let v = Version::new(node_addr, peer_stream.local_addr().unwrap());
//     let mut b = BytesMut::new();
//     v.encode(&mut b).unwrap();
//     dbg!(&b);
//     peer_stream.write_all(&b).await.unwrap();
//
//     let mut b = [0u8, 24];
//     peer_stream.read_exact(&mut b).await.unwrap();
//
//     dbg!(b);
// }

#[tokio::test]
async fn handshake_initiator_side() {
    let listener = TcpListener::bind("127.0.0.1:8081").await.unwrap();
    let node_addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    match listener.accept().await {
        Ok((mut peer_stream, addr)) => {
            let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
            assert!(matches!(version, Message::Version(..)));

            //  let h = MessageHeader::read_from_stream(&mut peer_stream).await;
            //  assert!(Version::read_from_stream(&mut peer_stream).await.is_ok());

            Message::Version(Version::new(node_addr, listener.local_addr().unwrap()))
                .write_to_stream(&mut peer_stream)
                .await;

            //  Version::new(node_addr, listener.local_addr().unwrap())
            //      .write_to_stream(&mut peer_stream)
            //      .await;

            let verack = Message::read_from_stream(&mut peer_stream).await.unwrap();
            assert!(matches!(verack, Message::Verack));

            Message::Verack.write_to_stream(&mut peer_stream).await;
        }
        Err(e) => println!("couldn't get client: {:?}", e),
    }
}
