#[link_section = ".realmode"]
static mut SAVE_RSP: usize = 0;
#[link_section = ".realmode"]
static mut SAVE_RBP: usize = 0;

#[link_section = ".realmode"]
static mut SAVE_IDT: x86_64::structures::DescriptorTablePointer =
    x86_64::structures::DescriptorTablePointer {
        limit: 0,
        base: x86_64::addr::VirtAddr::zero(),
    };

#[link_section = ".realmode"]
static mut SAVE_INNER: u32 = 0;

pub struct LongModeTrampoline;

impl blue_real::Trampoline for LongModeTrampoline {
    #[link_section = ".realmode"]
    #[inline(never)]
    unsafe fn trampoline(&self, code16: unsafe extern "cdecl" fn()) {
        SAVE_INNER = code16 as u32;

        SAVE_IDT = x86_64::instructions::tables::sidt();
        x86_64::instructions::tables::lidt(&x86_64::structures::DescriptorTablePointer {
            limit: 0x3ff,
            base: x86_64::addr::VirtAddr::new(0x0),
        });

        // must be asm, at least until we reach .code64 again
        core::arch::asm!(
            // start in long mode
            // save our stack
            "mov [{rsp}], rsp",
            "mov [{rbp}], rbp",

            // retf trick as a far jump into FIXME 0x18, the 16 bit code segment
            "push 0x18",
            "lea rax, [2f]",
            "push rax",
            "retfq",

            // 16-bit protected mode, paging enabled
            "2:",
            ".code16",

            // disable paging and protection
            "mov eax, cr0",
            "and eax, 0x7ffffffe",
            "mov cr0, eax",

            // set up real-mode-friendly segments stack, and then set CS
            "mov esp, 0xfff0",
            "xor ax, ax",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            "push 0x0",
            "lea eax, [3f]",
            "push eax",
            "retf",

            // real-mode
            "3:",
            //"mov ax, 0x0e00",
            //"or ax, cx",
            //"mov ebx, 7",
            //"int 0x10",
            "mov eax, [{inner}]",
            "call eax",

            // turn on paging and protection
            "mov eax, cr0",
            "or eax, 0x80000001",
            "mov cr0, eax",

            // switch to code64 (FIXME)
            "push 0x10",
            "lea eax, [4f]",
            "push eax",
            "retf",

            // back to long mode, set up data descriptors (FIXME)
            "4:",
            ".code64",
            "mov ax, 0x08",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",

            // restore stack
            "mov rsp, [{rsp}]",
            "mov rbp, [{rbp}]",

            rsp = sym SAVE_RSP,
            rbp = sym SAVE_RBP,
            inner = sym SAVE_INNER,

            out("rax") _,
            out("rcx") _,
            out("rdx") _,
        );

        x86_64::instructions::tables::lidt(&SAVE_IDT);
    }
}
