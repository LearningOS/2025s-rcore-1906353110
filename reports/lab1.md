# 实现的功能
就在入口那里添加了计算，暂时还不支持这里由于系统调用有限，所以全部用的数组来计数的。多写几个匹配。

# 简答作业
## 1. 正确进入 U 态后，程序的特征还应有：使用 S 态特权指令，访问 S 态寄存器后会报错。 请同学们可以自行测试这些内容（运行 三个 bad 测例 (ch2b_bad_*.rs) ）， 描述程序出错行为，同时注意注明你使用的 sbi 及其版本。

第一个0x0不是物理内存地址，qemu里面debug的内存映射，这个应该可以在qemu.log里面看到。
第二三个是因为指令权限不够，用户程序运行在U-mode，而sret和csrr指令需要S-mode的权限才能运行。

## 深入理解 trap.S 中两个函数 __alltraps 和 __restore 的作用，并回答如下问题:
### L40：刚进入 __restore 时，sp 代表了什么值。请指出 __restore 的两种使用情景。
刚进入，sp是内核栈顶，此时内核已经有值了，不是那种空的，存的是TrapCOntext信息。
run_first_task 执行完 switch 之后
run_next_task 执行完 switch 之后

### L43-L48：这几行汇编代码特殊处理了哪些寄存器？这些寄存器的的值对于进入用户态有何意义？请分别解释。
ld t0, 32*8(sp)
ld t1, 33*8(sp)
ld t2, 2*8(sp)
csrw sstatus, t0
csrw sepc, t1
csrw sscratch, t2

处理了 sstatus, sepc, sscratch 3 个寄存器
sstatus 当执行 SRET 指令以从陷阱处理程序返回时，如果 sstatus 寄存器的 SPP 位为 0，则特权级别设置为用户模式
sepc 暂存了上一次用户程序暂停的位置，是接下来用户程序要继续执行的 pc 位置
sscratch 暂存了用户上一次的 sp 栈位置，是接下来用户程序需要接着继续使用的

### L50-L56：为何跳过了 x2 和 x4？
ld x1, 1*8(sp)
ld x3, 3*8(sp)
.set n, 5
.rept 27
   LOAD_GP %n
   .set n, n+1
.endr

x2是sp，后面有单独的汇编代码维护sp；x4是线程寄存器tp，ch3实验没有使用到。

### L60：该指令之后，sp 和 sscratch 中的值分别有什么意义？
csrrw sp, sscratch, sp

这行代码的效果是交换sp和sscratch的值，交换之后sp指向用户栈，而sscratch指向内核栈。也就是从内核栈换到了用户栈。

### __restore：中发生状态切换在哪一条指令？为何该指令执行之后会进入用户态？
最后的sret指令发生状态切换，从S态切换到U态。

当CPU完成Trap处理准备返回的时候，需要通过一条S特权级的特权指令sret来完成，这一条指令具体完成以下功能：

CPU会将当前的特权级按照sstatus的SPP字段设置为U或者S；

CPU会跳转到sepc寄存器指向的那条指令，然后继续执行。

当CPU执行完一条指令并准备从用户特权级陷入（ Trap ）到S特权级的时候，硬件会自动完成：sstatus的SPP字段会被修改为 CPU当前的特权级（U/S），我们是从用户态进入trap的，所以最后sret也就会返回用户态。而在第一次进入用户程序时，os/src/trap/context.rs中有sstatus.set_spp(SPP::User);一行，所以也是进用户态。

### L13：该指令之后，sp 和 sscratch 中的值分别有什么意义？
csrrw sp, sscratch, sp

交换sp和sscratch的值，执行之后sp指向内核栈顶，sscratch指向用户栈顶，即实现用户栈 -> 内核栈。

### 从 U 态进入 S 态是哪一条指令发生的？
ecall，通过ecall调用sbi，或者调用会引发异常的指令或者时钟中断时，会从U态进入S态。
