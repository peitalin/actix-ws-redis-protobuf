

use std::time::{Instant, Duration};
use std::sync::Arc;

use actix::prelude::*;
use actix_web::{
    fs, http, middleware, server, Responder,
    ws, App, Error as AWError, HttpRequest, HttpResponse,
};
use actix::{Actor, ActorContext, AsyncContext, Handler, Recipient, StreamHandler};
use redis::{Commands, PubSubCommands, ControlFlow};

use crate::AppState;
use crate::listener::{
    ListenerActor, ListenControl, ListenUpdate
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct MyWebSocket {
    // Client needs to send ping every 20 secs at least
    hb: Instant,
    redis_client: Arc<redis::Client>,
    listener: Recipient<ListenControl>,
}
impl MyWebSocket {
    pub fn new(redis_client: Arc<redis::Client>, listener: Recipient<ListenControl>) -> Self {
        Self {
            hb: Instant::now(),
            redis_client: redis_client,
            listener: listener
        }
    }

    pub fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                println!("Client Websocket timeout, failed to ping back in 20sec. Disconnecting.");
                ctx.stop();
            }
            ctx.ping("");
        });
    }
}


impl Actor for MyWebSocket {
    type Context = ws::WebsocketContext<Self, AppState>;

    // Init heartbeat
    fn started(&mut self, ctx: &mut ws::WebsocketContext<Self, AppState>) {
        let msg = ListenControl::Subscribe(ctx.address().recipient());
        self.listener.do_send(msg).ok();
        self.hb(ctx);
    }

    fn stopped(&mut self, ctx: &mut ws::WebsocketContext<Self, AppState>) {
        let msg = ListenControl::Unsubscribe(ctx.address().recipient());
        self.listener.do_send(msg).ok();
    }
}

impl Handler<ListenUpdate> for MyWebSocket {
    type Result = ();

    fn handle(&mut self, msg: ListenUpdate, ctx: &mut ws::WebsocketContext<Self, AppState>) -> Self::Result {
        let ListenUpdate(vote) = msg;
        if let Ok(data) = serde_json::to_string(&vote) {
            ctx.text(data);
        }

    }
}

impl StreamHandler<ws::Message, ws::ProtocolError> for MyWebSocket {
    fn handle(&mut self, msg: ws::Message, ctx: &mut ws::WebsocketContext<Self, AppState>) {

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
                let conn = ctx.state().redis_client.get_connection().unwrap();
                // let conn = self.redis_client.get_connection().unwrap();
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




