
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
extern crate env_logger;
extern crate prost;
#[macro_use]
extern crate prost_derive;
extern crate tokio;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use actix::prelude::{
    Message as AMessage,
    Addr
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




pub struct AppState {
    // pub redis_addr: Arc<Addr<RedisActor>>
    pub redis_client: Arc<redis::Client>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Message)]
pub struct MyObj {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub number: String,
}


/// do websocket handshake and start `MyWebSocket` actor
pub fn ws_handler(req: &HttpRequest<AppState>) -> Result<HttpResponse, AWError> {
    let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
    ws::start(req, MyWebSocket::new(redis_client))
}


/// This handler uses `ProtoBufMessage` for loading protobuf object.
pub fn protobuf_handler(req: HttpRequest<AppState>) -> Box<Future<Item=HttpResponse, Error=AWError>> {
// pub fn protobuf_handler(reqt: (Json<MyObj>, HttpRequest<AppState>)) -> Box<Future<Item=HttpResponse, Error=AWError>> {

    // let (info, req) = reqt;

    // let redis: Arc<Addr<RedisActor>> = req.state().redis_addr.clone();
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

                    // // redis-async
                    // let myobj = redis.send(Command(resp_array!["SET", "futures", "WORKING"]));
                    //
                    // println!("Redis connected: {:?}\n", redis.connected());
                    //
                    // myobj.map_err(|err| println!("{:?}", err))
                    // .and_then(|res: Result<RespValue, ARError>| {
                    //     println!("redis result >>> {:?}\n", res);
                    //     // successful operations return "OK", so confirm that all returned as so
                    //     match res {
                    //         Ok(RespValue::SimpleString(ref x)) if x=="OK" => {
                    //             // future::ok(HttpResponse::Ok())
                    //             future::ok(())
                    //         },
                    //         _ => panic!("Redis error in publishing serialized buf data"),
                    //     }
                    // }).poll();

                    future::ok(resp)
                },
                _ => future::ok(resp),
            }

        })
        .responder() // from: actix_web::AsyncResponder trait
}

// fn index(msg: ProtoBuf<MyObj>) -> Result<HttpResponse, AWError> {
//     println!("model: {:?}", msg);
//     HttpResponse::Ok().protobuf_response(msg.0)  // <- send response
// }



fn cache_stuff(reqt: (Json<MyObj>, HttpRequest<AppState>)) -> HttpResponse {

    let (info, req) = reqt;
    let info = info.into_inner();

    let redis_client: Arc<redis::Client> = req.state().redis_client.clone();
    let conn = redis_client.get_connection().unwrap();

    // Serialize using protocul buffers
    let mut myobj_buffer = Vec::new();
    // encode value, and insert into body vector
    info.encode(&mut myobj_buffer)
        .map_err(|e| ProtoBufPayloadError::Serialize(e)).unwrap();

    let redis1: redis::RedisResult<String> = conn.set("name", &info.name);
    let redis2: redis::RedisResult<String> = conn.set("name", &info.name);
    let myobj: redis::RedisResult<String> = conn.publish("events", myobj_buffer.clone());
    HttpResponse::Ok().json(jsonResponse{
        status: String::from("Successfully serialized request into protobuf and stored in Redis."),
        request: info,
        protobuf_response: myobj_buffer,
    })
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct jsonResponse {
    status: String,
    request: MyObj,
    protobuf_response: Vec<u8>,
}


// fn del_stuff(req: HttpRequest<AppState>) -> Box<Future<Item=HttpResponse, Error=AWError>> {
//     // get redis actor state
//     let redis = req.state().redis_addr.clone();
//
//     redis.send(Command(resp_array!["DEL", "dd:name", "dd:number"]))
//          .map_err(AWError::from)
//          .and_then(|res: Result<RespValue, ARError>|
//             match &res {
//                 Ok(RespValue::Integer(x)) if x==&3 =>
//                     Ok(HttpResponse::Ok().body("\tsuccessfully deleted values\n")),
//                  _ => {
//                     println!("\n\tDELETE ---->{:?}\n", res);
//                     Ok(HttpResponse::InternalServerError().finish())
//                  }
//               })
//         .responder()
// }


// fn get_stuff(req: HttpRequest<AppState>) -> Box<Future<Item=HttpResponse, Error=AWError>> {
//
//     let redis = req.state().redis_addr.clone();
//
//     redis.send(Command(resp_array!["GET", "dd:name"]))
//         .map_err(AWError::from)
//         .and_then(|res: Result<RespValue, ARError>| {
//             match &res {
//                 Ok(RespValue::Integer(x)) => Ok(HttpResponse::Ok().body("successfully got values")),
//                 Ok(RespValue::BulkString(x)) => {
//                     let msg = String::from_utf8(x.to_owned());
//                     println!("\nMSG: {:?}\n", msg);
//                     Ok(HttpResponse::Ok().body(format!("successfully got value: {:?}", msg)))
//                 },
//                 _ => {
//                     println!("\n\tGET ----> {:?}\n", res);
//                     Ok(HttpResponse::InternalServerError().finish())
//                 },
//             }
//         })
//         .responder()
// }



// fn redis_pubsub() -> impl Future<Item=(), Error=()> {
//     let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 6379);
//     let msgs = client::pubsub_connect(&socket)
//         .and_then(move |connection| connection.subscribe("events"));
//
//     let the_loop = msgs.map_err(|_| ())
//         .and_then(|msgs| {
//             msgs.for_each(|message| {
//                 println!("{:?}", String::from_resp(message).unwrap());
//                 future::ok(())
//             })
//         });
//
//     the_loop
// }



fn main() {

    ::std::env::set_var("RUST_LOG", "actix_web=info,debug");
    env_logger::init();

    // actix::run(redis_pubsub);

    let sys = actix::System::new("protobuf-example");

    server::new(|| {

        let redis_client = Arc::new(redis::Client::open("redis://127.0.0.1/").unwrap());
        let app_state = AppState {
            redis_client,
        };
        // Redis
        App::with_state(app_state)
            .middleware(middleware::Logger::default())
            .resource("/", |r| r.method(http::Method::POST).with_async(protobuf_handler))
            .resource("/ws/", |r| r.method(http::Method::GET).f(ws_handler))
            .resource("/stuff", |r| {
                r.method(http::Method::POST).with(cache_stuff);
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

