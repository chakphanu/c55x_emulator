//! Data memory and I/O space as the core sees them: MMRs in the state, the rest straight
//! in the core's memory buffer; a data write into a code page and a port outside DMA /
//! McBSP are errors (counted).

use crate::core::{Core, MEM_BYTES, MMR_END};
use crate::err::{emu, R};

impl Core {
    pub fn mem_byte(&self, byte_addr: u32) -> u8 {
        unsafe { *self.mem.add(byte_addr as usize & (MEM_BYTES - 1)) }
    }

    /// The whole memory (MEM_BYTES bytes: program bytes, data word w at 2w..2w+1).
    pub fn memory(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.mem, MEM_BYTES) }
    }

    /// The whole memory for direct writes (no MMRs or code-page check).
    pub fn memory_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.mem, MEM_BYTES) }
    }

    /// Store bytes straight into memory: no MMRs or code-page check (Bus.load);
    /// bytes beyond the address space are refused, not wrapped.
    pub fn load_bytes(&mut self, byte_addr: u32, data: &[u8]) -> R<()> {
        let at = byte_addr as usize;
        if at + data.len() > MEM_BYTES {
            return emu(format!("section at 0x{:x} exceeds the address space", byte_addr));
        }
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), self.mem.add(at), data.len()) }
        Ok(())
    }

    pub fn read(&mut self, word: u32) -> R<u32> {
        let word = word & 0x7FFFFF;
        if word < MMR_END {
            return self.st.mmr(word, None);
        }
        let b = (2 * word) as usize;
        unsafe { Ok(((*self.mem.add(b) as u32) << 8) | *self.mem.add(b + 1) as u32) }
    }

    pub fn write(&mut self, word: u32, value: i128) -> R<()> {
        let word = word & 0x7FFFFF;
        let v = (value & 0xFFFF) as u32;
        if word < MMR_END {
            self.st.mmr(word, Some(v))?;
            return Ok(());
        }
        if self.code_pages[(word >> 7) as usize] {
            return self.code_page_write(word);
        }
        let b = (2 * word) as usize;
        unsafe {
            *self.mem.add(b) = (v >> 8) as u8;
            *self.mem.add(b + 1) = v as u8;
        }
        Ok(())
    }

    /// Nothing is decoded at run time, so code must not change under its instructions.
    #[cold]
    #[inline(never)]
    fn code_page_write(&mut self, word: u32) -> R<()> {
        self.code_page_writes += 1;
        emu(format!("data write into a code page at word 0x{:06x}", word))
    }

    /// Ports outside DMA / McBSP: no peripheral is modeled there.
    #[cold]
    #[inline(never)]
    fn unmodeled_port<T>(&mut self, port: u32, what: &str) -> R<T> {
        self.unmodeled_ports += 1;
        emu(format!("unmodeled port 0x{:04x} {}", port, what))
    }

    /// Count legal odd long accesses per PC when telemetry is enabled.
    #[inline(always)]
    fn odd_long(&mut self, word: u32) {
        if word & 1 != 0 {
            if let Some(m) = self.odd_longs.as_mut() {
                *m.entry(self.st.pc).or_insert(0) += 1;
            }
        }
    }

    pub fn read32(&mut self, word: u32) -> R<i128> {
        self.odd_long(word);
        // SWPU073E 3.3.2: addressed word is MSW, paired word is address XOR 1.
        Ok(((self.read(word)? as i128) << 16) | self.read(word ^ 1)? as i128)
    }

    pub fn write32(&mut self, word: u32, v: i128) -> R<()> {
        self.odd_long(word);
        self.write(word, v >> 16)?;
        self.write(word ^ 1, v)
    }

    pub fn port_read(&mut self, port: u32) -> R<u32> {
        let port = port & 0xFFFF;
        if let Some(v) = self.dma_read(port).or_else(|| self.mcbsp_read(port)) {
            return Ok(v);
        }
        self.unmodeled_port(port, "read")
    }

    pub fn port_write(&mut self, port: u32, v: i128) -> R<()> {
        let (port, v) = (port & 0xFFFF, (v & 0xFFFF) as u32);
        if self.dma_write(port, v)? || self.mcbsp_write(port, v)? {
            return Ok(());
        }
        self.unmodeled_port(port, "written")
    }
}
