//! Port of `src/decode.c` + `src/decode.h` — `Transfer-Encoding: chunked`,
//! `Content-Encoding` gzip/deflate/brotli et transcodage charset→UTF-8.
//!
//! Compression : `flate2` (backend `rust_backend` = `miniz_oxide` pur Rust) +
//! `brotli` (pur Rust). Charset : `encoding_rs` (jeu de labels WHATWG au
//! lieu d'iconv — divergence couverte, cf. [`Decode::charset_init`]).
//!
//! Quirks amont préservés :
//! - le header de chunk est lu par `strtol` **sans borne** sur le buffer
//!   NUL-terminé : whitespace C (dont `\n`) skippé, signe, préfixe `0x`
//!   optionnel, puis troncature `long`→`int` (i32). Une ligne non-hex ou
//!   `0` termine le flux (`finished`).
//! - `deflate` : sur erreur de données, l'output partiel est jeté et tout
//!   l'input du call est rejoué en raw-deflate.
//! - gzip : les octets produits avant l'erreur sont conservés.
//! - charset : sur séquence illégale, U+FFFD est émis et **un seul octet**
//!   source est sauté (comme l'`inPtr++` amont).

use std::io::{Cursor, Read};

use dillo_common::dlib;
use flate2::{Decompress, FlushDecompress, Status};
use tracing::{debug, error, warn};

/// `bufsize` — buffer de sortie amont.
const BUFSIZE: usize = 8 * 1024;

/// `isspace` locale C pour `strtol` (espace, \t \n \v \f \r).
const fn is_c_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

const fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn hex_val(b: u8) -> i64 {
    i64::from(match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => b - b'A' + 10,
    })
}

/// `strtol(s, NULL, 0x10)` — whitespace/sign/`0x` optionnels, digits hex,
/// saturation `LONG_MAX`/`MIN`. Aucun digit → 0. Lit jusqu'au premier
/// octet non-hex, sans borne (le caller garantit `\n` ou NUL amont).
fn strtol_hex(bytes: &[u8]) -> i64 {
    let mut i = 0;
    while i < bytes.len() && is_c_space(bytes[i]) {
        i += 1;
    }
    let neg = if matches!(bytes.get(i), Some(b'+')) {
        i += 1;
        false
    } else if matches!(bytes.get(i), Some(b'-')) {
        i += 1;
        true
    } else {
        false
    };
    // préfixe "0x" accepté en base 16 seulement s'il précède un digit
    if bytes.get(i) == Some(&b'0')
        && matches!(bytes.get(i + 1), Some(b'x' | b'X'))
        && bytes.get(i + 2).is_some_and(|b| is_hex(*b))
    {
        i += 2;
    }
    let mut val: i64 = 0;
    let mut any = false;
    while i < bytes.len() && is_hex(bytes[i]) {
        any = true;
        val = val.saturating_mul(16).saturating_add(hex_val(bytes[i]));
        i += 1;
    }
    if !any {
        0
    } else if neg {
        val.saturating_neg()
    } else {
        val
    }
}

/// `DecodeTransfer` — décodeur `Transfer-Encoding` incrémental.
#[derive(Debug)]
pub struct DecodeTransfer {
    leftover: Vec<u8>,
    /// `*(int *)dc->state` — amont stocke un `int` (troncature du `long`).
    chunk_remaining: i32,
    finished: bool,
}

impl DecodeTransfer {
    /// `a_Decode_transfer_init` — `Some` uniquement pour `"chunked"`
    /// (comparaison ASCII insensible à la casse), `None` sinon.
    pub fn new(format: Option<&str>) -> Option<Self> {
        let f = format?;
        // dStrAsciiCasecmp : comparaison complète, pas préfixe
        if dlib::str_ascii_casecmp(f.as_bytes(), b"chunked") != 0 {
            return None;
        }
        debug!("chunked!");
        Some(Self {
            leftover: Vec::new(),
            chunk_remaining: 0,
            finished: false,
        })
    }

