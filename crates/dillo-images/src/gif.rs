//! Port de `src/gif.c` — décodeur GIF incrémental (LZW maison amont,
//! pas de lib externe — le fichier C *est* le décodeur).
//!
//! `DilloGif` → [`DilloGif`] ; `Gif_process_bytes` = machine d'états
//! push verbatim (signature → descriptor → blocs/extensions → stream
//! LZW), y compris les quirks : `state = 999` sur erreur/trailer/end
//! code (« BUG » amont — jamais de retour en attente de blocs), end
//! code traité comme `error`, `Gif_decode` consomme tout.
//!
//! `a_Gif_callback(Op, data)` amont reçoit le `Client` — ici
//! [`ImageDecoder::callback`] ≙ `Gif_write`/`Gif_close`/`Gif_free`.

use std::cell::RefCell;
use std::rc::Rc;

use dillo_net::cache::{CacheClient, CA_CLOSE, CA_SEND};
use dillo_net::url::DilloUrl;

use crate::dicache::{
    dicache_close, dicache_set_cmap, dicache_set_parms, dicache_write, ImageDecoder,
};
use crate::image::DilloImage;
use crate::imgbuf::ImgType;

/// `INTERLACE`.
const INTERLACE: u32 = 0x40;
/// `LOCALCOLORMAP`.
const LOCALCOLORMAP: u8 = 0x80;

/// `IMAGE_MAX_AREA` (image.hh).
const IMAGE_MAX_AREA: u64 = 6000 * 6000;
/// `MAX_LWZ_BITS`.
const MAX_LWZ_BITS: usize = 12;

/// `LM_to_uint(a,b)`.
fn lm_to_uint(a: u8, b: u8) -> u32 {
    (u32::from(b) << 8) | u32::from(a)
}

/// États gif : 0 sig, 1 descriptor, 2 blocks/exts, 3 lzw data,
/// 999 error/done (quirk « BUG » amont — pas de reprise de blocs).
const ST_SIG: i32 = 0;
/// State 1.
const ST_LSD: i32 = 1;
/// State 2.
const ST_BLOCKS: i32 = 2;
/// State 3.
const ST_LZW: i32 = 3;
/// State 999.
const ST_DONE: i32 = 999;

/// `DilloGif` — état du décodeur GIF.
pub struct DilloGif {
    /// `Image` — retenu jusqu'au premier `set_parms` puis `NULL`
    /// amont (« may be freed by its owner ») → `Option` ici : les
    /// `set_parms` des frames suivantes no-op comme amont.
    image: Option<Rc<RefCell<DilloImage>>>,
    /// `url`/`version` — clés dicache.
    url: DilloUrl,
    /// `version`.
    version: i32,
    /// `state`.
    state: i32,
    /// `Start_Ofs` — offset déjà consommé dans `Client->Buf`.
    start_ofs: usize,
    /// `Flags` — `INTERLACE` copié depuis le descripteur d'image.
    flags: u32,
    /// `input_code_size`.
    input_code_size: u8,
    /// `linebuf` — ligne de pixels courante (`Width` octets).
    linebuf: Vec<u8>,
    /// `pass` — passe d'entrelacement.
    pass: u32,
    /// `y` — ligne courante émise.
    y: u32,
    /// `code_size` — taille courante des codes LZW (bits).
    code_size: i32,
    /// `Width`/`Height` — depuis l'image descriptor.
    width: u32,
    /// `Height`.
    height: u32,
    /// `ColorMap_ofs` — offset de la colormap dans `Buf`.
    colormap_ofs: usize,
    /// `ColorResolution`.
    color_resolution: u32,
    /// `NumColors`.
    num_colors: u32,
    /// `Background` — `Image->bg_color`.
    background: i32,
    /// `spill_line_index`.
    spill_line_index: usize,
    /// `transparent` — index GCE (-1 = absent).
    transparent: i32,
    /// `packet_size` — octets restants du data block LZW.
    packet_size: usize,
    /// `window` — fenêtre de bits (LSB first).
    window: u32,
    /// `bits_in_window`.
    bits_in_window: i32,
    /// `last_code`.
    last_code: usize,
    /// `line_index` — index pixel dans `linebuf`.
    line_index: usize,
    /// `spill_lines` — lignes de débordement de séquence.
    spill_lines: Vec<Vec<u8>>,
    /// `num_spill_lines_max`.
    num_spill_lines_max: usize,
    /// `length[code]` — longueur de la séquence du code.
    length: Vec<u32>,
    /// `code_and_byte[code]` = `(code << 8) | byte`.
    code_and_byte: Vec<u32>,
}

