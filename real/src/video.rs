#[inline]
pub fn inform(s: &[u8]) {
    for &c in s {
        printc(c);
    }
}

#[inline]
pub fn printc(c: u8) {
    unsafe {
        crate::real_asm!(
            "push ebx",
            "mov ax, [{ax}]",
            "mov ebx, 7",
            "int 0x10",
            "pop ebx",
            ax: u16 = static 0x0e00 | c as u16,
        );
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::video::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    ($fmt:expr) => {
        $crate::print!(concat!($fmt, "\r\n"))
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::print!(concat!($fmt, "\r\n"), $($arg)*)
    };
}

pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    let mut writer = BiosWriter {};
    writer.write_fmt(args).unwrap();
}

struct BiosWriter;

impl core::fmt::Write for BiosWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for c in s.bytes() {
            printc(c);
        }
        Ok(())
    }
}
