#![no_main]
use libfuzzer_sys::fuzz_target;

use tokio_util::codec::Decoder;
use ziggurat::tools::synthetic_node::MessageCodec;

fuzz_target!(|data: &[u8]| {
    let mut codec = MessageCodec::default();
    let _ = codec.decode(&mut data.into());
});
