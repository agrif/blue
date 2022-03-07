#![no_std]
#![feature(asm_const)]
#![feature(asm_sym)]
#![feature(naked_functions)]
#![feature(panic_info_message)]
#![feature(const_fn_trait_bound)]
#![feature(const_ptr_offset_from)]

pub const SECTOR_SIZE: u16 = 512;

type Result<T, E = &'static str> = core::result::Result<T, E>;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("");

    if let Some(loc) = info.location() {
        println!(
            "PANIC in file {} line {} column {}:",
            loc.file(),
            loc.line(),
            loc.column()
        );
    }

    if let Some(msg) = info.message() {
        println!("PANIC: {}", msg);
    } else {
        println!("PANIC");
    }

    loop {
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}

mod trampoline;
pub use trampoline::{set_trampoline, trampoline, Trampoline, Work, WorkOffset, WORK, WORK_SIZE};

pub mod disk;
pub mod mbr;
pub mod video;
