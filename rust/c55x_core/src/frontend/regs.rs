//! Register names of the front end: the MMR table and the dis55 aliases of
//! c55x_emu/registers.py (SPRU371F table 2-2).

/// MMR name -> word address.
pub fn mmr(name: &str) -> Option<i64> {
    const FIXED: &[(&str, i64)] = &[
        ("IER0", 0x00), ("IFR0", 0x01), ("ST0", 0x02), ("ST1", 0x03), ("ST3", 0x04),
        ("TRN0", 0x0F), ("SP", 0x18), ("BK03", 0x19), ("BRC0", 0x1A), ("XPC", 0x1E),
        ("CDP", 0x27), ("DPH", 0x2B), ("DP", 0x2E), ("PDP", 0x2F), ("BK47", 0x30),
        ("BKC", 0x31), ("BSA01", 0x32), ("BSA23", 0x33), ("BSA45", 0x34), ("BSA67", 0x35),
        ("BSAC", 0x36), ("TRN1", 0x38), ("BRC1", 0x39), ("BRS1", 0x3A), ("CSR", 0x3B),
        ("RSA0H", 0x3C), ("RSA0", 0x3D), ("REA0H", 0x3E), ("REA0", 0x3F), ("RSA1H", 0x40),
        ("RSA1", 0x41), ("REA1H", 0x42), ("REA1", 0x43), ("RPTC", 0x44), ("IER1", 0x45),
        ("IFR1", 0x46), ("DBIER0", 0x47), ("DBIER1", 0x48), ("IVPD", 0x49), ("IVPH", 0x4A),
        ("ST2", 0x4B), ("SSP", 0x4C), ("SPH", 0x4E), ("CDPH", 0x4F), ("BIOS37", 0x37),
    ];
    if let Some(&(_, a)) = FIXED.iter().find(|(n, _)| *n == name) {
        return Some(a);
    }
    let b = name.as_bytes();
    let digit = |c: u8, max: u8| (b'0'..=max).contains(&c).then(|| (c - b'0') as i64);
    match b {
        [b'A', b'R', d] => digit(*d, b'7').map(|i| 0x10 + i),
        [b'T', d] => digit(*d, b'3').map(|i| 0x20 + i),
        [b'A', b'C', d, part] => {
            let base = [0x08, 0x0B, 0x24, 0x28][digit(*d, b'3')? as usize];
            match part {
                b'L' => Some(base),
                b'H' => Some(base + 1),
                b'G' => Some(base + 2),
                _ => None,
            }
        }
        _ => None,
    }
}

/// dis55 '@NAME_L || mmap()' spellings -> MMR names.
pub fn alias(name: &str) -> &str {
    match name {
        "IMR0" => "IER0",
        "IMR1" => "IER1",
        "IFR0" => "IFR0",
        "PMST" => "ST3",
        "BIOS" => "BIOS37",
        "BOF01" => "BSA01",
        "BOF23" => "BSA23",
        "BOF45" => "BSA45",
        "BOF67" => "BSA67",
        "BOFC" => "BSAC",
        _ => name,
    }
}
