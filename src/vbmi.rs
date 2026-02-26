use crate::table::TABLE_U16;
use crate::{Decoder, DecoderError};
use std::arch::x86_64::{
    __m512i, _mm512_and_si512, _mm512_loadu_epi8, _mm512_mask_storeu_epi16,
    _mm512_maskz_loadu_epi8, _mm512_multishift_epi64_epi8, _mm512_permutexvar_epi8,
    _mm512_set1_epi16, _mm512_storeu_epi16,
};
use std::mem::MaybeUninit;

#[derive(Default, Debug)]
pub struct BitpackDecoderVBMI;

impl BitpackDecoderVBMI {
    #[inline(always)]
    unsafe fn unpack16_step<const MASKED: bool>(
        &self,
        x: __m512i,
        y: __m512i,
        bitwidth: usize,
        input_ptr: *const u8,
        input_len: usize,
    ) -> __m512i {
        unsafe {
            let load_mask = u64::MAX >> (64 - 4 * bitwidth);
            let mask = _mm512_set1_epi16((u16::MAX >> (16 - bitwidth)) as i16);

            let load_mask = if MASKED {
                load_mask >> ((4 * bitwidth).saturating_sub(input_len))
            } else {
                load_mask
            };

            let data = _mm512_maskz_loadu_epi8(load_mask, input_ptr.cast());
            let unpacked = _mm512_multishift_epi64_epi8(y, _mm512_permutexvar_epi8(x, data));
            _mm512_and_si512(unpacked, mask)
        }
    }
}

impl Decoder for BitpackDecoderVBMI {
    fn decode(
        &mut self,
        input: &[u8],
        bitwidth: usize,
        output: &mut [MaybeUninit<u16>],
    ) -> Result<usize, DecoderError> {
        if bitwidth > 16 {
            return Err(DecoderError::InvalidBitwidth(bitwidth));
        }

        let num_elements = output.len();
        let input_len = input.len();

        if (input_len * 8) < num_elements * bitwidth {
            return Err(DecoderError::BufferUnderrun {
                required: (num_elements * bitwidth).div_ceil(8),
                available: input_len,
            });
        }

        let input_ptr = input.as_ptr();
        let output_ptr = output.as_mut_ptr().cast::<i16>();

        let out_elems_per_step = 32;
        let in_bytes_per_step = bitwidth * 4;

        let mut bytes = 0;
        let mut elems = 0;

        unsafe {
            let x = _mm512_loadu_epi8(TABLE_U16[bitwidth].0.as_ptr().cast());
            let y = _mm512_loadu_epi8(TABLE_U16[bitwidth].1.as_ptr().cast());

            while elems + out_elems_per_step * 4 <= num_elements {
                for i in 0..4 {
                    let unpacked = self.unpack16_step::<false>(
                        x,
                        y,
                        bitwidth,
                        input_ptr.add(bytes + i * in_bytes_per_step),
                        in_bytes_per_step,
                    );
                    _mm512_storeu_epi16(output_ptr.add(elems + i * out_elems_per_step), unpacked);
                }

                bytes += 4 * in_bytes_per_step;
                elems += 4 * out_elems_per_step;
            }
            while elems + out_elems_per_step <= num_elements {
                let unpacked = self.unpack16_step::<false>(
                    x,
                    y,
                    bitwidth,
                    input_ptr.add(bytes),
                    in_bytes_per_step,
                );
                _mm512_storeu_epi16(output_ptr.add(elems), unpacked);

                bytes += in_bytes_per_step;
                elems += out_elems_per_step;
            }
            if elems < num_elements {
                let rem_bytes = input_len - bytes;
                let rem_elems = num_elements - elems;
                let unpacked =
                    self.unpack16_step::<true>(x, y, bitwidth, input_ptr.add(bytes), rem_bytes);
                let store_mask = u32::MAX >> (out_elems_per_step.saturating_sub(rem_elems) as u32);
                _mm512_mask_storeu_epi16(output_ptr.add(elems).cast(), store_mask, unpacked);

                elems += rem_elems;
            }
        }

        Ok(elems)
    }
}

#[cfg(test)]
mod tests {
    use crate::Decoder;
    use crate::vbmi::BitpackDecoderVBMI;

    fn decode8(input: &[u8]) {
        let mut decoder = BitpackDecoderVBMI::default();
        let expected = input.iter().copied().map(u16::from).collect::<Vec<_>>();

        let mut output = Vec::with_capacity(expected.len());

        let len = decoder
            .decode(input, 8, output.spare_capacity_mut())
            .unwrap();
        unsafe {
            output.set_len(len);
        }
        assert_eq!(&output, &expected);
    }

    #[test]
    fn test_remainder() {
        let input = (0..8).collect::<Vec<u8>>();
        for i in 1..input.len() {
            decode8(&input[..i]);
        }
    }

    #[test]
    fn test_small_batches_1() {
        let input = (0..64).collect::<Vec<u8>>();
        for i in 2..input.len() {
            decode8(&input[..i]);
        }
    }
    #[test]
    fn test_large() {
        let input = (0..1024).map(|i| (i % 256) as u8).collect::<Vec<u8>>();
        decode8(&input);
    }
}