    /// `a_Decode_transfer_process` — consomme `input`, retourne le corps
    /// décodé. `leftover` conserve les en-têtes partiels entre les calls.
    // les casts reproduisent les conversions `int`/`long` amont (dont la
    // troncature `strtol`→`int` sur `chunk_remaining`)
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    pub fn process(&mut self, input: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(input.len());
        self.leftover.extend_from_slice(input);
        let mut pos = 0usize;
        let mut remaining = self.leftover.len();

        while remaining > 0 {
            if self.chunk_remaining > 2 {
                // corps de chunk à copier
                let copylen = (self.chunk_remaining - 2).min(remaining as i32) as usize;
                output.extend_from_slice(&self.leftover[pos..pos + copylen]);
                self.chunk_remaining -= copylen as i32;
                remaining -= copylen;
                pos += copylen;
            }
            if self.chunk_remaining == 2 && remaining > 0 {
                // CR à jeter
                self.chunk_remaining -= 1;
                remaining -= 1;
                pos += 1;
            }
            if self.chunk_remaining == 1 && remaining > 0 {
                // LF à jeter
                self.chunk_remaining -= 1;
                remaining -= 1;
                pos += 1;
            }

            // en-tête de chunk : une ligne commençant par la longueur hex
            let Some(off) = self.leftover[pos..pos + remaining]
                .iter()
                .position(|&b| b == b'\n')
            else {
                break; // ligne incomplète
            };
            let eol = pos + off;

            let v = strtol_hex(&self.leftover[pos..]);
            if v == 0 {
                self.finished = true;
                break; // longueur 0 (ou non-hex) → fin
            }
            remaining -= (eol - pos) + 1;
            pos = eol + 1;
            self.chunk_remaining = v.wrapping_add(2) as i32; // + CRLF final
        }

        self.leftover.drain(..pos);
        output
    }

    /// `a_Decode_transfer_finished`.
    #[must_use]
    pub const fn finished(&self) -> bool {
        self.finished
    }
}

/// Accumule `input` dans le cursor sous-jacent (position conservée),
/// lit via `read` jusqu'à épuisement du flux disponible.
#[allow(clippy::cast_possible_truncation)] // position ≤ len du vec
fn read_decoded(
    dec: &mut brotli::Decompressor<Cursor<Vec<u8>>>,
    input: &[u8],
    output: &mut Vec<u8>,
) {
    let cur = dec.get_mut();
    let save = cur.position();
    cur.get_mut().extend_from_slice(input);
    cur.set_position(save);

    let mut buf = [0u8; BUFSIZE];
    loop {
        match dec.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => output.extend_from_slice(&buf[..n]),
        }
    }
    // compacter le leftover : jeter ce que le décodeur a consommé
    let cur = dec.get_mut();
    let consumed = cur.position() as usize;
    let inner = cur.get_mut();
    if consumed >= inner.len() {
        inner.clear();
    } else {
        inner.drain(..consumed);
    }
    cur.set_position(0);
}

/// Phase du décodeur gzip — zlib `inflateInit2(MAX_WBITS+16)` fait la même
/// machine : en-tête RFC1952, corps raw-deflate, trailer CRC32+ISIZE.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum GzipPhase {
    /// En-tête pas encore complet (FEXTRA/FNAME/FCOMMENT/FHCRC gérés).
    Header,
    /// Corps : `inflate` raw en cours (état dans `GzipState::infl`).
    Inflate,
    /// Corps terminé (`Z_STREAM_END`) : attend 8 octets de trailer.
    Trailer,
    /// Trailer vérifié ou erreur fatale : input ultérieur ignoré.
    Done,
}

/// `Decode_gzip` — équivalent du `z_stream` wbits=31 : vérifie l'en-tête
/// (magie, CM=8, bits réservés de FLG, FHCRC), le CRC32 et l'ISIZE.
#[doc(hidden)] // payload du variant `Decode::Gzip`
pub struct GzipState {
    phase: GzipPhase,
    /// `z_stream` raw (actif en phase `Inflate`).
    infl: Option<Decompress>,
    /// Octets non encore consommés (en-tête incomplet, trailer, restes
    /// post-stream-end).
    pending: Vec<u8>,
    /// CRC32 des octets décodés (pour le trailer).
    crc: crc32fast::Hasher,
    /// Total décodé (mod 2^32 → ISIZE).
    written: u64,
}

