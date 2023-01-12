use std::io::Read;

use base64::Engine;
use base64::engine::general_purpose;
use flate2::read::GzDecoder;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        panic!("没有参数可解压");
    }

    let need_uncompress_str = &args[1];

    let bytes = general_purpose::STANDARD
        .decode(need_uncompress_str)
        .unwrap();
    println!("\r\n解码成功");

    let mut uncompress_decoder = GzDecoder::new(bytes.as_slice());
    let mut uncompress_str = String::new();
    uncompress_decoder.read_to_string(&mut uncompress_str).unwrap();

    println!("\r\n解压缩成功\r\n\r\n{}", uncompress_str);
}
