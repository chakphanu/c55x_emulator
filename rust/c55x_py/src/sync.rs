//! Full state transfer between the Python Cpu object and the core (run start/end and
//! when Python code calls into the core between runs).

use pyo3::intern;
use pyo3::prelude::*;

use c55x_core::core::Core;
use c55x_core::state::PLAIN;

pub fn load(core: &mut Core, py: Python<'_>, cpu: &Bound<'_, PyAny>) -> PyResult<()> {
    let s = &mut core.st;
    macro_rules! get { ($n:literal) => { cpu.getattr(intern!(py, $n))?.extract()? }; }
    s.ac = get!("ac");
    s.t = get!("t");
    s.xar = get!("xar");
    s.xcdp = get!("xcdp"); s.xdp = get!("xdp"); s.xsp = get!("xsp"); s.ssp = get!("ssp");
    s.tc1 = get!("tc1"); s.tc2 = get!("tc2"); s.carry = get!("carry");
    s.st1 = get!("st1"); s.st2 = get!("st2"); s.st3 = get!("st3");
    s.ier0 = get!("ier0"); s.ier1 = get!("ier1"); s.ifr0 = get!("ifr0"); s.ifr1 = get!("ifr1");
    s.ivpd = get!("ivpd"); s.ivph = get!("ivph"); s.dbstat = get!("dbstat");
    s.brc0 = get!("brc0"); s.brc1 = get!("brc1"); s.brs1 = get!("brs1"); s.csr = get!("csr");
    s.rsa = [get!("rsa0"), get!("rsa1")];
    s.rea = [get!("rea0"), get!("rea1")];
    let la = cpu.getattr(intern!(py, "loop_active"))?;
    for i in 0..2 {
        let v = la.get_item(i)?;
        s.loop_active[i] = if v.is_none() { 0 } else if v.extract::<String>()? == "local" { 1 } else { 2 };
    }
    for (i, name) in PLAIN.iter().enumerate() {
        s.plain[i] = cpu.getattr(*name)?.extract()?;
    }
    s.reta = get!("reta"); s.cfct = get!("cfct"); s.pc = get!("pc");
    s.cycles = get!("cycles"); s.skip_next = get!("skip_next"); s.skip_data_next = get!("skip_data_next"); s.in_idle = get!("in_idle");
    Ok(())
}

pub fn store(core: &mut Core, py: Python<'_>, cpu: &Bound<'_, PyAny>) -> PyResult<()> {
    let s = &core.st;
    macro_rules! set { ($n:literal, $v:expr) => { cpu.setattr(intern!(py, $n), $v)? }; }
    set!("ac", s.ac.to_vec()); set!("t", s.t.to_vec()); set!("xar", s.xar.to_vec());
    set!("xcdp", s.xcdp); set!("xdp", s.xdp); set!("xsp", s.xsp); set!("ssp", s.ssp);
    set!("tc1", s.tc1); set!("tc2", s.tc2); set!("carry", s.carry);
    set!("st1", s.st1); set!("st2", s.st2); set!("st3", s.st3);
    set!("ier0", s.ier0); set!("ier1", s.ier1); set!("ifr0", s.ifr0); set!("ifr1", s.ifr1);
    set!("ivpd", s.ivpd); set!("ivph", s.ivph); set!("dbstat", s.dbstat);
    set!("brc0", s.brc0); set!("brc1", s.brc1); set!("brs1", s.brs1); set!("csr", s.csr);
    set!("rsa0", s.rsa[0]); set!("rsa1", s.rsa[1]); set!("rea0", s.rea[0]); set!("rea1", s.rea[1]);
    let la: Vec<Option<&str>> = s.loop_active.iter()
        .map(|&k| match k { 0 => None, 1 => Some("local"), _ => Some("ext") }).collect();
    set!("loop_active", la);
    for (i, name) in PLAIN.iter().enumerate() {
        cpu.setattr(*name, s.plain[i])?;
    }
    set!("reta", s.reta); set!("cfct", s.cfct); set!("pc", s.pc); set!("cycles", s.cycles);
    set!("skip_next", s.skip_next); set!("skip_data_next", s.skip_data_next); set!("in_idle", s.in_idle);
    let counter = cpu.getattr(intern!(py, "interrupts_taken"))?;
    for (n, c) in core.irq_counts.iter_mut().enumerate() {
        if *c != 0 {
            let old: u64 = counter.call_method1("get", (n, 0))?.extract()?;
            counter.set_item(n, old + *c)?;
            *c = 0;
        }
    }
    Ok(())
}
