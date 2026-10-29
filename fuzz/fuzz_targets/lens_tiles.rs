#![no_main]

use libfuzzer_sys::fuzz_target;
use zflow::lens::tiles::Decoder;

fuzz_target!(|data: &[u8]| {
    let mut decoder = Decoder::new(256, 128).expect("valid size");
    let _ = decoder.apply(data);
});
