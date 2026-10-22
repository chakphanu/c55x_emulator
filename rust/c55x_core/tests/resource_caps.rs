//! Allocation caps of the core (from tests/test_c55x_emu_resource_caps.py BusCaps and
//! CodeCacheCap). The caps follow the architecture: 24-bit program byte addresses and
//! 23-bit data word addresses are both 16 MiB (SPRU371F ch. 3), so memory and a load
//! cannot exceed that. The code-cache cap is an engineering limit against a runaway
//! decode, not an architectural number. The image cap is tested with c55x-run's loader
//! (rust/c55x_run/tests/cli.rs).

mod common;

use std::collections::HashMap;
use std::time::Instant;

use c55x_core::core::{Core, MEM_BYTES};
use c55x_core::frontend::Insn;
use c55x_core::icache::MAX_CACHED_INSNS;
use common::message;

fn machine() -> Core {
    fixture!("containment").core("t_nop")
}

mod bus_caps {
    use super::*;

    #[test]
    fn address_space_is_the_architectural_16_mib() {
        assert_eq!(MEM_BYTES, 1 << 24);
        let e = Core::from_listing(vec![0; MEM_BYTES / 2], &[], &HashMap::new()).map_err(message).err().unwrap();
        assert!(e.contains("memory of 8388608 bytes"), "{}", e);
    }

    #[test]
    fn load_outside_the_address_space_is_refused() {
        let mut cpu = machine();
        assert!(cpu.load_bytes(MEM_BYTES as u32 - 4, &[1, 2, 3, 4]).is_ok());     // exactly fits
        for (addr, n) in [(MEM_BYTES as u32 - 3, 4), (MEM_BYTES as u32, 1), (u32::MAX, 2)] {
            let e = cpu.load_bytes(addr, &vec![0; n]).map_err(message).err();
            assert!(e.is_some_and(|e| e.contains("exceeds the address space")), "{:#x} + {}", addr, n);
        }
    }

    #[test]
    fn many_small_loads_stay_fast() {
        // a test harness seeds memory one word at a time; an earlier revision kept a
        // per-call log and re-summed it, which was quadratic
        let mut cpu = machine();
        let t0 = Instant::now();
        for i in 0..200_000u32 {
            cpu.load_bytes(0x40000 + (i % 4096) * 2, &[0x12, 0x34]).unwrap_or_else(|e| panic!("{}", message(e)));
        }
        assert!(t0.elapsed().as_secs_f64() < 5.0);
    }
}

mod code_cache_cap {
    use super::*;

    #[test]
    fn runaway_decode_stops_with_an_explicit_error() {
        assert_eq!(MAX_CACHED_INSNS, 1 << 20);
        let rows: Vec<Insn> = (0..=MAX_CACHED_INSNS as u32)
            .map(|i| Insn { addr: 0x20000 + i, size: 1, text: "nop".into(), block_end: None }).collect();
        let e = Core::from_listing(vec![0; MEM_BYTES], &rows, &HashMap::new()).map_err(message).err().unwrap();
        assert!(e.contains("exceeds the code cache"), "{}", e);
    }
}
