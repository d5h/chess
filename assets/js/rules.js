class MovementRule {
    constructor(piece_ptr, placements_ptr, retval_ptr, retval_len) {
        let memory = wasm_memory.buffer;
        let piece_arr = new Uint8Array(memory, piece_ptr, 3);
        this.row = piece_arr[0];
        this.col = piece_arr[1];
        let piece_ascii = piece_arr[2];
        this.piece_name = String.fromCharCode(piece_ascii);
        this.stride = 8 + 1;  // Get these constants from Rust
        this.placements = new Uint8Array(memory, placements_ptr, (8 + 1) * this.stride);
        this.retval = new Uint8Array(memory, retval_ptr, retval_len);
        this.ri = 0;
        console.log(`Movement plugin called: (${this.row}, ${this.col}, ${this.piece_name})`);
    }

    piece_at(r, c) {
        // TODO: bounds check
        let piece_ascii = this.placements[r * this.stride + c];
        return piece_ascii !== 0 ? String.fromCharCode(piece_ascii) : null;
    }

    add_allowed_move(r, c, n) {
        // TODO: bounds check
        this.retval[this.ri] = r;
        this.retval[this.ri + 1] = c;
        this.retval[this.ri + 2] = n.charCodeAt(0);
        this.ri += 3;
    }
}

var rules = {
    movement_rule: (r) => {},
}

export function register_movement_rule(func) {
    rules.movement_rule = func;
}

export function init() {
    register_plugin = function (importObject) {
        importObject.env.movement_plugin = (piece_ptr, placements_ptr, retval_ptr, retval_len) => {
            let rule = new MovementRule(piece_ptr, placements_ptr, retval_ptr, retval_len);
            rules.movement_rule(rule);
        }
    };
    miniquad_add_plugin({register_plugin});
}
