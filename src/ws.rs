use std::collections::HashMap;
use std::sync::Mutex;

use axum::extract::ws::{Message as WsFrame, WebSocket};
use axum::extract::{Request, State, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::{
    auth,
    error::AppError,
    models::{MessageOut, MessageRow},
    AppState,
};

/// Fan-out registry: user id -> live socket senders (multiple tabs allowed).
pub struct WsHub {
    clients: Mutex<HashMap<i32, Vec<UnboundedSender<Value>>>>,
}

impl std::fmt::Debug for WsHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size = self.lock().len();
        f.debug_struct("WsHub").field("users", &size).finish()
    }
}

impl WsHub {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<i32, Vec<UnboundedSender<Value>>>> {
        self.clients.lock().expect("ws hub poisoned")
    }

    fn register(&self, user_id: i32, tx: UnboundedSender<Value>) {
        self.lock().entry(user_id).or_default().push(tx);
    }

    fn unregister(&self, user_id: i32, tx: &UnboundedSender<Value>) {
        let mut clients = self.lock();
        if let Some(list) = clients.get_mut(&user_id) {
            list.retain(|other| !other.same_channel(tx) && !other.is_closed());
            if list.is_empty() {
                clients.remove(&user_id);
            }
        }
    }

    /// Whether the user has at least one live socket (prunes dead senders).
    pub fn is_online(&self, user_id: i32) -> bool {
        let mut clients = self.lock();
        if let Some(list) = clients.get_mut(&user_id) {
            list.retain(|tx| !tx.is_closed());
            if list.is_empty() {
                clients.remove(&user_id);
                return false;
            }
            return true;
        }
        false
    }

    /// Deliver an event to every live socket of a user; prunes dead senders.
    pub fn send_to(&self, user_id: i32, event: Value) {
        let mut clients = self.lock();
        if let Some(list) = clients.get_mut(&user_id) {
            list.retain(|tx| tx.send(event.clone()).is_ok());
            if list.is_empty() {
                clients.remove(&user_id);
            }
        }
    }
}

/// GET /api/ws — cookie-authenticated chat socket.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, AppError> {
    let jar = axum_extra::extract::CookieJar::from_headers(request.headers());
    let user = auth::user_from_jar(&state, &jar).await?;
    let user_id = user.id;
    Ok(ws
        .on_upgrade(move |socket| handle_socket(socket, state, user_id))
        .into_response())
}

#[derive(serde::Deserialize)]
struct ClientEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    conversation_id: Option<i32>,
    #[serde(default)]
    body: Option<String>,
}

