use std::{net::SocketAddr, ops::ControlFlow};

use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket};
use futures::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};

use crate::{auth, config::config, state::*, types::*};

#[derive(Debug)]
pub struct WebsocketHandler {
    state: AppState,
    addr: SocketAddr,
}

impl WebsocketHandler {
    pub fn new(state: AppState, addr: SocketAddr) -> Self {
        Self { state, addr }
    }

    /// This handle will be spawned per connection.
    pub async fn handle(self, mut socket: WebSocket) {
        let (state, who) = (self.state, self.addr);
        let timeout_duration = std::time::Duration::from_secs(5);

        // Split the socket to get a sender and a receiver.
        let (sender, receiver) = socket.split();
        let (tx, rx) = tokio::sync::mpsc::channel::<Message>(1024);
        let _tx = tx.clone();

        // Spawn a task that will forward the message we receive through channel to the client.
        let mut send_task = tokio::spawn(Self::send_forwarder(sender, rx, who));

        // If any one of the tasks exit, abort the other.
        tokio::select! {
            rv_a = (&mut send_task) => {
                match rv_a {
                    Ok(_) => {},
                    Err(e) => tracing::error!("Error sending messages {e:?}")
                }
                recv_task.abort();
            },
            rv_b = (&mut recv_task) => {
                match rv_b {
                    Ok(_) => {},
                    Err(e) => tracing::error!("Error receiving messages {e:?}")
                }
                send_task.abort();
            }
            // _ = tokio::time::sleep(Duration::from_secs(5)) => {
            //     tracing::info!("closing connection with {who} after timeout");
            //     if let Err(e) = sender
            //         .send(Message::Close(Some(CloseFrame {
            //             code: axum::extract::ws::close_code::NORMAL,
            //             reason: Cow::from("timeout"),
            //         })))
            //         .await
            //     {
            //         tracing::error!("failed to send close frame to {who}: {e}");
            //     }
            // }
        }

        // returning from the handler means that either of the task was exited closes the websocket connection
        match subscription {
            Subscription::Client { .. } => {
                state.clients.write().await.remove(&who.to_string());
                tracing::info!("{who} disconnected");
            }
            Subscription::Node {
                api_key,
                exchange,
                public_ip,
                ..
            } => {
                let node_id = NodeId { api_key, exchange };
                let _ = state
                    .nodes
                    .write()
                    .await
                    .get_mut(&node_id)
                    .is_some_and(|node| {
                        node.active = false;
                        node.active
                    });
                tracing::info!("{node_id} from {public_ip} disconnected");

                // broadcast to all clients to let them know a node has disconnected.
                for (_, client) in state.clients.read().await.iter() {
                    let _ = client
                        .tx
                        .send(Message::Text(
                            serde_json::to_string(&WSMessage::NodeDisconnected {
                                exchange: node_id.exchange.clone(),
                                api_key: node_id.api_key.clone(),
                            })
                            .unwrap(),
                        ))
                        .await;
                }
            }
        }
    }

    /// helper to handle subscription.
    async fn handle_subscription(
        socket: &mut WebSocket,
        who: &SocketAddr,
        timeout_duration: std::time::Duration,
    ) -> Result<Subscription, String> {
        // receive the subscription message from client (will timeout if no response).
        let subscription = match tokio::time::timeout(timeout_duration, socket.recv()).await {
            Ok(Some(Ok(Message::Text(msg)))) => match serde_json::from_str::<Subscription>(&msg) {
                Ok(subscription) => subscription,
                Err(e) => {
                    return Err(format!(
                        "client {who} sent an invalid subscription message: {}",
                        e
                    ));
                }
            },
            Ok(Some(Ok(_))) => {
                return Err(format!("client {who} sent a message other than text"));
            }
            Ok(Some(Err(_))) => {
                return Err(format!("client {who} abruptly disconnected"));
            }
            Ok(None) => return Err(format!("")),
            Err(_) => {
                return Err(format!(
                    "did not receive subscription from {who} within {timeout_duration:?}"
                ));
            }
        };

        match subscription {
            Subscription::Client { ref access_token } => {
                // Validate token
                auth::auth0::decode_token(
                    access_token,
                    &auth::jwks(),
                    &config().jwks_path.to_str().unwrap_or("jwks.json"),
                    Some(&config().audience),
                )
                .map_err(|(_, err)| err)?;
            }
            Subscription::Node { .. } => {}
        }

        // send a response to the client
        if let Err(e) = socket
            .send(Message::Text(
                serde_json::to_string(&WSMessage::Subscription {
                    status: "connected".to_string(),
                    client_type: subscription.client_type(),
                })
                .unwrap(),
            ))
            .await
        {
            return Err(format!(
                "failed to send subscription response to {who}: {e}"
            ));
        };

        Ok(subscription)
    }

    async fn close(sender: &mut SplitSink<WebSocket, Message>, msg: &'static str) {
        if let Err(e) = sender
            .send(Message::Close(Some(CloseFrame {
                code: close_code::NORMAL,
                reason: std::borrow::Cow::from(msg),
            })))
            .await
        {
            tracing::error!("Could not send close frame due to {e}, probably it is ok?");
        }
    }

