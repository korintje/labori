use std::{path::PathBuf, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::{
    acquisition::Controller,
    error::{LaboriError, Result},
    model::{LiveEvent, SampleQuery, StartRequest, UserEventRequest},
    storage,
};

#[derive(Clone)]
struct AppState {
    controller: Controller,
    pool: SqlitePool,
    live: broadcast::Sender<LiveEvent>,
}

#[derive(Debug, Deserialize)]
struct SessionFilter {
    mode: Option<String>,
}

pub async fn serve(
    listen_addr: &str,
    web_root: &str,
    controller: Controller,
    pool: SqlitePool,
    live: broadcast::Sender<LiveEvent>,
) -> Result<()> {
    let state = Arc::new(AppState {
        controller,
        pool,
        live,
    });
    let root = PathBuf::from(web_root);
    let app = Router::new()
        .route("/api/status", get(status))
        .route("/api/measurements/start", post(start))
        .route("/api/measurements/stop", post(stop))
        .route("/api/sessions", get(sessions))
        .route("/api/sessions/:session_id", delete(remove_session))
        .route("/api/sessions/:session_id/samples", get(samples))
        .route(
            "/api/sessions/:session_id/events",
            get(events).post(add_event),
        )
        .route("/api/live", get(live_socket))
        .route_service("/", ServeFile::new(root.join("index.html")))
        .route_service("/multi", ServeFile::new(root.join("index-multi.html")))
        .nest_service("/assets", ServeDir::new(root.join("public")))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(listen_addr).await?;
    tracing::info!(%listen_addr, "labori web server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install shutdown signal handler");
    }
}

async fn status(State(state): State<Arc<AppState>>) -> Json<crate::model::MeasurementStatus> {
    Json(state.controller.status().await)
}

async fn start(
    State(state): State<Arc<AppState>>,
    Json(request): Json<StartRequest>,
) -> Result<impl IntoResponse> {
    Ok((
        StatusCode::CREATED,
        Json(state.controller.start(request).await?),
    ))
}

async fn stop(State(state): State<Arc<AppState>>) -> Result<Json<crate::model::MeasurementStatus>> {
    Ok(Json(state.controller.stop().await?))
}

async fn sessions(
    State(state): State<Arc<AppState>>,
    Query(filter): Query<SessionFilter>,
) -> Result<Json<Vec<crate::model::SessionSummary>>> {
    if let Some(mode) = filter.mode.as_deref() {
        if !["single", "single_log", "single_direct", "multi"].contains(&mode) {
            return Err(LaboriError::Invalid(
                "mode must be single, single_log, single_direct, or multi".into(),
            ));
        }
    }
    Ok(Json(
        storage::list_sessions(&state.pool, filter.mode.as_deref()).await?,
    ))
}

async fn samples(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<i64>,
    Query(query): Query<SampleQuery>,
) -> Result<Json<Vec<crate::model::Sample>>> {
    let query = query.bounded();
    Ok(Json(
        storage::read_samples(&state.pool, session_id, query.after_sequence, query.limit).await?,
    ))
}

async fn remove_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    let status = state.controller.status().await;
    if status.session_id == Some(session_id) {
        return Err(LaboriError::Busy);
    }
    storage::delete_session(&state.pool, session_id).await?;
    Ok(Json(json!({ "deleted": session_id })))
}

async fn events(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<i64>,
) -> Result<Json<Vec<crate::model::SessionEvent>>> {
    Ok(Json(storage::read_events(&state.pool, session_id).await?))
}

async fn add_event(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<i64>,
    Json(request): Json<UserEventRequest>,
) -> Result<Json<crate::model::SessionEvent>> {
    let request = request.validate()?;
    let status = state.controller.status().await;
    let at_sequence = request.at_sequence.unwrap_or(-1);
    let event = storage::insert_user_event(
        &state.pool,
        session_id,
        at_sequence,
        &request.kind,
        &request.message,
    )
    .await?;
    if status.session_id == Some(session_id) {
        let _ = state.live.send(LiveEvent::Notice {
            session_id,
            at_sequence,
            message: format!("{}: {}", event.kind, event.message),
        });
    }
    Ok(Json(event))
}

async fn live_socket(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let initial = LiveEvent::Status {
        status: state.controller.status().await,
    };
    if send_event(&mut sender, &initial).await.is_err() {
        return;
    }
    let mut events = state.live.subscribe();
    loop {
        tokio::select! {
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Ping(payload))) => {
                        if sender.send(Message::Pong(payload)).await.is_err() { break; }
                    }
                    Some(Ok(_)) => {}
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if send_event(&mut sender, &event).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        let notice = LiveEvent::Notice {
                            session_id: state.controller.status().await.session_id.unwrap_or_default(),
                            at_sequence: -1,
                            message: format!("live display skipped {count} events; recorded data is unaffected"),
                        };
                        if send_event(&mut sender, &notice).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn send_event(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    event: &LiveEvent,
) -> std::result::Result<(), ()> {
    let payload = serde_json::to_string(event).map_err(|_| ())?;
    sender.send(Message::Text(payload)).await.map_err(|_| ())
}
