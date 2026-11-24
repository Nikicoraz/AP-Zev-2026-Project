#![deny(unsafe_code)]
#![deny(clippy::pedantic)]

mod chart;
mod cartographer;

fn main() {
    cartographer::main();
}
