#![no_main]

use libfuzzer_sys::fuzz_target;
use zflow::lens::tiles::Decoder;

// A tile packet is parsed and checked whole before any pixel changes.
fuzz_target!(|data: &[u8]| {
    let mut decoder = Decoder::new(256, 128).expect("valid size");
    let _ = decoder.apply(data);
});
