#![deny(unsafe_code)]
#![deny(clippy::pedantic)]

mod cartographer;
mod chart;
mod dummyisland;

fn main() {
    cartographer::main();
}
