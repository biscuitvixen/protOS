//! The browser test harness: one embedded page, one WebSocket.
//!
//! Each connection gets the current layout description, then every
//! frame the render thread publishes, and can send layout changes and
//! input values back. Frames go out with send-latest semantics: a
//! client that falls behind misses frames rather than building a queue.

pub mod protocol;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use bytes::Bytes;

use crate::app::{Control, Shared, inputs_info};
use crate::layout::atlas::Atlas;
use crate::layout::{Layout, presets};
use protocol::{ClientMessage, ServerMessage, encode_frame};

const INDEX_HTML: &str = include_str!("web/index.html");

/// Claim the port. Done before the GPU is touched so a bind failure
/// exits cleanly with no render thread to tear down.
pub async fn bind(addr: SocketAddr) -> anyhow::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| anyhow::anyhow!("cannot listen on {addr}: {e}"))
}

pub async fn serve(listener: tokio::net::TcpListener, shared: Arc<Shared>) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_upgrade))
        .with_state(shared);
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(shared): State<Arc<Shared>>,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        tracing::info!(%peer, "client connected");
        connection(socket, shared).await;
        tracing::info!(%peer, "client disconnected");
    })
    .into_response()
}

async fn connection(mut socket: WebSocket, shared: Arc<Shared>) {
    let mut frames = shared.frames.subscribe();
    let mut layouts = shared.layout.subscribe();
    // Deliver the current layout before any frame, then mark the frame
    // receiver so the first loop iteration does not resend a stale one.
    let current = layouts.borrow_and_update().clone();
    if send_json(&mut socket, &ServerMessage::Layout(&current))
        .await
        .is_err()
    {
        return;
    }
    let inputs = inputs_info(&shared.inputs.lock().expect("input store lock"));
    if send_json(&mut socket, &ServerMessage::Inputs { inputs })
        .await
        .is_err()
    {
        return;
    }
    frames.mark_unchanged();
    let mut generation = current.generation;
    loop {
        tokio::select! {
            changed = layouts.changed() => {
                if changed.is_err() { return; }
                let info = layouts.borrow_and_update().clone();
                generation = info.generation;
                if send_json(&mut socket, &ServerMessage::Layout(&info)).await.is_err() { return; }
            }
            changed = frames.changed() => {
                if changed.is_err() { return; }
                let frame = frames.borrow_and_update().clone();
                let bytes = Bytes::from(encode_frame(&frame, generation));
                if socket.send(Message::Binary(bytes)).await.is_err() { return; }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        tracing::debug!(message = %text.as_str(), "from client");
                        if let Some(reply) = handle_client_message(text.as_str(), &shared)
                            && send_json(&mut socket, &reply).await.is_err()
                        {
                            return;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}

async fn send_json(socket: &mut WebSocket, message: &ServerMessage<'_>) -> Result<(), axum::Error> {
    let text = serde_json::to_string(message).expect("server messages serialise");
    socket.send(Message::Text(text.into())).await
}

/// Apply one message from the page; an error reply goes back to that
/// client only.
fn handle_client_message(text: &str, shared: &Shared) -> Option<ServerMessage<'static>> {
    let message: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            return Some(ServerMessage::Error {
                message: format!("bad message: {e}"),
            });
        }
    };
    let reply = apply_client_message(message, shared);
    if let Some(ServerMessage::Error { message }) = &reply {
        tracing::warn!(%message, "rejected client message");
    }
    reply
}

fn apply_client_message(message: ClientMessage, shared: &Shared) -> Option<ServerMessage<'static>> {
    match message {
        ClientMessage::Layout { preset, layout } => {
            let layout: Layout = match (preset, layout) {
                (Some(name), _) => match presets::load(&name) {
                    Some(l) => l,
                    None => {
                        return Some(ServerMessage::Error {
                            message: format!("no preset named {name:?}"),
                        });
                    }
                },
                (None, Some(l)) => l,
                (None, None) => {
                    return Some(ServerMessage::Error {
                        message: "layout message needs a preset or a layout".into(),
                    });
                }
            };
            if let Err(e) = Atlas::build(&layout) {
                return Some(ServerMessage::Error {
                    message: e.to_string(),
                });
            }
            let sent = shared
                .control
                .lock()
                .expect("control channel lock")
                .send(Control::SetLayout(layout));
            sent.err().map(|_| ServerMessage::Error {
                message: "render thread has stopped".into(),
            })
        }
        ClientMessage::Input { name, value } => match crate::contract::lookup_name(&name) {
            Some(id) => {
                shared
                    .inputs
                    .lock()
                    .expect("input store lock")
                    .set(id, value, Instant::now());
                None
            }
            None => Some(ServerMessage::Error {
                message: format!("unknown input {name:?}"),
            }),
        },
        ClientMessage::Reset => {
            shared.inputs.lock().expect("input store lock").reset();
            None
        }
    }
}
