use bytes::Bytes;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use parquet::util::bit_util::{BitReader, BitWriter};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use vbmi_bitpack_decoder::{BitpackDecoderVBMI, Decoder};

const BATCH_SIZE: usize = 4096;

fn create_random_bitpacked_data(rng: &mut StdRng, bitwidth: usize) -> Vec<u8> {
    let mut bw = BitWriter::new((BATCH_SIZE * bitwidth).div_ceil(8));
    (0..BATCH_SIZE).for_each(|_| bw.put_value(rng.gen_range(1..(1 << bitwidth)), bitwidth));
    bw.consume()
}

fn decode_arrow(input: Bytes, bitwidth: usize, output: &mut [u16]) -> usize {
    BitReader::new(input).get_batch::<u16>(output, bitwidth)
}

pub fn bench_decode(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(34563456);

    {
        let mut group = c.benchmark_group("decode_bitpacked");
        group.throughput(Throughput::Elements(BATCH_SIZE as u64));

        for bitwidth in 1..=16 {
            let bitpacked = Bytes::from(create_random_bitpacked_data(&mut rng, bitwidth));
            let mut output = Vec::with_capacity(BATCH_SIZE);

            group.bench_function(format!("vbmi-bitwidth-{bitwidth}"), |b| {
                b.iter(|| {
                    BitpackDecoderVBMI::default()
                        .decode(&bitpacked, bitwidth, &mut output.spare_capacity_mut())
                        .unwrap()
                })
            });

            output.resize(BATCH_SIZE, 0);

            group.bench_function(format!("arrow-bitwidth-{bitwidth}"), |b| {
                b.iter(|| decode_arrow(bitpacked.clone(), bitwidth, &mut output))
            });
        }
    }
}

criterion_group!(benches, bench_decode);
criterion_main!(benches);