/// En-tête gzip : `Err` = invalide (`Z_DATA_ERROR` amont), `Ok(None)` =
/// incomplet, `Ok(Some(len))` = en-tête complet de `len` octets.
// troncature u32→u16 voulue : FHCRC = 16 bits bas du CRC32 (RFC1952)
#[allow(clippy::cast_possible_truncation)]
fn parse_gzip_header(h: &[u8]) -> Result<Option<usize>, ()> {
    if h.len() < 10 {
        return Ok(None);
    }
    if h[0] != 0x1f || h[1] != 0x8b || h[2] != 8 {
        return Err(()); // bad magic / CM != deflate
    }
    let flg = h[3];
    if flg & 0xE0 != 0 {
        return Err(()); // bits réservés (zlib : "unknown header flags")
    }
    let mut i = 10usize;
    if flg & 0x04 != 0 {
        // FEXTRA : u16le len + len octets
        if h.len() < i + 2 {
            return Ok(None);
        }
        let xlen = usize::from(h[i]) | usize::from(h[i + 1]) << 8;
        i += 2 + xlen;
    }
    for flag in [0x08u8, 0x10] {
        // FNAME puis FCOMMENT : chaînes NUL-terminées
        if flg & flag != 0 {
            match h[i..].iter().position(|&b| b == 0) {
                Some(off) => i += off + 1,
                None => return Ok(None),
            }
        }
    }
    if flg & 0x02 != 0 {
        // FHCRC : CRC16 = CRC32 des octets précédents, low 16 bits
        if h.len() < i + 2 {
            return Ok(None);
        }
        let expect = u16::from_le_bytes([h[i], h[i + 1]]);
        let got = crc32fast::hash(&h[..i]) as u16;
        if expect != got {
            return Err(());
        }
        i += 2;
    }
    if h.len() < i {
        return Ok(None);
    }
    Ok(Some(i))
}

impl GzipState {
    fn new() -> Self {
        Self {
            phase: GzipPhase::Header,
            infl: None,
            pending: Vec::new(),
            crc: crc32fast::Hasher::new(),
            written: 0,
        }
    }

    // deltas u64→usize bornés par BUFSIZE ; `written as u32` = ISIZE mod 2^32
    #[allow(clippy::cast_possible_truncation)]
    fn process(&mut self, input: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(input);
        let mut output = Vec::new();

        if self.phase == GzipPhase::Header {
            match parse_gzip_header(&self.pending) {
                Err(()) => {
                    error!("gzip decompression error");
                    self.phase = GzipPhase::Done;
                    self.pending.clear();
                }
                Ok(None) => return output,
                Ok(Some(len)) => {
                    self.pending.drain(..len);
                    self.infl = Some(Decompress::new(false));
                    self.phase = GzipPhase::Inflate;
                }
            }
        }

        if self.phase == GzipPhase::Inflate {
            if let Some(d) = self.infl.as_mut() {
                let mut consumed = 0usize;
                let mut buf = [0u8; BUFSIZE];
                let mut ended = false;
                let mut fatal = false;
                while consumed < self.pending.len() {
                    let (in0, out0) = (d.total_in(), d.total_out());
                    let res =
                        d.decompress(&self.pending[consumed..], &mut buf, FlushDecompress::Sync);
                    let produced = (d.total_out() - out0) as usize;
                    // amont : append avant le test de statut
                    output.extend_from_slice(&buf[..produced]);
                    self.crc.update(&buf[..produced]);
                    self.written += produced as u64;
                    match res {
                        Ok(Status::Ok) => consumed += (d.total_in() - in0) as usize,
                        Ok(Status::StreamEnd) => {
                            consumed += (d.total_in() - in0) as usize;
                            ended = true;
                            break;
                        }
                        Ok(Status::BufError) => break,
                        Err(_) => {
                            error!("gzip decompression error");
                            fatal = true;
                            break;
                        }
                    }
                }
                self.pending.drain(..consumed);
                if fatal {
                    self.phase = GzipPhase::Done;
                    self.infl = None;
                    self.pending.clear();
                    return output;
                }
                if ended {
                    self.phase = GzipPhase::Trailer;
                    self.infl = None;
                }
            }
        }

        if self.phase == GzipPhase::Trailer && self.pending.len() >= 8 {
            let p = &self.pending;
            let crc_expect = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
            let isize_expect = u32::from_le_bytes([p[4], p[5], p[6], p[7]]);
            if crc_expect != self.crc.clone().finalize() || isize_expect != self.written as u32 {
                error!("gzip decompression error");
            }
            self.phase = GzipPhase::Done;
            self.pending.clear();
        }
        output
    }
}

