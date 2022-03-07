blue
----

Experiments in bare-metal x86 rust. Try it out:

    rustup component add llvm-tools-preview
    cargo run --release
    qemu-system-x86_64 --hda disk.img
