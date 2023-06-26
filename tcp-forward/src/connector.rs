use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    /// Listen host and port
    #[clap(short = 'L', value_parser)]
    pub(crate) listen: String,

    /// Forward host and port
    #[clap(short = 'F', value_parser)]
    pub(crate) forward: String,
}


pub enum Protocol {
    Http,
    Http2,
}

pub enum Transport {
    Tcp,
    Tls,
}

// 监听或者转发的链接
pub struct Connection<'a> {
    // 通信协议
    pub transport: Transport,
    // 传输协议
    pub host: &'a str,
    // 主机（ip或者域名）
    pub port: u16,            // 端口
}

impl<'a> Connection<'a> {
    pub fn new(transport: Transport, host: &str, port: u16) -> Connection {
        Connection {
            transport,
            host,
            port,
        }
    }

    pub fn display(&self) {
        let transport = match self.transport {
            Transport::Tcp => "tcp",
            Transport::Tls => "tls",
        };

        println!(
            "transport:{}, host: {}, port: {}", transport, self.host, self.port
        );
    }
}

pub(crate) fn parse_args(s: &str) -> Connection {
    let strs: Vec<&str> = s.split("://").collect();
    if strs.len() < 2 {
        println!("{} 格式错误, 例如： https://www.baidu.com", s)
    }

    let transport;

    if strs[0] == "http" {
        transport = Transport::Tcp;
    } else {
        transport = Transport::Tls;
    }

    let host_port: Vec<&str> = strs[1].split(":").collect();

    let host = match host_port[0] {
        "" => "127.0.0.1",
        _ => host_port[0],
    };

    let port;
    if host_port.len() < 2 {
        if strs[0] == "http" {
            port = 80;
        } else {
            port = 443;
        }
    } else {
        port = host_port[1].parse::<u16>().unwrap();
    }

    Connection::new(transport, host, port)
}