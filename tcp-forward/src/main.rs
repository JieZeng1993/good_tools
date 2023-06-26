use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::thread;

use clap::Parser;
use dns_lookup::lookup_host;

use crate::connector::{Args, Connection};

mod connector;

fn main() {
    let args = Args::parse();
    let l_conn = connector::parse_args(&args.listen);
    Connection::display(&l_conn);

    let f_conn = connector::parse_args(&args.forward);
    Connection::display(&f_conn);

    let tcp_listener = TcpListener::bind((l_conn.host, l_conn.port)).unwrap();
    while let Ok((mut from_stream, socket_addr)) = tcp_listener.accept() {
        println!("获取到请求：{}", socket_addr);
        let mut from_stream_clone = from_stream.try_clone().unwrap();
        let mut to_stream = TcpStream::connect((f_conn.host, f_conn.port)).unwrap();
        let mut to_stream_clone = to_stream.try_clone().unwrap();

        thread::spawn(move || {
            let mut to_buffer = [0; 64];
            while let Ok(to_length) = to_stream_clone.read(&mut to_buffer) {
                let write_from = from_stream_clone.write_all(&to_buffer[..to_length]);
                if write_from.is_err() {
                    println!("redirect_response_error:{}", write_from.unwrap_err());
                    break;
                }
            }
        });

        let mut from_buffer = [0; 64];
        while let Ok(from_length) = from_stream.read(&mut from_buffer) {
            let write_to = to_stream.write_all(&from_buffer[..from_length]);
            if write_to.is_err() {
                println!("redirect_request_error:{}", write_to.unwrap_err());
                break;
            }
        }
        println!("请求结束")
    }
    println!("Hello, world!");
}