/// Cible d'écriture de `Gif_sequence` — `obuf` amont est soit
/// `linebuf` soit `spill_lines[i]`.
#[derive(Clone, Copy)]
enum Obuf {
    /// `gif->linebuf`.
    Line,
    /// `gif->spill_lines[i]`.
    Spill(usize),
}

impl DilloGif {
    /// `a_Gif_new` — état initial (`dNew0`-équivalent +
    /// `transparent = -1`, `Background = Image->bg_color`).
    #[must_use]
    pub fn new(image: Rc<RefCell<DilloImage>>, url: DilloUrl, version: i32) -> Self {
        let bg = image.borrow().bg_color;
        Self {
            image: Some(image),
            url,
            version,
            state: ST_SIG,
            start_ofs: 0,
            flags: 0,
            input_code_size: 0,
            linebuf: Vec::new(),
            pass: 0,
            y: 0,
            code_size: 0,
            width: 0,
            height: 0,
            colormap_ofs: 0,
            color_resolution: 0,
            num_colors: 0,
            background: bg,
            spill_line_index: 0,
            transparent: -1,
            packet_size: 0,
            window: 0,
            bits_in_window: 0,
            last_code: 0,
            line_index: 0,
            spill_lines: Vec::new(),
            num_spill_lines_max: 0,
            // `[(1 << MAX_LWZ_BITS) + 1]` amont.
            length: vec![0; (1 << MAX_LWZ_BITS) + 1],
            code_and_byte: vec![0; (1 << MAX_LWZ_BITS) + 1],
        }
    }

    /// Écrit `byte` dans `obuf[o_index]` (≙ `obuf[--o_index]` avant
    /// décrément côté appelant).
    fn put(&mut self, obuf: Obuf, o_index: usize, byte: u8) {
        match obuf {
            Obuf::Line => {
                if o_index < self.linebuf.len() {
                    self.linebuf[o_index] = byte;
                }
            }
            Obuf::Spill(i) => {
                if let Some(l) = self.spill_lines.get_mut(i) {
                    if o_index < l.len() {
                        l[o_index] = byte;
                    }
                }
            }
        }
    }

    /// `Gif_data_blocks` — taille d'une suite de sous-blocs
    /// (`0` si incomplète).
    fn data_blocks(buf: &[u8]) -> usize {
        let mut size = 0usize;
        let mut bsize = buf.len();
        let mut off = 0usize;
        if bsize < 1 {
            return 0;
        }
        while buf[off] != 0 {
            let blk = usize::from(buf[off]);
            if bsize <= blk + 1 {
                return 0;
            }
            size += blk + 1;
            bsize -= blk + 1;
            off += blk + 1;
        }
        size + 1
    }

    /// `Gif_do_generic_ext` — extension ignorée (`Buf` = après le label).
    fn do_generic_ext(buf: &[u8]) -> usize {
        if buf.is_empty() {
            return 0;
        }
        let size = usize::from(buf[0]) + 1;
        if size > buf.len() {
            return 0;
        }
        let dsize = Self::data_blocks(&buf[size..]);
        if dsize == 0 {
            return 0;
        }
        let total = size + dsize;
        if total <= buf.len() {
            total
        } else {
            0
        }
    }

    /// `Gif_do_gc_ext` — Graphic Control Extension ; seul
    /// `transparent` (flag 0x1) est utilisé amont.
    fn do_gc_ext(&mut self, buf: &[u8]) -> usize {
        let size = usize::from(buf[0]) + 2;
        if buf.len() < 6 || size > buf.len() {
            return 0;
        }
        let flags = buf[1];
        if flags & 0x1 != 0 {
            self.transparent = i32::from(buf[4]);
        }
        size
    }

