use core::sync::atomic::{AtomicUsize, Ordering};

// a lot of the global state code here is modeled on the log crate

static mut TRAMPOLINE: &dyn Trampoline = &PanicTrampoline;

const UNINITIALIZED: usize = 0;
const INITIALIZING: usize = 1;
const INITIALIZED: usize = 2;

static STATE: AtomicUsize = AtomicUsize::new(UNINITIALIZED);

const SET_ERROR: &str = "trampoline already set";

pub trait Trampoline {
    unsafe fn trampoline(&self, code16: unsafe extern "cdecl" fn());
}

struct PanicTrampoline;

impl Trampoline for PanicTrampoline {
    unsafe fn trampoline(&self, _code16: unsafe extern "cdecl" fn()) {
        panic!("real mode trampoline not installed")
    }
}

pub fn set_trampoline(tramp: &'static dyn Trampoline) -> Result<(), &'static str> {
    let old_state = match STATE.compare_exchange(
        UNINITIALIZED,
        INITIALIZING,
        Ordering::SeqCst,
        Ordering::SeqCst,
    ) {
        Ok(s) | Err(s) => s,
    };

    match old_state {
        UNINITIALIZED => {
            unsafe {
                TRAMPOLINE = tramp;
            }
            STATE.store(INITIALIZED, Ordering::SeqCst);
            Ok(())
        }
        INITIALIZING => {
            while STATE.load(Ordering::SeqCst) == INITIALIZING {
                core::hint::spin_loop();
            }
            Err(SET_ERROR)
        }
        _ => Err(SET_ERROR),
    }
}

// unsafe: inner function *must* be .code16 and live in 0x0-0xffff
// currently in rust, this means they must be naked functions
// these functions must leave registers unmodified (except rax, rcx, rdx)
// (see the macro real_mode_asm!)
// also *wildly* not threadsafe
pub unsafe fn trampoline(code16: unsafe extern "cdecl" fn()) {
    assert!((code16 as usize) < 0x10000);
    if STATE.load(Ordering::SeqCst) != INITIALIZED {
        PanicTrampoline.trampoline(code16);
    } else {
        TRAMPOLINE.trampoline(code16);
    }
}

pub const WORK_SIZE: usize = 0x400;

#[repr(C, align(0x100))]
pub struct Work([u8; WORK_SIZE]);

pub struct WorkOffset<T> {
    pub offset: usize,
    marker: core::marker::PhantomData<T>,
}

impl Work {
    const fn new() -> Self {
        Self([0; WORK_SIZE])
    }

    pub const fn root() -> WorkOffset<()> {
        WorkOffset {
            offset: 0,
            marker: core::marker::PhantomData,
        }
    }

    pub const fn allocate<T>() -> WorkOffset<T>
    where
        T: bytemuck::Pod,
    {
        Self::root().allocate()
    }

    pub fn get<T>(&self, offset: &WorkOffset<T>) -> &T
    where
        T: bytemuck::Pod,
    {
        bytemuck::from_bytes(&self.0[offset.offset..offset.offset + core::mem::size_of::<T>()])
    }

    pub fn get_mut<T>(&mut self, offset: &WorkOffset<T>) -> &mut T
    where
        T: bytemuck::Pod,
    {
        bytemuck::from_bytes_mut(
            &mut self.0[offset.offset..offset.offset + core::mem::size_of::<T>()],
        )
    }

    pub fn put<T>(&mut self, offset: &WorkOffset<T>, value: T)
    where
        T: bytemuck::Pod,
    {
        *self.get_mut(offset) = value;
    }
}

impl<T> WorkOffset<T>
where
    T: bytemuck::Pod,
{
    pub const fn allocate<U>(&self) -> WorkOffset<U> {
        let mut offset = self.offset + core::mem::size_of::<T>();
        let align = core::mem::align_of::<U>();
        if offset % align > 0 {
            offset += align - (offset % align);
        }
        assert!(offset % align == 0);
        assert!(align < core::mem::align_of::<Work>());
        assert!(offset + core::mem::size_of::<U>() < WORK_SIZE);
        WorkOffset {
            offset: offset,
            marker: core::marker::PhantomData,
        }
    }
}

impl<T> core::fmt::Debug for WorkOffset<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WorkOffset")
            .field("offset", &self.offset)
            .field("size", &core::mem::size_of::<T>())
            .field("align", &core::mem::align_of::<T>())
            .finish()
    }
}