async fn handle_socket(socket: WebSocket, state: AppState, user_id: i32) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = unbounded_channel::<Value>();
    state.hub.register(user_id, out_tx.clone());
    mark_incoming_delivered(&state, user_id).await;

    let send_task = tokio::spawn(async move {
        while let Some(event) = out_rx.recv().await {
            if ws_tx.send(WsFrame::Text(event.to_string().into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(frame) = ws_rx.next().await {
        let text = match frame {
            Ok(WsFrame::Text(text)) => text,
            Ok(WsFrame::Close(_)) | Err(_) => break,
            Ok(_) => continue,
        };
        match serde_json::from_str::<ClientEvent>(&text) {
            Ok(event) => handle_client_event(&state, user_id, event).await,
            Err(_) => {
                state
                    .hub
                    .send_to(user_id, json!({"type":"error","detail":"Bad message"}));
            }
        }
    }

    state.hub.unregister(user_id, &out_tx);
    send_task.abort();
}

async fn handle_client_event(state: &AppState, user_id: i32, event: ClientEvent) {
    match event.kind.as_str() {
        "ping" => {
            state.hub.send_to(user_id, json!({"type":"pong"}));
        }
        "send" => {
            let (conv_id, body) = match (event.conversation_id, event.body) {
                (Some(id), Some(body)) => (id, body.trim().to_owned()),
                _ => {
                    state
                        .hub
                        .send_to(user_id, json!({"type":"error","detail":"Bad message"}));
                    return;
                }
            };
            if body.is_empty() || body.len() > 2000 {
                state.hub.send_to(
                    user_id,
                    json!({"type":"error","detail":"Message must be 1-2000 characters"}),
                );
                return;
            }
            match send_message(state, user_id, conv_id, &body).await {
                Ok((message, recipient_id)) => {
                    let event = json!({"type":"message","message":message});
                    state.hub.send_to(recipient_id, event.clone());
                    if recipient_id != user_id {
                        state.hub.send_to(user_id, event);
                    }
                }
                Err(detail) => {
                    state
                        .hub
                        .send_to(user_id, json!({"type":"error","detail":detail}));
                }
            }
        }
        "typing" => {
            let Some(conv_id) = event.conversation_id else {
                state
                    .hub
                    .send_to(user_id, json!({"type":"error","detail":"Bad message"}));
                return;
            };
            match conversation_peer(state, conv_id, user_id).await {
                Ok(other) => {
                    state.hub.send_to(
                        other,
                        json!({"type":"typing","conversation_id":conv_id,"user_id":user_id}),
                    );
                }
                Err(detail) => {
                    state
                        .hub
                        .send_to(user_id, json!({"type":"error","detail":detail}));
                }
            }
        }
        "read" => {
            let Some(conv_id) = event.conversation_id else {
                state
                    .hub
                    .send_to(user_id, json!({"type":"error","detail":"Bad message"}));
                return;
            };
            match conversation_peer(state, conv_id, user_id).await {
                Ok(other) => {
                    let result = sqlx::query(
                        "UPDATE messages SET read_at = now() \
                         WHERE conversation_id = $1 AND sender_id <> $2 AND read_at IS NULL",
                    )
                    .bind(conv_id)
                    .bind(user_id)
                    .execute(&state.pool)
                    .await;
                    if let Err(e) = result {
                        tracing::error!(error = %e, "failed to mark messages read");
                    } else {
                        state
                            .hub
                            .send_to(other, json!({"type":"read","conversation_id":conv_id}));
                    }
                }
                Err(detail) => {
                    state
                        .hub
                        .send_to(user_id, json!({"type":"error","detail":detail}));
                }
            }
        }
        other => {
            state.hub.send_to(
                user_id,
                json!({"type":"error","detail":format!("Unknown type {other}")}),
            );
        }
    }
}

/// The conversation's other participant; enforces membership.
async fn conversation_peer(
    state: &AppState,
    conversation_id: i32,
    user_id: i32,
) -> Result<i32, String> {
    let row: Option<(i32, i32)> =
        sqlx::query_as("SELECT user_a, user_b FROM conversations WHERE id = $1")
            .bind(conversation_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "conversation lookup failed");
                "Internal server error".to_owned()
            })?;
    match row {
        Some((a, b)) if a == user_id => Ok(b),
        Some((a, b)) if b == user_id => Ok(a),
        Some(_) => Err("Not a participant in this conversation".to_owned()),
        None => Err("Conversation not found".to_owned()),
    }
}

async fn send_message(
    state: &AppState,
    user_id: i32,
    conversation_id: i32,
    body: &str,
) -> Result<(MessageOut, i32), String> {
    let recipient = conversation_peer(state, conversation_id, user_id).await?;
    if crate::routes::blocks::is_blocked_either(&state.pool, user_id, recipient)
        .await
        .unwrap_or(false)
    {
        return Err("You cannot message this user".to_owned());
    }
    // A message pushed straight into a live socket counts as delivered.
    let delivered = state.hub.is_online(recipient);
    let row = sqlx::query_as::<_, MessageRow>(
        "INSERT INTO messages (conversation_id, sender_id, body, delivered_at) \
         VALUES ($1, $2, $3, CASE WHEN $4 THEN now() ELSE NULL END) \
         RETURNING id, conversation_id, sender_id, body, read_at, delivered_at, created_at",
    )
    .bind(conversation_id)
    .bind(user_id)
    .bind(body)
    .bind(delivered)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to insert message");
        "Internal server error".to_owned()
    })?;

    // Bell the recipient once per burst: only when they had no unread
    // messages in this conversation before this one.
    let unread_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages \
         WHERE conversation_id = $1 AND sender_id <> $2 AND read_at IS NULL AND id <> $3",
    )
    .bind(conversation_id)
    .bind(recipient)
    .bind(row.id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);
    if unread_before == 0 {
        crate::notify::notify(
            state,
            recipient,
            user_id,
            "message",
            Some("conversation"),
            Some(conversation_id),
        )
        .await;
        // Best-effort email mirrors the same burst (only when they had nothing unread).
        let recipient_info = sqlx::query_as::<_, (String, bool)>(
            "SELECT email, email_messages_enabled FROM users WHERE id = $1",
        )
        .bind(recipient)
        .fetch_optional(&state.pool)
        .await;
        if let Ok(Some((email, true))) = recipient_info {
            let sender_name = sqlx::query_scalar::<_, String>(
                "SELECT username FROM users WHERE id = $1",
            )
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
            if let Some(sender_name) = sender_name {
                if let Err(err) = crate::email::send_message_email(&state.config, &email, &sender_name, conversation_id).await {
                    tracing::warn!(error = %err, recipient_email = %email, "message email failed");
                }
            }
        }
    }

    Ok((MessageOut::from(row), recipient))
}

/// Called when a user's socket comes online: mark everything sent to them as
/// delivered and tell the other side of each conversation so their UI updates.
async fn mark_incoming_delivered(state: &AppState, user_id: i32) {
    let convs: Vec<(i32, i32, i32)> = sqlx::query_as(
        "SELECT DISTINCT c.id, c.user_a, c.user_b FROM conversations c \
         JOIN messages m ON m.conversation_id = c.id \
         WHERE m.delivered_at IS NULL AND m.sender_id <> $1 \
           AND (c.user_a = $1 OR c.user_b = $1)",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    if convs.is_empty() {
        return;
    }
    let result = sqlx::query(
        "UPDATE messages SET delivered_at = now() \
         WHERE delivered_at IS NULL AND sender_id <> $1 \
           AND conversation_id IN \
             (SELECT id FROM conversations WHERE user_a = $1 OR user_b = $1)",
    )
    .bind(user_id)
    .execute(&state.pool)
    .await;
    if let Err(e) = result {
        tracing::error!(error = %e, "failed to mark messages delivered");
        return;
    }
    for (conv_id, user_a, user_b) in convs {
        let peer = if user_a == user_id { user_b } else { user_a };
        state.hub.send_to(peer, json!({"type":"delivered","conversation_id":conv_id}));
    }
}