    /// `Gif_do_extension` — dispatch par label
    /// (`GC_Ext`/`Cmt_Ext`/`Txt_Ext`/`App_Ext`/défaut → generic).
    fn do_extension(&mut self, label: u8, buf: &[u8]) -> usize {
        /// `GC_Ext`.
        const GC_EXT: u8 = 0xf9;
        /// `Cmt_Ext`.
        const CMT_EXT: u8 = 0xfe;
        match label {
            GC_EXT => self.do_gc_ext(buf),
            CMT_EXT => Self::data_blocks(buf),
            _ => Self::do_generic_ext(buf),
        }
    }

    /// `Gif_lwz_init` — init décodeur LZW pour un frame.
    fn lwz_init(&mut self) {
        self.num_spill_lines_max = 1;
        self.spill_lines = vec![vec![0u8; self.width as usize]];
        self.bits_in_window = 0;
        // `last_code = (1 << input_code_size) + 1` ; amont memset
        // `code_and_byte` jusqu'à `1 + last_code` — le reste du
        // tableau est déjà nul et les entrées usagées sont réécrites
        // par `literal`/`sequence` avant lecture → memset complet
        // ici (équivalent observable).
        self.last_code = (1usize << self.input_code_size) + 1;
        self.code_and_byte.fill(0);
        self.code_size = i32::from(self.input_code_size) + 1;
        self.line_index = 0;
    }

    /// `Gif_emit_line` — `a_Dicache_write` + avance `y` verbatim
    /// (entrelacé : passes 0,1→+8, 2→+4, 3→+2, `pass>3` **sans**
    /// incrément — quirk du `switch` amont sans `default`).
    fn emit_line(&mut self, linebuf: &[u8]) {
        dicache_write(&self.url, self.version, linebuf, self.y);
        if self.flags & INTERLACE != 0 {
            match self.pass {
                0 | 1 => self.y += 8,
                2 => self.y += 4,
                3 => self.y += 2,
                _ => {}
            }
            if self.y >= self.height {
                self.pass += 1;
                self.y = match self.pass {
                    1 => 4,
                    2 => 2,
                    3 => 1,
                    // « error in the input image » amont → 0.
                    _ => 0,
                };
            }
        } else if self.y < self.height {
            self.y += 1;
        }
    }

    /// `Gif_literal` — pixel direct + mise à jour de la table.
    #[allow(clippy::cast_possible_truncation)]
    fn literal(&mut self, code: u32) {
        // Amont écrit sans borne — `line_index` reste < `Width` en
        // pratique ; on borne (slice) pour la sûreté.
        self.put(Obuf::Line, self.line_index, code as u8);
        self.line_index += 1;
        if self.line_index >= self.width as usize {
            let lb = std::mem::take(&mut self.linebuf);
            self.emit_line(&lb);
            self.linebuf = lb;
            self.line_index = 0;
        }
        self.length[self.last_code + 1] = 2;
        self.code_and_byte[self.last_code + 1] = code << 8;
        self.code_and_byte[self.last_code] |= code;
    }

