use std::convert::Infallible;
use std::net::SocketAddr;

use clap::Parser;
use http_body_util::{BodyExt, Empty, Full, StreamBody};
use hyper::{Request, Response};
use hyper::body::{Body, Bytes};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use tokio::io;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use crate::connector::{Args, Connection};

mod connector;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    let l_conn = connector::parse_args(&args.listen);
    Connection::display(&l_conn);

    let f_conn = connector::parse_args(&args.forward);
    Connection::display(&f_conn);

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind((l_conn.host, l_conn.port)).await?;

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(stream, service_fn(hello))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}

async fn hello(request: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let uri = request.uri();
    let header = request.headers();


    let mut url = "http://idea.lanyus.com/".to_owned() + uri.path();
    if uri.query().is_some() {
        url += &*("?".to_owned() + uri.query().unwrap());
    }
    let url = url.parse::<hyper::Uri>().unwrap();

    let host = url.host().expect("uri has no host");
    let port = url.port_u16().unwrap_or(80);
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(addr).await;
    if stream.is_err() {
        return Ok(Response::new(Full::new(Bytes::from("connect error"))));
    }
    let stream = stream.unwrap();

    let handshake = hyper::client::conn::http1::handshake(stream).await;
    if handshake.is_err() {
        return Ok(Response::new(Full::new(Bytes::from("handshake error"))));
    }
    let (mut sender, conn) = handshake.unwrap();
    tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            println!("Connection failed: {:?}", err);
        }
    });

    let authority = url.authority().unwrap().clone();

    let mut req_builder = Request::builder()
        .uri(url)
        .header(hyper::header::HOST, authority.as_str());
    for (header_name, header_value) in header {
        req_builder.headers_mut().map(|req_builder1| req_builder1.append(header_name, header_value.clone()));
    }

    // // Protect our server from massive bodies.
    // let upper = request.body().size_hint().upper().unwrap_or(u64::MAX);
    // if upper > 1024 * 64 {
    //     return Ok(Response::new(Full::new(Bytes::from("Body too big"))));
    // }
    //
    // // Await the whole body to be collected into a single `Bytes`...
    // let whole_body = request.collect().await.unwrap().to_bytes();
    // // Iterate the whole body in reverse order and collect into a new Vec.
    // let reversed_body = whole_body.iter()
    //     .rev()
    //     .cloned()
    //     .collect::<Vec<u8>>();

    let req = req_builder.body(request.into_body());
    if req.is_err() {
        return Ok(Response::new(Full::new(Bytes::from("assert req error"))));
    }
    let req = req.unwrap();

    let res = sender.send_request(req).await;
    if res.is_err() {
        return Ok(Response::new(Full::new(Bytes::from("request error"))));
    }
    let mut res = res.unwrap();

    println!("Response: {}", res.status());
    println!("Headers: {:#?}\n", res.headers());

    // Stream the body, writing each chunk to stdout as we get it
    // (instead of buffering and printing at the end).
    let chunks: Vec<Result<_, Infallible>> = vec![]
    while let Some(next) = res.frame().await {
        if next.is_err() {
            return Ok(Response::new(Full::new(Bytes::from("read frame error"))));
        }
        let frame = next.unwrap();
        if let Some(chunk) = frame.data_ref() {
            io::stdout().write_all(&chunk).await.expect("TODO: panic message");
        }
    }

    println!("\n\nDone!");

    let chunks: Vec<Result<_, Infallible>> = vec![
        Ok(Frame::data(Bytes::from(vec![1]))),
        Ok(Frame::data(Bytes::from(vec![2]))),
        Ok(Frame::data(Bytes::from(vec![3]))),
    ];
    let stream = futures_util::stream::iter(chunks);
    let mut body = StreamBody::new(res.frame());



    Ok(Response::new(Full::new(Bytes::from("Hello, World!"))))
}