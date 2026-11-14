//! Writes stored (uncompressed) deflate blocks: large files, but nothing beyond the
//! standard library.

/// The most bytes a stored deflate block holds.
const BLOCK: usize = 65_535;

/// `pixels` is RGBA8, rows top to bottom.
pub(crate) fn encode(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let row = width as usize * 4;
    assert_eq!(pixels.len(), row * height as usize, "RGBA8 rows");
    // Each scanline starts with its filter type (0: none). The scanlines go straight
    // into the file, cut into stored blocks: an image is tens of megabytes, too much to
    // copy once more for the scanlines and again for the zlib stream.
    let lines = pixels.chunks_exact(row.max(1)).take(height as usize);
    let raw_len = lines.len() * (row + 1);
    let blocks = raw_len.div_ceil(BLOCK).max(1);
    // The zlib header, the blocks with five bytes of header each, the checksum.
    let idat_len = 2 + raw_len + blocks * 5 + 4;

    let mut png = Vec::with_capacity(8 + (12 + 13) + (12 + idat_len) + 12);
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    // 8 bits per channel, RGBA, deflate, adaptive filtering, no interlace.
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);

    png.extend_from_slice(&(idat_len as u32).to_be_bytes());
    let start = png.len();
    png.extend_from_slice(b"IDAT");
    // Deflate with a 32 KiB window, no preset dictionary, fastest level.
    png.extend_from_slice(&[0x78, 0x01]);
    let mut stored = Stored {
        out: &mut png,
        left: raw_len,
        room: 0,
        checksum: Adler32::default(),
    };
    for line in lines {
        stored.write(&[0]);
        stored.write(line);
    }
    let checksum = stored.checksum.value();
    if raw_len == 0 {
        // A stream holds at least one block.
        png.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    png.extend_from_slice(&checksum.to_be_bytes());
    debug_assert_eq!(png.len() - start, 4 + idat_len);
    let crc = crc32(&png[start..]);
    png.extend_from_slice(&crc.to_be_bytes());

    chunk(&mut png, b"IEND", &[]);
    png
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Appends `left` bytes to `out` as uncompressed deflate blocks, as they are written.
struct Stored<'a> {
    out: &'a mut Vec<u8>,
    /// Bytes still to come that no block was started for.
    left: usize,
    /// Bytes the block being written still takes.
    room: usize,
    checksum: Adler32,
}

impl Stored<'_> {
    fn write(&mut self, mut data: &[u8]) {
        self.checksum.update(data);
        while !data.is_empty() {
            if self.room == 0 {
                let len = self.left.min(BLOCK);
                self.left -= len;
                self.room = len;
                let len = len as u16;
                self.out.push(u8::from(self.left == 0));
                self.out.extend_from_slice(&len.to_le_bytes());
                self.out.extend_from_slice(&(!len).to_le_bytes());
            }
            let (now, later) = data.split_at(data.len().min(self.room));
            self.out.extend_from_slice(now);
            self.room -= now.len();
            data = later;
        }
    }
}

/// `CRC_TABLES[0]` is the CRC of each byte value (polynomial 0xEDB88320, reflected);
/// `CRC_TABLES[k]` that of the byte followed by `k` zero bytes, so that eight bytes
/// are taken in one step ("slicing by 8").
const CRC_TABLES: [[u32; 256]; 8] = {
    let mut tables = [[0u32; 256]; 8];
    let mut byte = 0;
    while byte < 256 {
        let mut crc = byte as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        tables[0][byte] = crc;
        byte += 1;
    }
    let mut k = 1;
    while k < 8 {
        let mut byte = 0;
        while byte < 256 {
            let previous = tables[k - 1][byte];
            tables[k][byte] = (previous >> 8) ^ tables[0][(previous & 0xff) as usize];
            byte += 1;
        }
        k += 1;
    }
    tables
};

