use std::{
    env, fs,
    io::{self, Read},
};

fn main() {
    let input = match env::args().nth(1) {
        Some(path) => fs::read_to_string(path).expect("cannot read SearchJob file"),
        None => {
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .expect("cannot read SearchJob from stdin");
            input
        }
    };
    println!("{}", sp_kernel::solve_json(&input));
}