    /// helper to forward messages from channels to the underlying sink.
    async fn send_forwarder(
        mut sender: SplitSink<WebSocket, Message>,
        mut rx: tokio::sync::mpsc::Receiver<Message>,
        who: SocketAddr,
    ) {
        let retry_duration = std::time::Duration::from_millis(100);
        let mut retry_count = 0;
        const MAX_RETRIES: i32 = 5;

        'outer: loop {
            match rx.recv().await {
                Some(message) => {
                    // send message with retry
                    while let Err(e) = sender.send(message.clone()).await {
                        if retry_count == MAX_RETRIES {
                            tracing::error!("{MAX_RETRIES} retries exceeded while sending message, closing {who}");
                            break 'outer;
                        }
                        tracing::warn!(
                            "Failed sending message to {who}: {e}, retrying in {retry_duration:?}"
                        );
                        tokio::time::sleep(retry_duration).await;
                        retry_count += 1;
                    }
                    retry_count = 0;
                }
                None => {
                    tracing::warn!("mpsc channel is closed, sending close to {who}...");
                    Self::close(
                        &mut sender,
                        "Buffer was full on server-side, reconnect required",
                    )
                    .await;
                    break;
                }
            }
        }
    }

    /// helper to receive message from the stream and process them
    async fn recv_and_process(
        mut receiver: SplitStream<WebSocket>,
        state: AppState,
        who: SocketAddr,
    ) {
        'outer: loop {
            match receiver.next().await {
                None => {
                    tracing::warn!("stream from {who} was ended");
                    break;
                }
                Some(Err(err)) => {
                    tracing::error!("error occured while listening on {who}'s stream: {err}");
                    break;
                }
                Some(Ok(msg)) => {
                    if Self::process_message(&msg, who).is_break() {
                        break;
                    }

                    let msg = match msg {
                        Message::Text(t) => match serde_json::from_str::<WSMessage>(&t) {
                            Ok(msg) => msg,
                            Err(e) => {
                                tracing::error!("received malformed message: {e}");
                                tracing::error!("malformed message: {t}");
                                continue;
                            }
                        },
                        _ => continue,
                    };

                    let serialized_msg = serde_json::to_string(&msg).unwrap();
                    use WSMessage::*;
                    match msg {
                        Order { .. }
                        | SetLeverage { .. }
                        | Subscription { .. }
                        | NodeConnected { .. }
                        | NodeDisconnected { .. }
                        | AccountCreated { .. }
                        | AccountDeleted { .. } => {}
                        NodeError {
                            exchange,
                            api_key,
                            message,
                        } => {
                            let node_id = NodeId { api_key, exchange };
                            tracing::warn!("received error from node {node_id}: {message}");
                            for (_, client) in state.clients.read().await.iter() {
                                if let Err(e) =
                                    client.tx.send(Message::Text(serialized_msg.clone())).await
                                {
                                    tracing::error!("failed to send to {who}: {e}");
                                    break 'outer;
                                }
                            }
                        }
                        NodeNotification {
                            exchange,
                            api_key,
                            message,
                        } => {
                            let node_id = NodeId { api_key, exchange };
                            tracing::info!("received notification from node {node_id}: {message}");
                            for (_, client) in state.clients.read().await.iter() {
                                if let Err(e) =
                                    client.tx.send(Message::Text(serialized_msg.clone())).await
                                {
                                    tracing::error!("failed to send to {who}: {e}");
                                    break 'outer;
                                }
                            }
                        }
                        PositionUpdate {
                            exchange,
                            api_key,
                            data,
                        } => {
                            expand_update_state_and_broadcast!(positions, "position update", 'outer, (&state, who, serialized_msg, api_key, exchange, data));
                        }
                        BalanceUpdate {
                            exchange,
                            api_key,
                            data,
                        } => {
                            expand_update_state_and_broadcast!(balance, "balance update", 'outer, (&state, who, serialized_msg, api_key, exchange, data));
                        }
                        OpenOrdersUpdate {
                            exchange,
                            api_key,
                            data,
                        } => {
                            expand_update_state_and_broadcast!(open_orders, "open orders update", 'outer, (&state, who, serialized_msg, api_key, exchange, data));
                        }
                        LiquidationUpdate {
                            exchange,
                            api_key,
                            data,
                        } => {
                            expand_update_state_and_broadcast!(liquidations, "liquidation update", 'outer, (&state, who, serialized_msg, api_key, exchange, data));
                        }
                    }
                }
            }
        }
    }

    /// helper to print contents of messages to stdout. Has special treatment for Close.
    fn process_message(msg: &Message, who: SocketAddr) -> ControlFlow<(), ()> {
        match msg {
            Message::Text(t) => {
                tracing::debug!(">>> {who} sent str: {t:?}");
            }
            Message::Binary(d) => {
                tracing::debug!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
            }
            Message::Close(c) => {
                if let Some(cf) = c {
                    tracing::info!(
                        ">>> {} sent close with code {} and reason `{}`",
                        who,
                        cf.code,
                        cf.reason
                    );
                } else {
                    tracing::info!(">>> {who} somehow sent close message without CloseFrame");
                }
                return ControlFlow::Break(());
            }

            Message::Pong(v) => {
                tracing::debug!(">>> {who} sent pong with {v:?}");
            }
            // You should never need to manually handle Message::Ping, as axum's websocket library
            // will do so for you automagically by replying with Pong and copying the v according to
            // spec. But if you need the contents of the pings you can see them here.
            Message::Ping(v) => {
                tracing::debug!(">>> {who} sent ping with {v:?}");
            }
        }
        ControlFlow::Continue(())
    }
}
