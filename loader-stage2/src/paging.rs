const ADDRMASK: u64 = 0x000f_ffff_ffff_f000;
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const HUGE_PAGE: u64 = 1 << 7;

#[repr(C, align(4096))]
#[derive(Clone, Debug)]
pub struct PageTable {
    entries: [u64; 512],
}

impl PageTable {
    pub const fn new() -> Self {
        Self { entries: [0; 512] }
    }

    pub fn clear(&mut self) {
        self.entries = [0; 512];
    }

    pub fn map(&mut self, index: usize, addr: u64) {
        self.map_flags(index, addr, PRESENT | WRITABLE);
    }

    pub fn map_huge(&mut self, index: usize, addr: u64) {
        self.map_flags(index, addr, PRESENT | WRITABLE | HUGE_PAGE);
    }

    pub fn map_flags(&mut self, index: usize, addr: u64, flags: u64) {
        let value = (addr & ADDRMASK) | flags;

        // does not work: rust optimizes this so far, &self is u16
        // but it must be u32!
        //self.entries[index] = value;

        // weird asm shenanigans so that the linker can work
        // see also: main.rs / STAGE3_ENTRY
        // (rust really hates unreal mode)
        let high: u32 = (value >> 32) as u32;
        let low: u32 = (value & 0xffff_ffff) as u32;
        unsafe {
            core::arch::asm!(
                "mov dword ptr ds:[{2}], {0}",
                "mov dword ptr ds:[{2} + 4], {1}",
                in(reg) low,
                in(reg) high,
                in(reg) &self.entries[index],
            );
        }
    }
}

// linker script puts these above 0x10000
extern "cdecl" {
    static mut P1: PageTable;
    static mut P2: PageTable;
    static mut P3: PageTable;
    static mut P4: PageTable;
}

pub unsafe fn load() {
    // fill in an identity map for 2MB
    P1.clear();
    for i in 0..P1.entries.len() {
        P1.map(i, i as u64 * 0x1000);
    }

    P2.clear();
    P2.map(
        0,
        ((crate::BOOT_SEGMENT as u64) << 4) + core::ptr::addr_of!(P1) as u64,
    );

    P3.clear();
    P3.map(
        0,
        ((crate::BOOT_SEGMENT as u64) << 4) + core::ptr::addr_of!(P2) as u64,
    );

    P4.clear();
    P4.map(
        0,
        ((crate::BOOT_SEGMENT as u64) << 4) + core::ptr::addr_of!(P3) as u64,
    );

    core::arch::asm!(
        // set PAE and PGE bit
        "mov cr4, {}",
        in(reg) 0b10100000,
    );

    core::arch::asm!(
        // move &P4 into cr3
        "mov cr3, {}",
        in(reg) ((crate::BOOT_SEGMENT as u32) << 4) + core::ptr::addr_of!(P4) as u32,
    );
}
