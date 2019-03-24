
#![allow(unused_imports)]

extern crate actix;
extern crate actix_redis;
extern crate actix_protobuf;
extern crate actix_web;
extern crate bytes;

extern crate futures;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
extern crate pretty_env_logger;
extern crate prost;
#[macro_use]
extern crate prost_derive;
extern crate redis;
extern crate redis_async;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate sysinfo;
extern crate tokio;



use actix_redis::{Command, RedisActor};
use actix::prelude::{
    Message as AMessage,
    Addr, Actor, Recipient,
};
use actix_web::{
    dev,
    error,
    http,
    middleware,
    middleware::cors::Cors,
    multipart,
    server,
    ws,
    App, AsyncResponder,
    Error,
    Either,
    FutureResponse,
    HttpRequest, HttpResponse, HttpMessage,
    Json,
};
use bytes::{Buf, IntoBuf, BytesMut};
use futures::future::{Future, join_all};
use futures::{future, stream, Stream};

use redis_async::resp::{ RespValue, FromResp };
use redis_async::client;
use redis::Commands;

use std::sync::Arc;
use std::io::Read;
use std::io::Write;

use actix_protobuf::{
    ProtoBuf,
    ProtoBufMessage,
    ProtoBufPayloadError,
    ProtoBufResponseBuilder,
};

mod websocket;
use websocket::{ WebSocketActor };

mod subscriptions;
use subscriptions::{
    SubscriptionActor,
    SubscribeOnOff,
    BroadcastUpdate,
    BroadcastUpdateProto,
};

pub struct AppState {
    pub redis_client: Arc<redis::Client>,
    pub subscriptions: Addr<SubscriptionActor>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Message)]
pub struct MyObj {
    #[prost(int32, tag="1")]
    pub number: i32,
    #[prost(string, tag="2")]
    pub name: String,
}



/// do websocket handshake and start `MyWebSocket` actor
pub fn ws_handshake_handler(req: &HttpRequest<AppState>) -> Result<HttpResponse, Error> {
    let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
    let subscriptions = req.state().subscriptions.clone().recipient();
    let addr_ws = WebSocketActor::new(subscriptions, redis_client);
    ws::start(req, addr_ws)
}


/// Handles incoming Protobuf messages, then dispatches to WebSocket clients as binary stream
pub fn protobuf_to_protobuf_ws(reqt: (ProtoBuf<MyObj>, HttpRequest<AppState>)) -> impl Future<Item=Result<HttpResponse, Error>, Error=Error> {

    let (msg, req) = reqt;
    println!("model: {:?}", msg);

    let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
    let conn = redis_client.get_connection().unwrap();
    let subscriptions = req.state().subscriptions.clone();

    let resp = HttpResponse::Ok().protobuf(msg.0);
    if let Ok(res) = &resp {
        match res.body() {
            actix_web::Body::Binary(b) => {
                // create a BroadcastUpdateProto message
                let broadcast_update = BroadcastUpdateProto(b.to_owned().take());
                info!("{:?}", broadcast_update);
                // send the message to SubscriptionActor's handler
                let fut = subscriptions.do_send(broadcast_update);
            },
            _ => (),
        }
    };
    future::ok(resp)
}

/// Handles incoming JSON messages from GET, then dispatches to WebSocket clients as binary stream
fn json_to_protobuf_ws(reqt: (Json<MyObj>, HttpRequest<AppState>)) -> impl Future<Item=HttpResponse, Error=Error> {

    let (myobj, req) = reqt;
    let myobj = myobj.into_inner();
    let subscriptions = req.state().subscriptions.clone();

    // REDIS update
    let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
    let conn = redis_client.get_connection().unwrap();
    let redis1: redis::RedisResult<String> = conn.set("name", &myobj.name);
    let redis2: redis::RedisResult<String> = conn.set("number", &myobj.number.to_string());
    let redis3: redis::RedisResult<String> = conn.publish("events", &myobj.name);

    // serialize MyObj into protobuf
    let resp = HttpResponse::Ok().protobuf(myobj.clone());
    let pbuf_resp = match &resp {
        Err(_e) => None,
        Ok(res) => {
            match res.body() {
                actix_web::Body::Binary(b) => {
                    // create a BroadcastUpdateProto message
                    let broadcast_update = BroadcastUpdateProto(b.clone().take());
                    info!("{:?}", broadcast_update);
                    // send the message to SubscriptionActor's handler
                    let _fut = subscriptions.do_send(broadcast_update);
                    Some(b.to_owned().take().to_vec())
                },
                _ => None,
            }
        }
    };
    future::ok(HttpResponse::Ok().content_type("application/json").json(JsonResponse {
        status: String::from("Json successfully serialized and broadcasted to protobuf websocket clients"),
        request: myobj,
        protobuf_response: pbuf_resp,
    }))
}


