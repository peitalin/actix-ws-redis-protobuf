
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
extern crate serde_derive;
extern crate tokio;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use redis_async::resp::{ RespValue, FromResp };
use redis_async::client;
use redis::Commands;

use actix_redis::{Command, RedisActor, Error as ARError};
use actix::prelude::{
    Message as AMessage,
    Addr, Actor, Recipient,
};
use actix_web::{
    middleware, server, App, HttpRequest, HttpResponse,
    Json, AsyncResponder, http, Error as AWError,
    HttpMessage, ws,
};
use futures::future::{Future, join_all};
use futures::{future, Stream};
use bytes::{Buf, IntoBuf, BytesMut};

////////////////////////////////
// use crate::actix_protobuf::{
//     ProtoBufPayloadError,
//     ProtoBufBody,
//     ProtoBufResponseBuilder,
//     ProtoBuf,
// };


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


fn index(reqt: (ProtoBuf<MyObj>, HttpRequest<AppState>)) -> Result<HttpResponse, AWError> {
    let (msg, req) = reqt;
    println!("model: {:?}", msg);
    HttpResponse::Ok().protobuf(msg.0)
}


/// do websocket handshake and start `MyWebSocket` actor
pub fn ws_handshake_handler(req: &HttpRequest<AppState>) -> Result<HttpResponse, AWError> {
    let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
    let subscriptions = req.state().subscriptions.clone().recipient();
    let addr_ws = WebSocketActor::new(subscriptions, redis_client);
    ws::start(req, addr_ws)
}


/// Handles incoming Protobuf messages, then dispatches to WebSocket clients as binary stream
pub fn protobuf_handler_ws(reqt: (ProtoBuf<MyObj>, HttpRequest<AppState>)) -> impl Future<Item=Result<HttpResponse, AWError>, Error=AWError> {

    let (msg, req) = reqt;
    println!("model: {:?}", msg);

    let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
    let conn = redis_client.get_connection().unwrap();
    let subscriptions = req.state().subscriptions.clone();

    let resp = HttpResponse::Ok().protobuf(msg.0);
    match &resp {
        Ok(res) => {
            match res.body() {
                actix_web::Body::Binary(b) => {
                    // create a BroadcastUpdateProto message
                    let broadcast_update = BroadcastUpdateProto(b.to_owned().take());
                    info!("{:?}", broadcast_update);
                    // send the message to SubscriptionActor's handler
                    let fut = subscriptions.do_send(broadcast_update);
                    Ok(res)
                },
                _ => Ok(res)
            }
        },
        Err(e) => Err(e)
    };
    future::ok(resp)


    // ProtoBufBody::new(req)
    // .from_err()  // convert all errors into `Error`
    // .and_then(|value: MyObj| {
    //     println!("3. model:{:?}\n\t\tname: {:?}\n\t\tnumber: {:?}", value, value.name, value.number);
    //     future::ok(HttpResponse::Ok().protobuf(value).unwrap())
    // }).and_then(move |resp: HttpResponse| {
    //     match resp.body() {
    //         actix_web::Body::Binary(b) => {
    //             // b: Binary
    //             // take() => convert to Bytes
    //             // let buf: Vec<u8> = b.take().into_buf();
    //             // let buf: &[u8] = b.as_ref();
    //             // create a BroadcastUpdateProto message
    //             let broadcast_update = BroadcastUpdateProto(b.to_owned().take());
    //             info!("{:?}", broadcast_update);
    //             // send the message to SubscriptionActor's handler
    //             let fut = subscriptions.do_send(broadcast_update);
    //             future::ok(resp)
    //         },
    //         _ => future::ok(resp),
    //     }
    // }).responder() // from: actix_web::AsyncResponder trait
}

// /// Handles incoming JSON messages from GET, then dispatches to WebSocket clients as binary stream
// fn json_handler_ws(reqt: (Json<MyObj>, HttpRequest<AppState>)) -> impl Future<Item=HttpResponse, Error=AWError> {
//
//     let (myobj, req) = reqt;
//     let myobj = myobj.into_inner();
//     let subscriptions = req.state().subscriptions.clone();
//
//     // REDIS update
//     let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
//     let conn = redis_client.get_connection().unwrap();
//     let redis1: redis::RedisResult<String> = conn.set("name", &myobj.name);
//     let redis2: redis::RedisResult<String> = conn.set("number", &myobj.number);
//     let redis3: redis::RedisResult<String> = conn.publish("events", &myobj.name);
//
//     ProtoBufBody::new(&req)
//     .from_err()  // convert all errors into `Error`
//     .and_then(move |value: MyObj| {
//         println!("3. model:{:?}\n\t\tname: {:?}\n\t\tnumber: {:?}", value, value.name, value.number);
//         future::ok(HttpResponse::Ok().protobuf(myobj).unwrap())
//     }).and_then(move |resp: HttpResponse| {
//         match resp.body() {
//             actix_web::Body::Binary(b) => {
//                 // b: Binary
//                 // take() => convert to Bytes
//                 let broadcast_update = BroadcastUpdateProto(b.to_owned().take());
//                 info!("{:?}", broadcast_update);
//                 // send the message to SubscriptionActor's handler
//                 let fut = subscriptions.do_send(broadcast_update);
//                 future::ok(resp)
//             },
//             _ => future::ok(resp),
//         }
//     }).responder()
// }




// fn json_handler(reqt: (Json<MyObj>, HttpRequest<AppState>)) -> Box<Future<Item=HttpResponse, Error=AWError>> {
//
//     let (_myobj, req) = reqt;
//     let myobj = _myobj.into_inner();
//
//     let broadcast_update = BroadcastUpdate(myobj.clone());
//     let fut = req.state().subscriptions
//         .send(broadcast_update)
//         .then(move |res| {
//             future::ok(HttpResponse::Ok().content_type("application/json").json(JsonResponse {
//                 status: String::from(res.unwrap()),
//                 request: myobj,
//                 protobuf_response: None,
//             }))
//         });
//
//     Box::new(fut)
// }
//
// #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// struct JsonResponse {
//     status: String,
//     request: MyObj,
//     protobuf_response: Option<Vec<u8>>,
// }




fn main() {

    ::std::env::set_var("RUST_LOG", "actix_web=error,proto");
    pretty_env_logger::init();

    let sys = actix::System::new("protobuf-example");

    let addr = server::new(|| {

        let redis_client = Arc::new(redis::Client::open("redis://127.0.0.1/").unwrap());
        let subscriptions = SubscriptionActor::new().start();
        let app_state = AppState {
            redis_client,
            subscriptions,
        };
        // Redis
        App::with_state(app_state)
            .middleware(middleware::Logger::default())
            .resource("/ws/", |r| r.method(http::Method::GET).f(ws_handshake_handler))
            .resource("/ws/stuff", |r| r.method(http::Method::POST).with_async(protobuf_handler_ws))
            // .resource("/ws/stuff", |r| r.method(http::Method::POST).with(index))
            // a.() -> async.
            .resource("/stuff", |r| {
                // r.method(http::Method::POST).with_async(json_handler);
                // r.method(http::Method::POST).with_async(json_handler_ws);
                // r.method(http::Method::DELETE).f(del_stuff);
                // r.method(http::Method::GET).f(get_stuff);
            })

    }).bind("127.0.0.1:7070")
    .unwrap()
    .shutdown_timeout(1)
    .workers(1) // 1 worker, otherwise it splits requests
    .start();

    println!("Started http server: 127.0.0.1:7070");

    let _ = sys.run();
}




