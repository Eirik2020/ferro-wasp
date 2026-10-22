use heapless::Vec;

pub use alpha::input::validate as checked;

pub fn plain() -> Vec<u8, 4> {
    Vec::new()
}
