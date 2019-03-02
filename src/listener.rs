

use actix::{
    Actor,
    Context,
    Handler,
    Message,
    Recipient,
};
use std::collections::HashSet;
use crate::{ MyObj, AppState };
use actix_web::ws;
use futures::future::{Future, join_all};

//////////////////////////////
/// Listen Update Message
//////////////////////////////

#[derive(Debug, Clone)]
pub struct ListenUpdate(pub MyObj);

impl Message for ListenUpdate {
    type Result = ();
}

//////////////////////////////
/// Listener Actor
//////////////////////////////

#[derive(Clone)]
pub struct ListenerActor {
    listeners: HashSet<Recipient<ListenUpdate>>,
}
impl ListenerActor {
    pub fn new() -> Self {
        Self {
            listeners: HashSet::new(),
        }
    }
}
impl Actor for ListenerActor {
    type Context = Context<Self>;
}

impl Handler<ListenUpdate> for ListenerActor {
    type Result = ();

    fn handle(&mut self, msg: ListenUpdate, ctx: &mut Context<Self>) -> Self::Result {
        println!("no. listeners: {:?}", self.listeners.len());
        for listener in &self.listeners {
            listener.send(msg.clone());
        };
    }
}

//////////////////////////////
/// Controller Message for Actor
/// Adds and removes Actor Addr
//////////////////////////////

pub enum ListenControl {
    Subscribe(Recipient<ListenUpdate>),
    Unsubscribe(Recipient<ListenUpdate>),
}
impl Message for ListenControl {
    type Result = ();
}
impl Handler<ListenControl> for ListenerActor {
    type Result = ();

    fn handle(&mut self, msg: ListenControl, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ListenControl::Subscribe(listener) => {
                println!("Client joined");
                self.listeners.insert(listener);
            },
            ListenControl::Unsubscribe(listener) => {
                self.listeners.remove(&listener);
            }
        }
    }
}








