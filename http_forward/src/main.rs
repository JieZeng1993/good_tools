use std::convert::Infallible;

use clap::Parser;
use http::HeaderValue;
use http_body_util::{BodyExt, Full};
use hyper::{Request, Response};
use hyper::body::Bytes;
use hyper::http::uri::Scheme;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

use crate::connector::{Args, Connection};

mod connector;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    let l_conn = connector::parse_args(&args.listen);
    Connection::display(&l_conn);

    let f_conn = connector::parse_args(&args.forward);
    Connection::display(&f_conn);

    // 开启本地代理的监听
    let listener = TcpListener::bind((l_conn.host, l_conn.port)).await?;

    // 循环接收请求
    loop {
        let (stream, socket_addr) = listener.accept().await?;
        let forward_to = args.forward.clone();

        // 新增一个tokio线程，处理当前连接
        tokio::task::spawn(async move {
            let trace_id = Uuid::new_v4().to_string();
            println!("[{}]start deal connect {}", trace_id, socket_addr);

            // 组装为http请求，使用forward方法处理
            if let Err(err) = http1::Builder::new()
                // service_fn 使用 forward 方法处理每一次请求
                .serve_connection(stream, service_fn(|req: Request<hyper::body::Incoming>| async {
                    forward(req, forward_to.clone(), trace_id.clone()).await
                }))
                .await
            {
                println!("[{}]connect {} deal fail: {:?}", trace_id.clone(), socket_addr, err);
            }
            println!("[{}]finish deal connect {}", trace_id, socket_addr);
        });
    }
}

async fn forward(mut from_req: Request<hyper::body::Incoming>, forward_to: String, trace_id: String) -> Result<Response<Full<Bytes>>, Infallible> {
    //解析请求
    println!("[{}]origin:{}, method:{}", trace_id, from_req.uri(), from_req.method());

    let mut from_req_body_vec = vec![];
    let mut from_req_body = from_req.body_mut();
    while let Some(next) = from_req_body.frame().await {
        if next.is_err() {
            return Ok(Response::new(Full::new(Bytes::from("read from frame error"))));
        }
        let frame = next.unwrap();
        if let chunk = frame.into_data() {
            let mut frame = chunk.unwrap();
            from_req_body_vec.append(&mut frame.to_vec())
        }
    }
    println!("[{}]from request body: {}", trace_id, std::str::from_utf8(&from_req_body_vec).unwrap());

    let from_req_header = from_req.headers();

    let from_uri = from_req.uri();
    let forward_url;
    if from_uri.query().is_none() {
        forward_url = forward_to + from_uri.path();
    } else {
        forward_url = format!("{}{}?{}", forward_to, from_uri.path(), from_uri.query().unwrap());
    }
    let forward_url = forward_url.parse::<hyper::Uri>().unwrap();
    println!("[{}]redirect url:{}", trace_id, forward_url);

    let forward_host = forward_url.host().expect("url has no host");
    let forward_port = forward_url.port_u16().unwrap_or_else(|| {
        let scheme = forward_url.scheme().unwrap();
        if scheme.eq(&Scheme::HTTP) {
            80
        } else if scheme.eq(&Scheme::HTTPS) {
            443
        } else {
            panic!("not support schema：{}", scheme)
        }
    });
    let forward_addr = format!("{}:{}", forward_host, forward_port);

    //发起请求
    let forward_stream = TcpStream::connect(forward_addr).await;
    if forward_stream.is_err() {
        return Ok(Response::new(Full::new(Bytes::from("forward connect error"))));
    }
    let forward_stream = forward_stream.unwrap();

    let forward_handshake = hyper::client::conn::http1::handshake(forward_stream).await;
    if forward_handshake.is_err() {
        return Ok(Response::new(Full::new(Bytes::from("forward handshake error"))));
    }
    let (mut forward_sender, forward_conn) = forward_handshake.unwrap();
    let trace_id_clone = trace_id.clone();
    tokio::task::spawn(async move {
        if let Err(err) = forward_conn.await {
            println!("[{}]forward connect fail: {:?}", trace_id_clone, err);
        }
        println!("[{}]forward connect finish", trace_id_clone);
    });

    let authority = forward_url.authority().unwrap().clone();

    let mut forward_req_builder = Request::builder()
        .uri(forward_url).method(from_req.method());
    let mut forward_req_headers = forward_req_builder.headers_mut().unwrap();
    for (header_name, header_value) in from_req_header {
        if header_name.eq(&http::header::HOST) {
            //deal cross error
            forward_req_headers.append(header_name, HeaderValue::try_from(authority.to_string()).unwrap());
        } else {
            forward_req_headers.append(header_name, header_value.clone());
        }
    }

    let forward_req = forward_req_builder.body(Full::new(Bytes::from(from_req_body_vec)));
    if forward_req.is_err() {
        return Ok(Response::new(Full::new(Bytes::from("assert req error"))));
    }
    let forward_req = forward_req.unwrap();

    //解析转发后的响应
    println!("[{}]forward request Headers: {:#?}", trace_id, forward_req.headers());
    let forward_resp = forward_sender.send_request(forward_req).await;
    if forward_resp.is_err() {
        return Ok(Response::new(Full::new(Bytes::from("forward request error"))));
    }
    let mut forward_resp = forward_resp.unwrap();

    println!("[{}]forward response status: {}", trace_id, forward_resp.status());
    let forward_resp_headers = forward_resp.headers();
    println!("[{}]forward response Headers: {:#?}", trace_id, forward_resp_headers);
    let mut from_resp_builder = Response::builder();
    let mut forward_content_type = String::new();
    for (header_name, header_value) in forward_resp_headers {
        if header_name.eq(&http::header::CONTENT_TYPE) {
            forward_content_type = header_value.to_str().unwrap().to_string();
        }
        from_resp_builder.headers_mut().map(|resp_builder1| resp_builder1.append(header_name, header_value.clone()));
    }

    let mut from_resp_body_vec = vec![];
    let mut forward_resp_body = forward_resp.into_body();
    while let Some(next) = forward_resp_body.frame().await {
        if next.is_err() {
            return Ok(Response::new(Full::new(Bytes::from("read forward frame error"))));
        }
        let frame = next.unwrap();
        if let chunk = frame.into_data() {
            let mut frame = chunk.unwrap();
            from_resp_body_vec.append(&mut frame.to_vec())
        }
    }
    if forward_content_type.starts_with("image") {
        println!("[{}]response body type: {}", trace_id, forward_content_type);
    } else {
        if from_resp_body_vec.is_empty() {
            println!("[{}]response body is empty", trace_id);
        } else {
            println!("[{}]response body: {}", trace_id, std::str::from_utf8(&from_resp_body_vec).unwrap());
        }
    }

    Ok(from_resp_builder.body(Full::new(Bytes::from(from_resp_body_vec))).unwrap())
}