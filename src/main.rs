
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
    error::MultipartError,
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
mod multipart_raw;
use multipart_raw::upload;


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

    // let _redis_client: Arc<redis::Client> = req.state().redis_client.clone();
    // let _conn = redis_client.get_connection().unwrap();
    let subscriptions = req.state().subscriptions.clone();

    let resp = HttpResponse::Ok().protobuf(msg.0);
    if let Ok(res) = &resp {
        match res.body() {
            actix_web::Body::Binary(b) => {
                // create a BroadcastUpdateProto message
                let broadcast_update = BroadcastUpdateProto(b.to_owned().take());
                info!("{:?}", broadcast_update);
                // send the message to SubscriptionActor's handler
                let _fut = subscriptions.do_send(broadcast_update);
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
    let _redis1: redis::RedisResult<String> = conn.set("name", &myobj.name);
    let _redis2: redis::RedisResult<String> = conn.set("number", &myobj.number.to_string());
    let _redis3: redis::RedisResult<String> = conn.publish("events", &myobj.name);

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









fn main() {
    ::std::env::set_var("RUST_LOG", "actix_web=debug,proto");
    pretty_env_logger::init();

    // stream_to_gcloud("to be or not to be?".as_ref(), "gs://electric-assets/hamlet.txt");

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



