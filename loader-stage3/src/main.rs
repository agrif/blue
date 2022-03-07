#![no_std]
#![no_main]
#![feature(asm_sym)]
#![feature(naked_functions)]

use blue_real::println;

mod realmode;

extern "sysv64" {
    static REALMODE_IMAGE: &'static [u8];
    static mut REALMODE: &'static mut [u8];
    static mut BSS: &'static mut [u8];
}

#[link_section = ".startup"]
#[no_mangle]
extern "cdecl" fn _start() -> ! {
    // move realmode section to true home, zero bss
    unsafe {
        REALMODE.copy_from_slice(REALMODE_IMAGE);
        BSS.fill(0);
    }

    // install our real mode trampoline
    blue_real::set_trampoline(&realmode::LongModeTrampoline).unwrap();

    println!("BLUEloader/3");

    let mut disk = blue_real::disk::Disk::open(0x80)
        .unwrap()
        .read_table()
        .unwrap();
    let fs = disk.open(0).unwrap();
    let mut file = fs.root_dir().open_file("hello.txt").unwrap();

    let mut buf = [0u8; 0x100];
    use fatfs::Read;
    let amt = file.read(&mut buf).unwrap();
    println!(
        "hello.txt: {:x?}",
        core::str::from_utf8(&buf[..amt]).unwrap()
    );

    panic!("end of stage3");
}
