#![deny(unsafe_code)]
#![deny(clippy::pedantic)]

mod chart;
mod cartographer;
mod dummyisland;

fn main() {
    cartographer::main();
}