    /// `Gif_sequence` — expansion d'une séquence LZW : traversée à
    /// rebours, spill lines multi-lignes, quirk « `orig_code` ==
    /// `last_code` » (`KwKwK` — dernier octet = premier).
    #[allow(clippy::cast_possible_truncation)]
    fn sequence(&mut self, code_in: u32) {
        let mut code = code_in;
        let mut sequence_length = self.length[code as usize];
        let orig_code = code;
        let w = self.width as usize;
        let mut spill_line_index = self.spill_line_index;
        let num_spill_lines;

        self.length[self.last_code + 1] = sequence_length + 1;
        self.code_and_byte[self.last_code + 1] = code << 8;

        // Traversée arrière → spill lines si la séquence ne tient
        // pas dans la ligne courante.
        let mut obuf;
        let mut o_index: usize;
        let mut o_size: u32;
        if self.line_index + sequence_length as usize <= w {
            num_spill_lines = 0;
            obuf = Obuf::Line;
            o_index = self.line_index + sequence_length as usize;
            o_size = sequence_length.saturating_sub(1);
        } else {
            num_spill_lines = (self.line_index + sequence_length as usize - 1) / w;
            o_index = (self.line_index + sequence_length as usize - 1) % w + 1;
            if num_spill_lines > self.num_spill_lines_max {
                // Amont réalloue à `num_spill_lines << 1` en gardant
                // l'index — `resize_with` couvre le même effet.
                self.num_spill_lines_max = num_spill_lines << 1;
                self.spill_lines
                    .resize_with(self.num_spill_lines_max, || vec![0u8; w]);
            }
            spill_line_index = num_spill_lines - 1;
            obuf = Obuf::Spill(spill_line_index);
            o_size = o_index as u32;
        }
        self.line_index = o_index; /* for afterwards */

        // `last_byte_ptr = obuf + o_index - 1` — dernier octet de la
        // séquence dans la cible initiale (fixup KwKwK plus bas).
        let last_byte = (obuf, o_index.saturating_sub(1));

        while sequence_length > 1 {
            sequence_length = sequence_length.saturating_sub(o_size);
            // Écrit `o_size` octets dans `obuf[o_index - o_size .. o_index)`.
            while o_size > 0 && o_index > 0 {
                let cab = self.code_and_byte[code as usize];
                code = cab >> 8;
                o_index -= 1;
                self.put(obuf, o_index, (cab & 255) as u8);
                o_size -= 1;
            }
            // Prépare l'écriture dans la ligne suivante.
            if o_index == 0 {
                if spill_line_index > 0 {
                    spill_line_index -= 1;
                    obuf = Obuf::Spill(spill_line_index);
                    o_size = w as u32;
                } else {
                    obuf = Obuf::Line;
                    o_size = sequence_length.saturating_sub(1);
                }
                o_index = w;
            }
        }
        // Premier octet de la séquence — forcément littéral.
        let first_byte = code as u8;
        o_index = o_index.saturating_sub(1);
        self.put(obuf, o_index, first_byte);
        self.code_and_byte[self.last_code] |= u32::from(first_byte);

        // `if (orig_code == gif->last_code) *last_byte_ptr = code`.
        if orig_code as usize == self.last_code {
            self.put(last_byte.0, last_byte.1, first_byte);
        }

        // Lignes pleines émises.
        if self.line_index >= w {
            let lb = std::mem::take(&mut self.linebuf);
            self.emit_line(&lb);
            self.linebuf = lb;
            self.line_index = 0;
        }
        if num_spill_lines > 0 {
            if self.line_index > 0 {
                let lb = std::mem::take(&mut self.linebuf);
                self.emit_line(&lb);
                self.linebuf = lb;
            }
            let end = num_spill_lines - usize::from(self.line_index > 0);
            for si in 0..end {
                let sl = self.spill_lines[si].clone();
                self.emit_line(&sl);
            }
            // Swap dernière spill line ↔ linebuf (linebuf = temporaire).
            self.linebuf
                .swap_with_slice(&mut self.spill_lines[num_spill_lines - 1]);
        }
        self.spill_line_index = spill_line_index;
    }

    /// `Gif_process_code` → `1` `last_code`++, `0` reset table,
    /// `2` end, `<0` erreur.
    fn process_code(&mut self, code: u32, clear_code: u32) -> i32 {
        if code < clear_code {
            self.literal(code);
            1
        } else if code >= clear_code + 2 {
            if code as usize > self.last_code {
                return -1;
            }
            self.sequence(code);
            1
        } else if code == clear_code {
            0
        } else {
            2
        }
    }

