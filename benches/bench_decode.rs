use bytes::Bytes;
use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use fastlanes::BitPacking;
use parquet::util::bit_util::BitReader;

use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use vbmi_bitpack_decoder::{BitpackDecoderVBMI, Decoder};

const BATCH_SIZE: usize = 4096;

fn create_random_data(rng: &mut StdRng) -> Vec<u8> {
    let mut vec = vec![0_u8; BATCH_SIZE * size_of::<u16>()];
    rng.fill_bytes(&mut vec);
    vec
}

fn decode_arrow(input: Bytes, bitwidth: usize, output: &mut [u16]) -> usize {
    BitReader::new(input).get_batch::<u16>(output, bitwidth)
}

fn decode_fastlanes(input: &[u8], bitwidth: usize, output: &mut [u16]) {
    assert_eq!(output.len() % 1024, 0);
    let (prefix, mut input, suffix) = unsafe { input.align_to::<u16>() };
    assert!(prefix.is_empty() && suffix.is_empty());
    let in_chunk_len = 1024 * bitwidth / 16;
    output
        .as_chunks_mut::<1024>()
        .0
        .into_iter()
        .for_each(|out_chunk| {
            assert!(input.len() >= in_chunk_len);
            unsafe { u16::unchecked_unpack(bitwidth, input, out_chunk) }
            input = &input[in_chunk_len..];
        });
}

#[inline(never)]
fn memset(output: &mut Vec<u16>, value: u16) {
    output.fill(value);
}

pub fn bench_decode(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(34563456);
    let bitpacked = Bytes::from(create_random_data(&mut rng));
    let mut output = vec![0; BATCH_SIZE];

    {
        let mut group = c.benchmark_group("memset");
        group.throughput(Throughput::Elements(BATCH_SIZE as u64));
        group.bench_function("memset", |b| {
            b.iter(|| {
                assert_eq!(output.len(), BATCH_SIZE);
                memset(&mut output, black_box(0xABCD_u16));
            })
        });
    }

    {
        let mut group = c.benchmark_group("decode_bitpacked");
        group.throughput(Throughput::Elements(BATCH_SIZE as u64));

        for bitwidth in 1..=16 {
            output.clear();
            group.bench_function(format!("vbmi-bitwidth-{bitwidth}"), |b| {
                b.iter(|| {
                    assert_eq!(output.spare_capacity_mut().len(), BATCH_SIZE);
                    BitpackDecoderVBMI
                        .decode(&bitpacked, bitwidth, &mut output.spare_capacity_mut())
                        .unwrap()
                })
            });

            output.resize(BATCH_SIZE, 0);
            group.bench_function(format!("arrow-bitwidth-{bitwidth}"), |b| {
                b.iter(|| decode_arrow(bitpacked.clone(), bitwidth, &mut output))
            });

            output.resize(BATCH_SIZE, 0);
            group.bench_function(format!("fastlanes-bitwidth-{bitwidth}"), |b| {
                b.iter(|| decode_fastlanes(&bitpacked, bitwidth, &mut output))
            });
        }
    }
}

criterion_group!(benches, bench_decode);
criterion_main!(benches);
