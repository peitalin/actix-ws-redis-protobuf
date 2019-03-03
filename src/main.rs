
extern crate actix;
extern crate actix_web;
extern crate actix_redis;
#[macro_use]
extern crate redis_async;

extern crate serde;
#[macro_use]
extern crate serde_derive;

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
extern crate tokio;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use actix::prelude::{
    Message as AMessage,
    Addr, Actor, Recipient,
};
use actix_redis::{Command, RedisActor, Error as ARError};
use redis_async::resp::{ RespValue, FromResp };
use redis_async::client;

extern crate redis;
use redis::Commands;

use actix_web::{
    middleware, server, App, HttpRequest, HttpResponse,
    Json, AsyncResponder, http, Error as AWError,
    ws,
};

use futures::future::{Future, join_all};
use futures::{future, Stream};

mod protobuf;
use protobuf::{
    ProtoBufPayloadError,
    ProtoBufMessage,
    ProtoBufResponseBuilder,
};
use prost::Message;
use bytes::{Buf, IntoBuf, BigEndian};

mod websocket;
use websocket::{ MyWebSocket };

mod listener;
use listener::{
    ListenerActor,
    ListenUpdate,
    ListenControl
};




pub struct AppState {
    pub redis_client: Arc<redis::Client>,
    pub listener: Addr<ListenerActor>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Message)]
pub struct MyObj {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub number: String,
}



/// This handler uses `ProtoBufMessage` for loading protobuf object.
pub fn protobuf_handler(req: &HttpRequest<AppState>) -> impl Future<Item=HttpResponse, Error=AWError> {

    let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
    let conn = redis_client.get_connection().unwrap();

    ProtoBufMessage::new(req)
        .from_err()  // convert all errors into `Error`
        .and_then(|value: MyObj| {

            println!("\n3. model:{:?}\n\t\tname: {:?}\n\t\tnumber: {:?}", value, value.name, value.number);
            future::ok(HttpResponse::Ok().protobuf_response(value).unwrap())

        }).and_then(move |resp: HttpResponse| {

            match resp.body() {
                actix_web::Body::Binary(b) => {
                    // let buf = b.take().into_buf();
                    // let vec: Vec<u8> = buf.collect();
                    let buf: &[u8] = b.as_ref();
                    println!("buf >>> {:?}\n", buf);

                    // redis
                    let redis1: redis::RedisResult<String> = conn.set("redis", buf);
                    let redis2: redis::RedisResult<i32> = conn.publish("events", buf);
                    println!("\n\tredis1 set result: {:?}", redis1);
                    println!("\tredis2 publish result: {:?}\n", redis2);

                    future::ok(resp)
                },
                _ => future::ok(resp),
            }

        })
        .responder() // from: actix_web::AsyncResponder trait
}



// fn cache_stuff(reqt: (Json<MyObj>, HttpRequest<AppState>)) -> impl  Future<Item=HttpResponse, Error=AWError> {
//
//     let (myobj, req) = reqt;
//     let myobj = myobj.into_inner();
//
//     // Serialize using protocul buffers
//     let mut myobj_buffer = Vec::new();
//     // encode value, and insert into body vector
//     myobj.encode(&mut myobj_buffer)
//         .map_err(|e| ProtoBufPayloadError::Serialize(e)).unwrap();
//
//     // REDIS update
//     let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
//     let conn = redis_client.get_connection().unwrap();
//     let redis1: redis::RedisResult<String> = conn.set("name", &myobj.name);
//     let redis2: redis::RedisResult<String> = conn.set("name", &myobj.name);
//     let redis3: redis::RedisResult<String> = conn.publish("events", myobj_buffer.clone());
//
//     // Websocket broadcast
//     let listener: Addr<ListenerActor> = req.state().listener.clone();
//     let update = ListenUpdate(myobj.clone());
//     let fut = listener.send(update)
//         .then(move |res| {
//             println!("{:?}", res);
//             future::ok(HttpResponse::Ok().content_type("application/json").json(JsonResponse {
//                 status: String::from("Successfully received and broadcoast msg to websocket clients"),
//                 request: myobj,
//                 protobuf_response: None,
//             }))
//         });
//     Box::new(fut)
// }




/// do websocket handshake and start `MyWebSocket` actor
pub fn ws_handler(req: &HttpRequest<AppState>) -> Result<HttpResponse, AWError> {
    let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
    let listener = req.state().listener.clone().recipient();
    let addr_ws = MyWebSocket::new(redis_client, listener);
    ws::start(req, addr_ws)
}

fn new_comment(reqt: (Json<MyObj>, HttpRequest<AppState>)) -> Box<Future<Item=HttpResponse, Error=AWError>> {

    let (myobj, req) = reqt;
    let myobj = myobj.into_inner();
    let listener = req.state().listener.clone();
    let update = ListenUpdate(myobj.clone());
    let fut = listener.send(update)
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

    ::std::env::set_var("RUST_LOG", "actix_web=,proto=info,debug,warn,error");
    pretty_env_logger::init();
    info!("such information");
    warn!("o_O");
    error!("much error");

    // actix::run(redis_pubsub);

    let sys = actix::System::new("protobuf-example");


    let addr = server::new(|| {

        let redis_client = Arc::new(redis::Client::open("redis://127.0.0.1/").unwrap());
        let listener = ListenerActor::new().start();
        let app_state = AppState {
            redis_client,
            listener,
        };
        // Redis
        App::with_state(app_state)
            .middleware(middleware::Logger::default())
            .resource("/", |r| r.method(http::Method::POST).a(protobuf_handler))
            // a.() -> async.
            .resource("/ws/", |r| r.method(http::Method::GET).f(ws_handler))
            .resource("/stuff", |r| {
                r.method(http::Method::POST).with_async(new_comment);
                // r.method(http::Method::POST).with_async(cache_stuff);
                // r.method(http::Method::DELETE).f(del_stuff);
                // r.method(http::Method::GET).f(get_stuff);
            })

    }).bind("127.0.0.1:7070")
    .unwrap()
    .shutdown_timeout(1)
    .workers(2)
    .start();

    println!("Started http server: 127.0.0.1:7070");

    let _ = sys.run();
}