    /// `Gif_decode` — boucle data blocks + extraction codes LZW
    /// (fenêtre MSB, LSB first). Retour = octets consommés.
    #[allow(clippy::cast_possible_truncation)]
    fn decode(&mut self, buf: &[u8]) -> usize {
        let mut pos = 0usize;
        let mut bufsize = buf.len();
        let mut packet_size = self.packet_size;
        let mut window = self.window;
        let mut bits_in_window = self.bits_in_window;
        let mut code_size = self.code_size;
        let mut code_mask = (1u32 << code_size) - 1;
        let clear_code = 1u32 << self.input_code_size;
        let mut error = false;

        while bufsize > 0 && !error {
            // `lwz_bytes` = octets LZW restants dans ce packet.
            let mut lwz_bytes = packet_size.min(bufsize);
            bufsize -= lwz_bytes;
            packet_size -= lwz_bytes;
            while lwz_bytes > 0 {
                // Push l'octet en haut de la fenêtre (LSB first).
                window = (window >> 8) | (u32::from(buf[pos]) << 24);
                pos += 1;
                lwz_bytes -= 1;
                bits_in_window += 8;

                while bits_in_window >= code_size {
                    let code = (window >> (32 - bits_in_window)) & code_mask;
                    bits_in_window -= code_size;
                    match self.process_code(code, clear_code) {
                        1 => {
                            self.last_code += 1;
                            if (self.last_code & code_mask as usize) == 0 {
                                if self.last_code == (1 << MAX_LWZ_BITS) {
                                    self.last_code -= 1;
                                } else {
                                    code_size += 1;
                                    code_mask = (1 << code_size) - 1;
                                }
                            }
                        }
                        0 => {
                            self.last_code = clear_code as usize + 1;
                            code_size = i32::from(self.input_code_size) + 1;
                            code_mask = (1 << code_size) - 1;
                        }
                        _ => {
                            // `2` end code → `goto error` amont
                            // (« Could clean up better? »), `<0` idem.
                            error = true;
                            break;
                        }
                    }
                }
            }

            if bufsize == 0 || error {
                break;
            }
            // Début d'un nouveau data block.
            bufsize -= 1;
            packet_size = usize::from(buf[pos]);
            pos += 1;
            if packet_size == 0 {
                // Block terminator — « BUG: should Go back to getting
                // GIF blocks » amont.
                self.state = ST_DONE;
                break;
            }
        }

        self.packet_size = packet_size;
        self.window = window;
        self.bits_in_window = bits_in_window;
        self.code_size = code_size;
        if error {
            self.state = ST_DONE;
        }
        buf.len() - bufsize
    }

    /// `Gif_check_sig` — `GIF87a`/`GIF89a`, sinon warn+`state=999`.
    fn check_sig(&mut self, buf: &[u8]) -> usize {
        if buf.len() < 6 {
            return 0;
        }
        if buf[..6] != *b"GIF87a" && buf[..6] != *b"GIF89a" {
            self.state = ST_DONE;
            return 6;
        }
        self.state = ST_LSD;
        6
    }

    /// `Gif_do_color_table` — `ColorMap_ofs` (offset dans `Buf`
    /// complet), `NumColors`, taille `3 * (1 << (1 + CT_Size))`.
    /// `ct` = position de la table dans `buf_base`.
    fn do_color_table(&mut self, buf_base: &[u8], ct: &[u8], ct_size: u8) -> usize {
        let size = 3 * (1usize << (1 + ct_size));
        if size > ct.len() {
            return 0;
        }
        self.colormap_ofs = ct.as_ptr() as usize - buf_base.as_ptr() as usize;
        self.num_colors = 1 << (1 + ct_size);
        size
    }

    /// `Gif_get_descriptor` — Logical Screen Descriptor + Global CT.
    fn get_descriptor(&mut self, buf_base: &[u8], buf: &[u8]) -> usize {
        let mut size = 7usize;
        if buf.len() < 7 {
            return 0;
        }
        let flags = buf[4];
        if flags & LOCALCOLORMAP != 0 {
            let mysize = self.do_color_table(buf_base, &buf[7..], flags & 0x7);
            if mysize == 0 {
                return 0;
            }
            size += mysize;
        }
        self.color_resolution = u32::from((buf[4] & 0x70) >> 3) + 1;
        size
    }

