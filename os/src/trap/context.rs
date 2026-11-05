use riscv::register::sstatus::{self, Sstatus, SPP};

#[repr(C)]
#[derive(Debug)]
/// trap context structure containing sstatus, sepc and registers
pub struct TrapContext {
    /// General-Purpose Register x0-31
    pub x: [usize; 32],
    /// Supervisor Status Register
    pub sstatus: Sstatus,
    /// Supervisor Exception Program Counter
    pub sepc: usize,
    /// Floating-point register f0-31
    pub f:[u64; 32],
    /// Floating-point control and status register
    pub fcsr: u64,
}

impl TrapContext {
    /// put the sp(stack pointer) into x\[2\] field of TrapContext
    pub fn set_sp(&mut self, sp: usize) {
        self.x[2] = sp;
    }
    
    /// init the trap context of an application
    pub fn app_init_context(entry: usize, sp: usize) -> Self {
        // 开启 FPU：FS = Off 时用户态浮点指令会触发非法指令异常
        unsafe { sstatus::set_fs(sstatus::FS::Initial);}
        let mut sstatus = sstatus::read(); // CSR sstatus
        sstatus.set_spp(SPP::User); //previous privilege mode: user mode
        let mut cx = Self {
            x: [0; 32],
            sstatus,
            sepc: entry, // entry point of app
            f: [0; 32],
            fcsr: 0,
        };
        cx.set_sp(sp); // app's user stack pointer
        cx // return initial Trap Context of app
    }
}