impl core::ops::Deref for Work {
    type Target = [u8; WORK_SIZE];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for Work {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[link_section = ".realmode"]
pub static mut WORK: Work = Work::new();

/// internal detail of real_asm!
#[macro_export]
#[cfg(target_arch = "x86_64")]
macro_rules! _asm_code_default {
    () => {
        ".code64"
    };
}

// notably absent: .code32

/// internal detail of real_asm!
#[macro_export]
#[cfg(not(target_arch = "x86_64"))]
macro_rules! _asm_code_default {
    () => {
        ".code16"
    };
}

// real-mode embedded assembly
// you must put all parameters you need in/out in WORK
// this is exposed in the assembly as {0}
// this handles naked functions, .realmode, .code16, .code64, and ret for you
// use named parameters at the end, like asm!
// ax = static value,
// const, sym work as in asm!
// static creates a new static in .realmode, assigns symbol
// alloc allocates space automatically from WORK[0..], assigns *offset*
// static and alloc both bind their names to ptrs to the output value
#[macro_export]
macro_rules! real_asm {
    ($($line:literal),+) => { $crate::real_asm!($($line),+,) };
    ($($line:literal),+,$($name:ident $(:$ty:ty)? = $kind:tt $($val:expr)?),+,) => {
        $crate::real_asm!($($line),+,$($name $(:$ty)? = $kind $($val)?),+)
    };
    ($($line:literal),+,$($name:ident $(:$ty:ty)? = $kind:tt $($val:expr)?),*) => {
        $crate::real_asm!(@helper sort,
                          asm {$($line,)*},
                          const {},
                          sym {},
                          $($name $(:$ty)? = $kind $($val)?,)*
        )
    };

    (@helper sort, asm {$($line:literal,)*}, const {$($cname:ident : $cty:ty = $ckind:tt $cval:expr,)*}, sym {$($sname:ident $ssym:ident : $sty:ty = $skind:tt $sval:expr,)*}, $name:ident = const $val:expr, $($rest:tt)*) => {
        $crate::real_asm!(@helper sort,
                          asm {$($line,)*},
                          const {
                              $($cname : $cty = $ckind $cval,)*
                              $name : () = const $val,
                          },
                          sym {
                              $($sname $ssym : $sty = $skind $sval,)*
                          },
                          $($rest)*
        )
    };
    (@helper sort, asm {$($line:literal,)*}, const {$($cname:ident : $cty:ty = $ckind:tt $cval:expr,)*}, sym {$($sname:ident $ssym:ident : $sty:ty = $skind:tt $sval:expr,)*}, $name:ident = sym $val:expr, $($rest:tt)*) => {
        ::paste::paste! {
            $crate::real_asm!(@helper sort,
                              asm {$($line,)*},
                              const {
                                  $($cname : $cty = $ckind $cval,)*
                              },
                              sym {
                                  $($sname $ssym : $sty = $skind $sval,)*
                                  $name [<$val>] : () = sym $val,
                              },
                              $($rest)*
            )
        }
    };
    (@helper sort, asm {$($line:literal,)*}, const {$($cname:ident : $cty:ty = $ckind:tt $cval:expr,)*}, sym {$($sname:ident $ssym:ident : $sty:ty = $skind:tt $sval:expr,)*}, $name:ident : $ty:ty = alloc $val:expr, $($rest:tt)*) => {
        $crate::real_asm!(@helper sort,
                          asm {$($line,)*},
                          const {
                              $($cname : $cty = $ckind $cval,)*
                              $name : $ty = alloc $val,
                          },
                          sym {
                              $($sname $ssym : $sty = $skind $sval,)*
                          },
                          $($rest)*
        )
    };
    (@helper sort, asm {$($line:literal,)*}, const {$($cname:ident : $cty:ty = $ckind:tt $cval:expr,)*}, sym {$($sname:ident $ssym:ident : $sty:ty = $skind:tt $sval:expr,)*}, $name:ident : $ty:ty = alloc, $($rest:tt)*) => {
        $crate::real_asm!(@helper sort,
                          asm {
                              $($line,)*
                          },
                          const {
                              $($cname : $cty = $ckind $cval,)*
                              $name : $ty = alloc_uninit (),
                          },
                          sym {
                              $($sname $ssym : $sty = $skind $sval,)*
                          },
                          $($rest)*
        )
    };
    (@helper sort, asm {$($line:literal,)*}, const {$($cname:ident : $cty:ty = $ckind:tt $cval:expr,)*}, sym {$($sname:ident $ssym:ident : $sty:ty = $skind:tt $sval:expr,)*}, $name:ident : $ty:ty = static $val:expr, $($rest:tt)*) => {
        ::paste::paste! {
        $crate::real_asm!(@helper sort,
                          asm {$($line,)*},
                          const {
                              $($cname : $cty = $ckind $cval,)*
                          },
                          sym {
                              $($sname $ssym : $sty = $skind $sval,)*
                              $name [<__work_static_ $name>] : $ty = static $val,
                          },
                          $($rest)*
        )
        }
    };
    (@helper sort, asm {$($line:literal,)*}, const {$($cname:ident : $cty:ty = $ckind:tt $cval:expr,)*}, sym {$($sname:ident $ssym:ident : $sty:ty = $skind:tt $sval:expr,)*}, $name:ident : $ty:ty = static, $($rest:tt)*) => {
        ::paste::paste! {
            $crate::real_asm!(@helper sort,
                              asm {$($line,)*},
                              const {
                                  $($cname : $cty = $ckind $cval,)*
                              },
                              sym {
                                  $($sname $ssym : $sty = $skind $sval,)*
                                  $name [<__work_static_ $name>] : $ty = static_uninit (),
                              },
                              $($rest)*
            )
        }
    };
    (@helper sort, asm {$($line:literal,)*}, const {$($cname:ident : $cty:ty = $ckind:tt $cval:expr,)*}, sym {$($sname:ident $ssym:ident : $sty:ty = $skind:tt $sval:expr,)*},) => {
        $crate::real_asm!(@helper impl,
                          asm {$($line,)*},
                          const {
                              $($cname : $cty = $ckind $cval,)*
                          },
                          sym {
                              $($sname $ssym : $sty = $skind $sval,)*
                          },
                          all {
                              $($cname : $cty = $ckind $cval,)*
                              $($sname : $sty = $skind $sval,)*
                          }
        );
    };

    (@helper impl, asm {$($line:literal,)*}, const {$($cname:ident : $cty:ty = $ckind:tt $cval:expr,)*}, sym {$($sname:ident $ssym:ident : $sty:ty = $skind:tt $sval:expr,)*}, all {$($name:ident : $ty:ty = $kind:tt $val:expr,)*}) => {
        const __work_root: $crate::WorkOffset<()> = $crate::Work::root();
        $crate::real_asm!(@helper pre, __work_root, $($name : $ty = $kind $val,)*);

        #[link_section = ".realmode"]
        #[naked]
        unsafe extern "cdecl" fn __real_mode_asm() {
            ::core::arch::asm!(
                "; work = {0}",
                ".code16",
                $($line),+,
                "ret",
                $crate::_asm_code_default!(),
                sym $crate::WORK,
                $($cname = const $crate::real_asm!(@helper asmval, $cname : $cty = $ckind $cval),)*
                $($sname = sym $ssym,)*
                options(noreturn),
            );
        }

        $crate::trampoline(__real_mode_asm);

        $crate::real_asm!(@helper post, $($name : $ty = $kind $val,)*);
    };

    (@helper pre, $root:expr,) => {};
    (@helper pre, $root:expr, $name:ident : $ty:ty = const $val:expr, $($rest:tt)*) => {
        $crate::real_asm!(@helper pre, $root, $($rest)*);
    };
    (@helper pre, $root:expr, $name:ident : $ty:ty = sym $val:expr, $($rest:tt)*) => {
        $crate::real_asm!(@helper pre, $root, $($rest)*);
    };
    (@helper pre, $root:expr, $name:ident : $ty:ty = alloc_uninit $val:expr, $($rest:tt)*) => {
        ::paste::paste! {
            const [<__work_alloc_ $name>]: $crate::WorkOffset<$ty> = $root.allocate();
            $crate::real_asm!(@helper pre, [<__work_alloc_ $name>], $($rest)*);
        }
    };
    (@helper pre, $root:expr, $name:ident : $ty:ty = alloc $val:expr, $($rest:tt)*) => {
        ::paste::paste! {
            const [<__work_alloc_ $name>]: $crate::WorkOffset<$ty> = $root.allocate();
            $crate::WORK.put(&[<__work_alloc_ $name>], $val);
            $crate::real_asm!(@helper pre, [<__work_alloc_ $name>], $($rest)*);
        }
    };
    (@helper pre, $root:expr, $name:ident : $ty:ty = static $val:expr, $($rest:tt)*) => {
        ::paste::paste! {
            #[link_section = ".realmode"]
            static mut [<__work_static_ $name>]: $ty = ::const_default::ConstDefault::DEFAULT;
            [<__work_static_ $name>] = $val;
            $crate::real_asm!(@helper pre, $root, $($rest)*);
        }
    };

    (@helper pre, $root:expr, $name:ident : $ty:ty = static_uninit $val:expr, $($rest:tt)*) => {
        ::paste::paste! {
            #[link_section = ".realmode"]
            static mut [<__work_static_ $name>]: $ty = ::const_default::ConstDefault::DEFAULT;
            $crate::real_asm!(@helper pre, $root, $($rest)*);
        }
    };

    (@helper asmval, $name:ident : $ty:ty = const $val:expr) => {
        $val
    };
    (@helper asmval, $name:ident : $ty:ty = alloc $val:expr) => {
        ::paste::paste! { [<__work_alloc_ $name>].offset }
    };
    (@helper asmval, $name:ident : $ty:ty = alloc_uninit $val:expr) => {
        ::paste::paste! { [<__work_alloc_ $name>].offset }
    };

    (@helper post,) => {};
    (@helper post, $name:ident : $ty:ty = const $val:expr, $($rest:tt)*) => {
        $crate::real_asm!(@helper post, $($rest)*);
    };
    (@helper post, $name:ident : $ty:ty = sym $val:expr, $($rest:tt)*) => {
        $crate::real_asm!(@helper post, $($rest)*);
    };
    (@helper post, $name:ident : $ty:ty = alloc $val:expr, $($rest:tt)*) => {
        ::paste::paste! {
            let $name: &$ty = $crate::WORK.get(&[<__work_alloc_ $name>]);
            $crate::real_asm!(@helper post, $($rest)*);
        }
    };
    (@helper post, $name:ident : $ty:ty = alloc_uninit $val:expr, $($rest:tt)*) => {
        ::paste::paste! {
            let $name: &$ty = $crate::WORK.get(&[<__work_alloc_ $name>]);
            $crate::real_asm!(@helper post, $($rest)*);
        }
    };
    (@helper post, $name:ident : $ty:ty = static $val:expr, $($rest:tt)*) => {
        ::paste::paste! {
            let $name: &$ty = &[<__work_static_ $name>];
            $crate::real_asm!(@helper post, $($rest)*);
        }
    };
    (@helper post, $name:ident : $ty:ty = static_uninit $val:expr, $($rest:tt)*) => {
        ::paste::paste! {
            let $name: &$ty = &[<__work_static_ $name>];
            $crate::real_asm!(@helper post, $($rest)*);
        }
    };
}