    /// `Gif_do_img_desc` — image descriptor : dims, clamp
    /// `IMAGE_MAX_AREA`, `set_parms` INDEXED gamma `1/2.2` +
    /// `Image = NULL`, local CT, `input_code_size`, `lwz_init`,
    /// `set_cmap` si colormap posée.
    fn do_img_desc(&mut self, buf_base: &[u8], buf: &[u8]) -> usize {
        let mut size = 9 + 1usize;
        if buf.len() < 10 {
            return 0;
        }
        self.width = lm_to_uint(buf[4], buf[5]);
        self.height = lm_to_uint(buf[6], buf[7]);

        if self.width == 0
            || self.height == 0
            || u64::from(self.width) > IMAGE_MAX_AREA / u64::from(self.height)
        {
            self.state = ST_DONE;
            return 0;
        }

        if let Some(img) = self.image.take() {
            // `a_Dicache_set_parms(..., DILLO_IMG_TYPE_INDEXED, 1/2.2)`
            // puis `gif->Image = NULL` — frames suivantes : no-op.
            dicache_set_parms(
                &self.url,
                self.version,
                &img,
                self.width,
                self.height,
                ImgType::Indexed,
                1.0 / 2.2,
            );
        }

        let flags = buf[8];
        self.flags |= u32::from(flags) & INTERLACE;
        self.pass = 0;
        let mut off = 9usize;
        let mut rest = buf.len() - 9;
        if flags & LOCALCOLORMAP != 0 {
            let lsize = self.do_color_table(buf_base, &buf[off..], flags & 0x7);
            if lsize == 0 {
                return 0;
            }
            size += lsize;
            off += lsize;
            rest -= lsize;
        }
        if rest < 1 {
            return 0;
        }
        self.input_code_size = buf[off];
        if self.input_code_size > 8 {
            self.state = ST_DONE;
            return size;
        }
        self.y = 0;
        self.lwz_init();
        self.spill_line_index = 0;
        self.linebuf = vec![0u8; self.width as usize];
        self.state = ST_LZW;
        if self.colormap_ofs != 0 {
            let ofs = self.colormap_ofs;
            if ofs < buf_base.len() {
                let cmap = buf_base[ofs..].to_vec();
                dicache_set_cmap(
                    &self.url,
                    self.version,
                    self.background,
                    &cmap,
                    self.num_colors,
                    256,
                    self.transparent,
                );
            }
        }
        size
    }

    /// `GIF_Block` — boucle des blocs GIF (extensions/image/trailer)
    /// tant que `state == 2`.
    fn gif_block(&mut self, buf_base: &[u8], buf: &[u8]) -> usize {
        /// `Img_Desc`/`Trailer`/`Ext_Id`.
        const IMG_DESC: u8 = 0x2c;
        const TRAILER: u8 = 0x3b;
        const EXT_ID: u8 = 0x21;

        let mut size = 0usize;
        let mut pos = 0usize;
        let mut bsize = buf.len();

        if bsize < 1 {
            return 0;
        }
        while self.state == ST_BLOCKS {
            if bsize < 1 {
                return size;
            }
            bsize -= 1;
            let c = buf[pos];
            pos += 1;
            match c {
                EXT_ID => {
                    if bsize < 2 {
                        return size;
                    }
                    let label = buf[pos];
                    pos += 1;
                    bsize -= 1;
                    let mysize = self.do_extension(label, &buf[pos..]);
                    if mysize == 0 {
                        return size;
                    }
                    pos += mysize;
                    bsize -= mysize;
                    size += mysize + 2;
                }
                IMG_DESC => {
                    let mysize = self.do_img_desc(buf_base, &buf[pos..]);
                    if mysize == 0 {
                        return size;
                    }
                    size += 1 + mysize;
                    return size;
                }
                TRAILER => {
                    // « BUG: should close the rest of the file » amont.
                    self.state = ST_DONE;
                    return size + 1;
                }
                _ => {
                    self.state = ST_DONE;
                    return size + 1;
                }
            }
        }
        size
    }

