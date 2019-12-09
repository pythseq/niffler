/*
Copyright (c) 2018 Pierre Marijon <pierre.marijon@inria.fr>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

Originally from https://github.com/natir/yacrd/blob/3fc6ef8b5b51256f0c4bc45b8056167acf34fa58/src/file.rs
Changes:
  - make bzip2 and lzma support optional
*/

/* crates use */
use cfg_if::cfg_if;
use enum_primitive::{
    enum_from_primitive, enum_from_primitive_impl, enum_from_primitive_impl_ty, FromPrimitive,
};
use failure::{Error, Fail};
use flate2;

/* standard use */
use std::io;
use std::io::Read;

enum_from_primitive! {
    #[repr(u64)]
    #[derive(Debug, PartialEq)]
    pub enum CompressionFormat {
        Gzip = 0x1F8B,
        Bzip = 0x425A,
        Lzma = 0x00FD_377A_585A,
        No,
    }
}

#[derive(Debug, Fail)]
pub enum OCFError {
    #[fail(display = "Feature disabled, enabled it during compilation")]
    FeatureDisabled,
}

pub fn get_reader(
    in_stream: Box<dyn io::Read>,
) -> Result<(Box<dyn io::Read>, CompressionFormat), Error> {
    // check compression
    let (compression, in_stream) = read_compression(in_stream);

    // return readable and compression status
    match compression {
        CompressionFormat::Gzip => Ok((
            Box::new(flate2::read::GzDecoder::new(in_stream)),
            CompressionFormat::Gzip,
        )),
        CompressionFormat::Bzip => new_bz2_decoder(in_stream),
        CompressionFormat::Lzma => new_lzma_decoder(in_stream),
        CompressionFormat::No => Ok((in_stream, CompressionFormat::No)),
    }
}

fn read_compression<'a>(
    mut in_stream: Box<dyn io::Read>,
) -> (CompressionFormat, Box<dyn io::Read>) {
    let mut buf = [0u8; 5];

    in_stream
        .read_exact(&mut buf)
        .expect("Error durring reading first bit of file");

    let mut five_bit_val: u64 = 0;
    for (i, item) in buf.iter().enumerate().take(5) {
        five_bit_val |= (u64::from(*item)) << (8 * (4 - i));
    }

    if CompressionFormat::from_u64(five_bit_val) == Some(CompressionFormat::Lzma) {
        let cursor = io::Cursor::new(buf);
        return (CompressionFormat::Lzma, Box::new(cursor.chain(in_stream)));
    }

    let mut two_bit_val: u64 = 0;
    for (i, item) in buf.iter().enumerate().take(2) {
        two_bit_val |= (u64::from(*item)) << (8 * (1 - i));
    }

    let cursor = io::Cursor::new(buf);
    match CompressionFormat::from_u64(two_bit_val) {
        e @ Some(CompressionFormat::Gzip) | e @ Some(CompressionFormat::Bzip) => {
            (e.unwrap(), Box::new(cursor.chain(in_stream)))
        }
        _ => (CompressionFormat::No, Box::new(cursor.chain(in_stream))),
    }
}

cfg_if! {
    if #[cfg(feature = "bz2")] {
        use bzip2;

        fn new_bz2_encoder(out: Box<dyn io::Write>) -> Result<Box<dyn io::Write>, Error> {
            Ok(Box::new(bzip2::write::BzEncoder::new(
                out,
                bzip2::Compression::Best,
            )))
        }

        fn new_bz2_decoder(
            inp: Box<dyn io::Read>,
        ) -> Result<(Box<dyn io::Read>, CompressionFormat), Error> {
            use bzip2;
            Ok((
                Box::new(bzip2::read::BzDecoder::new(inp)),
                CompressionFormat::Bzip,
            ))
        }
    } else {
        fn new_bz2_encoder(_: Box<dyn io::Write>) -> Result<Box<dyn io::Write>, Error> {
            Err(OCFError::FeatureDisabled.into())
        }

        fn new_bz2_decoder(_: Box<dyn io::Read>) -> Result<(Box<dyn io::Read>, CompressionFormat), Error> {
            Err(OCFError::FeatureDisabled.into())
        }
    }
}

cfg_if! {
    if #[cfg(feature = "lzma")] {
      use xz2;

      fn new_lzma_encoder(out: Box<dyn io::Write>) -> Result<Box<dyn io::Write>, Error> {
          Ok(Box::new(xz2::write::XzEncoder::new(out, 9)))
      }

      fn new_lzma_decoder(
          inp: Box<dyn io::Read>,
      ) -> Result<(Box<dyn io::Read>, CompressionFormat), Error> {
          use xz2;
          Ok((
              Box::new(xz2::read::XzDecoder::new(inp)),
              CompressionFormat::Lzma,
          ))
      }
    } else {
      fn new_lzma_encoder(_: Box<dyn io::Write>) -> Result<Box<dyn io::Write>, Error> {
          Err(OCFError::FeatureDisabled.into())
      }

      fn new_lzma_decoder(_: Box<dyn io::Read>) -> Result<(Box<dyn io::Read>, CompressionFormat), Error> {
          Err(OCFError::FeatureDisabled.into())
      }
    }
}

