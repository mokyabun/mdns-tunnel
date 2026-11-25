//! Tunnel wire format: a hello (`MAGIC`, `u16` version), then frames of a big-endian `u16`
//! length followed by one mDNS packet. Length 0 is a keepalive.

use std::io::{self, Read, Write};

use anyhow::bail;

pub const MAGIC: [u8; 4] = *b"MDNT";
pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Keepalive,
    Packet(Vec<u8>),
}

pub fn write_frame(out: &mut impl Write, frame: &Frame) -> io::Result<()> {
    let payload: &[u8] = match frame {
        Frame::Keepalive => &[],
        Frame::Packet(packet) => packet,
    };

    let len = u16::try_from(payload.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("packet of {} bytes does not fit in a frame", payload.len()),
        )
    })?;

    // Write each frame in one call; two writers must never interleave inside a frame.
    out.write_all(&[len.to_be_bytes().as_slice(), payload].concat())
}

pub fn read_frame(input: &mut impl Read) -> io::Result<Frame> {
    let mut len = [0; 2];
    input.read_exact(&mut len)?;
    let len = u16::from_be_bytes(len) as usize;
    if len == 0 {
        return Ok(Frame::Keepalive);
    }

    let mut packet = vec![0; len];
    input.read_exact(&mut packet)?;
    Ok(Frame::Packet(packet))
}

pub fn write_hello(out: &mut impl Write) -> io::Result<()> {
    out.write_all(&[MAGIC.as_slice(), &PROTOCOL_VERSION.to_be_bytes()].concat())
}

pub fn read_hello(input: &mut impl Read) -> anyhow::Result<()> {
    let mut magic = [0; 4];
    input.read_exact(&mut magic)?;
    if magic != MAGIC {
        bail!("peer is not mdns-tunnel (magic {magic:02x?})");
    }

    let mut version = [0; 2];
    input.read_exact(&mut version)?;

    let peer = u16::from_be_bytes(version);
    if peer != PROTOCOL_VERSION {
        bail!("peer speaks protocol version {peer}, this relay speaks {PROTOCOL_VERSION}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn encode(frames: &[Frame]) -> Vec<u8> {
        let mut buf = Vec::new();
        for frame in frames {
            write_frame(&mut buf, frame).unwrap();
        }
        buf
    }

    #[test]
    fn packet_round_trip() {
        let frame = Frame::Packet(vec![1, 2, 3]);
        let buf = encode(std::slice::from_ref(&frame));
        assert_eq!(buf, [0, 3, 1, 2, 3]);
        assert_eq!(read_frame(&mut Cursor::new(buf)).unwrap(), frame);
    }

    #[test]
    fn keepalive_is_length_zero() {
        let buf = encode(&[Frame::Keepalive]);
        assert_eq!(buf, [0, 0]);
        assert_eq!(read_frame(&mut Cursor::new(buf)).unwrap(), Frame::Keepalive);
    }

    #[test]
    fn two_frames_back_to_back() {
        let frames = [Frame::Packet(vec![7; 300]), Frame::Packet(vec![9])];
        let mut input = Cursor::new(encode(&frames));
        assert_eq!(read_frame(&mut input).unwrap(), frames[0]);
        assert_eq!(read_frame(&mut input).unwrap(), frames[1]);
        let err = read_frame(&mut input).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn truncated_frame_is_an_error() {
        let mut buf = encode(&[Frame::Packet(vec![1, 2, 3])]);
        buf.pop();
        let err = read_frame(&mut Cursor::new(buf)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn refuses_oversized_packet() {
        let err = write_frame(&mut Vec::new(), &Frame::Packet(vec![0; 65_536])).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn hello_round_trip() {
        let mut buf = Vec::new();
        write_hello(&mut buf).unwrap();
        read_hello(&mut Cursor::new(buf)).unwrap();
    }

    #[test]
    fn wrong_magic() {
        let err = read_hello(&mut Cursor::new(b"HTTP/1".to_vec())).unwrap_err();
        assert!(err.to_string().contains("not mdns-tunnel"));
    }

    #[test]
    fn version_mismatch_names_peer_version() {
        let mut hello = MAGIC.to_vec();
        hello.extend_from_slice(&(PROTOCOL_VERSION + 1).to_be_bytes());
        let err = read_hello(&mut Cursor::new(hello)).unwrap_err();
        assert!(
            err.to_string()
                .contains(&(PROTOCOL_VERSION + 1).to_string())
        );
    }
}
