use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web_actors::ws;

use crate::hub::{Broadcast, HubCommand, ServerMessage};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct WsSession {
    hb: Instant,
    hub: Addr<crate::Hub>,
}

impl WsSession {
    /// Create a new websocket session that subscribes to `hub` and broadcasts incoming client
    /// messages (`Text`/`Binary`) back to the hub.
    pub fn new(hub: Addr<crate::Hub>) -> Self {
        Self {
            hb: Instant::now(),
            hub,
        }
    }

    fn start_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.hub
            .do_send(HubCommand::Subscribe(ctx.address().recipient()));
        self.start_heartbeat(ctx);
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        self.hub
            .do_send(HubCommand::Unsubscribe(ctx.address().recipient()));
    }
}

impl Handler<ServerMessage> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: ServerMessage, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ServerMessage::Text(text) => ctx.text(text),
            ServerMessage::Binary(bytes) => ctx.binary(bytes),
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, item: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = match item {
            Ok(m) => m,
            Err(_) => {
                ctx.stop();
                return;
            }
        };

        match msg {
            ws::Message::Ping(bytes) => {
                self.hb = Instant::now();
                ctx.pong(&bytes);
            }
            ws::Message::Pong(_) => {
                self.hb = Instant::now();
            }
            ws::Message::Text(text) => {
                self.hb = Instant::now();
                self.hub
                    .do_send(Broadcast(ServerMessage::Text(text.to_string())));
            }
            ws::Message::Binary(bytes) => {
                self.hb = Instant::now();
                self.hub
                    .do_send(Broadcast(ServerMessage::Binary(bytes)));
            }
            ws::Message::Close(reason) => {
                ctx.close(reason);
                ctx.stop();
            }
            ws::Message::Continuation(_) => ctx.stop(),
            ws::Message::Nop => {}
        }
    }
}
