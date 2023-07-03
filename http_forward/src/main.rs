use std::convert::Infallible;

use clap::Parser;
use http::{HeaderMap, HeaderValue, Method, Uri};
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
                    forward(req, &forward_to, &trace_id).await
                }))
                .await
            {
                println!("[{}]connect {} deal fail: {:?}", trace_id.clone(), socket_addr, err);
            }
            println!("[{}]finish deal connect {}", trace_id, socket_addr);
        });
    }
}

async fn forward(mut from_req: Request<hyper::body::Incoming>, forward_to: &str, trace_id: &str) -> Result<Response<Full<Bytes>>, Infallible> {
    //解析请求
    println!("[{}]origin:{}, method:{}", trace_id, from_req.uri(), from_req.method());

    let from_req_body_vec = read_body_data(from_req.body_mut(), trace_id, "reqBody", None).await;
    if from_req_body_vec.is_err() {
        return Ok(Response::new(Full::new(Bytes::from(from_req_body_vec.unwrap_err()))));
    }
    let from_req_body_vec = from_req_body_vec.unwrap();
    let from_req_header = from_req.headers();

    let (forward_addr, forward_url) = get_forward_addr(from_req.uri(), forward_to, trace_id);

    //发起请求
    let forward_stream = TcpStream::connect(forward_addr).await;
    if forward_stream.is_err() {
        println!("[{trace_id}]forward connect error");
        return Ok(Response::new(Full::new(Bytes::from("forward connect error"))));
    }
    let forward_stream = forward_stream.unwrap();

    let forward_handshake = hyper::client::conn::http1::handshake(forward_stream).await;
    if forward_handshake.is_err() {
        println!("[{trace_id}]forward handshake error");
        return Ok(Response::new(Full::new(Bytes::from("forward handshake error"))));
    }
    let (mut forward_sender, forward_conn) = forward_handshake.unwrap();
    let trace_id_clone = trace_id.to_string();
    tokio::task::spawn(async move {
        if let Err(err) = forward_conn.await {
            println!("[{trace_id_clone}]forward connect fail: {:?}", err);
        }
        println!("[{trace_id_clone}]forward connect finish");
    });

    let forward_req = assemble_redirect_req(forward_url, from_req.method(), from_req_body_vec, from_req_header, trace_id);
    if forward_req.is_err() {
        return Ok(Response::new(Full::new(Bytes::from(forward_req.unwrap_err()))));
    }
    let forward_req = forward_req.unwrap();

    //解析转发后的响应
    println!("[{trace_id}]forward request Headers: {:#?}", forward_req.headers());
    let forward_resp = forward_sender.send_request(forward_req).await;
    if forward_resp.is_err() {
        return Ok(Response::new(Full::new(Bytes::from("forward request error"))));
    }
    let mut forward_resp = forward_resp.unwrap();

    let forward_resp_headers = forward_resp.headers();
    println!("[{trace_id}]forward response status:{} Headers: {:#?}", forward_resp.status(), forward_resp_headers);
    let mut from_resp_builder = Response::builder();
    let mut forward_content_type = String::new();
    for (header_name, header_value) in forward_resp_headers {
        if header_name.eq(&http::header::CONTENT_TYPE) {
            forward_content_type = header_value.to_str().unwrap().to_string();
        }
        from_resp_builder.headers_mut().map(|resp_builder1| resp_builder1.append(header_name, header_value.clone()));
    }

    let forward_resp_body_vec = read_body_data(forward_resp.body_mut(), trace_id, "respBody", Some(&forward_content_type)).await;
    if forward_resp_body_vec.is_err() {
        return Ok(Response::new(Full::new(Bytes::from(forward_resp_body_vec.unwrap_err()))));
    }
    let forward_resp_body_vec = forward_resp_body_vec.unwrap();

    Ok(from_resp_builder.body(Full::new(Bytes::from(forward_resp_body_vec))).unwrap())
}

///将body 转换为字节
async fn read_body_data(body: &mut hyper::body::Incoming, trace_id: &str, source: &str, content_type: Option<&str>) -> Result<Vec<u8>, String> {
    let mut body_vec = vec![];
    loop {
        let frame = body.frame().await;
        if frame.is_none() {
            break;
        }
        let frame = frame.unwrap();
        if frame.is_err() {
            let err_msg = format!("read from {} error", source);
            println!("[{}]{}, {:?}", trace_id, err_msg, frame.unwrap_err());
            return Err(err_msg);
        }
        let frame = frame.unwrap().into_data();
        if frame.is_err() {
            let err_msg = format!("read data from {} error", source);
            println!("[{}]{}, {:?}", trace_id, err_msg, frame.unwrap_err());
            return Err(err_msg);
        }
        body_vec.append(&mut frame.unwrap().to_vec())
    }

    if content_type.is_some() && content_type.unwrap().starts_with("image") {
        println!("[{trace_id}]{source} body type: {}", content_type.unwrap());
    } else {
        if body_vec.is_empty() {
            println!("[{trace_id}]{source} body is empty");
        } else {
            println!("[{trace_id}]{source} body: {}", std::str::from_utf8(&body_vec).unwrap());
        }
    }

    Ok(body_vec)
}

///获取转发的addr以及url
fn get_forward_addr(from_uri: &Uri, forward_to: &str, trace_id: &str) -> (String, Uri) {
    let forward_url;
    if from_uri.query().is_none() {
        forward_url = format!("{}{}", forward_to, from_uri.path());
    } else {
        forward_url = format!("{}{}?{}", forward_to, from_uri.path(), from_uri.query().unwrap());
    }
    let forward_url = forward_url.parse::<Uri>().unwrap();
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
    (format!("{}:{}", forward_host, forward_port), forward_url)
}

///组装请求
fn assemble_redirect_req(forward_url: Uri, method: &Method, from_req_body_vec: Vec<u8>, from_req_header: &HeaderMap, trace_id: &str) -> Result<Request<Full<Bytes>>, String> {
    let authority = forward_url.authority().unwrap().clone();

    let mut forward_req_builder = Request::builder()
        .uri(forward_url).method(method);
    let forward_req_headers = forward_req_builder.headers_mut().unwrap();
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
        let err_msg = format!("assert req error");
        println!("[{trace_id}]{err_msg}");
        return Err(err_msg);
    }
    Ok(forward_req.unwrap())
}