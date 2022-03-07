#![no_std]
#![no_main]
#![feature(const_mut_refs)]
#![feature(asm_const)]
#![feature(asm_sym)]
#![feature(const_for)]

use fatfs::{Read, Seek, SeekFrom};

use blue_real::println;

mod a20;
mod gdt;
mod paging;

const BOOT_SEGMENT: u16 = 0x07c0;

extern "cdecl" {
    static mut BSS: &'static mut [u8];
    fn STAGE3_ENTRY() -> !;
}

struct RealTrampoline;

impl blue_real::Trampoline for RealTrampoline {
    unsafe fn trampoline(&self, code16: unsafe extern "cdecl" fn()) {
        core::arch::asm!(
            "call {}",
            in(reg) code16,
            out("eax") _,
            out("ecx") _,
            out("edx") _,
        );
    }
}

#[link_section = ".startup"]
#[no_mangle]
extern "cdecl" fn _start() -> ! {
    unsafe {
        BSS.fill(0);
    }

    blue_real::set_trampoline(&RealTrampoline).unwrap();

    println!("BLUEloader/2");

    unsafe {
        a20::enable();
        gdt::load();
        gdt::unreal_mode();
    }

    let mut disk = blue_real::disk::Disk::open(0x80)
        .unwrap()
        .read_table()
        .unwrap();
    let fs = disk.open(0).unwrap();
    let mut file = fs.root_dir().open_file("blue-loader-stage3.bin").unwrap();

    let size = file.seek(SeekFrom::End(0)).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();

    // find the segmented address of STAGE3_ENTRY
    // be careful: rust *really* wants to optimize STAGE3_ENTRY into a u16
    // in the resulting binary, but it is 32-bit!
    let mut buf_address: u32;
    unsafe {
        core::arch::asm!(
            "mov {1}, {0}",
            in(reg) STAGE3_ENTRY,
            out(reg) buf_address,
        );
    }
    buf_address -= (BOOT_SEGMENT as u32) << 4;

    let buf = unsafe { core::slice::from_raw_parts_mut(buf_address as *mut u8, size as usize) };
    file.read_exact(buf).unwrap();

    unsafe {
        paging::load();
        gdt::long_mode(STAGE3_ENTRY)
    }
}