fn json_to_json_ws(reqt: (Json<MyObj>, HttpRequest<AppState>)) -> Box<Future<Item=HttpResponse, Error=Error>> {
    let (_myobj, req) = reqt;
    let myobj = _myobj.into_inner();

    let broadcast_update = BroadcastUpdate(myobj.clone());
    let fut = req.state().subscriptions
        .send(broadcast_update)
        .then(move |res| {
            future::ok(HttpResponse::Ok().content_type("application/json").json(JsonResponse {
                status: String::from(res.unwrap()),
                request: myobj,
                protobuf_response: None,
            }))
        });
    Box::new(fut)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct JsonResponse {
    status: String,
    request: MyObj,
    protobuf_response: Option<Vec<u8>>,
}





pub fn upload(req: HttpRequest<AppState>) -> FutureResponse<HttpResponse> {
    Box::new(
        req.multipart()
            .map_err(error::ErrorInternalServerError)
            .map(handle_multipart_item)
            .flatten()
            .collect()
            .map(|sizes| HttpResponse::Ok().json(sizes))
            .map_err(|e| {
                error!("failed: {}", e);
                e
            }),
    )
}


pub fn save_file(
    field: multipart::Field<dev::Payload>,
) -> Box<Future<Item = i64, Error = Error>> {

    let content_disposition = &field.content_disposition();
    info!("content_disposition: {:?}", &content_disposition);

    let file_path_string = match content_disposition {
        Some(f) => {
            match f.get_filename() {
                Some(filename) => filename,
                None => "text.txt",
            }
        },
        None => "temp.txt"
    };
    let file = match std::fs::File::create(format!("dat/{}", &file_path_string)) {
        Ok(file) => file,
        Err(e) => return Box::new(future::err(error::ErrorInternalServerError(e))),
    };

    let mut stdin = match std::process::Command::new("gsutil")
        .arg("cp")
        .arg("-")
        .arg(format!("gs://electric-assets/{}", &file_path_string))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn() {
            Err(why) => panic!("Error spawning command `gsutil`: {:?}", why),
            Ok(process) => process.stdin.unwrap(),
        };

    let ffield = field.fold(0i64, move |acc, bytes| {
        let rt = stdin
            .write_all(bytes.as_ref())
            .map(|_| acc + bytes.len() as i64)
            .map_err(|e| {
                error!("file.write_all failed: {:?}", e);
                error::MultipartError::Payload(error::PayloadError::Io(e))
            });
        future::result(rt)
    })
    .map_err(|e| {
        error!("save_file failed, {:?}", e);
        error::ErrorInternalServerError(e)
    });
    Box::new(ffield)
}

pub fn handle_multipart_item(
    item: multipart::MultipartItem<dev::Payload>,
) -> Box<Stream<Item = i64, Error = Error>> {
    match item {
        multipart::MultipartItem::Field(field) => {
            debug!("field: {:?}", &field);
            Box::new(save_file(field).into_stream())
        },
        multipart::MultipartItem::Nested(mp) => {
            Box::new(
                mp.map_err(error::ErrorInternalServerError)
                    .map(handle_multipart_item)
                    .flatten(),
            )
        }
    }
}



fn stream_to_gcloud(bytestream: &[u8], destination: &str) {
    let process = match std::process::Command::new("gsutil")
        .arg("cp")
        .arg("-")
        .arg(destination)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn() {
            Err(why) => panic!("Error spawning command `gsutil`: {:?}", why),
            Ok(process) => process,
        };

    match process.stdin.unwrap().write_all(bytestream) {
        Err(why) => panic!("Error: piping stream to `| gsutil cp - {}`:\t{:?}", &destination, why),
        Ok(s) => println!("Success: Piped stream to `| gsutil cp - {}`:\t{:?}", &destination, s),
    }

    let mut s = String::new();
    match process.stdout.unwrap().read_to_string(&mut s) {
        Err(why) => panic!("Could read `| gsutil cp - {}` stdout: {}", &destination, why),
        Ok(_) => println!("`| gsutil cp - {}` responded with: {:?}", &destination, s),
    }
}


fn main() {
    ::std::env::set_var("RUST_LOG", "actix_web=debug,proto");
    pretty_env_logger::init();

    stream_to_gcloud("to be or not to be?".as_ref(), "gs://electric-assets/hamlet.txt");

    let sys = actix::System::new("protobuf-example");
    server::new(|| {
        let redis_client = Arc::new(redis::Client::open("redis://127.0.0.1/").unwrap());
        let subscriptions = SubscriptionActor::new().start();
        let app_state = AppState {
            redis_client,
            subscriptions,
        };

        App::with_state(app_state)
            .middleware(middleware::Logger::default())
            .configure(|app| {
                Cors::for_app(app)
                    .allowed_origin("http://localhost:9000")
                    .allowed_origin("http://127.0.0.1:9000")
                    .allowed_origin("http://localhost:8080")
                    .allowed_origin("http://127.0.0.1:8080")
                    .allowed_origin("exp://10.0.0.60:19000")
                    .allowed_methods(vec!["GET", "POST"])
                    .allowed_headers(vec![
                        http::header::ACCEPT,
                        http::header::CONTENT_TYPE,
                        http::header::AUTHORIZATION,
                    ])
                    .resource("/ws/", |r| r.method(http::Method::GET).f(ws_handshake_handler))
                    .resource("/ws/pb/pb/stuff", |r| r.method(http::Method::POST).with_async(protobuf_to_protobuf_ws))
                    .resource("/ws/json/pb/stuff", |r| r.method(http::Method::POST).with_async(json_to_protobuf_ws))
                    // a.() -> async.
                    .resource("/ws/json/json/stuff", |r| {
                        r.method(http::Method::POST).with_async(json_to_json_ws);
                    })
                    .resource("/images", |r| r.method(http::Method::POST).with(upload))

                    .register()
            })
    }).bind("127.0.0.1:7070")
    .unwrap()
    .workers(1)
    .start();

    println!("Started http server: http://127.0.0.1:7070");

    let _ = sys.run();
}



