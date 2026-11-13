//! Packet dissection (task 4B.7): Ethernet -> IPv4/IPv6 -> TCP/UDP -> DNS/HTTP summaries,
//! TCP stream reassembly and simple statistics. Pure functions over captured bytes; no I/O.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub src: String,
    pub dst: String,
    pub proto: &'static str,
    pub sport: u16,
    pub dport: u16,
    pub seq: u32,
    pub payload: Vec<u8>,
    pub summary: String,
}

fn be16(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

fn ipv4(b: &[u8]) -> String {
    format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
}

fn ipv6(b: &[u8]) -> String {
    (0..8)
        .map(|i| format!("{:x}", u16::from_be_bytes([b[i * 2], b[i * 2 + 1]])))
        .collect::<Vec<_>>()
        .join(":")
}

/// DNS name starting at `i` (compression pointers followed up to a small depth).
fn dns_name(b: &[u8], mut i: usize) -> Option<String> {
    let mut parts = Vec::new();
    for _ in 0..32 {
        let len = *b.get(i)? as usize;
        if len == 0 {
            return Some(parts.join("."));
        }
        if len & 0xC0 == 0xC0 {
            i = ((len & 0x3F) << 8) | *b.get(i + 1)? as usize;
            continue;
        }
        parts.push(String::from_utf8_lossy(b.get(i + 1..i + 1 + len)?).into_owned());
        i += 1 + len;
    }
    None
}

fn dns_summary(p: &[u8]) -> Option<String> {
    let flags = be16(p, 2)?;
    if be16(p, 4)? == 0 {
        return None;
    }
    let name = dns_name(p, 12)?;
    let kind = if flags & 0x8000 == 0 {
        "query"
    } else {
        "response"
    };
    Some(format!("DNS {kind} {name}"))
}

fn http_summary(p: &[u8]) -> Option<String> {
    let line = p.split(|&c| c == b'\r' || c == b'\n').next()?;
    let s = std::str::from_utf8(line).ok()?;
    let ok = [
        "GET ", "POST ", "PUT ", "DELETE ", "HEAD ", "OPTIONS ", "PATCH ", "HTTP/1.",
    ]
    .iter()
    .any(|m| s.starts_with(m));
    ok.then(|| format!("HTTP {s}"))
}

/// Dissects an Ethernet frame. `None` for frames that are not IPv4/IPv6 TCP or UDP.
pub fn dissect(frame: &[u8]) -> Option<Packet> {
    let (src, dst, proto, l4) = match be16(frame, 12)? {
        0x0800 => {
            let ip = frame.get(14..)?;
            let ihl = (*ip.first()? & 0x0F) as usize * 4;
            if ihl < 20 || ip.len() < ihl {
                return None;
            }
            (
                ipv4(ip.get(12..16)?),
                ipv4(ip.get(16..20)?),
                ip[9],
                ip.get(ihl..)?,
            )
        }
        0x86DD => {
            let ip = frame.get(14..)?;
            if ip.len() < 40 {
                return None;
            }
            (ipv6(&ip[8..24]), ipv6(&ip[24..40]), ip[6], &ip[40..])
        }
        _ => return None,
    };
    let (sport, dport) = (be16(l4, 0)?, be16(l4, 2)?);
    match proto {
        6 => {
            let off = (*l4.get(12)? >> 4) as usize * 4;
            let payload = l4.get(off..)?.to_vec();
            let seq = u32::from_be_bytes(l4.get(4..8)?.try_into().ok()?);
            let summary = http_summary(&payload)
                .unwrap_or_else(|| format!("TCP {sport} -> {dport} len={}", payload.len()));
            Some(Packet {
                src,
                dst,
                proto: "TCP",
                sport,
                dport,
                seq,
                payload,
                summary,
            })
        }
        17 => {
            let payload = l4.get(8..)?.to_vec();
            let summary = if sport == 53 || dport == 53 {
                dns_summary(&payload)
            } else {
                None
            }
            .unwrap_or_else(|| format!("UDP {sport} -> {dport} len={}", payload.len()));
            Some(Packet {
                src,
                dst,
                proto: "UDP",
                sport,
                dport,
                seq: 0,
                payload,
                summary,
            })
        }
        _ => None,
    }
}

/// Reassembles one direction of TCP streams: payloads ordered by sequence number, duplicate
/// (retransmitted) segments dropped. Key is `src:sport->dst:dport`.
pub fn reassemble(packets: &[Packet]) -> BTreeMap<String, Vec<u8>> {
    let mut segs: BTreeMap<String, BTreeMap<u32, &[u8]>> = BTreeMap::new();
    for p in packets
        .iter()
        .filter(|p| p.proto == "TCP" && !p.payload.is_empty())
    {
        let key = format!("{}:{}->{}:{}", p.src, p.sport, p.dst, p.dport);
        segs.entry(key)
            .or_default()
            .entry(p.seq)
            .or_insert(&p.payload);
    }
    segs.into_iter()
        .map(|(k, m)| (k, m.values().flat_map(|s| s.iter().copied()).collect()))
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub packets: usize,
    pub bytes: usize,
    pub by_proto: BTreeMap<String, usize>,
}

pub fn stats(packets: &[Packet]) -> Stats {
    let mut s = Stats::default();
    for p in packets {
        s.packets += 1;
        s.bytes += p.payload.len();
        *s.by_proto.entry(p.proto.to_string()).or_default() += 1;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eth_ipv4(proto: u8, l4: &[u8]) -> Vec<u8> {
        let mut f = vec![0u8; 12];
        f.extend([0x08, 0x00]);
        let mut ip = vec![
            0x45, 0, 0, 0, 0, 0, 0, 0, 64, proto, 0, 0, 10, 0, 0, 1, 10, 0, 0, 2,
        ];
        ip.extend(l4);
        f.extend(ip);
        f
    }

    fn tcp(sport: u16, dport: u16, seq: u32, data: &[u8]) -> Vec<u8> {
        let mut t = Vec::new();
        t.extend(sport.to_be_bytes());
        t.extend(dport.to_be_bytes());
        t.extend(seq.to_be_bytes());
        t.extend([0, 0, 0, 0, 0x50, 0x18, 0, 0, 0, 0, 0, 0]);
        t.extend(data);
        t
    }

    #[test]
    fn http_request_is_summarised() {
        let f = eth_ipv4(6, &tcp(5000, 80, 1, b"GET /x HTTP/1.1\r\nHost: a\r\n\r\n"));
        let p = dissect(&f).unwrap();
        assert_eq!(
            (p.src.as_str(), p.dst.as_str(), p.proto),
            ("10.0.0.1", "10.0.0.2", "TCP")
        );
        assert_eq!(p.summary, "HTTP GET /x HTTP/1.1");
    }

    #[test]
    fn dns_query_name_is_decoded() {
        let mut dns = vec![0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
        dns.extend(b"\x03www\x07example\x03com\x00\x00\x01\x00\x01");
        let mut udp = vec![0xC3, 0x50, 0, 53, 0, 0, 0, 0];
        udp.extend(dns);
        let p = dissect(&eth_ipv4(17, &udp)).unwrap();
        assert_eq!(p.summary, "DNS query www.example.com");
    }

    #[test]
    fn reassembly_orders_segments_and_drops_retransmits() {
        let mk = |seq, d: &[u8]| dissect(&eth_ipv4(6, &tcp(1, 2, seq, d))).unwrap();
        let pk = [mk(100, b"world"), mk(95, b"hello"), mk(100, b"world")];
        let streams = reassemble(&pk);
        assert_eq!(streams.values().next().unwrap(), b"helloworld");
        let s = stats(&pk);
        assert_eq!((s.packets, s.bytes, s.by_proto["TCP"]), (3, 15, 3));
    }

    #[test]
    fn truncated_and_foreign_frames_are_rejected() {
        assert!(dissect(&[]).is_none());
        assert!(dissect(&[0; 13]).is_none());
        let mut arp = vec![0u8; 12];
        arp.extend([0x08, 0x06, 0, 0]);
        assert!(dissect(&arp).is_none());
        assert!(dissect(&eth_ipv4(6, &[0, 1])).is_none());
    }
}
