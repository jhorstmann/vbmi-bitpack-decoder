use std::mem::MaybeUninit;

mod table;
mod vbmi;

pub use vbmi::*;

#[derive(Debug)]
pub enum DecoderError {
    InvalidBitwidth(usize),
    BufferUnderrun { available: usize, required: usize },
}

pub trait Decoder {
    fn decode(
        &mut self,
        input: &[u8],
        bitwidth: usize,
        output: &mut [MaybeUninit<u16>],
    ) -> Result<usize, DecoderError>;
}