/// `Decode` — décodeur de contenu (compression ou charset), dispatch enum
/// à la place des pointeurs de fonction `dc->decode`/`dc->free`.
pub enum Decode {
    /// `Decode_gzip` (`inflateInit2(MAX_WBITS+16)` — en-tête+trailer
    /// RFC1952 vérifiés à la main au-dessus de l'inflate raw).
    Gzip(GzipState),
    /// `Decode_deflate` (`inflateInit` — wrapper zlib).
    Deflate(Decompress),
    /// `Decode_raw_deflate` (`inflateInit2(-MAX_WBITS)`) après fallback.
    RawDeflate(Decompress),
    /// `Decode_brotli_process` (`ENABLE_BROTLI` — toujours compilé ici).
    Brotli(Box<brotli::Decompressor<Cursor<Vec<u8>>>>),
    /// `Decode_charset` — iconv→UTF-8 via `encoding_rs`.
    Charset {
        decoder: encoding_rs::Decoder,
        leftover: Vec<u8>,
    },
}

impl std::fmt::Debug for Decode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Gzip(_) => "Decode::Gzip",
            Self::Deflate(_) => "Decode::Deflate",
            Self::RawDeflate(_) => "Decode::RawDeflate",
            Self::Brotli(_) => "Decode::Brotli",
            Self::Charset { .. } => "Decode::Charset",
        })
    }
}

impl Decode {
    /// `a_Decode_content_init` — `"gzip"`/`"x-gzip"`/`"deflate"`/`"br"`
    /// (ASCII insensible à la casse). Inconnu/vide → `None` + warn amont.
    #[must_use]
    pub fn content_init(format: Option<&str>) -> Option<Self> {
        let f = format?;
        if f.is_empty() {
            return None;
        }
        if dlib::str_ascii_casecmp(f.as_bytes(), b"gzip") == 0
            || dlib::str_ascii_casecmp(f.as_bytes(), b"x-gzip") == 0
        {
            debug!("gzipped data!");
            Some(Self::Gzip(GzipState::new()))
        } else if dlib::str_ascii_casecmp(f.as_bytes(), b"deflate") == 0 {
            debug!("deflated data!");
            Some(Self::Deflate(Decompress::new(true)))
        } else if dlib::str_ascii_casecmp(f.as_bytes(), b"br") == 0 {
            debug!("brotli data!");
            Some(Self::Brotli(Box::new(brotli::Decompressor::new(
                Cursor::new(Vec::new()),
                BUFSIZE,
            ))))
        } else {
            warn!("Content-Encoding '{f}' not recognized.");
            None
        }
    }

    /// `a_Decode_charset_init` — `Some` pour tout label non-UTF-8 connu
    /// (`encoding_rs` couvre les labels WHATWG — `iconv` en couvrait
    /// davantage ; label inconnu → `None` + warn amont).
    #[must_use]
    pub fn charset_init(format: Option<&str>) -> Option<Self> {
        let f = format?;
        if f.is_empty() || dlib::str_ascii_casecmp(f.as_bytes(), b"UTF-8") == 0 {
            return None;
        }
        if let Some(enc) = encoding_rs::Encoding::for_label(f.as_bytes()) {
            Some(Self::Charset {
                decoder: enc.new_decoder(),
                leftover: Vec::new(),
            })
        } else {
            warn!("Unable to convert from character encoding: '{f}'");
            None
        }
    }

