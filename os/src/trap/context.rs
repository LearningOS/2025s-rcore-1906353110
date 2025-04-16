use riscv::register::sstatus::{self, Sstatus, SPP};
// use riscv::register::sstatus::{self, Sstatus};

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
}

impl TrapContext {
    /// put the sp(stack pointer) into x\[2\] field of TrapContext
    pub fn set_sp(&mut self, sp: usize) {
        self.x[2] = sp;
    }

    /// init the trap context of an application
    pub fn app_init_context(entry: usize, sp: usize) -> Self {
        let mut sstatus = sstatus::read(); // CSR sstatus

        // let sstatus = sstatus::read(); // CSR sstatus
        //测试了 确实没变 这里好像一直都是u态的的 估计还是不完备

        // println!("sstatus pre: {:?}", sstatus);
        // println!("entry: {:#x}", entry);
        // println!("sp: {:#x}", sp);

        sstatus.set_spp(SPP::User); //previous privilege mode: user mode

        let mut cx = Self {
            x: [0; 32],
            sstatus,
            sepc: entry, // entry point of app
        };
        cx.set_sp(sp); // app's user stack pointer
        // println!("{:?}",cx);
        cx // return initial Trap Context of app
    }
}
