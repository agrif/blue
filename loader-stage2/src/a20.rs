fn check_raw(a: u8, b: u8) -> bool {
    let mut a20_enabled: u16;
    unsafe {
        core::arch::asm!(
            // save state
            "pushf",
            "push ds",
            "push es",
            "push di",
            "push si",

            // set es = 0, ds = 0xffff
            "xor ax, ax",
            "mov es, ax",
            "not ax",
            "mov ds, ax",
            "mov di, 0x0500",
            "mov si, 0x0510",

            // read two bytes and save them
            "mov al, byte ptr es:[di]",
            "push ax",
            "mov al, byte ptr ds:[si]",
            "push ax",

            // write two bytes that are in the same place if A20 is off
            "mov byte ptr es:[di], {0}",
            "mov byte ptr ds:[si], {1}",

            // compare the first location to what we wrote to second
            "cmp byte ptr es:[di], {1}",

            // restore old data
            "pop ax",
            "mov byte ptr ds:[si], al",
            "pop ax",
            "mov byte ptr es:[di], al",

            // default output: off
            "mov ax, 0",

            // if our cmp above was equal, A20 must be off, so skip to end
            "je 2f",

            // if we're still here, A20 is on
            "mov ax, 1",

            // restore state and exit
            "2:",
            "pop si",
            "pop di",
            "pop es",
            "pop ds",
            "popf",
            in(reg_byte) a,
            in(reg_byte) b,
            out("ax") a20_enabled,
        );
    }
    a20_enabled > 0
}

pub fn check() -> bool {
    // check twice, just in case what's there happens to be our test value
    check_raw(0x00, 0xff) && check_raw(0xff, 0x00)
}

unsafe fn fast_a20_gate() {
    core::arch::asm!(
        "in al, 0x92",
        "test al, 2",
        "jnz 2f",
        "or al, 2",
        "and al, 0xfe",
        "out 0x92, al",
        "2:",
        out("al") _,
    );
}

pub unsafe fn enable() {
    // this is a minefield:
    // https://wiki.osdev.org/A20_Line
    // lets try the easy one and hope it works
    for method in [fast_a20_gate] {
        if check() {
            return;
        }
        method();
    }

    if !check() {
        panic!("Could not enable A20 line.");
    }
}
