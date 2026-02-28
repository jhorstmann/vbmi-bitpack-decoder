# Blazing fast bitpack decoding using avx512-vbmi instructions

The [avx512-vbmi instruction set](vbmi) introduces two instructions that are very useful for
decoding bitpacked data.

 - [`vpermb`](vpermb) (`__mm512_permutexvar_epi8`): Shuffles 8-bit integers across lanes.
 - [`vpmultishiftqb`](vpmultishiftqb) (`_mm512_multishift_epi64_epi8`): Creates 8 8-bit integers
   per 64-bit lane by shifting that 64-bit word by an arbitrary amount.   

Combining these two instructions with masked loads allows to build a fully table-driven bitpack
decoder for arbitrary bit widths up to 30. In Parquet, bit width encoding works on blocks of 8
values, which ensures that each block ends on a byte boundary.

Since an avx512 register can hold 32 16-bit values, we can process 4 blocks into 16-bit values
per step. For 8-bit values we could process 8 blocks at once, for 32-bit values it would be 2
blocks per step.

| Output Type | Blocks per step | Input bytes per step | Output words per step |
|-------------|-----------------|----------------------|-----------------------|
| 8-bit       | 8               | bitwidth * 8         | 64                    |
| 16-bit      | 4               | bitwidth * 4         | 32                    |
| 32-bit      | 2               | bitwidth * 2         | 16                    |

The general algorithm works as follows:

 - We load the bytes needed for each decode step. This depends on the bit width and the number
   of blocks per step explained above. The load mask needed can be calculated once up front.
 - We shuffle these bytes across the whole 512-bit register using `vpermb`, so that for each
   output byte the required bits are inside the same 64-bit lane.
 - This allow `vpmultishiftqb` to reassemble the output bytes.
 - We need to mask out high bits of each output word using `vpandq`.
 - We store the resulting 512-bit register to memory.

This means that excluding loads/stores, there are only 3 operations needed per 32 16-bit words,
64 8-bit values or 16 32-bit values. All these operations are independent of the bit width, the
load mask, permutation indices and shift amounts can be calculated or looked up in a table but
the operations themselves do not change. This is a huge difference to many other bitpack decoders,
which require generating specific code sequences per bit width.

The algorithm also does not require padding the input to larger power-of-2 sized chunks. For
arbitrary input sizes there needs to be additional code to handle the tail that is not a multiple
of the input bytes per step. This does not change the algorithm steps, except that we need to
calculate a separate load and store mask that takes the amount of remaining input bytes and
output words into account.

As an example, consider a block of values encoded using a bit width of 3. The bits corresponding
to an output word are marked by letters corresponding to that word from A-H. Bytes are separated
by space and bits per bytes are written starting from the most significant one.

```
CCBBBAAA FEEEDDDC HHHGGGFF
```

Since we use a masked load, the remaining bytes in the register will be set to all zeroes.
Since we are only decoding 8 values, the following description will only look at the lower
128 bits of the avx512 register.

After shuffling these bytes are distributed in the block like this:

```
CCBBBAAA FEEEDDDC HHHGGGFF 00000000 00000000 00000000 00000000 00000000
FEEEDDDC HHHGGGFF 00000000 00000000 00000000 00000000 00000000 00000000
```

To reassemble into 16-bit words we now need to, for each byte, select sequence of 8 bits
starting from a certain position inside its 64-bit lane. The shift amounts for this example
then are:

```
0 8 3 11 6 14 9 17
4 12 7 15 10 18 13 21
```

Giving the resulting bits:

```
CCBBBAAA FEEEDDDC DDCCCBBB GFFFEEED EEDDDCCC HGGGFFFE FFEEEDDD 0HHHGGGF
GGFFFEEE 0000HHHG HHGGGFFF 0000000H 00HHHGGG 00000000 00000HHH 00000000
```

Masking each 16-bit word to only retain 3 bits then results in:

```
00000AAA 00000000 00000BBB 00000000 00000CCC 00000000 00000DDD 00000000
00000EEE 00000000 00000FFF 00000000 00000GGG 00000000 00000HHH 00000000
```

## Performance

On an Intel Tigerlake maching (i9-11900KB), this algorithm can decode batches of 4096 16-bit values
At a throughput of around 52 billion elements per second, and so is mostly limited by memory
bandwidth.

(TODO: Add results on Zen5)

 [vbmi]: https://en.wikipedia.org/wiki/AVX-512#BW,_DQ_and_VBMI
 [vpermb]: https://www.felixcloutier.com/x86/vpermb
 [vpmultishiftqb]: https://www.felixcloutier.com/x86/vpmultishiftqb