    /// `Gif_process_bytes` — machine d'états push avec fallthrough
    /// (sig→lsd→blocks→lzw ; `default` = erreur → consomme tout).
    fn process_bytes(&mut self, ibuf: &[u8], buf_base: &[u8]) -> usize {
        let mut tmp = ibuf;
        let mut consumed = 0usize;

        if self.state == ST_SIG {
            let mysize = self.check_sig(tmp);
            if mysize == 0 {
                return 0;
            }
            consumed += mysize;
            tmp = &tmp[mysize..];
            if self.state != ST_LSD {
                return consumed;
            }
            // fallthrough
        }
        if self.state == ST_LSD {
            let mysize = self.get_descriptor(buf_base, tmp);
            if mysize == 0 {
                return consumed;
            }
            consumed += mysize;
            tmp = &tmp[mysize..];
            self.state = ST_BLOCKS;
            // fallthrough
        }
        if self.state == ST_BLOCKS {
            let mysize = self.gif_block(buf_base, tmp);
            if mysize == 0 {
                return consumed;
            }
            consumed += mysize;
            tmp = &tmp[mysize..];
            if self.state != ST_LZW {
                return consumed;
            }
            // fallthrough
        }
        if self.state == ST_LZW {
            let mysize = self.decode(tmp);
            if mysize == 0 {
                return consumed;
            }
            consumed += mysize;
            tmp = &tmp[mysize..];
            // fallthrough → default
        }
        // `default:` amont — erreur : consommer tout l'input.
        consumed + tmp.len()
    }
}

/// `Gif_write` amont — traite `Buf + Start_Ofs` (`Buf` = snapshot
/// complet du client ; `process_bytes` reçoit `buf_base` pour les
/// offsets colormap).
fn gif_write(gif: &mut DilloGif, buf: &[u8]) {
    if buf.is_empty() {
        return;
    }
    let slice = &buf[gif.start_ofs..];
    let consumed = gif.process_bytes(slice, buf);
    if consumed >= 1 {
        gif.start_ofs += consumed;
    }
}

/// `a_Gif_callback` ≙ [`ImageDecoder`] : `Send`→`Gif_write`,
/// `Close`→`Gif_close` (`a_Dicache_close` + `Gif_free`≙drop),
/// `Abort`→`Gif_free` ≙ [`ImageDecoder::abort`].
impl ImageDecoder for DilloGif {
    fn callback(&mut self, op: i32, client: &mut CacheClient) {
        if op == CA_SEND {
            gif_write(self, &client.buf);
        } else if op == CA_CLOSE {
            dicache_close(&self.url, self.version, client);
        }
    }

    fn abort(&mut self) {
        // `Gif_free` — le drop Rust libère buffers/spill lines.
        self.spill_lines.clear();
        self.linebuf.clear();
    }
}