pub fn get_writer(
    out_stream: Box<dyn io::Write>,
    format: CompressionFormat,
) -> Result<Box<dyn io::Write>, Error> {
    match format {
        CompressionFormat::Gzip => Ok(Box::new(flate2::write::GzEncoder::new(
            out_stream,
            flate2::Compression::best(),
        ))),
        CompressionFormat::Bzip => new_bz2_encoder(out_stream),
        CompressionFormat::Lzma => new_lzma_encoder(out_stream),
        CompressionFormat::No => Ok(Box::new(out_stream)),
    }
}

#[cfg(test)]
mod test {

    use super::*;

    const GZIP_FILE: &'static [u8] = &[0o037, 0o213, 0o0, 0o0, 0o0];
    const BZIP_FILE: &'static [u8] = &[0o102, 0o132, 0o0, 0o0, 0o0];
    const LZMA_FILE: &'static [u8] = &[0o375, 0o067, 0o172, 0o130, 0o132];
    const LOREM_IPSUM: &'static [u8] = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit. Ut ultricies scelerisque diam, a scelerisque enim sagittis at.";

    mod compression_format_detection {
        use super::*;

        #[test]
        fn gzip() {
            let (compression, _) = read_compression(Box::new(GZIP_FILE));
            assert_eq!(compression, CompressionFormat::Gzip);
        }

        #[test]
        fn bzip() {
            let (compression, _) = read_compression(Box::new(BZIP_FILE));
            assert_eq!(compression, CompressionFormat::Bzip);
        }

        #[test]
        fn lzma() {
            let (compression, _) = read_compression(Box::new(LZMA_FILE));
            assert_eq!(compression, CompressionFormat::Lzma);
        }
    }

    mod compress_uncompress {
        use super::*;

        #[test]
        fn gzip() {
            {
                let wfile =
                    std::fs::File::create("tmp.gz").expect("We can't create tmp.file for test");

                let mut writer = get_writer(Box::new(wfile), CompressionFormat::Gzip).unwrap();
                writer
                    .write_all(LOREM_IPSUM)
                    .expect("Error durring write of data");
            }

            let rfile = std::fs::File::open("tmp.gz").expect("We can't read tmp.file for test");
            let (mut reader, compression) =
                get_reader(Box::new(rfile)).expect("Error in reading of tmp.file");

            assert_eq!(compression, CompressionFormat::Gzip);

            let mut buffer = Vec::new();
            reader
                .read_to_end(&mut buffer)
                .expect("Error durring reading");
            assert_eq!(LOREM_IPSUM, buffer.as_slice());
        }

        #[cfg(feature = "bz2")]
        #[test]
        fn bzip() {
            {
                let wfile =
                    std::fs::File::create("tmp.bz2").expect("We can't create tmp.file for test");

                let mut writer = get_writer(Box::new(wfile), CompressionFormat::Bzip).unwrap();
                writer
                    .write_all(LOREM_IPSUM)
                    .expect("Error durring write of data");
            }

            let rfile = std::fs::File::open("tmp.bz2").expect("We can't read tmp.file for test");
            let (mut reader, compression) =
                get_reader(Box::new(rfile)).expect("Error in reading of tmp.file");

            assert_eq!(compression, CompressionFormat::Bzip);

            let mut buffer = Vec::new();
            reader
                .read_to_end(&mut buffer)
                .expect("Error durring reading");
            assert_eq!(LOREM_IPSUM, buffer.as_slice());
        }

        #[cfg(feature = "lzma")]
        #[test]
        fn lzma() {
            {
                let wfile =
                    std::fs::File::create("tmp.xz").expect("We can't create tmp.file for test");

                let mut writer = get_writer(Box::new(wfile), CompressionFormat::Lzma).unwrap();
                writer
                    .write_all(LOREM_IPSUM)
                    .expect("Error durring write of data");
            }

            let rfile = std::fs::File::open("tmp.xz").expect("We can't read tmp.file for test");
            let (mut reader, compression) =
                get_reader(Box::new(rfile)).expect("Error in reading of tmp.file");

            assert_eq!(compression, CompressionFormat::Lzma);

            let mut buffer = Vec::new();
            reader
                .read_to_end(&mut buffer)
                .expect("Error durring reading");
            assert_eq!(LOREM_IPSUM, buffer.as_slice());
        }
    }
}
