INCLUDE ../layout.ld

MEMORY {
    body : ORIGIN = 0, LENGTH = 360
    blocklist : ORIGIN = 360, LENGTH = 80
}

ENTRY(_start);

SECTIONS {
    .text      : { *(.startup) *(.text*) } > body
    .rodata    : { *(.rodata*) }           > body
    .data      : { *(.data*) }             > body
    .bss       : { *(.bss*) }              > body
    .blocklist : { KEEP(*(.blocklist*)) }  > blocklist

    /DISCARD/ : { *(.eh_frame*) }
}
