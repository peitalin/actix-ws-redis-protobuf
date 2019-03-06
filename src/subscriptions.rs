//////////////////////////////
/// Naming Conventions:
///
/// 1. Actors are nouns
///     - "SubscriptionActor"
///     - "WebSocketActor"
///
/// 2. Messages are verbs
///     - "BroadcastUpdate" -> Update
///     - "SubscribeOnOff" -> Subscribe
///
//////////////////////////////

use actix::{
    Actor,
    Context,
    Handler,
    Message,
    Recipient,
    StreamHandler,
};
use actix_web::ws;
use std::collections::HashSet;
use crate::{ MyObj, AppState };
use futures::future::{Future, join_all};


//////////////////////////////
/// Broadcast Update Message
//////////////////////////////

#[derive(Debug, Clone)]
pub struct BroadcastUpdate(pub MyObj);

impl Message for BroadcastUpdate {
    type Result = String;
}

#[derive(Debug, Clone)]
pub struct BroadcastUpdateProto(pub bytes::Bytes);

impl Message for BroadcastUpdateProto {
    type Result = String;
}

//////////////////////////////
/// Subscription Actor
/// Holds the addr of websocket clients
//////////////////////////////

#[derive(Clone)]
pub struct SubscriptionActor {
    subscribers: HashSet<Recipient<BroadcastUpdate>>,
    subscribers_proto: HashSet<Recipient<BroadcastUpdateProto>>,
}
impl SubscriptionActor {
    pub fn new() -> Self {
        Self {
            subscribers: HashSet::new(),
            subscribers_proto: HashSet::new(),
        }
    }
}
impl Actor for SubscriptionActor {
    type Context = Context<Self>;
}

impl Handler<BroadcastUpdate> for SubscriptionActor {
    type Result = String;

    fn handle(&mut self, msg: BroadcastUpdate, ctx: &mut Context<Self>) -> Self::Result {
        debug!("no. listeners: {:?}", &self.subscribers.len());
        debug!("msg: {:?}", &msg.0);
        for recipient in &self.subscribers {
            recipient.do_send(msg.clone()).ok();
        };
        String::from("Message successfully broadcasted to websocket clients")
    }
}

impl Handler<BroadcastUpdateProto> for SubscriptionActor {
    type Result = String;

    fn handle(&mut self, msg: BroadcastUpdateProto, ctx: &mut Context<Self>) -> Self::Result {
        debug!("no. listeners proto: {:?}", &self.subscribers_proto.len());
        debug!("msg proto: {:?}", &msg.0);
        for recipient in &self.subscribers_proto {
            recipient.do_send(msg.clone()).ok();
        };
        String::from("Protobuf Message successfully broadcasted to websocket clients")
    }
}



//////////////////////////////
/// Controller Message for Actor
/// Adds and removes Actors from
/// pool of Websocket clients
//////////////////////////////

pub enum SubscribeOnOff {
    Subscribe(Recipient<BroadcastUpdate>),
    Unsubscribe(Recipient<BroadcastUpdate>),
}
impl Message for SubscribeOnOff {
    type Result = ();
}
impl Handler<SubscribeOnOff> for SubscriptionActor {
    type Result = ();

    fn handle(&mut self, msg: SubscribeOnOff, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            SubscribeOnOff::Subscribe(addr) => {
                debug!("Client joined");
                self.subscribers.insert(addr);
            },
            SubscribeOnOff::Unsubscribe(addr) => {
                debug!("Client left");
                self.subscribers.remove(&addr);
            }
        }
    }
}

pub enum SubscribeOnOffProto {
    Subscribe(Recipient<BroadcastUpdateProto>),
    Unsubscribe(Recipient<BroadcastUpdateProto>),
}
impl Message for SubscribeOnOffProto {
    type Result = ();
}
impl Handler<SubscribeOnOffProto> for SubscriptionActor {
    type Result = ();

    fn handle(&mut self, msg: SubscribeOnOffProto, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            SubscribeOnOffProto::Subscribe(addr) => {
                debug!("Client joined proto");
                self.subscribers_proto.insert(addr);
            },
            SubscribeOnOffProto::Unsubscribe(addr) => {
                debug!("Client left proto");
                self.subscribers_proto.remove(&addr);
            }
        }
    }
}


