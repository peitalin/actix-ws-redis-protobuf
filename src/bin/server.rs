use actix::{Actor, Addr};
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;

use actix_ws_redis_protobuf::{Broadcast, Hub, ServerMessage, WsSession};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct MyObjJson {
    number: i32,
    name: String,
}

#[derive(Debug, serde::Serialize)]
struct JsonResponse<T> {
    status: &'static str,
    request: T,
}

async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    hub: web::Data<Addr<Hub>>,
) -> Result<HttpResponse, Error> {
    ws::start(WsSession::new(hub.get_ref().clone()), &req, stream)
}

async fn json_to_json_ws(hub: web::Data<Addr<Hub>>, body: web::Json<MyObjJson>) -> HttpResponse {
    let payload = match serde_json::to_string(&body.0) {
        Ok(s) => s,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    hub.do_send(Broadcast(ServerMessage::Text(payload)));
    HttpResponse::Ok().json(JsonResponse {
        status: "JSON broadcast to websocket clients",
        request: body.0,
    })
}

#[cfg(feature = "protobuf")]
mod protobuf_routes {
    use super::*;

    use bytes::Bytes;
    use prost::Message;

    use actix_ws_redis_protobuf::proto::MyObj;

    #[cfg(not(feature = "redis"))]
    async fn broadcast_and_maybe_publish(hub: &Addr<Hub>, bytes: Bytes) {
        hub.do_send(Broadcast(ServerMessage::Binary(bytes.clone())));
    }

    #[cfg(feature = "redis")]
    async fn broadcast_and_maybe_publish(
        hub: &Addr<Hub>,
        bytes: Bytes,
        redis: Option<web::Data<actix_ws_redis_protobuf::redis_bridge::RedisBridgeConfig>>,
    ) {
        hub.do_send(Broadcast(ServerMessage::Binary(bytes.clone())));

        if let Some(redis) = redis {
            let cfg = redis.get_ref().clone();
            tokio::spawn(async move {
                let _ = actix_ws_redis_protobuf::redis_bridge::publish_bytes(&cfg, bytes).await;
            });
        }
    }

    #[cfg(not(feature = "redis"))]
    pub async fn json_to_protobuf_ws(
        hub: web::Data<Addr<Hub>>,
        body: web::Json<MyObjJson>,
    ) -> HttpResponse {
        let msg = MyObj {
            number: body.0.number,
            name: body.0.name.clone(),
        };
        let bytes = Bytes::from(msg.encode_to_vec());

        broadcast_and_maybe_publish(hub.get_ref(), bytes).await;

        HttpResponse::Ok().json(JsonResponse {
            status: "JSON encoded to protobuf and broadcast as binary",
            request: body.0,
        })
    }

    #[cfg(feature = "redis")]
    pub async fn json_to_protobuf_ws(
        hub: web::Data<Addr<Hub>>,
        body: web::Json<MyObjJson>,
        redis: Option<web::Data<actix_ws_redis_protobuf::redis_bridge::RedisBridgeConfig>>,
    ) -> HttpResponse {
        let msg = MyObj {
            number: body.0.number,
            name: body.0.name.clone(),
        };
        let bytes = Bytes::from(msg.encode_to_vec());

        broadcast_and_maybe_publish(hub.get_ref(), bytes, redis).await;

        HttpResponse::Ok().json(JsonResponse {
            status: "JSON encoded to protobuf and broadcast as binary",
            request: body.0,
        })
    }

    #[cfg(not(feature = "redis"))]
    pub async fn protobuf_to_protobuf_ws(
        hub: web::Data<Addr<Hub>>,
        body: web::Bytes,
    ) -> HttpResponse {
        let bytes = Bytes::copy_from_slice(&body);
        if MyObj::decode(bytes.clone()).is_err() {
            return HttpResponse::BadRequest().finish();
        }

        broadcast_and_maybe_publish(hub.get_ref(), bytes.clone()).await;

        HttpResponse::Ok()
            .content_type("application/protobuf")
            .body(bytes)
    }

    #[cfg(feature = "redis")]
    pub async fn protobuf_to_protobuf_ws(
        hub: web::Data<Addr<Hub>>,
        body: web::Bytes,
        redis: Option<web::Data<actix_ws_redis_protobuf::redis_bridge::RedisBridgeConfig>>,
    ) -> HttpResponse {
        let bytes = Bytes::copy_from_slice(&body);
        if MyObj::decode(bytes.clone()).is_err() {
            return HttpResponse::BadRequest().finish();
        }

        broadcast_and_maybe_publish(hub.get_ref(), bytes.clone(), redis).await;

        HttpResponse::Ok()
            .content_type("application/protobuf")
            .body(bytes)
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()));
    env_logger::init();

    let hub = Hub::default().start();

    #[cfg(feature = "redis")]
    let redis_cfg = match (std::env::var("REDIS_URL"), std::env::var("REDIS_CHANNEL")) {
        (Ok(url), channel) => Some(actix_ws_redis_protobuf::redis_bridge::RedisBridgeConfig {
            url,
            channel: channel.unwrap_or_else(|_| "events".to_string()),
        }),
        (Err(_), _) => None,
    };

    #[cfg(feature = "redis")]
    if let Some(cfg) = redis_cfg.clone() {
        let _ = actix_ws_redis_protobuf::redis_bridge::spawn_redis_subscriber(cfg, hub.clone()).await;
    }

    HttpServer::new(move || {
        let app = App::new()
            .app_data(web::Data::new(hub.clone()))
            .route("/ws/", web::get().to(ws_handler))
            .route("/ws/json/json/stuff", web::post().to(json_to_json_ws));

        #[cfg(feature = "redis")]
        let app = if let Some(cfg) = redis_cfg.clone() {
            app.app_data(web::Data::new(cfg))
        } else {
            app
        };

        #[cfg(feature = "protobuf")]
        let app = app
            .route(
                "/ws/json/pb/stuff",
                web::post().to(protobuf_routes::json_to_protobuf_ws),
            )
            .route(
                "/ws/pb/pb/stuff",
                web::post().to(protobuf_routes::protobuf_to_protobuf_ws),
            );

        app
    })
    .bind(("127.0.0.1", 7070))?
    .run()
    .await
}
