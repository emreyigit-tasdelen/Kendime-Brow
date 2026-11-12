//! pcapng writer (task 4B.6): section header, interface description, enhanced packet blocks and
//! Decryption Secrets Blocks carrying an NSS-format TLS key log, so Wireshark can decrypt the
//! capture without extra files. Little-endian, microsecond timestamps.

const SHB: u32 = 0x0A0D_0D0A;
const IDB: u32 = 1;
const EPB: u32 = 6;
const DSB: u32 = 0x0000_000A;
const TLS_KEYLOG: u32 = 0x544C_534B; // "TLSK"
pub const LINKTYPE_ETHERNET: u16 = 1;
pub const LINKTYPE_RAW_IP: u16 = 101;

fn block(out: &mut Vec<u8>, kind: u32, body: &[u8]) {
    let pad = (4 - body.len() % 4) % 4;
    let total = (12 + body.len() + pad) as u32;
    out.extend(kind.to_le_bytes());
    out.extend(total.to_le_bytes());
    out.extend(body);
    out.extend(std::iter::repeat_n(0u8, pad));
    out.extend(total.to_le_bytes());
}

pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    /// Starts a capture with one interface of the given link type.
    pub fn new(linktype: u16) -> Self {
        let mut buf = Vec::new();
        let mut shb = Vec::new();
        shb.extend(0x1A2B_3C4Du32.to_le_bytes());
        shb.extend(1u16.to_le_bytes());
        shb.extend(0u16.to_le_bytes());
        shb.extend((-1i64).to_le_bytes());
        block(&mut buf, SHB, &shb);
        let mut idb = Vec::new();
        idb.extend(linktype.to_le_bytes());
        idb.extend(0u16.to_le_bytes());
        idb.extend(65535u32.to_le_bytes());
        block(&mut buf, IDB, &idb);
        Self { buf }
    }

    /// `micros` is the capture time in microseconds since the Unix epoch.
    pub fn packet(&mut self, micros: u64, data: &[u8]) {
        let mut b = Vec::with_capacity(20 + data.len());
        b.extend(0u32.to_le_bytes());
        b.extend(((micros >> 32) as u32).to_le_bytes());
        b.extend((micros as u32).to_le_bytes());
        b.extend((data.len() as u32).to_le_bytes());
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(data);
        block(&mut self.buf, EPB, &b);
    }

    /// Embeds an NSS key log (`CLIENT_RANDOM <hex> <hex>` lines).
    pub fn tls_keys(&mut self, keylog: &str) {
        let mut b = Vec::new();
        b.extend(TLS_KEYLOG.to_le_bytes());
        b.extend((keylog.len() as u32).to_le_bytes());
        b.extend(keylog.as_bytes());
        block(&mut self.buf, DSB, &b);
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
    }

    /// Walks the blocks, checking that leading and trailing lengths agree and are 4-aligned.
    fn blocks(b: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let mut i = 0;
        let mut out = Vec::new();
        while i < b.len() {
            let kind = u32_at(b, i);
            let len = u32_at(b, i + 4) as usize;
            assert_eq!(len % 4, 0);
            assert_eq!(u32_at(b, i + len - 4) as usize, len);
            out.push((kind, b[i + 8..i + len - 4].to_vec()));
            i += len;
        }
        out
    }

    #[test]
    fn header_and_interface_come_first() {
        let b = Writer::new(LINKTYPE_ETHERNET).finish();
        let bl = blocks(&b);
        assert_eq!(bl.len(), 2);
        assert_eq!(bl[0].0, SHB);
        assert_eq!(u32_at(&bl[0].1, 0), 0x1A2B_3C4D);
        assert_eq!(bl[1].0, IDB);
        assert_eq!(u16::from_le_bytes([bl[1].1[0], bl[1].1[1]]), 1);
    }

    #[test]
    fn packets_are_padded_and_keep_their_length() {
        let mut w = Writer::new(LINKTYPE_RAW_IP);
        w.packet(1_700_000_000_123_456, &[1, 2, 3, 4, 5]);
        let bl = blocks(&w.finish());
        let (kind, body) = &bl[2];
        assert_eq!(*kind, EPB);
        assert_eq!(u32_at(body, 12), 5); // captured length
        assert_eq!(u32_at(body, 16), 5); // original length
        assert_eq!(&body[20..25], &[1, 2, 3, 4, 5]);
        let ts = (u64::from(u32_at(body, 4)) << 32) | u64::from(u32_at(body, 8));
        assert_eq!(ts, 1_700_000_000_123_456);
    }

    #[test]
    fn key_log_goes_into_a_decryption_secrets_block() {
        let log = "CLIENT_RANDOM aa bb\n";
        let mut w = Writer::new(LINKTYPE_ETHERNET);
        w.tls_keys(log);
        let bl = blocks(&w.finish());
        let (kind, body) = &bl[2];
        assert_eq!(*kind, DSB);
        assert_eq!(u32_at(body, 0), TLS_KEYLOG);
        assert_eq!(u32_at(body, 4) as usize, log.len());
        assert_eq!(&body[8..8 + log.len()], log.as_bytes());
    }
}
