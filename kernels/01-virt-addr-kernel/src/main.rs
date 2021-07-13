#![feature(naked_functions, asm, global_asm)]
#![feature(alloc_error_handler)]
#![feature(panic_info_message)]
#![feature(generator_trait)]
#![feature(destructuring_assignment)]
#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
mod console;
mod sbi;
mod executor;
mod mm;

use core::panic::PanicInfo;

pub extern "C" fn rust_main(hartid: usize, dtb_pa: usize) -> ! {
    println!("[kernel] Hart id = {}, DTB physical address = {:#x}", hartid, dtb_pa);
    mm::heap_init();
    mm::test_frame_alloc();
    // 页帧分配器。对整个物理的地址空间来说，无论有多少个核，页帧分配器只有一个。
    let from = mm::PhysAddr(0x80420000).page_number::<mm::Sv39>();
    let to = mm::PhysAddr(0x80800000).page_number::<mm::Sv39>(); // 暂时对qemu写死
    let frame_alloc = spin::Mutex::new(mm::StackFrameAllocator::new(from, to));
    let mut kernel_addr_space = mm::PagedAddrSpace::try_new_in(mm::Sv39, &frame_alloc)
        .expect("allocate page to create kernel paged address space");
    mm::test_map_solve();
    kernel_addr_space.allocate_map(
        mm::VirtAddr(0x80000000).page_number::<mm::Sv39>(), 
        mm::PhysAddr(0x80000000).page_number::<mm::Sv39>(), 
        1024,
        mm::Sv39Flags::R | mm::Sv39Flags::W | mm::Sv39Flags::X
    ).expect("allocate one mapped space");
    let (vpn, ppn, n) = get_trampoline_paging_config::<mm::Sv39>();
    let trampoline_va_start = vpn.addr_begin::<mm::Sv39>();
    kernel_addr_space.allocate_map(
        vpn, ppn, n,
        mm::Sv39Flags::R | mm::Sv39Flags::W | mm::Sv39Flags::X | mm::Sv39Flags::U
    ).expect("allocate trampoline code mapped space");
    kernel_addr_space.allocate_map(
        mm::VirtAddr(0x80400000).page_number::<mm::Sv39>(), 
        mm::PhysAddr(0x80400000).page_number::<mm::Sv39>(), 
        32,
        mm::Sv39Flags::R | mm::Sv39Flags::W | mm::Sv39Flags::X | mm::Sv39Flags::U
    ).expect("allocate user mapped space");
    mm::test_asid_alloc();
    let max_asid = mm::max_asid();
    let mut asid_alloc = mm::StackAsidAllocator::new(max_asid);
    let kernel_asid = asid_alloc.allocate_asid().expect("alloc kernel asid");
    let kernel_satp = unsafe {
        mm::activate_paged_riscv_sv39(kernel_addr_space.root_page_number(), kernel_asid)
    };
    // println!("kernel satp = {:x?}", kernel_satp);
    let mut rt = executor::Runtime::new_user(
        0x80400000, 
        kernel_satp,// todo: use user satp
        trampoline_va_start
    ); 
    use core::pin::Pin;
    use core::ops::Generator;
    loop {
        match Pin::new(&mut rt).resume(()) {
            s => println!("state: {:?}", s),
        }
    }
    // sbi::shutdown()
}       

fn get_trampoline_paging_config<M: mm::PageMode>() -> (mm::VirtPageNum, mm::PhysPageNum, usize) {
    let (trampoline_pa_start, trampoline_pa_end) = {
        extern "C" { fn strampoline(); fn etrampoline(); }
        (strampoline as usize, etrampoline as usize)
    };
    assert_ne!(trampoline_pa_start, trampoline_pa_end, "trampoline code not declared");
    let trampoline_len = trampoline_pa_end - trampoline_pa_start;
    let trampoline_va_start = usize::MAX - trampoline_len + 1;
    let vpn = mm::VirtAddr(trampoline_va_start).page_number::<M>();
    let ppn = mm::PhysAddr(trampoline_pa_start).page_number::<M>();
    let n = trampoline_len >> M::FRAME_SIZE_BITS;
    // println!("va = {:x?}, pa = {:x?} {:x?}", trampoline_va_start, trampoline_pa_start, trampoline_pa_end);
    // println!("l = {:x?}", trampoline_len);
    // println!("vpn = {:x?}, ppn = {:x?}, n = {}", vpn, ppn, n);
    (vpn, ppn, n)
}

#[cfg_attr(not(test), panic_handler)]
#[allow(unused)]
fn panic(info: &PanicInfo) -> ! {
    if let Some(location) = info.location() {
        println!(
            "Panicked at {}:{} {}",
            location.file(),
            location.line(),
            info.message().unwrap()
        );
    } else {
        println!("Panicked: {}", info.message().unwrap());
    }
    sbi::shutdown()
}

const BOOT_STACK_SIZE: usize = 4096 * 4 * 8;

#[link_section = ".bss.stack"]
static mut BOOT_STACK: [u8; BOOT_STACK_SIZE] = [0; BOOT_STACK_SIZE];

#[naked]
#[link_section = ".text.entry"] 
#[export_name = "_start"]
unsafe extern "C" fn entry() -> ! {
    asm!("
    # 1. set sp
    # sp = bootstack + (hartid + 1) * 0x10000
    add     t0, a0, 1
    slli    t0, t0, 14
1:  auipc   sp, %pcrel_hi({boot_stack})
    addi    sp, sp, %pcrel_lo(1b)
    add     sp, sp, t0

    # 2. jump to rust_main (absolute address)
1:  auipc   t0, %pcrel_hi({rust_main})
    addi    t0, t0, %pcrel_lo(1b)
    jr      t0
    ", 
    boot_stack = sym BOOT_STACK, 
    rust_main = sym rust_main,
    options(noreturn))
}