    /// `a_Decode_process` — dispatche sur le variant.
    pub fn process(&mut self, input: &[u8]) -> Vec<u8> {
        match self {
            Self::Gzip(gz) => gz.process(input),
            Self::Brotli(br) => {
                // amont : `avail_in == 0` → output vide sans toucher l'état
                if input.is_empty() {
                    return Vec::new();
                }
                let mut output = Vec::new();
                read_decoded(br, input, &mut output);
                output
            }
            Self::Deflate(d) => {
                let (output, data_error) = inflate_process(d, input, InflateKind::DeflateWrapper);
                if data_error {
                    // swap `dc->decode = Decode_raw_deflate` + rejeu de
                    // tout l'input du call (output partiel jeté)
                    warn!("Deflate decompression error. Certain servers illegally fail to send data in a zlib wrapper. Let's try raw deflate.");
                    *self = Self::RawDeflate(Decompress::new(false));
                    return self.process(input);
                }
                output
            }
            Self::RawDeflate(d) => inflate_process(d, input, InflateKind::Raw).0,
            Self::Charset { decoder, leftover } => charset_process(decoder, leftover, input),
        }
    }
}

/// Famille de la boucle `Decode_*` : choisit le message d'erreur amont
/// (gzip passe par `GzDecoder`, pas par cette boucle).
#[derive(Copy, Clone)]
enum InflateKind {
    /// `Decode_deflate` — `Z_DATA_ERROR` → fallback raw (géré par caller).
    DeflateWrapper,
    /// `Decode_raw_deflate` — `"raw deflate decompression also failed"`.
    Raw,
}

/// Boucle `inflate` commune à gzip/deflate/raw : `Z_SYNC_FLUSH`, buffer
/// 8 Kio, octets produits **avant** le test de statut (amont appelle
/// `dStr_append_l` puis examine `rc`).
///
/// Retourne `(output, data_error)` — `data_error` ≙ `Z_DATA_ERROR`
/// (seul cas où `Decode_deflate` bascule en raw).
#[allow(clippy::cast_possible_truncation)] // deltas ≤ BUFSIZE
fn inflate_process(d: &mut Decompress, input: &[u8], kind: InflateKind) -> (Vec<u8>, bool) {
    let mut output = Vec::new();
    let mut consumed = 0usize;
    let mut buf = [0u8; BUFSIZE];

    while consumed < input.len() {
        let (in0, out0) = (d.total_in(), d.total_out());
        let res = d.decompress(&input[consumed..], &mut buf, FlushDecompress::Sync);
        let produced = (d.total_out() - out0) as usize;
        output.extend_from_slice(&buf[..produced]);
        match res {
            Ok(Status::Ok) => {
                consumed += (d.total_in() - in0) as usize;
            }
            Ok(Status::StreamEnd | Status::BufError) => break,
            Err(e) => {
                match kind {
                    // message émis par le caller avant le swap
                    InflateKind::DeflateWrapper => {}
                    InflateKind::Raw => {
                        error!("raw deflate decompression also failed");
                    }
                }
                // Z_NEED_DICT amont ≠ Z_DATA_ERROR : pas de fallback
                return (output, e.needs_dictionary().is_none());
            }
        }
    }
    (output, false)
}

