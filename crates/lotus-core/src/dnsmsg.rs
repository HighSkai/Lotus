//! Minimal DNS message encoder/decoder used by health checks and diagnostics.
//! Supports building a single-question query and reading A/AAAA/TXT answers.

use std::net::{Ipv4Addr, Ipv6Addr};

pub const TYPE_A: u16 = 1;
pub const TYPE_TXT: u16 = 16;
pub const TYPE_AAAA: u16 = 28;

pub fn build_query(id: u16, name: &str, qtype: u16) -> Vec<u8> {
    let mut m = Vec::with_capacity(32 + name.len());
    m.extend_from_slice(&id.to_be_bytes());
    m.extend_from_slice(&[0x01, 0x00]); // RD
    m.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]); // QD=1
    for label in name.trim_end_matches('.').split('.') {
        let l = label.as_bytes();
        m.push(l.len().min(63) as u8);
        m.extend_from_slice(&l[..l.len().min(63)]);
    }
    m.push(0);
    m.extend_from_slice(&qtype.to_be_bytes());
    m.extend_from_slice(&[0, 1]); // IN
    m
}

#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    A(Ipv4Addr),
    Aaaa(Ipv6Addr),
    Txt(String),
    Other(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub id: u16,
    pub rcode: u8,
    pub answers: Vec<Answer>,
}

fn skip_name(b: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let len = *b.get(i)? as usize;
        if len == 0 {
            return Some(i + 1);
        }
        if len & 0xC0 == 0xC0 {
            return Some(i + 2);
        }
        i += 1 + len;
    }
}

pub fn parse_response(b: &[u8]) -> Option<Response> {
    if b.len() < 12 {
        return None;
    }
    let id = u16::from_be_bytes([b[0], b[1]]);
    let rcode = b[3] & 0x0F;
    let qd = u16::from_be_bytes([b[4], b[5]]) as usize;
    let an = u16::from_be_bytes([b[6], b[7]]) as usize;
    let mut i = 12;
    for _ in 0..qd {
        i = skip_name(b, i)? + 4;
    }
    let mut answers = Vec::new();
    for _ in 0..an {
        i = skip_name(b, i)?;
        let ty = u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]);
        let rdlen = u16::from_be_bytes([*b.get(i + 8)?, *b.get(i + 9)?]) as usize;
        let start = i + 10;
        let rdata = b.get(start..start + rdlen)?;
        answers.push(match ty {
            TYPE_A if rdlen == 4 => Answer::A(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3])),
            TYPE_AAAA if rdlen == 16 => {
                let mut o = [0u8; 16];
                o.copy_from_slice(rdata);
                Answer::Aaaa(Ipv6Addr::from(o))
            }
            TYPE_TXT => {
                let mut s = String::new();
                let mut j = 0;
                while j < rdata.len() {
                    let l = rdata[j] as usize;
                    s.push_str(&String::from_utf8_lossy(rdata.get(j + 1..j + 1 + l)?));
                    j += 1 + l;
                }
                Answer::Txt(s)
            }
            other => Answer::Other(other),
        });
        i = start + rdlen;
    }
    Some(Response { id, rcode, answers })
}
