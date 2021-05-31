// Message dimensions.
pub const HEADER_LEN: usize = 24;
pub const MAX_MESSAGE_LEN: usize = 2 * 1024 * 1024;

// Network version identifier.
pub const MAGIC: [u8; 4] = [0xfa, 0x1a, 0xf9, 0xbf];

// Message command bytes.
pub const VERSION_COMMAND: [u8; 12] = *b"version\0\0\0\0\0";
pub const VERACK_COMMAND: [u8; 12] = *b"verack\0\0\0\0\0\0";
pub const PING_COMMAND: [u8; 12] = *b"ping\0\0\0\0\0\0\0\0";
pub const PONG_COMMAND: [u8; 12] = *b"pong\0\0\0\0\0\0\0\0";
pub const GETADDR_COMMAND: [u8; 12] = *b"getaddr\0\0\0\0\0";
pub const ADDR_COMMAND: [u8; 12] = *b"addr\0\0\0\0\0\0\0\0";
pub const GETHEADERS_COMMAND: [u8; 12] = *b"getheaders\0\0";
pub const HEADERS_COMMAND: [u8; 12] = *b"headers\0\0\0\0\0";
pub const GETBLOCKS_COMMAND: [u8; 12] = *b"getblocks\0\0\0";
pub const BLOCK_COMMAND: [u8; 12] = *b"block\0\0\0\0\0\0\0";
pub const GETDATA_COMMAND: [u8; 12] = *b"getdata\0\0\0\0\0";
pub const INV_COMMAND: [u8; 12] = *b"inv\0\0\0\0\0\0\0\0\0";
pub const NOTFOUND_COMMAND: [u8; 12] = *b"notfound\0\0\0\0";
pub const MEMPOOL_COMMAND: [u8; 12] = *b"mempool\0\0\0\0\0";
pub const TX_COMMAND: [u8; 12] = *b"tx\0\0\0\0\0\0\0\0\0\0";
pub const REJECT_COMMAND: [u8; 12] = *b"reject\0\0\0\0\0\0";
pub const FILTERLOAD_COMMAND: [u8; 12] = *b"filterload\0\0";
pub const FILTERADD_COMMAND: [u8; 12] = *b"filteradd\0\0\0";
pub const FILTERCLEAR_COMMAND: [u8; 12] = *b"filterclear\0";
