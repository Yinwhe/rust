#![feature(box_syntax)]

// EMIT_MIR uniform_array_move_out.move_out_from_end.built.after.mir
fn move_out_from_end() {
    let a = [box 1, box 2];
    let [.., _y] = a;
}

// EMIT_MIR uniform_array_move_out.move_out_by_subslice.built.after.mir
fn move_out_by_subslice() {
    let a = [box 1, box 2];
    let [_y @ ..] = a;
}

fn main() {
    move_out_by_subslice();
    move_out_from_end();
}