/// `Decode_charset` — boucle `iconv` : `E2BIG` → itère, `EILSEQ` → U+FFFD
/// + skip 1 octet, `EINVAL` (séquence partielle en fin) → leftover.
fn charset_process(
    decoder: &mut encoding_rs::Decoder,
    leftover: &mut Vec<u8>,
    input: &[u8],
) -> Vec<u8> {
    use encoding_rs::DecoderResult;

    leftover.extend_from_slice(input);
    let mut output = Vec::new();
    let mut pos = 0usize;
    let mut buf = [0u8; BUFSIZE];

    loop {
        let (result, read, written) =
            decoder.decode_to_utf8_without_replacement(&leftover[pos..], &mut buf, false);
        pos += read;
        output.extend_from_slice(&buf[..written]);
        match result {
            DecoderResult::InputEmpty => break,
            DecoderResult::OutputFull => {}
            DecoderResult::Malformed(_, _) => {
                // EILSEQ : U+FFFD puis un seul octet sauté (inPtr++ amont)
                output.extend_from_slice("\u{FFFD}".as_bytes());
                pos += 1;
            }
        }
        if pos >= leftover.len() {
            break;
        }
    }
    leftover.drain(..pos);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::bufread::{DeflateEncoder, GzEncoder, ZlibEncoder};
    use flate2::Compression;

    fn compress<R: Read>(mut r: R) -> Vec<u8> {
        let mut v = Vec::new();
        r.read_to_end(&mut v).unwrap();
        v
    }

    // ---- Transfer-Encoding ----

    #[test]
    fn transfer_init_only_chunked() {
        assert!(DecodeTransfer::new(Some("chunked")).is_some());
        assert!(DecodeTransfer::new(Some("Chunked")).is_some());
        assert!(DecodeTransfer::new(Some("gzip")).is_none());
        assert!(DecodeTransfer::new(None).is_none());
    }

    #[test]
    fn chunked_one_shot() {
        let mut t = DecodeTransfer::new(Some("chunked")).unwrap();
        let out = t.process(b"5\r\nhello\r\n0\r\n\r\n");
        assert_eq!(out, b"hello");
        assert!(t.finished());
    }

    #[test]
    fn chunked_byte_by_byte() {
        let mut t = DecodeTransfer::new(Some("chunked")).unwrap();
        let wire = b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n";
        // amont : `finished` dès que la ligne "0" est complète — les
        // trailers finaux ne sont pas consommés
        let end_of_zero = wire.windows(3).position(|w| w == b"0\r\n").unwrap() + 2;
        let mut all = Vec::new();
        for i in 0..wire.len() {
            all.extend(t.process(&wire[i..=i]));
            assert_eq!(t.finished(), i >= end_of_zero);
        }
        assert_eq!(all, b"Wikipedia");
    }

    #[test]
    fn chunked_split_body_and_crlf() {
        let mut t = DecodeTransfer::new(Some("chunked")).unwrap();
        assert_eq!(t.process(b"a\r\nabc"), b"abc");
        // corps coupé : 'a' reste en leftover puis CRLF du chunk
        assert_eq!(t.process(b"defghij\r"), b"defghij");
        assert_eq!(t.process(b"\n0\r"), b"");
        assert_eq!(t.process(b"\n\r\n"), b"");
        assert!(t.finished());
    }

    #[test]
    fn chunked_extension_and_hex() {
        // amont : strtol base 16 → extensions ";" ignorées, "0x" accepté
        let mut t = DecodeTransfer::new(Some("chunked")).unwrap();
        let out = t.process(b"5;ext=1\r\nhello\r\n0x4\r\nbye!\r\n0\r\n\r\n");
        assert_eq!(out, b"hellobye!");
        assert!(t.finished());
    }

    #[test]
    fn chunked_garbage_header_finishes() {
        // quirk amont : strtol==0 sur non-hex → finished (comme "0")
        let mut t = DecodeTransfer::new(Some("chunked")).unwrap();
        let out = t.process(b"zz\r\nhello\r\n");
        assert_eq!(out, b"");
        assert!(t.finished());
    }

    #[test]
    fn chunked_leading_ws_and_int_truncation() {
        // strtol skippe le whitespace C — dont '\n' (ligne vide tolérée)
        let mut t = DecodeTransfer::new(Some("chunked")).unwrap();
        let out = t.process(b"  3\r\nhey\r\n0\r\n\r\n");
        assert_eq!(out, b"hey");
        assert!(t.finished());
    }

    // ---- Content-Encoding ----

    #[test]
    fn content_init_formats() {
        assert!(Decode::content_init(Some("gzip")).is_some());
        assert!(Decode::content_init(Some("x-gzip")).is_some());
        assert!(Decode::content_init(Some("deflate")).is_some());
        assert!(Decode::content_init(Some("br")).is_some());
        assert!(Decode::content_init(Some("identity")).is_none());
        assert!(Decode::content_init(None).is_none());
        assert!(Decode::content_init(Some("")).is_none());
    }

    #[test]
    fn gzip_roundtrip_and_incremental() {
        let wire = compress(GzEncoder::new(
            &b"hello world, hello"[..],
            Compression::default(),
        ));
        let mut d = Decode::content_init(Some("gzip")).unwrap();
        let mut out = Vec::new();
        for ch in wire.chunks(3) {
            out.extend(d.process(ch));
        }
        assert_eq!(out, b"hello world, hello");
    }

    #[test]
    fn gzip_golden_vector() {
        // "hello" gzipé (format RFC1952 : deflate cb48cdc9c90700,
        // CRC32("hello")=0x3610a686 LE, ISIZE=5 LE)
        const WIRE: &[u8] = &[
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xcb, 0x48, 0xcd, 0xc9,
            0xc9, 0x07, 0x00, 0x86, 0xa6, 0x10, 0x36, 0x05, 0x00, 0x00, 0x00,
        ];
        let mut d = Decode::content_init(Some("gzip")).unwrap();
        assert_eq!(d.process(WIRE), b"hello");
    }

    #[test]
    fn deflate_zlib_wrapper() {
        let wire = compress(ZlibEncoder::new(
            &b"deflated text"[..],
            Compression::default(),
        ));
        let mut d = Decode::content_init(Some("deflate")).unwrap();
        assert_eq!(d.process(&wire), b"deflated text");
    }

    #[test]
    fn deflate_raw_fallback() {
        // serveur mal élevé : raw deflate sans wrapper zlib →
        // amont bascule en Decode_raw_deflate et rejoue l'input
        let wire = compress(DeflateEncoder::new(
            &b"no wrapper"[..],
            Compression::default(),
        ));
        let mut d = Decode::content_init(Some("deflate")).unwrap();
        assert_eq!(d.process(&wire), b"no wrapper");
        // et les calls suivants restent en raw
        assert!(matches!(d, Decode::RawDeflate(_)));
    }

    #[test]
    fn deflate_invalid_data_gives_partial_then_error() {
        let mut d = Decode::content_init(Some("deflate")).unwrap();
        let out = d.process(b"\xff\xff\xff\xff");
        assert_eq!(out, b"");
    }

    #[test]
    fn brotli_roundtrip() {
        let mut w = brotli::CompressorWriter::new(Vec::new(), BUFSIZE, 5, 22);
        std::io::Write::write_all(&mut w, b"brotli payload").unwrap();
        let wire = w.into_inner();
        let mut d = Decode::content_init(Some("br")).unwrap();
        let mut out = Vec::new();
        for ch in wire.chunks(2) {
            out.extend(d.process(ch));
        }
        assert_eq!(out, b"brotli payload");
    }

    // ---- Charset ----

    #[test]
    fn charset_init_utf8_is_passthrough() {
        assert!(Decode::charset_init(Some("UTF-8")).is_none());
        assert!(Decode::charset_init(Some("utf-8")).is_none());
        assert!(Decode::charset_init(Some("")).is_none());
        assert!(Decode::charset_init(None).is_none());
        assert!(Decode::charset_init(Some("windows-1251")).is_some());
    }

    #[test]
    fn charset_windows_1251() {
        let mut d = Decode::charset_init(Some("windows-1251")).unwrap();
        // "Привет" en cp1251
        let out = d.process(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]);
        assert_eq!(out, "Привет".as_bytes());
    }

    #[test]
    fn charset_iso_8859_1() {
        let mut d = Decode::charset_init(Some("iso-8859-1")).unwrap();
        assert_eq!(d.process(&[0xE9]), "é".as_bytes());
    }

    #[test]
    fn charset_partial_sequence_leftover() {
        let mut d = Decode::charset_init(Some("utf-16le")).unwrap();
        // '€' = E2 82 AC en UTF-16LE? non — U+20AC little-endian: AC 20
        let out1 = d.process(&[0xAC]); // moitié d'une unité UTF-16
        assert_eq!(out1, b"");
        let out2 = d.process(&[0x20]);
        assert_eq!(out2, "€".as_bytes());
    }

    #[test]
    fn charset_eilseq_skips_one_byte() {
        let mut d = Decode::charset_init(Some("utf-16le")).unwrap();
        // unité haute isolée 0x00D8 puis 'A' 0x0041 : amont saute 1 octet
        // par erreur → le second octet forme une nouvelle paire
        let out = d.process(&[0x00, 0xD8, 0x41, 0x00]);
        assert!(out.windows(3).any(|w| w == "\u{FFFD}".as_bytes()));
    }
}
