use std::io::{Seek, SeekFrom, Write};

use byteorder::{LittleEndian, WriteBytesExt};
use rand::Rng;

// this should be the same as in loader-stage1/linker.ld
const LOADER_STAGE1_BLOCKLIST: u64 = 360;

const LOADER_STAGE1: &[u8] = include_bytes!(env!("BLUE_LOADER_STAGE1"));

// stage1 must fit before the partition table
static_assertions::const_assert!(LOADER_STAGE1.len() <= 440);

const LOADER_STAGE2: &[u8] = include_bytes!(env!("BLUE_LOADER_STAGE2"));
const LOADER_STAGE3: &[u8] = include_bytes!(env!("BLUE_LOADER_STAGE3"));

const SECTOR_SIZE: u16 = 512;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = rand::thread_rng();

    let f = std::fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .open("disk.img")?;
    f.set_len(SECTOR_SIZE as u64 * 2 * 1024 * 16)?;
    let mut f = fscommon::BufStream::new(f);

    let mut mbr = mbrman::MBR::new_from(&mut f, SECTOR_SIZE as u32, rng.gen())?;
    mbr[1] = mbrman::MBRPartitionEntry {
        boot: false,
        sys: 0x0c, // FAT32 with LBA
        first_chs: mbrman::CHS::empty(),
        last_chs: mbrman::CHS::empty(),
        starting_lba: 2048,
        sectors: mbr.disk_size - 2048,
    };
    mbr.write_into(&mut f)?;

    f.seek(SeekFrom::Start(0))?;
    f.write_all(LOADER_STAGE1)?;

    let fs_start = mbr[1].starting_lba as u64 * SECTOR_SIZE as u64;
    let mut blocklist: Vec<(u64, u32)> = Vec::new();

    {
        let mut fatimg = fscommon::StreamSlice::new(
            &mut f,
            fs_start,
            fs_start + mbr[1].sectors as u64 * SECTOR_SIZE as u64,
        )?;

        fatfs::format_volume(
            &mut fatfs::StdIoWrapper::new(&mut fatimg),
            fatfs::FormatVolumeOptions::new()
                .fat_type(fatfs::FatType::Fat32)
                .volume_id(rng.gen())
                .volume_label(*b"Blue\0\0\0\0\0\0\0"),
        )?;

        let fs = fatfs::FileSystem::new(fatimg, fatfs::FsOptions::new())?;
        let root = fs.root_dir();
        let mut stage2 = root.create_file("blue-loader-stage2.bin")?;
        stage2.write_all(LOADER_STAGE2)?;
        let mut stage3 = root.create_file("blue-loader-stage3.bin")?;
        stage3.write_all(LOADER_STAGE3)?;
        let mut hello = root.create_file("hello.txt")?;
        hello.write_all(b"Hello, blue!")?;

        for extent in stage2.extents() {
            let extent = extent?;
            let start = fs_start + extent.offset;
            let size = extent.size;

            if let Some(last) = blocklist.last_mut() {
                let last_end = last.0 + last.1 as u64;
                if start == last_end {
                    last.1 += size;
                } else {
                    blocklist.push((start, size));
                }
            } else {
                blocklist.push((start, size));
            }
        }
    }

    // write blocklist to stage1
    f.seek(SeekFrom::Start(LOADER_STAGE1_BLOCKLIST))?;
    for (start, size) in blocklist.iter() {
        assert!(start % SECTOR_SIZE as u64 == 0);
        let start_sec = start / SECTOR_SIZE as u64;
        assert!((start_sec as u32) as u64 == start_sec);
        let size_sec = (size + SECTOR_SIZE as u32 - 1) / SECTOR_SIZE as u32;
        f.write_u32::<LittleEndian>(start_sec as u32)?;
        f.write_u32::<LittleEndian>(size_sec)?;
    }

    Ok(())
}
