// Derived from https://github.com/seanmonstar/warp/blob/master/examples/websockets_chat.rs

use futures_util::{SinkExt, StreamExt, TryFutureExt};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};
use warp::{http, Filter, Reply};

// Need to add player color
type Player = mpsc::UnboundedSender<Message>;
type Game = HashMap<Uuid, Player>;
type Games = Arc<RwLock<HashMap<Uuid, Game>>>;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let games = Games::default();
    let games = warp::any().map(move || games.clone());

    // Create a game
    let create =
        warp::path("create")
            .and(warp::ws())
            .and(games.clone())
            .map(|ws: warp::ws::Ws, games| {
                ws.on_upgrade(move |websocket| create_game(websocket, games))
            });

    // Join a game
    let join = warp::path!("join" / String).and(warp::ws()).and(games).map(
        |game_id: String, ws: warp::ws::Ws, games| {
            if let Ok(game_id) = Uuid::parse_str(&game_id) {
                ws.on_upgrade(move |websocket| join_game(websocket, game_id, games))
                    .into_response()
            } else {
                eprintln!("invalid join ID: {}", game_id);
                warp::reply::with_status("Invalid game ID", http::StatusCode::BAD_REQUEST)
                    .into_response()
            }
        },
    );

    let ui = warp::path("ui").and(warp::fs::dir("/srv/chess"));

    let routes = ui.or(create).or(join);
    warp::serve(routes.with(warp::log("server")))
        .run(([0, 0, 0, 0], 58597))
        .await;
}

async fn create_game(ws: WebSocket, games: Games) {
    let game_id = Uuid::new_v4();
    let game = HashMap::new();
    games.write().await.insert(game_id, game);
    join_game(ws, game_id, games).await;
}

async fn join_game(ws: WebSocket, game_id: Uuid, games: Games) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);

    let player_id = Uuid::new_v4();
    {
        let mut w = games.write().await;
        if let Some(game) = w.get_mut(&game_id) {
            if game.is_empty() {
                // First player, send them the game ID
                let game_info = format!(r#"{{"game_id": "{}"}}"#, game_id);
                if let Err(_) = tx.send(Message::text(game_info)) {
                    // This should get handled below by player_disconnected.
                }
            }
            game.insert(player_id, tx);
        } else {
            eprintln!("non-existant game ID: {}", game_id);
            return;
        }
    }

    // Backgroud task that sends messages back to the client.
    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            ws_tx
                .send(message)
                .unwrap_or_else(|e| {
                    eprintln!("websocket send error: {}", e);
                })
                .await;
        }
    });

    // Receive messages from the client and forward them to other players.
    while let Some(result) = ws_rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!(
                    "websocket error(game_id={}, player_id={}): {}",
                    game_id, player_id, e
                );
                break;
            }
        };
        process_message(game_id, player_id, msg, &games).await;
    }

    // user_ws_rx stream will keep processing as long as the user stays
    // connected. Once they disconnect, then...
    player_disconnected(game_id, player_id, &games).await;
}

async fn process_message(game_id: Uuid, player_id: Uuid, msg: Message, games: &Games) {
    // Skip any non-Text messages...
    let msg = if let Ok(s) = msg.to_str() {
        s
    } else {
        return;
    };

    eprintln!(
        "websocket message(game_id={}, player_id={}): {}",
        game_id, player_id, msg
    );
    {
        let r = games.read().await;
        if let Some(game) = r.get(&game_id) {
            for (&pid, tx) in game.iter() {
                if pid != player_id {
                    if let Err(_disconnected) = tx.send(Message::text(msg.clone())) {}
                }
            }
        }
    }
}

async fn player_disconnected(game_id: Uuid, player_id: Uuid, games: &Games) {
    eprintln!("player disconnected(game_id={}): {}", game_id, player_id);

    {
        let mut w = games.write().await;
        if let Some(game) = w.get_mut(&game_id) {
            game.remove(&player_id);
            if game.is_empty() {
                eprintln!("all players left game: {}", game_id);
                w.remove(&game_id);
            }
        }
    }
}

// JS client:
// let ws = new WebSocket("ws://localhost:4001/echo");
// ws.onmessage = function (event) {
//   console.log(event.data);
// };
// ws.send("hello");
