//! Streaming log redaction, UTF-8 reconstruction and bounded escape/framing regressions.

use dashboard_helper_protocol::Error;
#[test]
/// A newly registered nonce is redacted after prior text; intermediate escape syntax never reaches output.
fn late_delegation_secret_registration_and_escape_intermediates() {
    let mut f = StreamFramer::new(&[&"a".repeat(64)]).unwrap();
    let mut out = String::new();
    f.push(b"prefix", |s, _| out.push_str(&s)).unwrap();
    let nonce = "b".repeat(64);
    f.register_secret(&nonce).unwrap();
    f.push(nonce.as_bytes(), |s, _| out.push_str(&s)).unwrap();
    f.finish(|s, _| out.push_str(&s)).unwrap();
    assert_eq!(out, "prefix[redacted]");
    assert_eq!(frame(&[b"a\x1b(Bz"], &[]).0, "az");
}
use studio_helper::jobs::logs::StreamFramer;
/// Returns concatenated framed `chunks` and per-record progress flags after redacting
/// `secrets`; asserts each output/retention bound and panics on invalid fixture input.
fn frame(
    chunks: &[&[u8]],
    secrets: &[&str],
) -> (String, Vec<bool>) {
    let mut f = StreamFramer::new(secrets).unwrap();
    let mut texts = Vec::new();
    let mut progress = Vec::new();
    for bytes in chunks {
        f.push(bytes, |s, p| {
            assert!(s.len() <= 256);
            texts.push(s);
            progress.push(p);
        })
        .unwrap();
        assert!(f.retained_bytes() <= 326);
    }
    f.finish(|s, p| {
        texts.push(s);
        progress.push(p);
    })
    .unwrap();
    (texts.concat(), progress)
}
#[test]
/// Every secret split and ANSI-interleaved reconstruction is redacted; duplicate/excess secret sets fail.
fn fragmented_three_secrets_and_escape_reconstruction_never_reach_sinks() {
    let a = "a".repeat(64);
    let b = format!("{}b", "a".repeat(63));
    let c = "c".repeat(64);
    for split in 0..=64 {
        let (text, _) = frame(
            &[
                &a.as_bytes()[..split],
                &a.as_bytes()[split..],
                b.as_bytes(),
                c.as_bytes(),
            ],
            &[&a, &b, &c],
        );
        assert_eq!(text, "[redacted][redacted][redacted]");
    }
    let escaped = format!("{}\x1b[31m{}", &a[..32], &a[32..]);
    let chunks: Vec<_> = escaped.as_bytes().chunks(1).collect();
    assert_eq!(frame(&chunks, &[&a]).0, "[redacted]");
    assert!(StreamFramer::new(&[&a, &a]).is_err());
    assert!(StreamFramer::new(&[&a, &b, &c, &"d".repeat(64)]).is_err());
}
#[test]
/// Split Unicode survives; malformed tails replace, CR progress stays distinct, long streams
/// remain bounded and post-finish/oversize chunks are rejected.
fn utf8_malformed_cr_and_endless_output_stay_bounded() {
    let bytes = "界😀é".as_bytes();
    for split in 0..=bytes.len() {
        assert_eq!(frame(&[&bytes[..split], &bytes[split..]], &[]).0, "界😀é");
    }
    assert_eq!(frame(&[b"a\xffb\xe2", b"\x82"], &[]).0, "a�b�");
    let (text, p) = frame(&[b"1\r", b"\n2\r3\n"], &[]);
    assert_eq!(text, "123");
    assert_eq!(p, [true, true, false]);
    let block = vec![b'x'; 4096];
    let chunks = vec![block.as_slice(); 1024];
    assert_eq!(frame(&chunks, &[]).0.len(), 4 * 1024 * 1024);
    assert_eq!(frame(&[b"good\x1b]", block.as_slice()], &[]).0, "good");
    let mut f = StreamFramer::new(&[]).unwrap();
    assert_eq!(f.push(&vec![0; 4097], |_, _| {}), Err(Error::Bounds));
    f.finish(|_, _| {}).unwrap();
    assert_eq!(f.push(b"late", |_, _| {}), Err(Error::Closed));
}