fn crc32(data: &[u8]) -> u32 {
    let t = &CRC_TABLES;
    let mut crc = !0u32;
    let (words, remainder) = data.as_chunks::<8>();
    for word in words {
        let low = crc ^ u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
        let high = u32::from_le_bytes([word[4], word[5], word[6], word[7]]);
        crc = t[7][(low & 0xff) as usize]
            ^ t[6][((low >> 8) & 0xff) as usize]
            ^ t[5][((low >> 16) & 0xff) as usize]
            ^ t[4][(low >> 24) as usize]
            ^ t[3][(high & 0xff) as usize]
            ^ t[2][((high >> 8) & 0xff) as usize]
            ^ t[1][((high >> 16) & 0xff) as usize]
            ^ t[0][(high >> 24) as usize];
    }
    for &byte in remainder {
        crc = t[0][((crc ^ u32::from(byte)) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
}

struct Adler32 {
    a: u32,
    b: u32,
}

impl Default for Adler32 {
    fn default() -> Self {
        Self { a: 1, b: 0 }
    }
}

impl Adler32 {
    fn update(&mut self, data: &[u8]) {
        const MOD: u32 = 65_521;
        // 5552 bytes keep the sums below u32::MAX before the modulo.
        for chunk in data.chunks(5552) {
            for &byte in chunk {
                self.a += u32::from(byte);
                self.b += self.a;
            }
            self.a %= MOD;
            self.b %= MOD;
        }
    }

    fn value(&self) -> u32 {
        (self.b << 16) | self.a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adler32(data: &[u8]) -> u32 {
        let mut sum = Adler32::default();
        sum.update(data);
        sum.value()
    }

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    fn chunks(png: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let mut out = Vec::new();
        let mut at = 8;
        while at < png.len() {
            let len = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            let body = &png[at + 4..at + 8 + len];
            let crc = u32::from_be_bytes(png[at + 8 + len..at + 12 + len].try_into().unwrap());
            assert_eq!(crc32(body), crc);
            out.push((body[..4].try_into().unwrap(), body[4..].to_vec()));
            at += 12 + len;
        }
        out
    }

    fn inflate_stored(z: &[u8]) -> Vec<u8> {
        assert_eq!(&z[..2], &[0x78, 0x01]);
        assert_eq!(
            u16::from_be_bytes([z[0], z[1]]) % 31,
            0,
            "zlib header check"
        );
        let mut out = Vec::new();
        let mut at = 2;
        loop {
            let last = z[at] & 1 == 1;
            let len = u16::from_le_bytes([z[at + 1], z[at + 2]]);
            assert_eq!(!len, u16::from_le_bytes([z[at + 3], z[at + 4]]));
            out.extend_from_slice(&z[at + 5..at + 5 + len as usize]);
            at += 5 + len as usize;
            if last {
                break;
            }
        }
        assert_eq!(&z[at..], &adler32(&out).to_be_bytes());
        out
    }

    /// The CRC one bit at a time, as the specification gives it.
    fn crc32_bitwise(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// The file built the plain way, each stage in a buffer of its own.
    fn encode_in_steps(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let row = width as usize * 4;
        let mut raw = Vec::new();
        for line in pixels.chunks_exact(row.max(1)).take(height as usize) {
            raw.push(0);
            raw.extend_from_slice(line);
        }
        let mut z = vec![0x78, 0x01];
        let mut blocks = raw.chunks(65_535).peekable();
        if blocks.peek().is_none() {
            z.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
        }
        while let Some(block) = blocks.next() {
            let len = block.len() as u16;
            z.push(u8::from(blocks.peek().is_none()));
            z.extend_from_slice(&len.to_le_bytes());
            z.extend_from_slice(&(!len).to_le_bytes());
            z.extend_from_slice(block);
        }
        let (mut a, mut b) = (1u32, 0u32);
        for &byte in &raw {
            a = (a + u32::from(byte)) % 65_521;
            b = (b + a) % 65_521;
        }
        z.extend_from_slice(&((b << 16) | a).to_be_bytes());

        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut header = Vec::new();
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        for (kind, data) in [(b"IHDR", &header[..]), (b"IDAT", &z[..]), (b"IEND", &[])] {
            png.extend_from_slice(&(data.len() as u32).to_be_bytes());
            let body = [&kind[..], data].concat();
            png.extend_from_slice(&body);
            png.extend_from_slice(&crc32_bitwise(&body).to_be_bytes());
        }
        png
    }

    #[test]
    fn files_are_those_built_in_steps_at_every_block_boundary() {
        // Scanlines of 1 + 4 * width bytes: none, a few, and around one, two and three
        // full blocks of 65,535 bytes (with a scanline ending on and across the edge).
        let sizes = [
            (0, 0),
            (0, 3),
            (3, 0),
            (1, 1),
            (7, 5),
            (16_383, 1),
            (16_384, 1),
            (1, 13_107),
            (1, 13_108),
            (1, 26_214),
            (4_095, 4),
            (4_096, 4),
            (3, 5_041),
            (3, 5_042),
            (257, 191),
            (128, 383),
        ];
        for (width, height) in sizes {
            let len = width as usize * height as usize * 4;
            let pixels: Vec<u8> = (0..len).map(|i| (i * 7 % 253) as u8).collect();
            let png = encode(width, height, &pixels);
            assert!(
                png == encode_in_steps(width, height, &pixels),
                "{width}x{height}"
            );
            assert_eq!(png.len(), png.capacity(), "{width}x{height}: sized once");
        }
        let data: Vec<u8> = (0..70_000u32).map(|i| (i % 249) as u8).collect();
        assert_eq!(crc32(&data), crc32_bitwise(&data));
        // Every byte value, at every length around the eight bytes taken in one step
        // and from every start.
        let data: Vec<u8> = (0..1_024u32).map(|i| (i * 167 % 256) as u8).collect();
        for start in 0..8 {
            for len in 0..40 {
                let bytes = &data[start..start + len];
                assert_eq!(crc32(bytes), crc32_bitwise(bytes), "{start}, {len}");
            }
        }
        assert_eq!(crc32(&data), crc32_bitwise(&data));
    }

    #[test]
    fn encodes_rgba_rows_with_a_filter_byte_each() {
        // 2x2: red, green / blue, transparent.
        let pixels = [
            255, 0, 0, 255, 0, 255, 0, 255, //
            0, 0, 255, 255, 0, 0, 0, 0,
        ];
        let png = encode(2, 2, &pixels);
        let chunks = chunks(&png);
        let kinds: Vec<&[u8; 4]> = chunks.iter().map(|(k, _)| k).collect();
        assert_eq!(kinds, [b"IHDR", b"IDAT", b"IEND"]);
        assert_eq!(chunks[0].1, [0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0, 0, 0]);
        let raw = inflate_stored(&chunks[1].1);
        assert_eq!(raw[0], 0);
        assert_eq!(&raw[1..9], &pixels[..8]);
        assert_eq!(raw[9], 0);
        assert_eq!(&raw[10..], &pixels[8..]);
    }

    #[test]
    fn large_images_split_into_several_blocks() {
        // 200x100 RGBA: 80,100 bytes of scanlines, two stored blocks.
        let pixels: Vec<u8> = (0..200 * 100 * 4).map(|i| (i % 251) as u8).collect();
        let chunks = chunks(&encode(200, 100, &pixels));
        let raw = inflate_stored(&chunks[1].1);
        assert_eq!(raw.len(), 100 * (1 + 200 * 4));
        let rows: Vec<u8> = raw
            .chunks(1 + 200 * 4)
            .flat_map(|line| line[1..].to_vec())
            .collect();
        assert_eq!(rows, pixels);
    }
}