/// `a_Gif_new` ≙ factory [`crate::dicache::DecoderFactory`].
#[must_use]
pub fn gif_new(
    image: Rc<RefCell<DilloImage>>,
    url: DilloUrl,
    version: i32,
) -> Box<dyn ImageDecoder> {
    Box::new(DilloGif::new(image, url, version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dicache::{
        dicache_callback, dicache_get_entry, dicache_image, dicache_set_factory,
        dicache_set_image_factory, DicEntryState, DIC_GIF, DIC_LAST,
    };
    use crate::image::{ImgRenderer, NullRenderer};
    use dillo_net::cache::{self, CacheClient};
    use dillo_net::http::HttpWeb;

    /// GIF 1×1 `GIF89a` classique (2 couleurs global CT, 1 pixel noir).
    const GIF_1X1: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, // GIF89a
        0x01, 0x00, 0x01, 0x00, // w=1 h=1
        0x80, 0x00, 0x00, // GCT flag, bg, aspect
        0x00, 0x00, 0x00, 0xff, 0xff, 0xff, // CT: noir, blanc
        0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, // GCE
        0x2c, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, // img desc
        0x02, 0x02, 0x44, 0x01, 0x00, // LZW code_size=2, blk 2: 44 01, term
        0x3b, // trailer
    ];

    struct FakeFactory(Rc<RefCell<NullRenderer>>);
    impl crate::dicache::ImageFactory for FakeFactory {
        fn new_image(&mut self, _w: &HttpWeb) -> (Rc<RefCell<dyn ImgRenderer>>, f32) {
            (self.0.clone() as _, 96.0)
        }
    }

    fn client(key: i32, web: &Rc<HttpWeb>, buf: &[u8]) -> CacheClient {
        CacheClient {
            key,
            url: web.url.clone(),
            version: 0,
            buf: buf.to_vec(),
            callback: None,
            web: web.clone(),
        }
    }

    fn setup(url: &str) -> (Rc<HttpWeb>, Rc<RefCell<NullRenderer>>) {
        let nr = Rc::new(RefCell::new(NullRenderer::default()));
        dicache_set_image_factory(Box::new(FakeFactory(nr.clone())));
        dicache_set_factory(DIC_GIF, gif_new);
        let web = Rc::new(HttpWeb::new(
            dillo_net::url::url_new(url.as_bytes(), None).unwrap(),
            None,
            0,
        ));
        let _ = dicache_image(DIC_GIF, "image/gif", &web).unwrap();
        (web, nr)
    }

    #[test]
    fn decode_1x1_end_to_end() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _r) = setup("http://t/px.gif");
        let mut cl = client(1, &web, GIF_1X1);
        dicache_callback(CA_SEND, &mut cl);
        dicache_callback(CA_CLOSE, &mut cl);
        let url = web.url.clone();
        let e = dicache_get_entry(&url, 1);
        // Après close : entrée unref'd + close_client — l'entrée peut
        // avoir disparu (ref_count 0 + pas d'imgbuf → supprimée).
        if let Some(e) = e {
            let eb = e.borrow();
            assert_eq!(eb.state, crate::dicache::DicEntryState::Close);
        }
    }

    #[test]
    fn incremental_feed_same_result() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, rndr) = setup("http://t/px2.gif");
        let url = web.url.clone();
        // Feed en deux morceaux : header partiel puis tout.
        let mut cl = client(2, &web, &GIF_1X1[..20]);
        dicache_callback(CA_SEND, &mut cl);
        cl.buf = GIF_1X1.to_vec();
        dicache_callback(CA_SEND, &mut cl);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        {
            let eb = e.borrow();
            assert_eq!(eb.state, DicEntryState::Write);
            // Pixel décodé → imgbuf a 1 ligne noire (cmap[0]).
            let imgbuf = eb.imgbuf.as_ref().unwrap();
            assert_eq!(imgbuf.width, 1);
            assert_eq!(imgbuf.height, 1);
            assert_eq!(imgbuf.row_copy(0), vec![0, 0, 0]);
        }
        // Le renderer a reçu set_buffer + draw_row(0).
        let nr = rndr.borrow();
        assert!(nr.buf.is_some());
        assert_eq!(nr.drawn, vec![0]);
        drop(nr);
    }

    #[test]
    fn bad_signature_finishes() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _r) = setup("http://t/bad.gif");
        let url = web.url.clone();
        let mut cl = client(3, &web, b"NOTAGIF!!garbage-data");
        dicache_callback(CA_SEND, &mut cl);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        // state 999 → entrée reste Empty (pas de set_parms).
        assert_eq!(e.borrow().state, DicEntryState::Empty);
    }

    #[test]
    fn suspicious_dimensions_rejected() {
        crate::dicache::dicache_freeall();
        cache::cache_init();
        let (web, _r) = setup("http://t/big.gif");
        let url = web.url.clone();
        // Image descriptor w=0xffff h=0xffff (> MAX_AREA) —
        // `2c` à l'offset 28, w/h LE aux offsets 33..37.
        let mut data = GIF_1X1[..38].to_vec();
        data[33] = 0xff;
        data[34] = 0xff;
        data[35] = 0xff;
        data[36] = 0xff;
        let mut cl = client(4, &web, &data);
        dicache_callback(CA_SEND, &mut cl);
        let e = dicache_get_entry(&url, DIC_LAST).unwrap();
        assert_eq!(e.borrow().state, DicEntryState::Empty);
    }
}
