use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast::error::RecvError;

use crate::AppState;

pub async fn handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| client_loop(socket, state))
}

async fn client_loop(socket: WebSocket, state: AppState) {
    let (mut tx, mut rx) = socket.split();
    let mut events = state.events.subscribe();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(25));
    heartbeat.tick().await; // first tick fires immediately

    if tx.send(Message::Text(r#"{"type":"HELLO"}"#.into())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            event = events.recv() => {
                let text = match event {
                    Ok(text) => text,
                    // Client fell behind: tell it to refetch the whole board.
                    Err(RecvError::Lagged(_)) => r#"{"type":"RESYNC"}"#.to_string(),
                    Err(RecvError::Closed) => break,
                };
                if tx.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            incoming = rx.next() => match incoming {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                _ => {} // pings are answered automatically; client messages are ignored
            },
            _ = heartbeat.tick() => {
                if tx.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }
}
