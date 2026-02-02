use std::collections::HashSet;

use actix::prelude::*;
use bytes::Bytes;

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub enum ServerMessage {
    Text(String),
    Binary(Bytes),
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub struct Broadcast(pub ServerMessage);

#[derive(Debug, Message)]
#[rtype(result = "()")]
pub enum HubCommand {
    Subscribe(Recipient<ServerMessage>),
    Unsubscribe(Recipient<ServerMessage>),
}

#[derive(Default)]
/// Broadcast hub that fans out messages to subscribed websocket sessions (or any `Recipient`).
pub struct Hub {
    subscribers: HashSet<Recipient<ServerMessage>>,
}

impl Actor for Hub {
    type Context = Context<Self>;
}

impl Handler<HubCommand> for Hub {
    type Result = ();

    fn handle(&mut self, msg: HubCommand, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            HubCommand::Subscribe(recipient) => {
                self.subscribers.insert(recipient);
            }
            HubCommand::Unsubscribe(recipient) => {
                self.subscribers.remove(&recipient);
            }
        }
    }
}

impl Handler<Broadcast> for Hub {
    type Result = ();

    fn handle(&mut self, msg: Broadcast, _ctx: &mut Self::Context) -> Self::Result {
        for recipient in &self.subscribers {
            recipient.do_send(msg.0.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use tokio::sync::mpsc;

    struct TestClient {
        tx: mpsc::UnboundedSender<ServerMessage>,
    }

    impl Actor for TestClient {
        type Context = Context<Self>;
    }

    impl Handler<ServerMessage> for TestClient {
        type Result = ();

        fn handle(&mut self, msg: ServerMessage, _ctx: &mut Self::Context) -> Self::Result {
            let _ = self.tx.send(msg);
        }
    }

    #[actix_rt::test]
    async fn broadcasts_to_subscribers() {
        let hub = Hub::default().start();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let client = TestClient { tx }.start();
        hub.do_send(HubCommand::Subscribe(client.recipient()));

        hub.do_send(Broadcast(ServerMessage::Text("hi".to_string())));

        let msg = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");

        match msg {
            ServerMessage::Text(s) => assert_eq!(s, "hi"),
            ServerMessage::Binary(_) => panic!("unexpected binary message"),
        }
    }
}
