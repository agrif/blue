INCLUDE ../layout.ld

ENTRY(_start);

SECTIONS {
    . = STAGE3_ENTRY;
    .text       : { *(.startup) *(.text*) }

    .realmode 0x500 : AT(LOADADDR(.text) + SIZEOF(.text)) {
        KEEP(*(.realmode*));
    }
    . = LOADADDR(.realmode) + SIZEOF(.realmode);

    .rodata : AT(.) {
        REALMODE_IMAGE = .;
        QUAD(LOADADDR(.realmode));
        QUAD(SIZEOF(.realmode));
        REALMODE = .;
        QUAD(ADDR(.realmode));
        QUAD(SIZEOF(.realmode));
        BSS = .;
        QUAD(ADDR(.bss));
        QUAD(SIZEOF(.bss));
        
        *(.rodata*);
    }
    .data       : { *(.data*) }
    .bss        : { *(.bss*) }

    /DISCARD/ : { *(.eh_frame*) }
}
