#![feature(io_slice_advance)]
// mod buffer;
// pub use buffer::Buffer;

#[cfg(feature = "ringbahn")]
mod ringbahn;

mod buffer;
mod mmap;

pub mod legacy;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
