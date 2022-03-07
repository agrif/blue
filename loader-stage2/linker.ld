INCLUDE ../layout.ld

ENTRY(_start);

/* move page tables out into unreal land, away from stack
   careful, though: physical address must be 0x1000 aligned
   and we are in a non-zero segment! */
P1 = 0x10400;
P2 = 0x11400;
P3 = 0x12400;
P4 = 0x13400;

SECTIONS {
    . = STAGE2_ENTRY;
    .text     : { *(.startup) *(.text*) }
    .realmode : { *(.realmode*) }
    .rodata   : {
        BSS = .;
        LONG(ADDR(.bss));
        LONG(SIZEOF(.bss));
        
        *(.rodata*);
    }
    .data     : { *(.data*) }
    .bss      : { *(.bss*) }

    /DISCARD/ : { *(.eh_frame*) }
}
