//! ATA PIO data path (ata.rs): sector blocks between the disk object and the data
//! register, and the INTRQ / BSY sequencing per block.

use crate::ata::{Op, Step, ABRT, BSY, DRDY, DRQ, DSC, ERR, IDNF, SECTOR};
use crate::core::Core;
use crate::err::{emu, R};

impl Core {
    fn ata_out_of_range(&mut self) {
        let a = self.ata();
        a.error = IDNF | ABRT;
        a.pending = None;
        self.ata_finish(ERR);
    }

    pub(crate) fn ata_read_block(&mut self) -> R<()> {
        let Some(Op::Read { lba, left, multi }) = self.ata().pending else {
            return emu("ATA: read step without a read command".into());
        };
        let a = self.ata();
        let n = a.block(multi, left);
        if lba + n as u64 > a.sectors {
            self.ata_out_of_range();
            return Ok(());
        }
        for i in 0..n as u64 {
            let data = self.disk_read(lba + i)?;
            self.ata().buf.extend(data.chunks_exact(2).map(|p| p[0] as u16 | (p[1] as u16) << 8));
        }
        let a = self.ata();
        a.pending = Some(Op::Read { lba: lba + n as u64, left: left - n, multi });
        a.status = DRDY | DSC | DRQ;
        self.ata_irq();
        Ok(())
    }

    pub(crate) fn ata_data_read(&mut self) -> R<u16> {
        let a = self.ata();
        let Some(v) = a.buf.pop_front() else {
            a.note("data read without DRQ".into());
            return Ok(0);
        };
        if a.buf.is_empty() {
            if matches!(a.pending, Some(Op::Read { left, .. }) if left > 0) {
                a.status = BSY | DSC;
                self.ata_later(Step::ReadBlock);
            } else {
                a.pending = None;
                a.status = DRDY | DSC;
            }
        }
        Ok(v)
    }

    pub(crate) fn ata_want_block(&mut self, irq: bool) -> R<()> {
        let Some(Op::Write { lba, left, multi }) = self.ata().pending else {
            return emu("ATA: write step without a write command".into());
        };
        let a = self.ata();
        if lba + a.block(multi, left) as u64 > a.sectors {
            self.ata_out_of_range();
            return Ok(());
        }
        a.status = DRDY | DSC | DRQ;
        if irq {
            self.ata_irq();
        }
        Ok(())
    }

    pub(crate) fn ata_data_write(&mut self, value: u16) -> R<()> {
        let a = self.ata();
        let Some(Op::Write { lba, left, multi }) = a.pending else {
            a.note("data write without DRQ".into());
            return Ok(());
        };
        a.buf.push_back(value);
        let n = a.block(multi, left);
        if a.buf.len() < n as usize * SECTOR / 2 {
            return Ok(());
        }
        let bytes: Vec<u8> = a.buf.drain(..).flat_map(|w| [w as u8, (w >> 8) as u8]).collect();
        for (i, chunk) in bytes.chunks_exact(SECTOR).enumerate() {
            self.disk_write(lba + i as u64, chunk)?;
        }
        let a = self.ata();
        a.pending = Some(Op::Write { lba: lba + n as u64, left: left - n, multi });
        a.status = BSY | DSC;
        self.ata_later(if left - n > 0 { Step::WantBlock(true) } else { Step::Done });
        Ok(())
    }

    fn disk_read(&mut self, lba: u64) -> R<Vec<u8>> {
        self.host(|h, ctx| h.disk_read(ctx, lba))
    }

    fn disk_write(&mut self, lba: u64, data: &[u8]) -> R<()> {
        self.host(|h, ctx| h.disk_write(ctx, lba, data))
    }
}
