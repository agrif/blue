#[repr(C, packed)]
pub struct DescriptorTablePointer {
    pub limit: u16,
    pub base: u32,
}

impl DescriptorTablePointer {
    pub unsafe fn lgdt(&self) {
        core::arch::asm!("lgdt [{}]", in(reg) self);
    }

    pub unsafe fn lidt(&self) {
        core::arch::asm!("lidt [{}]", in(reg) self);
    }
}

#[derive(Clone, Debug)]
pub struct GlobalDescriptorTable {
    table: [u64; 8],
    next: usize,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum PrivilegeLevel {
    Ring0 = 0,
    Ring1 = 1,
    Ring2 = 2,
    Ring3 = 3,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum SegmentType {
    Data = 0,
    Code = 1,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum SegmentSize {
    Code16 = 0b00,
    Code32 = 0b10,
    Code64 = 0b01,
}

impl GlobalDescriptorTable {
    pub const fn new() -> Self {
        Self {
            table: [0; 8],
            next: 1,
        }
    }

    pub const fn add_entry(
        &mut self,
        base: u32,
        limit: u32,
        privilege: PrivilegeLevel,
        typ: SegmentType,
        size: SegmentSize,
    ) -> u16 {
        let mut entry = 0;

        entry |= ((base & 0xff00_0000) as u64) << 32;
        entry |= ((base & 0x00ff_ffff) as u64) << 16;

        entry |= ((limit & 0x000f_0000) as u64) << 32;
        entry |= ((limit & 0x0000_ffff) as u64) << 0;

        let mut access_byte: u8 = 0;
        access_byte |= 1 << 7; // present bit, always 1
        access_byte |= (privilege as u8) << 5; // ring
        access_byte |= 1 << 4; // 0 = task, 1 = code or data
        access_byte |= (typ as u8) << 3; // 0 = data, 1 = code
        access_byte |= 0 << 2; // data: 0 = up, 1 = down / code:cross priv
        access_byte |= 1 << 1; // data: write / code: read, 1 = allowed
        access_byte |= 0 << 0; // accessed bit, set by cpu when accessed
        entry |= (access_byte as u64) << 40;

        let mut flags: u8 = 0;
        flags |= 1 << 3; // granularity, 0 = byte, 1 = 4kiB
        flags |= (size as u8) << 1; // code size, 00 = 16, 10 = 32, 01 = 64
        flags |= 0 << 0; // reserved
        entry |= ((flags & 0x0f) as u64) << 52;

        unsafe { self.add_raw(entry) }
    }

    pub const unsafe fn add_raw(&mut self, entry: u64) -> u16 {
        if self.next >= self.table.len() {
            panic!("GDT full");
        }
        let index = self.next;
        self.table[index] = entry;
        self.next += 1;

        self.selector(index)
    }

    pub const fn selector(&self, index: usize) -> u16 {
        // extract the ring from the entry to use
        let privelege = ((self.table[index] >> 45) & 0b11) as u16;
        ((index as u16) << 3) | privelege
    }

    pub fn load(&'static self) {
        let ptr = DescriptorTablePointer {
            limit: (self.next * core::mem::size_of_val(&self.table[0]) - 1) as u16,
            // careful: our DS is set to BOOT_SEGMENT
            base: self.table.as_ptr() as u32 + ((crate::BOOT_SEGMENT as u32) << 4),
        };
        unsafe {
            ptr.lgdt();
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GDTInfo {
    pub data32: u16,
    pub code64: u16,
}

extern "cdecl" {
    static mut GDT: GlobalDescriptorTable;
}

static mut GDTINFO: Option<GDTInfo> = None;

pub fn load() -> &'static GDTInfo {
    unsafe {
        if let Some(ref info) = GDTINFO {
            return info;
        }

        let mut gdt = GlobalDescriptorTable::new();

        let data32 = gdt.add_entry(
            0x0000_0000,
            0x000f_ffff,
            PrivilegeLevel::Ring0,
            SegmentType::Data,
            SegmentSize::Code32,
        );
        let code64 = gdt.add_entry(
            0x0000_0000,
            0x000f_ffff,
            PrivilegeLevel::Ring0,
            SegmentType::Code,
            SegmentSize::Code64,
        );
        // temporary 0x18 16 bit code selector for stage3, FIXME
        let code16 = gdt.add_entry(
            0x0000_0000,
            0x0000_000f,
            PrivilegeLevel::Ring0,
            SegmentType::Code,
            SegmentSize::Code16,
        );

        GDT = gdt;
        GDT.load();

        GDTINFO.insert(GDTInfo { data32, code64 })
    }
}

pub unsafe fn unreal_mode() {
    let GDTInfo { data32, .. } = load();
    core::arch::asm!(
        // stop interrupts and save old data segment
        "cli",
        "push ds",

        // set pmode bit
        "mov eax, cr0",
        "or al, 1",
        "mov cr0, eax",

        // expected jump
        "jmp 2f",
        "2:",

        // load data descriptor
        "mov ds, {:x}",

        // unset pmode bit
        "and al, 0xfe",
        "mov cr0, eax",

        // expected jump
        "jmp 3f",
        "3:",

        // restore old data segment and interrupts
        "pop ds",
        "sti",

        in(reg) *data32,
        out("eax") _,
    );
}

pub unsafe fn long_mode(entry: unsafe extern "cdecl" fn() -> !) -> ! {
    let zero_idt = DescriptorTablePointer { limit: 0, base: 0 };
    let GDTInfo { data32, code64 } = load();

    // turn all interrupts off, until we make it inside
    core::arch::asm!("cli");

    // load an empty idt. any interrupt now will triple-fault
    zero_idt.lidt();

    core::arch::asm!(
        // set the LME bit
        "mov ecx, 0xc0000080",
        "rdmsr",
        "or eax, 0x00000100",
        "wrmsr",
        out("ecx") _,
        out("edx") _,
        out("eax") _,
    );

    core::arch::asm!(
        // turn on paging and protection simultaneously
        "mov ebx, cr0",
        "or ebx, 0x80000001",
        "mov cr0, ebx",
        out("ebx") _,
    );

    core::arch::asm!(
        // set selectors, setup stack, and jump to long mode
        // stack is placed at the end of our boot segment
        // (this is where it is now, we just reset it to top)
        "mov ds, {0}",
        "mov es, {0}",
        "mov ss, {0}",
        "mov esp, {1}",
        "mov ebp, {1}",
        "push {2}",
        "push {3}",
        "retf",
        in(reg) *data32 as u32,
        const ((crate::BOOT_SEGMENT as u32) << 4) + 0xfff0,
        in(reg) *code64 as u32,
        in(reg) entry as u32,
        options(noreturn),
    )
}
