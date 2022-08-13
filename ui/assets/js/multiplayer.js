export class Multiplayer {
    constructor() {
        // public
        this.game_id = null;
        this.on_created = (game_id) => {};
        this.on_opponent_join = (color) => {};
        this.on_opponent_move = (src_row, src_col, dst_row, dst_col) => {};
        this.color = null;

        // private
        this._ws = null;
    }

    create() {
        this.close();
        this._connect(`create`, (message) => {
            this.dispatch(message);
        });
    }

    join(game_id) {
        this.close();
        this._connect(`join/${game_id}`, (message) => {
            this.dispatch(message);
        });
    }

    dispatch(event) {
        console.log(`Received message: ${event.data}`);
        let data = JSON.parse(event.data);
        if (data.game_id) {
            // This message is received by the player creating the game. It
            // gives them the game ID which they can use to share a link with
            // another player.
            this.game_id = data.game_id;
            this.on_created(this.game_id);
        } else if (data.joined) {
            // This message is received by the player creating the game. They
            // should assign colors and send the other player their color.
            let white = Math.random() < 0.5;
            this.color = white ? "white" : "black";
            let other = white ? "black" : "white";
            let msg = JSON.stringify({
                color: other,
            });
            this._ws.send(msg);
            this.on_opponent_join(this.color);
        } else if (data.color) {
            // This message is received by the player not creating the game.
            // It tells them their color.
            this.color = data.color;
            this.on_opponent_join(this.color);
        } else if (data.src_row) {
            // This message is sent when the other player makes a move. It
            // should be validated and applied locally.
            this.on_opponent_move(
                data.src_row, data.src_col, data.dst_row, data.dst_col
            );
        }
    }

    on_move(src_row, src_col, dst_row, dst_col) {
        if (this._ws) {
            let data = JSON.stringify({
                src_row, src_col, dst_row, dst_col
            });
            this._ws.send(data);
        }
    }

    close() {
        if (this._ws) {
            this._ws.close();
            this._ws = null;
        }
    }

    _connect(path, onmessage) {
        let host = location.host;
        this._ws = new WebSocket(`wss://${host}/${path}`);
        this._ws.onmessage = onmessage;
        // Do this because wss:// isn't implemented in local dev
        this._ws.onerror = (evt) => {
            console.log("Trying ws");
            this._ws = new WebSocket(`ws://${host}/${path}`);
            this._ws.onmessage = onmessage;
        }
    }
}

export function init_multiplayer(on_move, get_player_color) {
    register_plugin = function (importObject) {
        importObject.env.on_move = on_move;
        importObject.env.get_player_color = get_player_color;
    };
    miniquad_add_plugin({register_plugin});
}
