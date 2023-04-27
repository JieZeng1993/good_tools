use std::fs::File;
use std::io::{Read, Write};

use base64::Engine;
use base64::engine::general_purpose;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        println!("第一个参数 文件路径， 第二个参数 指定输出路径");
        return;
    }

    if args[1].eq("-d") {
        base64_to_file();
    } else if args[1].eq("-e") {
        file_to_base64();
    } else {
        println!("第一个参数只能是 -e(文件转base64) 或 -d(base64转文件)")
    }
}

fn file_to_base64() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        println!("第二个参数 文件路径， 第三个参数 指定输出路径");
        return;
    }

    let mut source_file = File::open(&args[2]).unwrap();
    let mut buf: Vec<u8> = vec![];
    source_file.read_to_end(&mut buf).unwrap();
    println!("读取文件完成");

    let encode_str = general_purpose::STANDARD.encode(buf);

    println!("编码完成");

    let mut dest_file = File::create(&args[3]).unwrap();

    dest_file.write(encode_str.as_ref()).unwrap();

    println!("\r\n转换成功\r\n");
}


fn base64_to_file() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        println!("第二个参数 文件路径， 第三个参数 指定输出路径");
        return;
    }

    let mut source_file = File::open(&args[2]).unwrap();
    let mut buf = String::new();
    source_file.read_to_string(&mut buf).unwrap();

    println!("读取文件完成");

    let decoded_bytes = general_purpose::STANDARD
        .decode(buf).unwrap();

    println!("解码完成");

    let mut dest_file = File::create(&args[3]).unwrap();

    dest_file.write(decoded_bytes.as_slice()).unwrap();

    println!("\r\n转换成功\r\n");
}
