#![no_std]
#![no_main]
#![feature(asm_const)]
#![feature(asm_sym)]

const SECTOR_SIZE: u16 = 512;
const BOOT_SEGMENT: u16 = 0x07c0;

#[repr(C, packed)]
struct Blocks {
    offset: u32,
    count: u32,
}

#[link_section = ".blocklist"]
#[no_mangle]
static STAGE2: [Blocks; 10] = [
    Blocks {
        offset: 0x1,
        count: 0xb000 / SECTOR_SIZE as u32,
    },
    Blocks {
        offset: 0,
        count: 0,
    },
    Blocks {
        offset: 0,
        count: 0,
    },
    Blocks {
        offset: 0,
        count: 0,
    },
    Blocks {
        offset: 0,
        count: 0,
    },
    Blocks {
        offset: 0,
        count: 0,
    },
    Blocks {
        offset: 0,
        count: 0,
    },
    Blocks {
        offset: 0,
        count: 0,
    },
    Blocks {
        offset: 0,
        count: 0,
    },
    Blocks {
        offset: 0,
        count: 0,
    },
];

extern "cdecl" {
    fn STAGE2_ENTRY() -> !;
}

#[inline]
fn hlt() {
    unsafe {
        core::arch::asm!("hlt");
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        hlt();
    }
}

#[inline]
pub fn inform(s: &[u8]) {
    for &c in s {
        unsafe {
            core::arch::asm!(
                "int 0x10",
                in("ax") (0x0e00 | (c as u16)),
                in("ebx") 7,
            );
        }
    }
}

#[inline]
fn error(msg: &[u8]) {
    inform(b"ERROR: ");
    inform(msg);
    loop {
        hlt();
    }
}

#[repr(C, packed)]
struct Dap {
    size: u8,
    zero: u8,
    sectors: u16,
    buffer: u32,
    startlba: u64,
}

impl Dap {
    #[inline]
    fn new(sectors: u16, buffer: u32, startlba: u64) -> Self {
        Self {
            size: core::mem::size_of::<Self>() as u8,
            zero: 0,
            sectors,
            buffer,
            startlba,
        }
    }
    #[inline]
    unsafe fn reset(drv: u8) -> bool {
        let mut ret: u8;
        core::arch::asm!(
            "int $0x13",
            lateout("ah") ret,
            in("ah") 0u8,
            in("dl") drv,
        );
        ret == 0
    }

    #[inline]
    unsafe fn read(&self, drv: u8) -> bool {
        let mut ret: u8;
        let address = self as *const Self;
        core::arch::asm!(
            "push si",
            "mov si, bx",
            "int $0x13",
            "pop si",
            in("bx") address,
            lateout("ah") ret,
            in("ah") 0x42u8,
            in("dl") drv,
        );
        ret == 0
    }
}

#[link_section = ".startup"]
#[no_mangle]
extern "C" fn _start() -> ! {
    unsafe {
        core::arch::asm!(
            "jmpl ${0}, $2f",
            "2:",
            "cli",
            "movw ${0}, %ax",
            "movw %ax, %ds",
            "movw %ax, %es",
            "movw %ax, %ss",
            "movl $0x0FFF0, %esp",
            "movl $0x0FFF0, %ebp",
            "sti",
            const BOOT_SEGMENT,
            options(att_syntax),
        );
    }

    inform(b"BLUEloader/1\r\n");

    unsafe {
        let drive = 0x80; // always the boot drive

        let mut dest = (STAGE2_ENTRY as *const unsafe fn() -> !) as u32;

        if !Dap::reset(drive) {
            error(b"reset");
        }

        for chunk in STAGE2.iter() {
            if chunk.count == 0 {
                break;
            }

            // FIXME bioses will only really move 16 sectors at a time...
            let d = Dap::new(
                chunk.count as u16,
                dest | ((BOOT_SEGMENT as u32) << 16),
                chunk.offset as u64,
            );

            if !d.read(drive) {
                error(b"read");
            } else {
                dest += chunk.count as u32 * SECTOR_SIZE as u32;
            }
        }
    }

    // reset stack and jump to stage2
    unsafe {
        core::arch::asm!(
            "mov esp, 0xfff0",
            "mov ebp, 0xfff0",
            "jmp {}",
            sym STAGE2_ENTRY,
            options(noreturn),
        );
    }
}
