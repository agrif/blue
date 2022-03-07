use byteorder::{ByteOrder, LittleEndian};

use crate::Result;

#[derive(Clone, Debug)]
pub struct PartitionTable {
    pub signature: u32,
    pub copy_protected: bool,
    pub table: [PartitionEntry; 4],
}

#[derive(Clone, Debug)]
pub struct PartitionEntry {
    pub status: u8,
    pub first: CHS,
    pub typ: u8,
    pub last: CHS,
    pub first_lba: u32,
    pub sectors: u32,
}

#[derive(Clone, Debug)]
pub struct CHS(u16, u8, u8);

impl PartitionTable {
    pub const fn new() -> Self {
        Self {
            signature: 0,
            copy_protected: false,
            table: [
                PartitionEntry::empty(),
                PartitionEntry::empty(),
                PartitionEntry::empty(),
                PartitionEntry::empty(),
            ],
        }
    }

    pub fn load_boot_sector(&mut self, data: &[u8]) -> Result<()> {
        assert!(data.len() == 512);

        if data[510] != 0x55 || data[511] != 0xaa {
            return Err("bad MBR signature");
        }

        self.signature = LittleEndian::read_u32(&data[440..]);
        self.copy_protected = 0x5a5a == LittleEndian::read_u16(&data[444..]);
        self.table[0] = PartitionEntry::read(&data[446..462]);
        self.table[1] = PartitionEntry::read(&data[462..478]);
        self.table[2] = PartitionEntry::read(&data[478..494]);
        self.table[3] = PartitionEntry::read(&data[494..510]);

        Ok(())
    }
}

impl PartitionEntry {
    const fn empty() -> Self {
        Self {
            status: 0,
            first: CHS(0, 0, 0),
            typ: 0,
            last: CHS(0, 0, 0),
            first_lba: 0,
            sectors: 0,
        }
    }

    fn read(data: &[u8]) -> Self {
        assert!(data.len() == 16);
        let status = data[0];
        let first = CHS::read(&data[1..4]);
        let typ = data[4];
        let last = CHS::read(&data[5..8]);
        let first_lba = LittleEndian::read_u32(&data[8..]);
        let sectors = LittleEndian::read_u32(&data[12..]);
        Self {
            status,
            first,
            typ,
            last,
            first_lba,
            sectors,
        }
    }
}

impl CHS {
    fn read(data: &[u8]) -> Self {
        assert!(data.len() == 3);
        let head = data[0];
        let sector = data[1] & 0b0011_1111;
        let cylinder = (data[2] as u16) | (((data[1] & 0b1100_0000) as u16) << 8);
        CHS(cylinder, head, sector)
    }
}
