

use std::time::{Instant, Duration};
use std::sync::Arc;

use actix::prelude::*;
use actix_web::{
    fs, http, middleware, server, Responder,
    ws, App, Error as AWError, HttpRequest, HttpResponse,
};
use actix::{
    Actor, ActorContext, AsyncContext,
    Handler, Recipient, StreamHandler,
};
use redis::{Commands, PubSubCommands, ControlFlow};

use crate::{AppState, MyObj};
use crate::subscriptions::{
    SubscriptionActor,
    SubscribeOnOff,
    SubscribeOnOffProto,
    BroadcastUpdate,
    BroadcastUpdateProto,
};
use crate::protobuf::{ProtoBufPayloadError};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct WebSocketActor {
    // Client needs to send ping every 20 secs at least
    hb: Instant,
    redis_client: Arc<redis::Client>,
    subscriptions: Recipient<SubscribeOnOffProto>,
}

impl WebSocketActor {
    pub fn new(subscriptions: Recipient<SubscribeOnOffProto>, redis_client: Arc<redis::Client>) -> Self {
        Self {
            hb: Instant::now(),
            redis_client: redis_client,
            subscriptions: subscriptions
        }
    }

    pub fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                warn!("Client Websocket timeout, failed to ping back in 30sec. Disconnecting.");
                ctx.stop();
            }
            ctx.ping("");
        });
    }
}


impl Actor for WebSocketActor {
    type Context = ws::WebsocketContext<Self, AppState>;

    // Init heartbeat
    fn started(&mut self, ctx: &mut ws::WebsocketContext<Self, AppState>) {
        // let msg = SubscribeOnOff::Subscribe(ctx.address().recipient());
        // self.subscriptions.do_send(msg).ok();

        let msg = SubscribeOnOffProto::Subscribe(ctx.address().recipient());
        self.subscriptions.do_send(msg).ok();
        // do_send(msg) to Recipient<SubscribeOnOff>
        self.hb(ctx);
    }

    fn stopped(&mut self, ctx: &mut ws::WebsocketContext<Self, AppState>) {
        // let msg = SubscribeOnOff::Unsubscribe(ctx.address().recipient());
        // self.subscriptions.do_send(msg).ok();

        let msg = SubscribeOnOffProto::Unsubscribe(ctx.address().recipient());
        self.subscriptions.do_send(msg).ok();
    }
}

impl Handler<BroadcastUpdate> for WebSocketActor {
    type Result = String;

    /// Handler for BroadcastUpdate messages. Broadcasts to web-socket clients.
    /// called when -> recipient<BroadcastUpdate>.do_send(msg.clone()).ok();

    fn handle(&mut self, msg: BroadcastUpdate, ctx: &mut ws::WebsocketContext<Self, AppState>) -> Self::Result {
        let BroadcastUpdate(vote) = msg;
        debug!("WebSocketActor => handle -> {:?}", &vote);
        if let Ok(data) = serde_json::to_string(&vote) {
            ctx.text(data); // Send data as text() via websocket
        };
        String::from(format!("{:?}", &vote))
    }
}

impl Handler<BroadcastUpdateProto> for WebSocketActor {
    type Result = String;

    fn handle(&mut self, msg: BroadcastUpdateProto, ctx: &mut ws::WebsocketContext<Self, AppState>) -> Self::Result {
        // Handle BroadcastUpdateProto binary message here
        let BroadcastUpdateProto(b) = msg;
        // let buf: &[u8] = b.as_ref();
        debug!("WebSocketActor => handle -> {:?}", &b);

        // if let Message::Binary(ref b) = msg {
        //   let response = Response::decode(v);
        // }

        ctx.binary(b.clone());
        String::from(format!("{:?}", b))
    }

}


impl StreamHandler<ws::Message, ws::ProtocolError> for WebSocketActor {

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




