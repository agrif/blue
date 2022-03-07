use crate::Result;

#[derive(Clone, Debug)]
pub struct Disk {
    id: u8,
    start: u64,
    length: u64,
}

#[derive(Debug)]
pub struct DiskCursor {
    disk: Disk,
    pos: u64,
    lba_in_buffer: Option<u64>,
    buffer: [u8; crate::SECTOR_SIZE as usize],
}

#[derive(Clone, Debug)]
pub struct PartitionedDisk {
    disk: Disk,
    table: crate::mbr::PartitionTable,
}

#[repr(C, packed)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct Dap {
    size: u8,
    zero: u8,
    sectors: u16,
    buffer: u32,
    startlba: u64,
}

impl Disk {
    pub fn open(id: u8) -> Result<Self> {
        let mut s = Self {
            id,
            start: 0,
            length: 0,
        };
        s.reset()?;
        s.length = s.read_length()?;
        Ok(s)
    }

    fn read_length(&mut self) -> Result<u64> {
        #[repr(C, packed)]
        #[derive(Clone, Copy, Default, bytemuck::Zeroable, bytemuck::Pod)]
        struct Info {
            info_size: u16,
            flags: u16,
            cylinders: u32,
            heads: u32,
            sectors: u32,
            absolute_sectors: u64,
            bytes_per_sector: u16,
        }

        unsafe {
            crate::real_asm!(
                "push si",
                "mov ah, 0x48",
                "mov dl, [{0} + {id}]",
                "lea si, [{0} + {info}]",
                "int 0x13",
                "mov [{0} + {ret}], ah",
                "pop si",
                info: Info = alloc Info {
                    info_size: core::mem::size_of::<Info>() as u16,
                    .. Default::default()
                },
                id: u8 = alloc self.id,
                ret: u8 = alloc 1,
            );

            if *ret != 0 {
                Err("could not read disk size")
            } else {
                Ok(info.absolute_sectors)
            }
        }
    }

    pub fn reset(&mut self) -> Result<()> {
        unsafe {
            crate::real_asm!(
                "mov ah, 0",
                "mov dl, [{0} + {id}]",
                "int 0x13",
                "mov [{0} + {ret}], ah",
                id: u8 = alloc self.id,
                ret: u8 = alloc 1,
            );

            if *ret != 0 {
                Err("could not reset disk")
            } else {
                Ok(())
            }
        }
    }

    pub fn narrow(&self, start: u64, length: u64) -> Result<Self> {
        if start + length > self.length {
            return Err("narrowed region too large")?;
        }
        Ok(Self {
            id: self.id,
            start: self.start + start,
            length: length,
        })
    }

    pub fn read<'a>(
        &self,
        start: u64,
        buffer: &'a mut [u8; crate::SECTOR_SIZE as usize],
    ) -> Result<&'a [u8]> {
        if start >= self.length {
            return Err("read past end of disk");
        }

        unsafe {
            crate::real_asm!(
                "push si",
                "lea ax, [{0} + {realbuffer}]",
                "mov [{0} + {dap} + {data_addr}], ax",
                "mov [{0} + {dap} + {data_addr} + 2], ds",
                "mov ah, 0x42",
                "mov dl, [{0} + {id}]",
                "lea si, [{0} + {dap}]",
                "int 0x13",
                "mov [{0} + {ret}], ah",
                "pop si",

                realbuffer: [u8; crate::SECTOR_SIZE as usize] = alloc,
                data_addr = const memoffset::offset_of!(Dap, buffer),
                dap: Dap = alloc Dap {
                    size: core::mem::size_of::<Dap>() as u8,
                    zero: 0,
                    sectors: 1,
                    buffer: 0,
                    startlba: self.start + start,
                },
                id: u8 = alloc self.id,
                ret: u8 = alloc 1,
            );

            if *ret != 0 {
                Err("could not read disk")
            } else {
                buffer.copy_from_slice(&realbuffer[..]);
                Ok(buffer)
            }
        }
    }

    pub fn cursor(&self) -> DiskCursor {
        DiskCursor {
            disk: self.clone(),
            pos: 0,
            lba_in_buffer: None,
            buffer: [0; crate::SECTOR_SIZE as usize],
        }
    }

    pub fn read_table(&self) -> Result<PartitionedDisk> {
        let mut buffer = [0; crate::SECTOR_SIZE as usize];
        let mut table = crate::mbr::PartitionTable::new();
        table.load_boot_sector(self.read(0, &mut buffer)?)?;
        Ok(PartitionedDisk { disk: self.clone(), table })
    }
}

impl PartitionedDisk {
    pub fn open(
        &mut self,
        id: usize,
    ) -> Result<fatfs::FileSystem<DiskCursor, fatfs::DefaultTimeProvider, fatfs::LossyOemCpConverter>>
    {
        let start = self.table.table[id].first_lba as u64;
        let length = self.table.table[id].sectors as u64;
        if length == 0 {
            return Err("partition does not exist");
        }
        let cur = self.disk.narrow(start, length)?.cursor();
        fatfs::FileSystem::new(cur, fatfs::FsOptions::new()).map_err(|_| "could not open fs")
    }
}

impl fatfs::IoBase for DiskCursor {
    type Error = (); // can't implement IoError for &'static str :(
}

impl fatfs::Read for DiskCursor {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        // determine sector and local offset
        let sector = self.pos / crate::SECTOR_SIZE as u64;
        let offset = (self.pos % crate::SECTOR_SIZE as u64) as usize;

        // is this EOF?
        if sector >= self.disk.length {
            if offset == 0 {
                // end of file
                return Ok(0);
            }
            // malformed read
            return Err(());
        }

        // read that sector!
        if Some(sector) != self.lba_in_buffer {
            self.disk.read(sector, &mut self.buffer).map_err(|_| ())?;
            self.lba_in_buffer = Some(sector);
        }
        let amount = buf.len().min(self.buffer.len() - offset);
        buf[..amount].copy_from_slice(&self.buffer[offset..offset + amount]);
        self.pos += amount as u64;
        Ok(amount)
    }
}

impl fatfs::Seek for DiskCursor {
    fn seek(&mut self, pos: fatfs::SeekFrom) -> Result<u64, Self::Error> {
        match pos {
            fatfs::SeekFrom::Start(offset) => {
                self.pos = offset;
            }
            fatfs::SeekFrom::End(offset) => {
                self.pos = (self.disk.length as i64 * crate::SECTOR_SIZE as i64 + offset) as u64;
            }
            fatfs::SeekFrom::Current(offset) => {
                self.pos = (self.pos as i64 + offset) as u64;
            }
        }

        if self.disk.length * (crate::SECTOR_SIZE as u64) < self.pos {
            // seek to negative offset, or past end
            return Err(());
        }

        Ok(self.pos)
    }
}

impl fatfs::Write for DiskCursor {
    fn write(&mut self, _buf: &[u8]) -> Result<usize, Self::Error> {
        panic!("write unimplemented")
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        panic!("write unimplemented")
    }
}
