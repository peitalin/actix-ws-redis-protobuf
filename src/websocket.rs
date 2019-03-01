

use std::time::{Instant, Duration};
use actix::prelude::*;
use actix_web::{
    fs, http, middleware, server,
    ws, App, Error as AWError, HttpRequest, HttpResponse,
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(60);

use crate::AppState;
use std::sync::Arc;
use redis::{Commands, PubSubCommands, ControlFlow};


pub struct MyWebSocket {
    // Client needs to send ping every 20 secs at least
    hb: Instant,
    redis_client: Arc<redis::Client>,
    count: i32,
}

impl Actor for MyWebSocket {
    type Context = ws::WebsocketContext<Self, AppState>;

    // Init heartbeat
    fn started(&mut self, ctx: &mut Self::Context) {
        // let mut conn = self.redis_client.get_connection().unwrap();
        // let redis1 = conn.subscribe(&["events"], |msg| {
        //     // increment messages seen counter
        //     self.count += 1;
        //     match self.count {
        //         // stop after receiving 10 messages
        //         10 => ControlFlow::Break(()),
        //         _ => ControlFlow::Continue,
        //     }
        //
        // });
        self.hb(ctx);
    }
}

impl StreamHandler<ws::Message, ws::ProtocolError> for MyWebSocket {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {

        // process websocket
        match msg {
            ws::Message::Ping(ping) => {
                println!("Ping: {:?}", &ping);
                self.hb = Instant::now();
                ctx.pong(&ping);
            },
            ws::Message::Pong(pong) => {
                println!("Pong: {:?}", pong);
                self.hb = Instant::now();
            },
            ws::Message::Text(text) => {
                println!("Text: {:?}", text);
                let conn = self.redis_client.get_connection().unwrap();
                let redis1: redis::RedisResult<i32> = conn.publish("events", &text);
                println!("Redis Result: {:?}", redis1);
                ctx.text(text)
            },
            ws::Message::Binary(bin) => {
                println!("Binary: {:?}", bin);
                ctx.binary(bin)
            },
            ws::Message::Close(close) => {
                println!("Close: {:?}", close);
                ctx.stop();
            }
        }
    }
}

impl MyWebSocket {

    pub fn new(redis_client: Arc<redis::Client>) -> Self {
        Self {
            hb: Instant::now(),
            redis_client: redis_client,
            count: 0,
        }
    }

    pub fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeatsk
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                println!("Client Websocket timeout, failed to ping back in 20sec. Disconnecting.");
                ctx.stop();
            }
            ctx.ping("");
        });
    }
}




